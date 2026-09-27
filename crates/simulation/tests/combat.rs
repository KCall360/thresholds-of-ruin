use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    combat::{CombatSpec, DamageType, Objective},
    Action, ActorId, Game,
};
use tor_world::{Direction, Location, Position, RegionId};

fn at(x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z },
    }
}
fn duel() -> Game {
    let mut game = Game::two_room_in_stone(42);
    for (x, faction) in [(1, "a"), (2, "b")] {
        let id = game
            .spawn_actor(at(x, 1, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut spec = CombatSpec {
            faction: faction.into(),
            ..Default::default()
        };
        spec.attack.bonus = 100;
        game.configure_combat(id, spec).unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([
            ("a".into(), BTreeSet::from(["b".into()])),
            ("b".into(), BTreeSet::from(["a".into()])),
        ]),
    )
    .unwrap();
    game.refresh_navigation();
    game
}

#[test]
fn diagonal_reach_is_conservative_at_blocked_corners() {
    let mut game = duel();
    game.teleport(ActorId(2), at(2, 2, 0)).unwrap();
    assert!(game.attack_available(ActorId(1), ActorId(2)));
    game.set_wall(at(2, 1, 0), true).unwrap();
    assert!(!game.attack_available(ActorId(1), ActorId(2)));
}

#[test]
fn target_departure_discards_preparation_without_recovery() {
    let mut game = duel();
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    game.act(ActorId(2), Action::Move(Direction::East)).unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.next_actor(), Some(ActorId(1)));
    assert_eq!(game.tick(), 0);
}

#[test]
fn immunity_does_not_interrupt_a_later_attack() {
    let mut game = duel();
    let mut hero = CombatSpec::default();
    hero.attack.bonus = 100;
    hero.immunities.insert(DamageType::Impact);
    game.configure_combat(ActorId(1), hero).unwrap();
    let mut enemy = CombatSpec::default();
    enemy.attack.bonus = 100;
    enemy.attack.wind_up = 30;
    game.configure_combat(ActorId(2), enemy).unwrap();
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    game.act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    assert_eq!(game.health(ActorId(1)), Some((30, 30)));
    assert_eq!(game.health(ActorId(2)), Some((26, 30)));
    assert_eq!(game.tick(), 70);
}

#[test]
fn paused_progress_checkpoint_and_rng_have_identical_continuation() {
    let mut game = duel();
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    assert_eq!(game.pause_preparation(ActorId(1)), Some(ActorId(2)));
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
    for game in [&mut game, &mut restored] {
        game.act(ActorId(1), Action::Attack { target: ActorId(2) })
            .unwrap();
        game.act(ActorId(2), Action::Wait).unwrap();
    }
    assert_eq!(game, restored);
}

#[test]
fn ai_chooses_only_a_perceived_hostile() {
    let mut game = duel();
    game.configure_ai(ActorId(2), Default::default()).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((ActorId(2), Action::Attack { target: ActorId(1) }))
    );
}

#[test]
fn any_starting_character_can_win_but_mobs_cannot() {
    let mut game = duel();
    let objective = Objective {
        anchor: at(2, 1, 0),
        item: None,
        disclosed: false,
        continue_play: true,
    };
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        Some(objective.clone()),
        BTreeMap::new(),
    )
    .unwrap();
    assert!(game.run_outcome().victor.is_none());
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1), ActorId(2)]),
        Some(objective),
        BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(game.run_outcome().victor, Some(ActorId(2)));
    assert!(!game.run_outcome().terminal);
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .combat
        .unwrap()
        .objective
        .is_none());
}

#[test]
fn human_death_has_no_next_actor_and_keeps_a_corpse() {
    let mut game = duel();
    let mut enemy = CombatSpec::default();
    enemy.attack.bonus = 100;
    enemy.attack.damage = BTreeMap::from([(DamageType::Vital, 100)]);
    game.configure_combat(ActorId(2), enemy).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    let outcome = game
        .act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    assert!(outcome.next_actor.is_none());
    assert!(game.run_outcome().terminal);
    assert_eq!(game.health(ActorId(1)), Some((0, 30)));
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .ground_items
        .iter()
        .any(|i| i.name.ends_with("corpse")));
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    assert_eq!(
        Game::restore_checkpoint(game.checkpoint(&mut shared), &shared),
        Some(game)
    );
}

#[test]
fn wizard_relocation_discards_windup_without_charging_recovery() {
    let mut game = duel();
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    assert!(game.preparation(ActorId(1)).is_some());
    game.teleport(ActorId(1), at(1, 2, 0)).unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.next_actor(), Some(ActorId(1)));
    assert_eq!(game.tick(), 0);
}

#[test]
fn rotated_vertical_portal_reaches_a_targets_occupied_head_cell() {
    use tor_world::{rotate_vector, Extent, Passage, Region, World};
    let mut game = Game::new(World::new(vec![], vec![]).unwrap(), 42);
    for id in [1, 2] {
        game.add_region(Region {
            id: RegionId(id),
            name: "shaft".into(),
            bounds: Extent::new(8, 4, 16).unwrap(),
        })
        .unwrap();
    }
    let from = at(7, 1, 10);
    let head = Location {
        region: RegionId(2),
        position: Position { x: 2, y: 1, z: 15 },
    };
    let rotation = (0..24)
        .find(|r| {
            rotate_vector(*r, [1, 0, 0]) == [0, 0, -1] && rotate_vector(*r, [0, 0, 1]) == [1, 0, 0]
        })
        .unwrap();
    game.connect_portal_area(
        Passage {
            from,
            direction: Direction::East,
            to: head,
        },
        rotation,
        1,
        1,
    )
    .unwrap();
    let attacker = game
        .spawn_actor(from, NonZeroU64::new(100).unwrap())
        .unwrap();
    let target = game
        .spawn_actor(
            Location {
                position: Position {
                    z: 14,
                    ..head.position
                },
                ..head
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.set_body(
        target,
        tor_simulation::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            mass: 80,
        },
    )
    .unwrap();
    for id in [attacker, target] {
        let mut spec = CombatSpec::default();
        spec.attack.bonus = 100;
        game.configure_combat(id, spec).unwrap();
    }
    assert!(game.attack_available(attacker, target));
    game.act(attacker, Action::Attack { target }).unwrap();
    game.act(target, Action::Wait).unwrap();
    assert_eq!(game.health(target), Some((26, 30)));
}

#[test]
fn same_tick_attack_boundary_has_bounded_perception_work() {
    let mut game = duel();
    let third = game
        .spawn_actor(at(3, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_combat(third, CombatSpec::default()).unwrap();
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    let before = tor_simulation::diagnostics::work_counts();
    game.act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    let after = tor_simulation::diagnostics::work_counts();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.next_actor(), Some(third));
    // One new attack validation and one validation per pending attack. No
    // additional perception pass is needed when the scheduler stays at this tick.
    assert!(after.scenes - before.scenes <= 3);
    assert_eq!(game.health(ActorId(1)), Some((30, 30)));
    assert_eq!(game.health(ActorId(2)), Some((30, 30)));
}
