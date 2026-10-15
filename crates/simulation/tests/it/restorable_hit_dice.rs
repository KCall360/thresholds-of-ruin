//! Latest-HD effects use actual ordinal ownership and the ordinary rebuild.
use super::creature_fixture as fixture;
use tor_simulation::attributes::Skill;
use tor_simulation::creatures::{
    record::CreatureRecord, BuildAdvancement, CreatureBuild, PlannedAdvancement, RestorationPolicy,
};
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::{ActorId, CreatureIdentity, Game};

fn authored(hd: usize) -> CreatureBuild {
    let original = fixture::build(fixture::species());
    let mut build = CreatureBuild::new(
        original.content().clone(),
        original.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial; hd], 42).unwrap(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    if hd > 1 {
        build.train(hd - 1, Skill::Athletics).unwrap();
    }
    let restoration = RestorationPolicy::Authored(
        build
            .ledger()
            .entries()
            .iter()
            .zip(build.choices())
            .map(|(die, choices)| PlannedAdvancement {
                source: die.source(),
                choices: choices.clone(),
            })
            .collect(),
    );
    CreatureBuild::from_recorded(
        build.content().clone(),
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        BuildAdvancement {
            restoration,
            ..build.advancement().clone()
        },
        vec![],
    )
    .unwrap()
}

fn game(build: CreatureBuild) -> Game {
    let mut game = Game::two_room_in_stone(42, build.content().clone());
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "drained subject".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    game
}

fn effect(kind: &str) -> tor_simulation::rules::Effect {
    serde_json::from_value(serde_json::json!({ "kind": kind })).unwrap()
}

#[test]
fn ordinary_latest_hd_drain_and_restoration_keep_exact_seeded_ordinal_and_owned_choices() {
    let build = authored(3);
    let original = build.clone();
    let mut game = game(build);
    assert!(game
        .apply_effects(ActorId(1), &[effect("drain_latest_hit_die")])
        .unwrap());
    let drained = game.creature(ActorId(1)).unwrap();
    assert_eq!(drained.build().ledger().total_hd(), 2);
    assert_eq!(
        drained.build().ledger().entries(),
        &original.ledger().entries()[..2]
    );
    assert_eq!(drained.build().choices(), &original.choices()[..2]);
    assert!(game
        .apply_effects(ActorId(1), &[effect("restore_hit_die")])
        .unwrap());
    let restored = game.creature(ActorId(1)).unwrap();
    assert_eq!(restored.build(), &original);
    assert_eq!(restored.earned_progress().earned_xp(), 0);
    assert_eq!(restored.earned_progress().spent_xp(), 0);
    assert!(!game
        .apply_effects(ActorId(1), &[effect("restore_hit_die")])
        .unwrap());
}

#[test]
fn hit_die_restoration_after_checkpoint_uses_applied_watermark_and_original_root() {
    let mut game = game(authored(3));
    game.apply_effects(ActorId(1), &[effect("drain_latest_hit_die")])
        .unwrap();
    let before = CreatureRecord::capture(&game.creature(ActorId(1)).unwrap());
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(checkpoint, shared)).unwrap();
    let (checkpoint, shared) = serde_json::from_slice(&bytes).unwrap();
    let mut reopened =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(
        CreatureRecord::capture(&reopened.creature(ActorId(1)).unwrap()),
        before
    );
    for state in [&mut game, &mut reopened] {
        state
            .apply_effects(ActorId(1), &[effect("restore_hit_die")])
            .unwrap();
        assert_eq!(state.creature(ActorId(1)).unwrap().build(), &authored(3));
    }
    assert_eq!(reopened, game);
}

#[test]
fn last_hit_die_drain_uses_sticky_death_and_cannot_restore_a_dead_actor() {
    let mut game = game(authored(1));
    assert!(game
        .apply_effects(ActorId(1), &[effect("drain_latest_hit_die")])
        .unwrap());
    let dead = game.creature(ActorId(1)).unwrap();
    assert_eq!(dead.build().ledger().total_hd(), 0);
    assert!(dead.health().dead());
    let record = CreatureRecord::capture(&dead);
    assert!(!game
        .apply_effects(ActorId(1), &[effect("restore_hit_die")])
        .unwrap());
    assert_eq!(
        CreatureRecord::capture(&game.creature(ActorId(1)).unwrap()),
        record
    );
}
