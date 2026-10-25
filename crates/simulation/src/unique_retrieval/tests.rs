use super::*;
use crate::floor_activation::*;
use crate::{Action, ItemClass, ItemLocation};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use tor_world::Position;

pub(super) fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}
pub(super) fn retrieval_game() -> Game {
    let content =
        crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
            .unwrap();
    let mut game = Game::region_corridor(42, 4, content);
    let protagonist = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(protagonist))
        .unwrap();
    game.configure_run(
        protagonist,
        BTreeSet::from([protagonist]),
        None,
        BTreeMap::new(),
    )
    .unwrap();
    game.configure_population(PopulationPlan {
        protagonist,
        carried_item_pools: BTreeMap::new(),
        profiles: BTreeMap::from([(
            "plain".into(),
            PopulationProfile {
                version: 1,
                stages: vec![],
            },
        )]),
    })
    .unwrap();
    for (region, depth) in [(1, 25), (2, 26), (3, 27)] {
        game.register_activation_scope(PreparedScope {
            rooms: vec![],
            scope: format!("dungeon/{depth}"),
            depth,
            profile: "plain".into(),
            members: if region == 2 {
                BTreeSet::from([RegionId(2), RegionId(4)])
            } else {
                BTreeSet::from([RegionId(region)])
            },
            cells: vec![at(region, 2, 1)],
            protected: BTreeSet::new(),
        })
        .unwrap();
    }
    let mut item = ItemSpec::ordinary("Amulet of Yendor".into());
    item.class = ItemClass::Amulet;
    game.configure_unique_retrieval(UniqueRetrievalPolicy {
        version: RETRIEVAL_VERSION,
        protagonist,
        family: "dungeon".into(),
        minimum_depth: 26,
        item,
        exit: TerminalExit {
            at: at(1, 1, 1),
            direction: Direction::Up,
        },
    })
    .unwrap();
    game
}
#[test]
fn actual_deep_entries_create_then_relocate_one_ordinary_identity() {
    let mut game = retrieval_game();
    let rng = game.rules_rng;
    game.activate_initial_floor().unwrap();
    assert_eq!(game.unique_retrieval_state().unwrap().item, None);
    game.begin_retrieval_transaction();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let first = game.unique_retrieval_state().unwrap().clone();
    let item = first
        .item
        .expect("first eligible entry creates the artifact");
    assert_eq!((first.entry_count, first.placement_count), (1, 1));
    assert_eq!(first.entries.len(), 1);
    assert!(
        matches!(first.entries[0].result, RetrievalResult::Created { item: id, .. } if id == item)
    );
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    assert_eq!(game.retrieval_entries().last().unwrap().scope, "dungeon/27");
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let state = game.unique_retrieval_state().unwrap();
    assert_eq!(
        (state.item, state.entry_count, state.placement_count),
        (Some(item), 3, 3)
    );
    assert_eq!(
        state.entries.len(),
        1,
        "only the latest owning transaction is retained"
    );
    assert_eq!(state.entries[0].scope, "dungeon/26");
    assert_eq!(
        game.activation_count(),
        3,
        "revisit never duplicates population receipts"
    );
    assert_eq!(game.items.len(), 1);
    assert_eq!(game.next_item_id, item.0 + 1);
    assert_eq!(game.rules_rng, rng);
}
#[test]
fn carried_deep_entry_preserves_item_and_placement_stream() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let id = game
        .unique_retrieval_state()
        .unwrap()
        .item
        .expect("created");
    game.items
        .edit(id, |item| item.location = ItemLocation::Carried(ActorId(1)))
        .unwrap();
    let before = game.items[&id].clone();
    game.begin_retrieval_transaction();
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    let state = game.unique_retrieval_state().unwrap();
    assert_eq!((state.entry_count, state.placement_count), (2, 1));
    assert_eq!(
        state.entries[0].result,
        RetrievalResult::AlreadyCarried { item: id }
    );
    assert_eq!(game.items[&id], before);
}
#[test]
fn initial_deep_entry_is_once_only_and_observation_does_nothing() {
    let mut game = retrieval_game();
    game.actors.get_mut(&ActorId(1)).unwrap().location = at(2, 0, 1);
    game.activate_initial_floor().unwrap();
    assert!(game.unique_retrieval_state().unwrap().item.is_some());
    let before = game.clone();
    game.activate_initial_floor().unwrap();
    game.observe(ActorId(1)).unwrap();
    assert_eq!(game, before);
}
#[test]
fn intentional_surface_up_requires_exact_item_and_never_wins_by_position() {
    let mut game = retrieval_game();
    let mut decoy = ItemSpec::ordinary("Amulet of Yendor".into());
    decoy.class = ItemClass::Amulet;
    decoy.identity = "decoy".into();
    decoy.archetype = "decoy".into();
    let id = game.spawn_item_stack(at(1, 1, 1), None, 1, decoy).unwrap();
    game.act(
        ActorId(1),
        Action::Take {
            item: id,
            quantity: Some(1),
        },
    )
    .unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    assert!(!game.run_outcome().terminal);
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(1), Action::Move(Direction::Up)),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before, "missing bound item is a zero-time refusal");
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let artifact = game
        .unique_retrieval_state()
        .unwrap()
        .item
        .expect("created");
    game.items
        .edit(artifact, |item| {
            item.location = ItemLocation::Carried(ActorId(1))
        })
        .unwrap();
    game.teleport(ActorId(1), at(1, 1, 1)).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    assert!(!game.run_outcome().terminal);
    let tick = game.tick();
    game.act(ActorId(1), Action::Move(Direction::Up)).unwrap();
    assert_eq!(game.run_outcome().victor, Some(ActorId(1)));
    assert!(game.run_outcome().terminal);
    assert_eq!(
        game.tick(),
        tick,
        "accepted exit has no dungeon recovery tick"
    );
}
#[test]
fn late_artifact_allocation_failure_discards_population_and_entry() {
    let mut game = retrieval_game();
    game.next_item_id = u64::MAX;
    let before = game.clone();
    assert_eq!(
        game.teleport(ActorId(1), at(2, 0, 1)),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
}

#[test]
fn same_floor_region_changes_and_other_actors_never_relocate_the_artifact() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    let location = game.items[&item].location;
    game.teleport(ActorId(1), at(4, 0, 1)).unwrap();
    assert_eq!(game.unique_retrieval_state().unwrap().entry_count, 1);
    assert!(game.retrieval_entries().is_empty());
    assert_eq!(game.items[&item].location, location);
    let other = game
        .spawn_actor(at(1, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.teleport(other, at(3, 1, 0)).unwrap();
    assert_eq!(game.unique_retrieval_state().unwrap().entry_count, 1);
    assert_eq!(game.items[&item].location, location);
}
#[test]
fn ordinary_movement_creates_at_the_accepted_entry() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(1, 11, 1)).unwrap();
    game.act(ActorId(1), Action::Move(Direction::East)).unwrap();
    let entry = &game.retrieval_entries()[0];
    assert_eq!(entry.cause, EntryCause::Movement);
    assert_eq!(entry.tick, 0);
    assert!(matches!(entry.result, RetrievalResult::Created { .. }));
}
#[test]
fn singleton_entrant_pile_and_same_cell_are_valid_placements() {
    let mut game = retrieval_game();
    let arrival = at(2, 0, 1);
    for x in 0..12 {
        for y in 0..3 {
            let at = at(2, x, y);
            if at != arrival {
                game.world.set_wall(at, true).unwrap();
            }
        }
    }
    game.spawn_item_stack(arrival, None, 1, ItemSpec::ordinary("ordinary pile".into()))
        .unwrap();
    game.teleport(ActorId(1), arrival).unwrap();
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    assert_eq!(game.items[&item].location, ItemLocation::Ground(arrival));
    game.teleport(ActorId(1), at(1, 1, 1)).unwrap();
    game.teleport(ActorId(1), arrival).unwrap();
    assert_eq!(
        game.retrieval_entries()[0].result,
        RetrievalResult::Relocated {
            item,
            from: RetrievalSource::Ground(arrival),
            at: arrival,
            placement: 2
        }
    );
}
#[test]
fn current_ground_includes_newly_opened_cells_and_blocks_unsupported_bridges() {
    let mut game = retrieval_game();
    let scope = game.activation_scope_at(RegionId(2)).unwrap().clone();
    assert!(!scope.cells.contains(&at(2, 3, 1)));
    assert!(game
        .retrieval_ground_candidates(&scope, at(2, 0, 1))
        .unwrap()
        .contains(&at(2, 3, 1)));
    for x in 0..12 {
        for y in 0..3 {
            if y != 1 || x > 4 {
                game.world.set_wall(at(2, x, y), true).unwrap();
            }
        }
    }
    game.world.set_cell_gravity(at(2, 2, 1), [1, 0, 0]).unwrap();
    let cells = game
        .retrieval_ground_candidates(&scope, at(2, 0, 1))
        .unwrap();
    assert_eq!(
        cells,
        [at(2, 0, 1), at(2, 1, 1)],
        "unsupported bridge may not connect supported banks"
    );
    game.world.set_wall(at(2, 1, 1), true).unwrap();
    assert_eq!(
        game.retrieval_ground_candidates(&scope, at(2, 0, 1))
            .unwrap(),
        [at(2, 0, 1)]
    );
    game.world.set_wall(at(2, 1, 1), false).unwrap();
    assert_eq!(
        game.retrieval_ground_candidates(&scope, at(2, 0, 1))
            .unwrap(),
        cells
    );
}
#[test]
fn counter_or_batch_exhaustion_discards_the_whole_entry() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    for which in 0..3 {
        let mut candidate = game.clone();
        let state = candidate.unique_retrieval.as_mut().unwrap();
        match which {
            0 => state.entry_count = u64::MAX,
            1 => state.placement_count = u64::MAX,
            _ => state.entries = vec![state.entries[0].clone(); MAX_RETRIEVAL_ENTRIES],
        }
        let before = candidate.clone();
        // This is a nested entry in one owning transaction, so batch evidence
        // must remain present rather than being cleared by top-level teleport.
        let result = candidate.retrieval_entry(ActorId(1), at(3, 0, 1), EntryCause::Movement);
        assert_eq!(result, Err(GameError::IdentityExhausted));
        assert_eq!(candidate, before);
    }
}
#[test]
fn empty_supported_candidate_set_discards_population_and_movement() {
    let mut game = retrieval_game();
    game.world.set_cell_gravity(at(2, 0, 1), [1, 0, 0]).unwrap();
    let before = game.clone();
    assert_eq!(
        game.teleport(ActorId(1), at(2, 0, 1)),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, before);
}
#[test]
fn physical_entry_relocates_before_finishing_the_original_tick_cohort() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let artifact = game.unique_retrieval_state().unwrap().item.unwrap();
    game.teleport(ActorId(1), at(1, 8, 1)).unwrap();
    let driver = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mover = game
        .spawn_actor(at(1, 2, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 100;
    game.actors.get_mut(&mover).unwrap().ready_at = 100;
    game.set_actor_velocity(ActorId(1), [crate::physics::TERMINAL, 0, 0])
        .unwrap();
    game.set_actor_velocity(mover, [crate::physics::TERMINAL, 0, 0])
        .unwrap();
    let loose = game
        .spawn_item_stack(at(1, 2, 0), None, 1, ItemSpec::ordinary("loose".into()))
        .unwrap();
    game.items
        .edit(loose, |item| {
            item.motion.velocity = [crate::physics::TERMINAL, 0, 0]
        })
        .unwrap();
    assert!(matches!(
        game.act_for_host(driver, Action::Wait).unwrap(),
        crate::CandidateStep::AwaitingRegions(_)
    ));
    let entry = &game.retrieval_entries()[0];
    assert_eq!((entry.cause, entry.tick), (EntryCause::Physics, 32));
    assert!(matches!(entry.result, RetrievalResult::Relocated { item, .. } if item == artifact));
    assert_eq!(
        game.retrieval_placements_at(32).collect::<Vec<_>>(),
        [artifact]
    );
    assert_eq!(game.actors[&mover].location, at(1, 6, 2));
    assert_eq!(
        game.items[&loose].location,
        ItemLocation::Ground(at(1, 6, 0))
    );
}
#[test]
fn terminal_exit_is_disclosed_without_a_fake_landing_and_other_actors_cannot_use_it() {
    let mut game = retrieval_game();
    let view = game.observe(ActorId(1)).unwrap();
    assert!(view
        .exits
        .iter()
        .any(|exit| exit.location == at(1, 1, 1) && exit.direction == Direction::Up));
    assert!(game
        .world
        .physics_neighbor(at(1, 1, 1), Direction::Up)
        .is_none());
    assert!(!game.run_outcome().terminal);
    game.teleport(ActorId(1), at(1, 2, 1)).unwrap();
    let other = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(!game.is_terminal_exit(other, at(1, 1, 1), Direction::Up));
    assert!(
        !game
            .observe(other)
            .unwrap()
            .exits
            .iter()
            .any(|exit| exit.location == at(1, 1, 1) && exit.direction == Direction::Up),
        "terminal affordance belongs to the configured protagonist"
    );
    assert_eq!(
        game.validate_intention_action(other, Action::Move(Direction::Up)),
        Err(GameError::Blocked)
    );
}

#[test]
fn checkpoint_preserves_small_objective_state_and_ordinary_identity() {
    use crate::checkpoint::{RestoreContext, SharedState};
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut context = RestoreContext::new(&shared, game.content.clone());
    context.bind_spatial_roots(&[&snapshot]).unwrap();
    assert_eq!(context.restore(snapshot).unwrap(), game);
}
#[test]
fn changing_objective_states_keep_population_and_policy_pool_references_shared() {
    use crate::checkpoint::SharedState;
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    let mut shared = SharedState::default();
    let mut snapshots = vec![];
    for i in 0..128 {
        game.teleport(ActorId(1), at(if i % 2 == 0 { 2 } else { 3 }, 0, 1))
            .unwrap();
        snapshots.push(game.checkpoint(&mut shared));
    }
    let encoded = serde_json::to_value((snapshots, shared)).unwrap();
    assert_eq!(encoded[1]["floor_activations"].as_array().unwrap().len(), 1);
    assert_eq!(
        encoded[1]["retrieval_policies"]
            .as_array()
            .expect("immutable policy must be pooled")
            .len(),
        1
    );
    for snapshot in encoded[0].as_array().unwrap() {
        assert_eq!(
            snapshot["floor_activation"],
            serde_json::json!({"state":0,"wandering":null})
        );
        assert_eq!(snapshot["unique_retrieval"]["policy"], 0);
        assert_eq!(
            snapshot["unique_retrieval"]["entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
#[test]
fn checkpoint_objective_validation_is_specific_to_each_boundary() {
    use crate::checkpoint::{RestoreContext, SharedState, Snapshot};
    let mut game = retrieval_game();
    game.act(ActorId(1), Action::Wait).unwrap();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let mut shared = SharedState::default();
    let encoded = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    assert!(
        encoded["unique_retrieval"].is_object(),
        "checkpoint must carry objective state"
    );
    for field in ["tick", "next_item_id", "next_actor_id"] {
        let mut bad = encoded.clone();
        bad[field] = serde_json::json!(0);
        let mut context = RestoreContext::new(&shared, game.content.clone());
        let spatial_binding: crate::checkpoint::Snapshot =
            serde_json::from_value(encoded.clone()).unwrap();
        context.bind_spatial_roots(&[&spatial_binding]).unwrap();
        assert!(
            context
                .restore(serde_json::from_value::<Snapshot>(bad).unwrap())
                .is_err(),
            "{field}"
        );
    }
    for (field, value) in [("entry_count", 0), ("placement_count", 0), ("policy", 99)] {
        let mut bad = encoded.clone();
        bad["unique_retrieval"][field] = serde_json::json!(value);
        let mut context = RestoreContext::new(&shared, game.content.clone());
        let spatial_binding: crate::checkpoint::Snapshot =
            serde_json::from_value(encoded.clone()).unwrap();
        context.bind_spatial_roots(&[&spatial_binding]).unwrap();
        assert!(
            context
                .restore(serde_json::from_value::<Snapshot>(bad).unwrap())
                .is_err(),
            "{field}"
        );
    }
}

#[test]
fn queued_terminal_up_uses_an_endpoint_guard_and_executes_without_a_landing() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    game.items
        .edit(item, |entry| {
            entry.location = ItemLocation::Carried(ActorId(1))
        })
        .unwrap();
    game.teleport(ActorId(1), at(1, 1, 1)).unwrap();
    game.admit_intention(
        ActorId(1),
        Action::Move(Direction::Up),
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    assert!(
        game.intentions_valid(),
        "terminal queue must be valid before execution and restore"
    );
    let execution = game.execute_next_intention().unwrap();
    assert!(
        matches!(execution.outcome.unwrap().kind, crate::OutcomeKind::Escaped { item: escaped } if escaped == item)
    );
    assert_eq!(game.run_outcome().victor, Some(ActorId(1)));
}
#[test]
fn diagonal_supported_ground_cannot_cross_unsupported_side_cells() {
    let mut game = retrieval_game();
    let scope = game.activation_scope_at(RegionId(2)).unwrap().clone();
    for x in 0..12 {
        for y in 0..3 {
            if x > 1 || y > 1 {
                game.world.set_wall(at(2, x, y), true).unwrap();
            }
        }
    }
    game.world.set_cell_gravity(at(2, 1, 0), [0, 1, 0]).unwrap();
    game.world.set_cell_gravity(at(2, 0, 1), [1, 0, 0]).unwrap();
    assert_eq!(
        game.retrieval_ground_candidates(&scope, at(2, 0, 0))
            .unwrap(),
        [at(2, 0, 0)]
    );
}
#[test]
fn carried_latest_batch_cannot_forge_an_extra_placement() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    game.items
        .edit(item, |entry| {
            entry.location = ItemLocation::Carried(ActorId(1))
        })
        .unwrap();
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    assert!(game.retrieval_state_valid());
    game.unique_retrieval.as_mut().unwrap().placement_count = 2;
    assert!(
        !game.retrieval_state_valid(),
        "carried entry cannot consume a placement ordinal"
    );
}
#[test]
fn retrieval_named_records_reject_positional_sequence_encodings() {
    let mut game = retrieval_game();
    let policy = serde_json::to_value(game.unique_retrieval_policy().unwrap()).unwrap();
    let sequence = serde_json::json!([
        policy["version"],
        policy["protagonist"],
        policy["family"],
        policy["minimum_depth"],
        policy["item"],
        policy["exit"]
    ]);
    assert!(serde_json::from_value::<UniqueRetrievalPolicy>(sequence).is_err());
    let sequence = serde_json::json!([policy["exit"]["at"], policy["exit"]["direction"]]);
    assert!(serde_json::from_value::<TerminalExit>(sequence).is_err());
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let entry = serde_json::to_value(&game.retrieval_entries()[0]).unwrap();
    let sequence = serde_json::json!([
        entry["scope"],
        entry["depth"],
        entry["entrant"],
        entry["tick"],
        entry["ordinal"],
        entry["cause"],
        entry["result"]
    ]);
    assert!(serde_json::from_value::<RetrievalEntry>(sequence).is_err());
    let state = serde_json::to_value(game.unique_retrieval_state().unwrap()).unwrap();
    let sequence = serde_json::json!([
        state["policy"],
        state["initialized"],
        state["item"],
        state["entry_count"],
        state["placement_count"],
        state["entries"]
    ]);
    assert!(serde_json::from_value::<UniqueRetrievalState>(sequence).is_err());
}

fn carried_at_surface() -> (Game, ItemId) {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    game.items
        .edit(item, |entry| {
            entry.location = ItemLocation::Carried(ActorId(1))
        })
        .unwrap();
    game.teleport(ActorId(1), at(1, 1, 1)).unwrap();
    (game, item)
}

fn checkpoint_round_trip(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let encoded = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&encoded).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap()
}

#[test]
fn queued_terminal_exit_round_trips_and_rechecks_stale_endpoint_and_ownership() {
    let (mut queued, item) = carried_at_surface();
    queued
        .admit_intention(
            ActorId(1),
            Action::Move(Direction::Up),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let mut restored = checkpoint_round_trip(&queued);
    assert_eq!(restored, queued);
    assert_eq!(
        restored.execute_next_intention(),
        queued.clone().execute_next_intention()
    );
    assert!(restored.run_outcome().terminal);
    for moved_actor in [false, true] {
        let mut stale = queued.clone();
        let expected = if moved_actor {
            stale.actors.get_mut(&ActorId(1)).unwrap().location = at(1, 2, 1);
            GameError::InvalidLocation
        } else {
            stale
                .items
                .edit(item, |entry| {
                    entry.location = ItemLocation::Ground(at(1, 1, 1))
                })
                .unwrap();
            GameError::ItemUnavailable
        };
        let mut restored = checkpoint_round_trip(&stale);
        let tick = restored.tick();
        let inventory = restored.items.clone();
        assert_eq!(
            restored.execute_next_intention().unwrap().outcome,
            Err(expected)
        );
        assert_eq!(restored.tick(), tick);
        assert_eq!(restored.items, inventory);
        assert!(!restored.run_outcome().terminal);
        assert!(restored.pending_intention(ActorId(1)).is_none());
    }
}

#[test]
fn queued_terminal_context_is_required_and_rejects_forged_item_or_endpoint() {
    use crate::checkpoint::{SharedState, Snapshot};
    let (mut game, _) = carried_at_surface();
    game.admit_intention(
        ActorId(1),
        Action::Move(Direction::Up),
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    let mut shared = SharedState::default();
    let value = serde_json::to_value((game.checkpoint(&mut shared), shared)).unwrap();
    let mut missing = value.clone();
    missing[1]["intentions"]["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("terminal_exit_context");
    assert!(serde_json::from_value::<(Snapshot, SharedState)>(missing).is_err());
    for replacement in [
        serde_json::Value::Null,
        serde_json::json!({"at":at(1, 1, 1),"direction":"Up","item":99}),
        serde_json::json!({"at":at(1, 2, 1),"direction":"Up","item":1}),
    ] {
        let mut bad = value.clone();
        bad[1]["intentions"]["entries"][0]["terminal_exit_context"] = replacement;
        let (snapshot, shared) = serde_json::from_value(bad).unwrap();
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content.clone()).is_err());
    }
}

#[test]
fn private_retrieval_snapshot_rejects_positional_and_incomplete_encodings() {
    use crate::checkpoint::Snapshot;
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let original = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let state = &original["unique_retrieval"];
    let mut sequence = original.clone();
    sequence["unique_retrieval"] = serde_json::json!([
        state["policy"],
        state["initialized"],
        state["item"],
        state["entry_count"],
        state["placement_count"],
        state["entries"]
    ]);
    assert!(serde_json::from_value::<Snapshot>(sequence).is_err());
    for field in [
        "policy",
        "initialized",
        "item",
        "entry_count",
        "placement_count",
        "entries",
    ] {
        let mut missing = original.clone();
        missing["unique_retrieval"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<Snapshot>(missing).is_err(),
            "{field}"
        );
    }
    let mut unknown = original.clone();
    unknown["unique_retrieval"]["unknown"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Snapshot>(unknown).is_err());
    let encoded = serde_json::to_string(&original).unwrap();
    let duplicate = encoded.replacen("\"policy\":0", "\"policy\":0,\"policy\":0", 1);
    assert_ne!(duplicate, encoded);
    assert!(serde_json::from_str::<Snapshot>(&duplicate).is_err());
}

#[test]
fn checkpoint_retrieval_receipt_addresses_are_bound_to_their_own_floor() {
    use crate::checkpoint::Snapshot;
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let original = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    for bad_at in [at(1, 1, 1), at(99, 0, 0), at(2, 100, 1)] {
        let mut bad = original.clone();
        bad["unique_retrieval"]["entries"][0]["result"]["at"] =
            serde_json::to_value(bad_at).unwrap();
        let snapshot = serde_json::from_value::<Snapshot>(bad).unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared, game.content.clone()).is_err(),
            "{bad_at:?}"
        );
    }
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    game.items
        .edit(item, |entry| {
            entry.location = ItemLocation::Ground(at(1, 3, 1))
        })
        .unwrap();
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    assert_eq!(
        checkpoint_round_trip(&game),
        game,
        "shallow sources need not be in the entered floor"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let original = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    for bad_at in [at(99, 0, 0), at(1, 100, 1)] {
        let mut bad = original.clone();
        bad["unique_retrieval"]["entries"][0]["result"]["from"]["ground"] =
            serde_json::to_value(bad_at).unwrap();
        let snapshot = serde_json::from_value::<Snapshot>(bad).unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared, game.content.clone()).is_err(),
            "{bad_at:?}"
        );
    }
}

#[test]
fn rewind_restores_branch_state_without_reusing_abandoned_item_identity() {
    use crate::checkpoint::{RestoreContext, SharedState};
    let mut game = retrieval_game();
    game.activate_initial_floor().unwrap();
    let mut shared = SharedState::default();
    let uncreated = game.checkpoint(&mut shared);
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let first_id = game.unique_retrieval_state().unwrap().item.unwrap();
    let first_location = game.items[&first_id].location;
    let created = game.checkpoint(&mut shared);
    let created_game = game.clone();
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    context.bind_spatial_roots(&[&uncreated, &created]).unwrap();
    let mut before = context.restore(uncreated).unwrap();
    before.continue_identities(&game);
    assert_eq!(before.unique_retrieval_state().unwrap().item, None);
    before.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let next_id = before.unique_retrieval_state().unwrap().item.unwrap();
    assert!(next_id > first_id);
    assert_eq!(
        before.items[&next_id].location, first_location,
        "placement is independent of allocated identity"
    );
    assert!(!before.items.contains_key(&first_id));
    let mut after = context.restore(created).unwrap();
    after.continue_identities(&game);
    assert_eq!(
        after.unique_retrieval_state(),
        created_game.unique_retrieval_state()
    );
    assert_eq!(after.items[&first_id].location, first_location);
    assert_eq!(after.items.len(), 1);
}

#[test]
fn ordinary_take_drop_preserves_identity_and_requires_explicit_terminal_up() {
    let mut game = retrieval_game();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    let ItemLocation::Ground(location) = game.items[&item].location else {
        panic!("ground")
    };
    game.teleport(ActorId(1), location).unwrap();
    game.act(
        ActorId(1),
        Action::Take {
            item,
            quantity: None,
        },
    )
    .unwrap();
    game.teleport(ActorId(1), at(1, 1, 1)).unwrap();
    game.act(
        ActorId(1),
        Action::Drop {
            item,
            quantity: None,
        },
    )
    .unwrap();
    assert!(!game.run_outcome().terminal);
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(1), Action::Move(Direction::Up)),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
    game.act(
        ActorId(1),
        Action::Take {
            item,
            quantity: None,
        },
    )
    .unwrap();
    assert!(
        !game.run_outcome().terminal,
        "taking while standing at the exit is not an ascent"
    );
    assert_eq!(game.items.len(), 1);
    assert_eq!(game.next_item_id, item.0 + 1);
    game.act(ActorId(1), Action::Move(Direction::Up)).unwrap();
    assert!(game.run_outcome().terminal);
}

#[test]
fn internal_ascent_and_descent_do_not_require_the_artifact() {
    let mut game = retrieval_game();
    for (region, x, name) in [(1, 2, "inner"), (2, 1, "inner")] {
        game.world
            .register_named_anchors(
                RegionId(region),
                BTreeMap::from([(name.into(), at(region, x, 1).position)]),
            )
            .unwrap();
    }
    game.world
        .connect_named_stair(
            at(1, 2, 1),
            Direction::Down,
            tor_world::NamedAnchor {
                region: RegionId(2),
                name: "inner".into(),
            },
        )
        .unwrap();
    game.world
        .connect_named_stair(
            at(2, 1, 1),
            Direction::Up,
            tor_world::NamedAnchor {
                region: RegionId(1),
                name: "inner".into(),
            },
        )
        .unwrap();
    game.teleport(ActorId(1), at(1, 2, 1)).unwrap();
    let before = game.tick();
    game.act(ActorId(1), Action::Move(Direction::Down)).unwrap();
    assert_eq!(
        game.tick(),
        before + 100,
        "internal descent keeps its ordinary cost"
    );
    assert_eq!(game.actors[&ActorId(1)].location, at(2, 1, 1));
    let item = game.unique_retrieval_state().unwrap().item.unwrap();
    assert!(matches!(
        game.items[&item].location,
        ItemLocation::Ground(_)
    ));
    game.act(ActorId(1), Action::Move(Direction::Up)).unwrap();
    assert_eq!(
        game.tick(),
        before + 200,
        "internal ascent keeps its ordinary cost"
    );
    assert_eq!(game.actors[&ActorId(1)].location, at(1, 2, 1));
    assert!(!game.run_outcome().terminal);
}

#[test]
fn cancelling_preparation_with_terminal_up_uses_the_same_atomic_ownership_check() {
    for carried in [false, true] {
        let (mut game, item) = carried_at_surface();
        game.content = crate::test_creatures::content();
        crate::test_creatures::configure(
            &mut game,
            ActorId(1),
            "neutral",
            crate::test_creatures::species(),
        );
        let target = game
            .spawn_actor(at(1, 2, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            target,
            "neutral",
            crate::test_creatures::species(),
        );
        let intention = game
            .admit_intention(
                ActorId(1),
                Action::UseAbility {
                    id: game
                        .content
                        .rules()
                        .document()
                        .abilities
                        .iter()
                        .find(|ability| ability.basic_attack)
                        .unwrap()
                        .id
                        .clone(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(game.preparation(ActorId(1)).is_some());
        if !carried {
            game.items
                .edit(item, |entry| {
                    entry.location = ItemLocation::Ground(at(1, 1, 1))
                })
                .unwrap();
        }
        let before = game.clone();
        let result = game.cancel_intention_with_step(ActorId(1), intention, Some(Direction::Up));
        if carried {
            result.unwrap();
            assert_eq!(game.run_outcome().victor, Some(ActorId(1)));
            assert!(game.preparation(ActorId(1)).is_none());
            assert_eq!(game.tick(), before.tick());
            assert_eq!(game.health(target), before.health(target));
        } else {
            assert_eq!(result, Err(GameError::ItemUnavailable));
            assert_eq!(game, before);
        }
    }
}
#[test]
fn host_teleport_preserves_an_entry_from_cancel_released_preload_continuation() {
    use crate::rules::*;
    use crate::{CandidateStep, ReferencePoint, ReferenceTarget, RegionTransition};
    let mut document = crate::test_creatures::ruleset().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    ability.cost = None;
    ability.reach = Reach::Sight { range: 6 };
    ability.timing = Timing {
        source: TimingSource::Fixed {
            prepare: 10,
            recover: 20,
        },
        scaled_by: None,
    };
    ability.strikes = StrikeSource::Authored {
        strikes: [0, 20]
            .into_iter()
            .map(|offset| StrikeDef {
                cancellable: false,
                offset,
                unlock: None,
                check: None,
                on_success: vec![],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            })
            .collect(),
    };
    let ability_id = ability.id.clone();
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content.clone(),
        species_id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let template = retrieval_game();
    let regions = (1..=8)
        .map(|id| tor_world::Region {
            id: RegionId(id),
            name: format!("room{id}"),
            bounds: tor_world::Extent::new(12, 7, 1).unwrap(),
        })
        .collect();
    let passages = (1..8)
        .flat_map(|id| {
            [
                tor_world::Passage {
                    from: at(id, 11, 1),
                    direction: Direction::East,
                    to: at(id + 1, 0, 1),
                },
                tor_world::Passage {
                    from: at(id + 1, 0, 1),
                    direction: Direction::West,
                    to: at(id, 11, 1),
                },
            ]
        })
        .collect();
    let mut game = Game::new(
        tor_world::World::new(regions, passages).unwrap(),
        42,
        content,
    );
    for (location, duration) in [(at(1, 9, 1), 100), (at(1, 4, 4), 100), (at(8, 4, 5), 10)] {
        let actor = game
            .spawn_actor(location, NonZeroU64::new(duration).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
        .unwrap();
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::new(),
    )
    .unwrap();
    game.configure_population(PopulationPlan {
        protagonist: ActorId(1),
        carried_item_pools: BTreeMap::new(),
        profiles: BTreeMap::from([(
            "plain".into(),
            PopulationProfile {
                version: 1,
                stages: vec![],
            },
        )]),
    })
    .unwrap();
    for scope in template.activation_scopes() {
        game.register_activation_scope(scope.clone()).unwrap();
    }
    game.configure_unique_retrieval(template.unique_retrieval_policy().unwrap().clone())
        .unwrap();
    for y in 0..7 {
        game.world.set_wall(at(1, 8, y), true).unwrap();
    }
    game.world
        .connect(
            tor_world::Passage {
                from: at(1, 4, 0),
                direction: Direction::North,
                to: at(8, 4, 6),
            },
            0,
        )
        .unwrap();
    // Authored external actors both observe, as they do in the real host.
    for actor in [ActorId(1), ActorId(2)] {
        game.add_reference_point(ReferencePoint {
            target: ReferenceTarget::Actor(actor),
            active_radius: Some(0),
            load_radius: Some(0),
            observes: true,
        })
        .unwrap();
    }
    game.activate_initial_floor().unwrap();
    game.set_actor_velocity(ActorId(1), [8192, 0, 0]).unwrap();
    game.act(ActorId(1), Action::Wait).unwrap();
    let intention = game
        .admit_intention(
            ActorId(2),
            Action::UseAbility {
                id: ability_id,
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(3)),
                supplies: Default::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.act(ActorId(3), Action::Wait).unwrap();
    assert_eq!(game.tick(), 10);
    assert!(game.preparation(ActorId(2)).unwrap().has_prefix());
    assert_eq!(game.actors[&ActorId(1)].location.region, RegionId(1));

    #[derive(Default)]
    struct Records(BTreeMap<crate::RecordId, tor_world::Shared<crate::RegionRecord>>);
    impl crate::RecordStore for Records {
        fn put(&mut self, id: crate::RecordId, record: tor_world::Shared<crate::RegionRecord>) {
            self.0.insert(id, record);
        }
        fn get(&mut self, id: crate::RecordId) -> Option<tor_world::Shared<crate::RegionRecord>> {
            self.0.get(&id).cloned()
        }
    }
    let mut records = Records::default();
    let frontier = RegionTransition {
        active: BTreeSet::from([RegionId(1)]),
        loaded: BTreeSet::from([RegionId(1)]),
    };
    let (_, report, _) = game
        .transition_regions_counted_deferred(&frontier, &mut records)
        .unwrap();
    game.complete_region_transition(&report).unwrap();
    assert_eq!(game.tick(), 10);
    assert_eq!(
        game.region_state(RegionId(8)),
        Some(crate::RegionState::Active)
    );

    // Legal wizard wall commands hide the portal from the external caster.
    // Its paid preparation, rather than its observer, still pins the target.
    for x in 0..8 {
        game.set_wall(at(1, x, 2), true).unwrap();
        let (_, report, _) = game
            .transition_regions_counted_deferred(&frontier, &mut records)
            .unwrap();
        game.complete_region_transition(&report).unwrap();
        assert_eq!(game.tick(), 10);
        assert_eq!(
            game.region_state(RegionId(8)),
            Some(crate::RegionState::Active)
        );
        assert!(game.preparation(ActorId(2)).unwrap().has_prefix());
    }

    // A real private CancelIntention commits without a residency transition.
    game.begin_retrieval_transaction();
    game.cancel_intention_with_step(ActorId(2), intention, None)
        .unwrap();
    assert_eq!(game.tick(), 10);
    assert!(game.actors[&ActorId(2)].ready_at > 10);
    assert!(game.retrieval_entries().is_empty());

    // The next owning host command is Wizard::Teleport into floor27. Its
    // load_for_wizard stage settles explicit destination/source extras first.
    game.begin_retrieval_transaction();
    let frontier = RegionTransition {
        active: BTreeSet::from([RegionId(1), RegionId(3)]),
        loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
    };
    let (_, report, _) = game
        .transition_regions_counted_deferred(&frontier, &mut records)
        .unwrap();
    assert!(
        report.frozen.contains(&RegionId(8)),
        "the canceled remote target loses its active pin"
    );
    game.complete_region_transition(&report).unwrap();
    assert!(game.population_entry_pending());
    assert_eq!(game.retrieval_entries().len(), 1);
    assert_eq!(game.retrieval_entries()[0].cause, EntryCause::Physics);
    while game.population_entry_pending() {
        let (_, report, _) = game
            .transition_regions_counted_deferred(&frontier, &mut records)
            .unwrap();
        game.complete_region_transition(&report).unwrap();
    }
    let first = game.retrieval_entries()[0].clone();
    assert!(matches!(
        game.teleport_for_host(ActorId(1), at(3, 0, 1)).unwrap(),
        CandidateStep::AwaitingRegions(())
    ));
    assert_eq!(game.unique_retrieval_state().unwrap().entry_count, 2);
    assert_eq!(
        game.retrieval_entries().len(),
        2,
        "one host record owns both the preload physical entry and its explicit teleport"
    );
    assert_eq!(game.retrieval_entries()[0], first);
    assert_eq!(game.retrieval_entries()[1].cause, EntryCause::Teleport);
    assert_eq!(game.retrieval_entries()[1].ordinal, 2);
}

#[test]
fn host_action_preserves_entries_until_the_next_owning_transaction() {
    let mut game = retrieval_game();
    game.begin_retrieval_transaction();
    game.teleport_for_host(ActorId(1), at(2, 0, 1)).unwrap();
    game.complete_region_transition(&Default::default())
        .unwrap();
    let first = game.retrieval_entries()[0].clone();
    game.act_for_host(ActorId(1), Action::Wait).unwrap();
    assert_eq!(game.retrieval_entries(), [first]);
    assert_eq!(game.unique_retrieval_state().unwrap().entry_count, 1);

    // Direct convenience calls own their whole transaction and clear the old
    // batch, including a shallow teleport that makes no objective entry.
    game.act(ActorId(1), Action::Wait).unwrap();
    assert!(game.retrieval_entries().is_empty());
    game.teleport(ActorId(1), at(3, 0, 1)).unwrap();
    assert_eq!(game.retrieval_entries().len(), 1);
    assert_eq!(game.retrieval_entries()[0].ordinal, 2);
    game.teleport(ActorId(1), at(1, 0, 1)).unwrap();
    assert!(game.retrieval_entries().is_empty());
    assert_eq!(game.unique_retrieval_state().unwrap().entry_count, 2);
}

#[test]
fn accepted_terminal_up_stops_scheduled_attack_recovery_and_hunger_work() {
    use crate::creatures::{Content, CreatureBuild};
    use crate::nutrition::NutritionRequirement;
    use crate::rules::{Modifier, ModifierOp, Ruleset, Scope, Stat};
    let (mut game, item) = carried_at_surface();
    let mut species = crate::test_creatures::species();
    species.melee =
        crate::test_creatures::natural_melee(100, 60, crate::combat::DamageType::Impact, 4);
    species.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::HealthRecoveryPeriod,
            value: 60,
        },
    });
    let original = crate::test_creatures::build(species);
    let mut document = original.ruleset().document().clone();
    document.nutrition = serde_json::from_value(serde_json::json!({
        "kind":"enabled","initial":25,"maximum":30,"hungry_below":10,
        "weak_below":5,"starve_at":-5,"unit_ticks":10,
        "hungry_recovery":{"kind":"normal"},"weak_recovery":{"kind":"normal"},
        "empty_recovery":{"kind":"normal"},
        "regeneration_cost":{"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0}
    }))
    .unwrap();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        original.content().catalog().clone(),
    )
    .unwrap();
    let build = CreatureBuild::new(
        content.clone(),
        original.species_id().clone(),
        original.ledger().clone(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    game.content = content;
    game.actors.get_mut(&ActorId(1)).unwrap().turn_ticks = NonZeroU64::new(50).unwrap();
    let enemy = game
        .spawn_actor(at(1, 2, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    for (actor, faction) in [(ActorId(1), "player"), (enemy, "enemy")] {
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: faction.into(),
                faction: faction.into(),
            },
            build.clone(),
        )
        .unwrap();
        game.configure_nutrition(actor, NutritionRequirement::Required)
            .unwrap();
        crate::test_creatures::injure(&mut game, actor, 1).unwrap();
    }
    game.act(ActorId(1), Action::Wait).unwrap();
    game.act(
        enemy,
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
            supplies: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(game.tick(), 50);
    let pending = game.preparation(enemy).unwrap();
    assert_eq!(pending.started + pending.remaining, 60);
    assert!(game.creature(enemy).unwrap().health_recovery().elapsed() > 0);
    let before = game.clone();

    // These are live deadlines, rather than idle timer fields: ordinary waiting
    // completes the attack, heals the enemy and consumes another hunger unit.
    let mut waiting = game.clone();
    waiting.act(ActorId(1), Action::Wait).unwrap();
    assert!(waiting.tick() >= 60);
    assert!(waiting.preparation(enemy).is_none());
    assert_ne!(waiting.health(enemy), before.health(enemy));
    assert_ne!(
        waiting.creature(ActorId(1)).unwrap().nutrition(),
        before.creature(ActorId(1)).unwrap().nutrition()
    );

    let outcome = game.act(ActorId(1), Action::Move(Direction::Up)).unwrap();
    assert_eq!(outcome.kind, crate::OutcomeKind::Escaped { item });
    assert_eq!((outcome.at_tick, outcome.next_tick), (50, 50));
    assert!(outcome.next_actor.is_none());
    assert!(game.run_outcome().terminal);
    assert_eq!(game.tick(), before.tick());
    assert_eq!(
        game.actors[&ActorId(1)].ready_at,
        before.actors[&ActorId(1)].ready_at
    );
    assert_eq!(game.actors[&enemy], before.actors[&enemy]);
    assert_eq!(game.creature(ActorId(1)), before.creature(ActorId(1)));
    assert_eq!(game.preparation(enemy), before.preparation(enemy));
    assert_eq!(game.rules_rng, before.rules_rng);
    assert!(game.combat_events().is_empty());
}
