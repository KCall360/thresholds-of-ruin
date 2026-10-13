use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::arena::{ArenaLimits, ArenaStop};
use tor_simulation::{
    ai::AiProfile, checkpoint::SharedState, creatures::Template, Action, ActorId, CreatureIdentity,
    Game,
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
    Game::restore_checkpoint(snapshot, &shared, game.content().clone())
        .expect("valid arena checkpoint")
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
    assert_eq!(
        game.arena_run().unwrap().stop().cloned(),
        Some(ArenaStop::ActionLimit)
    );
    assert!(game.run_outcome().terminal);
    assert_eq!(game.queued_intentions().count(), 0);
    assert_eq!(round_trip(&game), game);
    let before = game.clone();
    assert!(game.admit_ai_intention(ActorId(2)).is_err());
    assert!(game.act(ActorId(2), Action::Wait).is_err());
    assert_eq!(game, before);
    kill_selected(&mut game);
    assert!(game.run_outcome().terminal);
    assert_eq!(
        game.arena_run().unwrap().stop().cloned(),
        Some(ArenaStop::ActionLimit)
    );
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
    assert_eq!(
        game.arena_run().unwrap().stop().cloned(),
        Some(ArenaStop::TickLimit)
    );
    assert!(game.arena_run().unwrap().actions < 100);
    assert_eq!(game.queued_intentions().count(), 0);
    assert_eq!(round_trip(&game), game);
}

#[test]
fn elimination_records_surviving_team_without_waiting_for_limits() {
    let mut game = bounded(1000, 100);
    game.apply_effects(ActorId(3), &[damage(1_000_000)])
        .unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.arena_run().unwrap().actions, 0);
    assert_eq!(
        game.arena_run().unwrap().stop().cloned(),
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
    game.apply_effects(ActorId(2), &[damage(1_000_000)])
        .unwrap();
    assert_eq!(
        game.arena_run().unwrap().stop().cloned(),
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
        assert_eq!(
            game.configure_arena_run_with_limits(
                ActorId(1),
                BTreeSet::from([ActorId(1), ActorId(2), ActorId(3)]),
                BTreeMap::new(),
                limits
            ),
            Err(tor_simulation::GameError::Run(
                tor_simulation::run_policy::RunError::InvalidLimits
            ))
        );
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
        (
            "progress",
            serde_json::json!({"kind":"stopped","reason":{"type":"tick_limit"}}),
        ),
    ] {
        let mut invalid = value.clone();
        invalid["combat"]["run"]["state"][field] = replacement;
        let snapshot = serde_json::from_value(invalid).unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
            "{field}"
        );
    }
    let mut missing = value;
    missing["combat"].as_object_mut().unwrap().remove("run");
    assert!(serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(missing).is_err());
}

#[test]
fn stopping_cancels_paid_preparation_without_refunding_its_start_charge() {
    use tor_simulation::resources::Resource;
    for (ticks, actions, expected) in [
        (1000, 1, ArenaStop::ActionLimit),
        (7, 100, ArenaStop::TickLimit),
    ] {
        let mut game = bounded(ticks, actions);
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        build
            .set_templates(vec![tor_simulation::content_ids::TemplateId::new(
                "arena_fear",
            )
            .unwrap()])
            .unwrap();
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
                id: tor_simulation::content_ids::AbilityId::new("fear").unwrap(),
                target: ActorId(3),
                supplies: Default::default(),
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
        assert_eq!(game.arena_run().unwrap().stop().cloned(), Some(expected));
        assert!(game.preparation(ActorId(1)).is_none());
        let creature = game.creature(ActorId(1)).unwrap();
        let costs = creature.costs();
        assert_eq!(costs.resources().balance(Resource::Focus), balance - 1);
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.available(Resource::Focus), balance - 1);
        assert_eq!(round_trip(&game), game);
    }
}

fn unconfigured() -> Game {
    let mut game = Game::two_room_in_stone(42, arena_build().content().clone());
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
        let build = arena_build();
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
    game.apply_effects(ActorId(1), &[damage(1_000_000)])
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
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
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
    assert_eq!(value["combat"]["run"]["kind"], "arena");
    let mut missing = value.clone();
    missing["combat"].as_object_mut().unwrap().remove("run");
    assert!(serde_json::from_value::<Snapshot>(missing).is_err());
    let mut unknown = value.clone();
    unknown["combat"]["run"]["kind"] = serde_json::json!("unknown");
    assert!(serde_json::from_value::<Snapshot>(unknown).is_err());
    let mut invalid = value;
    invalid["combat"]["run"]["kind"] = serde_json::json!("adventure");
    assert!(serde_json::from_value::<Snapshot>(invalid).is_err());
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
    assert!(run.paused());
    assert_eq!(run.advance_remaining(), 0);
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
    let mut game = Game::two_room_in_stone(42, arena_build().content().clone());
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
            arena_build(),
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
    assert_eq!(game.arena_run().unwrap().advance_remaining(), 0);
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
    let mut missing = value.clone();
    missing["combat"]["run"]["state"]
        .as_object_mut()
        .unwrap()
        .remove("progress");
    assert!(serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(missing).is_err());
    let mut invalid = value;
    invalid["combat"]["run"]["state"]["progress"] =
        serde_json::json!({"kind":"active", "control":{"kind":"paused","remaining":101}});
    assert!(Game::restore_checkpoint(
        serde_json::from_value(invalid).unwrap(),
        &shared,
        game.content().clone()
    )
    .is_err());
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
    assert_eq!(run.stop().cloned(), Some(ArenaStop::TickLimit));
    assert!(run.actions < 5);
    assert_eq!(run.advance_remaining(), 0);
    assert_eq!(round_trip(&game), game);
    let before = game.clone();
    assert!(game.control_arena(false, 0).is_err());
    assert_eq!(game, before);
}

fn arena_build() -> tor_simulation::creatures::CreatureBuild {
    use tor_simulation::{
        content_ids::{AbilityId, TemplateId},
        creatures::{Content, CreatureBuild},
        rules::{Modifier, ModifierOp, Scope, Stat},
    };
    let original = super::creature_state::build();
    let mut catalog = (*original.content().catalog()).clone();
    for name in ["arena_health", "arena_fear"] {
        let mut template = Template::new(TemplateId::new(name).unwrap(), 0);
        template.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 100,
            },
        });
        if name == "arena_fear" {
            template.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: AbilityId::new("fear").unwrap(),
                },
            });
        }
        catalog.templates.insert(template.id.clone(), template);
    }
    let content = Content::new(original.content().rules().clone(), catalog).unwrap();
    CreatureBuild::from_recorded(
        content,
        original.species_id().clone(),
        original.initial_attributes(),
        original.ledger().clone(),
        original.binding(),
        original.advancement().clone(),
        vec![TemplateId::new("arena_health").unwrap()],
    )
    .unwrap()
}
fn damage(amount: u32) -> tor_simulation::rules::Effect {
    use tor_simulation::rules::{AmountExpr, DamageComponentExpr, DamageExpr, Effect};
    Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: tor_simulation::combat::DamageType::Vital,
                descriptor: None,
                amount: AmountExpr::Fixed { amount },
            }],
        },
    }
}
