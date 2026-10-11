use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::grants::Ability;
use tor_simulation::{ActorId, BodySpec, CreatureIdentity, Game, MemoryRecords, RegionTransition};
use tor_world::{
    Direction, Extent, Location, NamedAnchor, Passage, Position, Region, RegionId, World,
};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

fn fixture() -> (Game, ActorId, ActorId) {
    let world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room{id}"),
                bounds: Extent::new(20, 12, 4).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42);
    let (reference, reference_actor) = super::abilities::game();
    let mut ids = vec![];
    for (location, build) in [
        (
            at(1, 1, 1, 0),
            reference.creature(reference_actor).unwrap().build().clone(),
        ),
        (at(1, 7, 1, 0), super::creature_state::build()),
    ] {
        let id = game
            .spawn_actor(location, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject{}", id.0),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        ids.push(id);
    }
    (game, ids[0], ids[1])
}

fn assert_ranged(game: &Game, source: ActorId, target: ActorId, available: bool) {
    let before = game.clone();
    for ability in [Ability::MagicBolt, Ability::Fear] {
        assert_eq!(
            game.ability_target_available(source, ability, target),
            available,
            "{ability:?} targeting {target:?}"
        );
    }
    assert_eq!(*game, before);
}

fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(snapshot, &shared).unwrap()
}

#[test]
fn targeting_reconstructs_after_checkpoints_and_portal_occlusion_changes() {
    let (mut game, source, target) = fixture();
    game.teleport(source, at(1, 18, 1, 0)).unwrap();
    game.teleport(target, at(2, 0, 1, 0)).unwrap();
    game.connect(
        Passage {
            from: at(1, 19, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 1, 0),
        },
        1,
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
    game = round_trip(&game);
    assert_ranged(&game, source, target, true);
    game.set_wall(at(1, 19, 1, 0), true).unwrap();
    assert_ranged(&game, source, target, false);
    game = round_trip(&game);
    assert_ranged(&game, source, target, false);
    game.set_wall(at(1, 19, 1, 0), false).unwrap();
    assert_ranged(&game, source, target, true);
}

#[test]
fn ranged_targeting_uses_six_manhattan_cells_and_current_occlusion() {
    let (mut game, source, target) = fixture();
    for (location, available) in [
        (at(1, 7, 1, 0), true),
        (at(1, 8, 1, 0), false),
        (at(1, 4, 4, 0), true),
        (at(1, 4, 5, 0), false),
        (at(1, 1, 1, 3), true),
    ] {
        game.teleport(target, location).unwrap();
        assert_ranged(&game, source, target, available);
    }
    game.teleport(target, at(1, 7, 1, 0)).unwrap();
    for y in 0..12 {
        for z in 0..4 {
            game.set_wall(at(1, 4, y, z), true).unwrap();
        }
    }
    assert_ranged(&game, source, target, false);
    game.set_wall(at(1, 4, 1, 0), false).unwrap();
    assert_ranged(&game, source, target, true);
}

#[test]
fn ranged_targeting_measures_from_the_eye_to_visible_occupied_cells() {
    let (mut game, source, target) = fixture();
    game.set_body(
        source,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.teleport(target, at(1, 7, 1, 1)).unwrap();
    assert_ranged(&game, source, target, true);
    game.teleport(target, at(1, 7, 1, 0)).unwrap();
    assert_ranged(&game, source, target, false);
    game.set_body(
        target,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
}

#[test]
fn targeting_crosses_rotated_physical_portals_but_not_abstract_stair_disclosures() {
    let (mut game, source, target) = fixture();
    game.teleport(source, at(1, 18, 1, 0)).unwrap();
    game.teleport(target, at(2, 0, 1, 0)).unwrap();
    assert_ranged(&game, source, target, false);
    game.connect(
        Passage {
            from: at(1, 19, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 1, 0),
        },
        1,
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
    assert!(!game.ability_target_available(source, Ability::PowerStrike, target));

    let (mut game, source, target) = fixture();
    let landing = at(2, 1, 1, 0);
    game.teleport(target, landing).unwrap();
    game.register_named_anchors(
        RegionId(2),
        BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 1, 1, 0),
        Direction::Up,
        NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    assert!(game
        .scene(source)
        .unwrap()
        .iter()
        .any(|cell| cell.location == landing));
    assert_ranged(&game, source, target, false);
}

#[test]
fn targeting_rejects_self_unknown_dead_frozen_and_ungranted_actors() {
    let (mut game, source, target) = fixture();
    assert_ranged(&game, source, target, true);
    assert_ranged(&game, source, source, false);
    assert_ranged(&game, source, ActorId(999), false);
    assert_ranged(&game, ActorId(999), target, false);
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert_ranged(&game, source, target, false);
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert_ranged(&game, source, target, true);
    let mut build = game.creature(source).unwrap().build().clone();
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(source, build).unwrap();
    assert_ranged(&game, source, target, false);
    game.teleport(target, at(1, 2, 1, 0)).unwrap();
    assert!(game.ability_target_available(source, Ability::BasicMelee, target));
    assert!(!game.ability_target_available(source, Ability::PowerStrike, target));
    game.apply_effects(
        target,
        &[tor_simulation::EffectSpec::Damage {
            components: BTreeMap::from([(tor_simulation::combat::DamageType::Vital, 1_000_000)]),
        }],
    )
    .unwrap();
    assert!(!game.ability_target_available(source, Ability::BasicMelee, target));
}
