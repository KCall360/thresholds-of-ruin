use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    attacks::MeleeAttack,
    attributes::{Attributes, ManaBinding, Skill},
    combat::{AttackOutcome, DamageType, DisclosedCombatEvent, Injury, Objective},
    creatures::{CreatureBuild, Species},
    damage::{DamageComponent, DamageSpec},
    grants::{Grant, Selector},
    progression::{CreatureType, HdLedger, HdSource},
    Action, ActorId, CreatureIdentity, Game,
};
use tor_world::{Direction, Location, Position, RegionId};

fn at(x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z },
    }
}
fn subject(bonus: i32, wind_up: u64, kind: DamageType, amount: u32) -> Species {
    let component = DamageComponent::fixed(kind, None, amount);
    let primary = component.key();
    Species {
        id: "combat_subject".into(),
        kind: CreatureType::Humanoid,
        subtypes: BTreeSet::new(),
        default_attributes: Attributes::new([0; 6]).unwrap(),
        anatomy: tor_simulation::AnatomySpec::humanoid(),
        melee: MeleeAttack::new(
            Skill::HeavyWeaponry,
            bonus,
            wind_up,
            40,
            DamageSpec::new(vec![component], Some(primary)).unwrap(),
        )
        .unwrap(),
        grants: vec![Grant::Health(22)],
    }
}

fn configure_subject(game: &mut Game, actor: ActorId, species: Species, faction: &str) {
    let build = CreatureBuild::new(
        species,
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "figure".into(),
            faction: faction.into(),
        },
        build,
    )
    .unwrap();
}

fn duel() -> Game {
    duel_with_subjects(
        [
            subject(100, 60, DamageType::Impact, 4),
            subject(100, 60, DamageType::Impact, 4),
        ],
        ["a", "b"],
    )
}

fn duel_with_subjects(subjects: [Species; 2], factions: [&str; 2]) -> Game {
    let mut game = Game::two_room_in_stone(42);
    for ((x, faction), species) in [1, 2].into_iter().zip(factions).zip(subjects) {
        let id = game
            .spawn_actor(at(x, 1, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        configure_subject(&mut game, id, species, faction);
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
    let mut hero = subject(100, 60, DamageType::Impact, 4);
    hero.grants
        .push(Grant::Immunity(Selector::Category(DamageType::Impact)));
    let mut game = duel_with_subjects(
        [hero, subject(100, 30, DamageType::Impact, 4)],
        ["neutral", "neutral"],
    );
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
    for actor in [ActorId(1), ActorId(2)] {
        let creature = game
            .creature(actor)
            .expect("combat subjects own their builds");
        assert_eq!(creature.build().ledger().entries().len(), 1);
        assert_eq!(creature.health().maximum(), 30);
    }
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    assert_eq!(
        game.pause_preparation(ActorId(1)),
        Some(tor_simulation::Work::Attack { target: ActorId(2) })
    );
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
fn queued_autonomous_execution_matches_recorded_action_validation_and_restoration() {
    let mut game = duel();
    game.configure_ai(ActorId(2), Default::default()).unwrap();
    let unchanged = game.clone();
    assert!(game.admit_ai_intention(ActorId(1)).is_err());
    assert_eq!(game, unchanged);
    let early = game.admit_ai_intention(ActorId(2)).unwrap();
    let waiting = game.clone();
    assert!(game.execute_next_intention().is_none());
    assert_eq!(game, waiting);
    game.cancel_intention(ActorId(2), early).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    let mut replay = game.clone();
    let (_, expected) = replay.next_ai_action().unwrap();
    replay
        .admit_intention(
            ActorId(2),
            expected,
            tor_simulation::IntentionOrigin::Autonomous,
        )
        .unwrap();
    let expected_outcome = replay.execute_next_intention().unwrap().outcome.unwrap();
    let before = tor_simulation::diagnostics::work_counts().route_searches;
    game.admit_ai_intention(ActorId(2)).unwrap();
    let execution = game.execute_next_intention().unwrap();
    let actual = execution.action.unwrap();
    let outcome = execution.outcome.unwrap();
    assert_eq!(
        tor_simulation::diagnostics::work_counts().route_searches - before,
        1
    );
    assert_eq!(actual, expected);
    assert_eq!(outcome, expected_outcome);
    assert_eq!(game, replay);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
    assert_eq!(game, restored);
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
    let mut game = duel_with_subjects(
        [
            subject(100, 60, DamageType::Impact, 4),
            subject(100, 60, DamageType::Vital, 100),
        ],
        ["a", "neutral"],
    );
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
fn combat_views_carry_events_and_injury_as_data() {
    let mut game = duel_with_subjects(
        [
            subject(100, 60, DamageType::Impact, 4),
            subject(100, 30, DamageType::Impact, 4),
        ],
        ["a", "neutral"],
    );
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    game.act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    // The enemy's quicker blow lands first and interrupts the wind-up.
    let view = game.observe(ActorId(1)).unwrap().combat.unwrap();
    assert_eq!(
        view.events,
        [
            DisclosedCombatEvent::Attack {
                attacker: Some(ActorId(2)),
                target: Some(ActorId(1)),
                outcome: AttackOutcome::Hit,
            },
            DisclosedCombatEvent::Interrupted { actor: ActorId(1) },
        ]
    );
    assert_eq!(view.actors, [(ActorId(2), false, Injury::Healthy)]);
}

#[test]
fn a_resisted_blow_is_no_injury_and_a_landed_one_wounds() {
    let mut hero = subject(100, 60, DamageType::Impact, 4);
    hero.grants
        .push(Grant::Immunity(Selector::Category(DamageType::Impact)));
    let mut game = duel_with_subjects(
        [hero, subject(100, 30, DamageType::Impact, 4)],
        ["neutral", "neutral"],
    );
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    game.act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    let view = game.observe(ActorId(1)).unwrap().combat.unwrap();
    assert!(view.events.contains(&DisclosedCombatEvent::Attack {
        attacker: Some(ActorId(2)),
        target: Some(ActorId(1)),
        outcome: AttackOutcome::NoInjury,
    }));
    assert!(view.events.contains(&DisclosedCombatEvent::Attack {
        attacker: Some(ActorId(1)),
        target: Some(ActorId(2)),
        outcome: AttackOutcome::Hit,
    }));
    assert_eq!(view.actors, [(ActorId(2), false, Injury::Wounded)]);
    // The next action's view carries only that action's events.
    game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    let view = game.observe(ActorId(1)).unwrap().combat.unwrap();
    assert!(!view.events.iter().any(|e| matches!(
        e,
        DisclosedCombatEvent::Attack {
            outcome: AttackOutcome::NoInjury,
            ..
        }
    )));
}

#[test]
fn a_death_is_an_event_naming_the_dead() {
    let mut game = duel_with_subjects(
        [
            subject(100, 60, DamageType::Impact, 4),
            subject(100, 60, DamageType::Vital, 100),
        ],
        ["a", "neutral"],
    );
    game.act(ActorId(1), Action::Wait).unwrap();
    game.act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    let view = game.observe(ActorId(1)).unwrap().combat.unwrap();
    assert_eq!(
        view.events,
        [
            DisclosedCombatEvent::Attack {
                attacker: Some(ActorId(2)),
                target: Some(ActorId(1)),
                outcome: AttackOutcome::Hit,
            },
            DisclosedCombatEvent::Died { actor: ActorId(1) },
        ]
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
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    for id in [attacker, target] {
        configure_subject(
            &mut game,
            id,
            subject(100, 60, DamageType::Impact, 4),
            "neutral",
        );
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
    configure_subject(
        &mut game,
        third,
        subject(2, 60, DamageType::Impact, 4),
        "neutral",
    );
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
