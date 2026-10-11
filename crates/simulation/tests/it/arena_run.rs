use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::arena::{ArenaLimits, ArenaStop};
use tor_simulation::{
    ai::AiProfile, checkpoint::SharedState, creatures::Template, grants::Grant, Action, ActorId,
    CreatureIdentity, EffectSpec, Game,
};
use tor_world::{Location, Position, RegionId};

fn bounded(ticks: u64, actions: u64) -> Game {
    let mut game = unconfigured();
    game.configure_arena_run_with_limits(
        ActorId(1),
        BTreeSet::from([ActorId(1), ActorId(2), ActorId(3)]),
        BTreeMap::from([
            ("left".into(), BTreeSet::from(["right".into()])),
            ("right".into(), BTreeSet::from(["left".into()])),
        ]),
        ArenaLimits { ticks, actions },
    )
    .unwrap();
    game.refresh_navigation();
    game
}

fn round_trip(game: &Game) -> Game {
    let mut shared = SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    Game::restore_checkpoint(snapshot, &shared).expect("valid arena checkpoint")
}

#[test]
fn action_limit_counts_execution_and_stops_before_advancing_time() {
    let mut game = bounded(1000, 1);
    game.admit_ai_intention(ActorId(1)).unwrap();
    game.admit_ai_intention(ActorId(2)).unwrap();
    assert_eq!(game.arena_run().unwrap().actions, 0);
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.arena_run().unwrap().actions, 1);
    assert_eq!(game.arena_run().unwrap().stop, Some(ArenaStop::ActionLimit));
    assert!(game.run_outcome().terminal);
    assert_eq!(game.queued_intentions().count(), 0);
    assert_eq!(round_trip(&game), game);
    let before = game.clone();
    assert!(game.admit_ai_intention(ActorId(2)).is_err());
    assert!(game.act(ActorId(2), Action::Wait).is_err());
    assert_eq!(game, before);
    kill_selected(&mut game);
    assert!(game.run_outcome().terminal);
    assert_eq!(game.arena_run().unwrap().stop, Some(ArenaStop::ActionLimit));
    assert_eq!(round_trip(&game), game);
}

#[test]
fn tick_limit_wakes_between_actor_decisions_without_overshoot() {
    let mut game = bounded(7, 100);
    for _ in 0..10 {
        if game.run_outcome().terminal {
            break;
        }
        let (actor, _) = game.next_ai_action().expect("active arena AI");
        game.admit_ai_intention(actor).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
    }
    assert_eq!(game.tick(), 7);
    assert_eq!(game.arena_run().unwrap().stop, Some(ArenaStop::TickLimit));
    assert!(game.arena_run().unwrap().actions < 100);
    assert_eq!(game.queued_intentions().count(), 0);
    assert_eq!(round_trip(&game), game);
}

#[test]
fn elimination_records_surviving_team_without_waiting_for_limits() {
    let mut game = bounded(1000, 100);
    game.apply_effects(
        ActorId(3),
        &[EffectSpec::Damage {
            components: BTreeMap::from([(tor_simulation::combat::DamageType::Vital, 1_000_000)]),
        }],
    )
    .unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.arena_run().unwrap().actions, 0);
    assert_eq!(
        game.arena_run().unwrap().stop,
        Some(ArenaStop::Elimination {
            winner: Some("left".into()),
        })
    );
    assert!(game.run_outcome().terminal);
    assert_eq!(round_trip(&game), game);
}

#[test]
fn single_team_inspection_stays_active_until_every_participant_dies() {
    let mut game = unconfigured();
    game.configure_arena_run(
        ActorId(1),
        BTreeSet::from([ActorId(1), ActorId(2)]),
        BTreeMap::new(),
    )
    .unwrap();
    assert!(!game.run_outcome().terminal);
    kill_selected(&mut game);
    assert!(!game.run_outcome().terminal);
    game.apply_effects(
        ActorId(2),
        &[EffectSpec::Damage {
            components: BTreeMap::from([(tor_simulation::combat::DamageType::Vital, 1_000_000)]),
        }],
    )
    .unwrap();
    assert_eq!(
        game.arena_run().unwrap().stop,
        Some(ArenaStop::Elimination { winner: None })
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn invalid_limits_reject_setup_atomically() {
    let mut game = unconfigured();
    let before = game.clone();
    for limits in [
        ArenaLimits {
            ticks: 0,
            actions: 1,
        },
        ArenaLimits {
            ticks: 1,
            actions: 10001,
        },
    ] {
        assert!(game
            .configure_arena_run_with_limits(
                ActorId(1),
                BTreeSet::from([ActorId(1), ActorId(2), ActorId(3)]),
                BTreeMap::new(),
                limits
            )
            .is_err());
        assert_eq!(game, before);
    }
}

#[test]
fn checkpoint_rejects_inconsistent_bounds_and_stop_evidence() {
    let game = bounded(7, 100);
    let mut shared = SharedState::default();
    let value = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    for (field, replacement) in [
        ("limits", serde_json::json!({"ticks":0,"actions":100})),
        ("actions", serde_json::json!(100)),
        ("started_at", serde_json::json!(1)),
        ("stop", serde_json::json!({"type":"tick_limit"})),
    ] {
        let mut invalid = value.clone();
        invalid["combat"]["arena"][field] = replacement;
        let snapshot = serde_json::from_value(invalid).unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared).is_none(),
            "{field}"
        );
    }
    let mut missing = value;
    missing["combat"].as_object_mut().unwrap().remove("arena");
    assert!(serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(missing).is_err());
}

#[test]
fn stopping_cancels_paid_preparation_without_refunding_its_start_charge() {
    use tor_simulation::{grants::Ability, resources::Resource};
    for (ticks, actions, expected) in [
        (1000, 1, ArenaStop::ActionLimit),
        (7, 100, ArenaStop::TickLimit),
    ] {
        let mut game = bounded(ticks, actions);
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        let mut template = Template::new("arena_health", 0);
        template.grants = vec![Grant::Health(100), Grant::Ability(Ability::Fear)];
        build.set_templates(vec![template]).unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        let balance = game
            .creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Focus);
        game.admit_ai_intention(ActorId(1)).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(
            execution.action,
            Some(Action::UseAbility {
                ability: Ability::Fear,
                target: ActorId(3),
            })
        );
        execution.outcome.unwrap();
        for _ in 0..10 {
            if game.run_outcome().terminal {
                break;
            }
            let (actor, _) = game.next_ai_action().unwrap();
            game.admit_ai_intention(actor).unwrap();
            game.execute_next_intention().unwrap().outcome.unwrap();
        }
        assert_eq!(game.arena_run().unwrap().stop, Some(expected));
        assert!(game.preparation(ActorId(1)).is_none());
        let costs = game.creature(ActorId(1)).unwrap().costs();
        assert_eq!(costs.resources().balance(Resource::Focus), balance - 1);
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.available(Resource::Focus), balance - 1);
        assert_eq!(round_trip(&game), game);
    }
}

fn unconfigured() -> Game {
    let mut game = Game::two_room_in_stone(42);
    for (x, faction) in [(1, "left"), (2, "left"), (3, "right")] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut build = super::creature_state::build();
        let mut health = Template::new("arena_health", 0);
        health.grants = vec![Grant::Health(100)];
        build.set_templates(vec![health]).unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("combatant {x}"),
                faction: faction.into(),
            },
            build,
        )
        .unwrap();
        game.configure_ai(
            actor,
            AiProfile {
                flee_percent: 0,
                ..Default::default()
            },
        )
        .unwrap();
    }
    game
}

fn fixture(arena: bool) -> Game {
    let mut game = unconfigured();
    let participants = BTreeSet::from([ActorId(1), ActorId(2), ActorId(3)]);
    let hostility = BTreeMap::from([
        ("left".into(), BTreeSet::from(["right".into()])),
        ("right".into(), BTreeSet::from(["left".into()])),
    ]);
    if arena {
        game.configure_arena_run(ActorId(1), participants, hostility)
            .unwrap();
    } else {
        game.configure_run(ActorId(1), participants, None, hostility)
            .unwrap();
    }
    game.refresh_navigation();
    game
}

fn kill_selected(game: &mut Game) {
    game.apply_effects(
        ActorId(1),
        &[EffectSpec::Damage {
            components: BTreeMap::from([(tor_simulation::combat::DamageType::Vital, 1_000_000)]),
        }],
    )
    .unwrap();
}

#[test]
fn arena_ai_continues_after_selected_death_and_checkpoint_restore() {
    let mut game = fixture(true);
    kill_selected(&mut game);
    assert!(!game.alive(ActorId(1)));
    assert_eq!(game.run_outcome().deceased, Some(ActorId(1)));
    assert!(!game.run_outcome().terminal);
    let mut shared = SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(restored, game);
    for _ in 0..4 {
        let (actor, _) = restored
            .next_ai_action()
            .expect("surviving arena AI decision");
        assert_ne!(actor, ActorId(1));
        restored.admit_ai_intention(actor).unwrap();
        restored.execute_next_intention().unwrap().outcome.unwrap();
    }
    assert!(restored.tick() > 0);
    assert!(!restored.run_outcome().terminal);
}

#[test]
fn ordinary_run_selected_death_still_ends_the_run() {
    let mut game = fixture(false);
    kill_selected(&mut game);
    assert!(game.run_outcome().terminal);
    assert!(game.admit_ai_intention(ActorId(2)).is_err());
    let before = game.clone();
    assert!(game.act(ActorId(2), Action::Wait).is_err());
    assert_eq!(game, before);
}

#[test]
fn arena_mode_is_required_and_cannot_be_reinterpreted_as_a_live_adventure() {
    use tor_simulation::checkpoint::Snapshot;
    let mut game = fixture(true);
    kill_selected(&mut game);
    let mut shared = SharedState::default();
    let value = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    assert_eq!(value["combat"]["run_mode"], "arena");
    let mut missing = value.clone();
    missing["combat"]
        .as_object_mut()
        .unwrap()
        .remove("run_mode");
    assert!(serde_json::from_value::<Snapshot>(missing).is_err());
    let mut unknown = value.clone();
    unknown["combat"]["run_mode"] = serde_json::json!("unknown");
    assert!(serde_json::from_value::<Snapshot>(unknown).is_err());
    let mut invalid = value;
    invalid["combat"]["run_mode"] = serde_json::json!("adventure");
    assert!(Game::restore_checkpoint(serde_json::from_value(invalid).unwrap(), &shared).is_none());
}

#[test]
fn configured_runs_cannot_switch_into_or_out_of_arena_death_policy() {
    let participants = BTreeSet::from([ActorId(1), ActorId(2), ActorId(3)]);
    let mut ordinary = fixture(false);
    let before = ordinary.clone();
    assert!(ordinary
        .configure_arena_run(ActorId(1), participants.clone(), BTreeMap::new())
        .is_err());
    assert_eq!(ordinary, before);
    let mut arena = fixture(true);
    let before = arena.clone();
    assert!(arena
        .configure_run(ActorId(1), participants, None, BTreeMap::new())
        .is_err());
    assert_eq!(arena, before);
}

#[test]
fn arena_setup_rejects_dead_participants_without_changing_the_game() {
    let mut game = unconfigured();
    kill_selected(&mut game);
    let before = game.clone();
    assert!(game
        .configure_arena_run(
            ActorId(1),
            BTreeSet::from([ActorId(1), ActorId(2), ActorId(3)]),
            BTreeMap::new()
        )
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn paused_arena_preserves_admissions_and_steps_exactly_one_committed_action() {
    let mut game = bounded(1000, 100);
    game.admit_ai_intention(ActorId(1)).unwrap();
    game.control_arena(true, 0).unwrap();
    let before = game.clone();
    assert!(game.next_actor().is_none());
    assert!(game.execute_next_intention().is_none());
    assert!(game.act(ActorId(1), Action::Wait).is_err());
    assert_eq!(game, before);
    assert_eq!(round_trip(&game), game);
    game.control_arena(true, 1).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.tick(), 0);
    let run = game.arena_run().unwrap();
    assert!(run.paused);
    assert_eq!(run.advance_remaining, 0);
    assert_eq!(run.actions, 1);
    assert!(game.next_actor().is_none());
    assert_eq!(round_trip(&game), game);
    game.control_arena(false, 0).unwrap();
    for _ in 0..3 {
        let (actor, _) = game.next_ai_action().unwrap();
        game.admit_ai_intention(actor).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
    }
    assert!(game.tick() > 0);
}

#[test]
fn granting_arena_execution_advances_to_the_next_future_decision_after_restart() {
    let mut game = Game::two_room_in_stone(42);
    for (x, turn) in [(1, 100), (2, 10)] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(turn).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("idle {x}"),
                faction: "idle".into(),
            },
            super::creature_state::build(),
        )
        .unwrap();
    }
    game.configure_arena_run_with_limits(
        ActorId(1),
        BTreeSet::from([ActorId(1), ActorId(2)]),
        BTreeMap::new(),
        ArenaLimits {
            ticks: 1000,
            actions: 100,
        },
    )
    .unwrap();
    game.control_arena(true, 2).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.arena_run().unwrap().actions, 2);
    game = round_trip(&game);
    game.control_arena(true, 1).unwrap();
    assert_eq!(game.tick(), 10);
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    assert_eq!(
        game.arena_run().unwrap().actions,
        2,
        "granting permission is not an action"
    );
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.tick(), 10);
    assert_eq!(game.arena_run().unwrap().actions, 3);
    assert_eq!(game.arena_run().unwrap().advance_remaining, 0);
    game.control_arena(false, 0).unwrap();
    assert_eq!(game.tick(), 20);
    assert_eq!(game.next_actor(), Some(ActorId(2)));
}

#[test]
fn invalid_arena_controls_reject_without_mutating_game() {
    let mut game = bounded(1000, 3);
    for (paused, budget) in [(false, 1), (true, 4), (true, 10001)] {
        let before = game.clone();
        assert!(game.control_arena(paused, budget).is_err());
        assert_eq!(game, before);
    }
    let mut ordinary = fixture(false);
    let before = ordinary.clone();
    assert!(ordinary.control_arena(true, 0).is_err());
    assert_eq!(ordinary, before);
}

#[test]
fn arena_checkpoint_rejects_missing_or_inconsistent_control_permissions() {
    let game = bounded(1000, 100);
    let mut shared = SharedState::default();
    let value = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    for field in ["paused", "advance_remaining"] {
        let mut missing = value.clone();
        missing["combat"]["arena"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(missing).is_err());
    }
    for (paused, remaining) in [(false, 1), (true, 101)] {
        let mut invalid = value.clone();
        invalid["combat"]["arena"]["paused"] = serde_json::json!(paused);
        invalid["combat"]["arena"]["advance_remaining"] = serde_json::json!(remaining);
        assert!(
            Game::restore_checkpoint(serde_json::from_value(invalid).unwrap(), &shared).is_none()
        );
    }
}

#[test]
fn encounter_stop_discards_unused_step_permission() {
    let mut game = bounded(7, 100);
    game.control_arena(true, 5).unwrap();
    for _ in 0..5 {
        if game.run_outcome().terminal {
            break;
        }
        let (actor, _) = game.next_ai_action().unwrap();
        game.admit_ai_intention(actor).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
    }
    let run = game.arena_run().unwrap();
    assert_eq!(game.tick(), 7);
    assert_eq!(run.stop, Some(ArenaStop::TickLimit));
    assert!(run.actions < 5);
    assert_eq!(run.advance_remaining, 0);
    assert_eq!(round_trip(&game), game);
    let before = game.clone();
    assert!(game.control_arena(false, 0).is_err());
    assert_eq!(game, before);
}
