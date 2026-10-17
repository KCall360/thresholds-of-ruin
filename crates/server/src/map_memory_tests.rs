//! Final-candidate acquisition and pure read boundaries for persisted map memory.
use super::*;
use tor_simulation::{map_work_counts, reset_map_work_counts, MapWorkCounts};

fn actor() -> ActorId {
    ActorId(1)
}
fn sim_actor() -> SimActor {
    SimActor(1)
}
fn origin() -> Position {
    Position {
        region: 1,
        x: 1,
        y: 1,
        z: 0,
    }
}
fn window() -> tor_protocol::MapWindow {
    tor_protocol::MapWindow {
        min: tor_protocol::Position {
            x: -31,
            y: -16,
            z: -2,
        },
        max: tor_protocol::Position { x: 31, y: 16, z: 3 },
    }
}

fn wizard(
    engine: &mut Engine,
    request: &str,
    operation: WizardOperation,
) -> Result<CommandResult, Failure> {
    engine.command(
        "map-test",
        "test",
        actor(),
        request,
        &engine.branch().clone(),
        Command::Wizard {
            expected_revision: engine.revision(actor()).unwrap(),
            operation,
        },
    )
}

fn no_map_writes(work: MapWorkCounts) {
    assert_eq!(work.observed_cells, 0);
    assert_eq!(work.placement_proposals, 0);
    assert_eq!(work.appearance_changes, 0);
    assert_eq!(work.neighbor_changes, 0);
    assert_eq!(work.witness_changes, 0);
    assert_eq!(work.owner_copies, 0);
    assert_eq!(work.directory_entries_copied, 0);
    assert_eq!(work.bucket_entries_copied, 0);
    assert_eq!(work.appearance_payload_copies, 0);
}

#[test]
fn initial_map_knowledge_precedes_reads_and_is_in_the_initial_rewind_boundary() {
    reset_map_work_counts();
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    assert!(map_work_counts().observed_cells > 0);
    assert!(!engine
        .game
        .map_disclosure_changed_since(&engine.game, sim_actor()));
    let root = engine.game.map_knowledge_root(sim_actor());
    assert!(engine
        .boundaries
        .front()
        .unwrap()
        .game
        .map_knowledge_is_current(sim_actor(), &root));
    let before = engine.game.clone();
    reset_map_work_counts();
    let projection = engine.explored_map(actor(), window()).unwrap();
    assert!(!projection.cells.is_empty());
    assert!(projection.complete && !projection.limited);
    assert!(engine.map_projection_is_current(actor(), &projection));
    assert_eq!(engine.game, before);
    no_map_writes(map_work_counts());
}

#[test]
fn map_windows_must_be_inside_the_current_normal_viewport() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let normal = window();
    let mut outside = normal;
    outside.min.x += 1;
    outside.max.x += 1;
    let mut remote = normal;
    remote.min.x += 10_000;
    remote.max.x += 10_000;
    let mut lower = normal;
    lower.min.z -= 1;
    lower.max.z -= 1;
    for invalid in [outside, remote, lower] {
        reset_map_work_counts();
        assert_eq!(
            engine.explored_map(actor(), invalid).unwrap_err().code,
            ErrorCode::InvalidAction
        );
        assert_eq!(map_work_counts(), MapWorkCounts::default());
    }
    assert!(engine.explored_map(actor(), normal).unwrap().complete);
    let point = tor_protocol::MapWindow {
        min: tor_protocol::Position { x: 0, y: 0, z: 0 },
        max: tor_protocol::Position { x: 0, y: 0, z: 0 },
    };
    assert!(engine.explored_map(actor(), point).unwrap().complete);
}

#[test]
fn off_sight_edge_slab_matches_the_full_normal_projection() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("paired-stairs");
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/tests/paired-stairs");
    std::fs::create_dir_all(source.join("regions")).unwrap();
    for file in [
        "scenario.toml",
        "rules.toml",
        "regions/1.toml",
        "regions/2.toml",
        "regions/3.toml",
    ] {
        std::fs::copy(original.join(file), source.join(file)).unwrap();
    }
    std::fs::write(
        source.join("regions/1.toml"),
        "id = 1\nname = \"Upper floor\"\nsize = [32, 5, 1]\nchamber = true\n\
         anchors = { stair = [1, 1, 0] }\n",
    )
    .unwrap();
    crate::scenario_package::validate(&source).unwrap();
    let scenario = crate::scenario_package::load(&source, 42, None, false).unwrap();
    let mut engine = Engine::memory(scenario).unwrap();
    let mut edge_key = None;
    for step in 1..=32 {
        engine
            .command(
                "map-test",
                "test",
                actor(),
                &format!("edge-walk-{step}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(actor()).unwrap(),
                    action: Action::Move {
                        direction: if step <= 16 {
                            Direction::East
                        } else {
                            Direction::West
                        },
                    },
                },
            )
            .unwrap();
        if step == 16 {
            let observation = engine.observation(actor()).unwrap();
            edge_key = Some(
                observation
                    .visible_cells
                    .iter()
                    .find(|cell| cell.position == tor_protocol::Position { x: 15, y: 0, z: 0 })
                    .expect("ordinary walking must disclose the future viewport-edge cell")
                    .key
                    .clone(),
            );
        }
    }
    let slab = tor_protocol::MapWindow {
        min: tor_protocol::Position {
            x: 31,
            ..window().min
        },
        max: window().max,
    };
    assert!(engine
        .revision_view(actor())
        .unwrap()
        .scene
        .iter()
        .filter(|cell| cell.physical)
        .all(|cell| cell.offset.x < slab.min.x - 1));
    let full = engine.explored_map(actor(), window()).unwrap();
    assert!(full.complete && !full.limited, "{full:?}");
    let expected: Vec<_> = full
        .cells
        .iter()
        .filter(|cell| slab.contains(cell.position))
        .cloned()
        .collect();
    assert!(expected
        .iter()
        .any(|cell| Some(&cell.key) == edge_key.as_ref()));
    let before = engine.game.clone();
    reset_map_work_counts();
    let partial = engine.explored_map(actor(), slab).unwrap();
    assert_eq!(
        partial.cells, expected,
        "an edge slab without a live seed must retain the full viewport's remembered cells"
    );
    assert_eq!(partial.complete, full.complete);
    assert_eq!(partial.ambiguous, full.ambiguous);
    assert_eq!(partial.limited, full.limited);
    assert_eq!(engine.game, before);
    no_map_writes(map_work_counts());
}

#[test]
fn unchanged_wait_builds_no_ordinary_views_or_scenes_and_writes_no_map_knowledge() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let root = engine.game.map_knowledge_root(sim_actor());
    let tick = engine.game.tick();
    let before = tor_simulation::diagnostics::work_counts();
    reset_map_work_counts();
    let (_, profile) = engine
        .command_profiled(
            "map-test",
            "test",
            actor(),
            "unchanged-wait",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor()).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    let after = tor_simulation::diagnostics::work_counts();
    assert!(engine.game.tick() > tick);
    assert_eq!(after.observations - before.observations, 0);
    assert_eq!(after.scenes - before.scenes, 0);
    assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 0);
    assert_eq!(profile.perception_calls, 0);
    assert_eq!(profile.scene_calls, 0);
    assert_eq!(map_work_counts(), MapWorkCounts::default());
    assert!(engine.game.map_knowledge_is_current(sim_actor(), &root));
}

#[test]
fn item_only_changes_refresh_saved_appearance_even_when_the_physical_scene_is_equal() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    // The diagnostic scenario already puts a token at the actor's origin.
    // This adjacent, visible cell starts empty, so the witness isolates addition.
    let destination = Position { x: 2, ..origin() };
    let before = engine.explored_map(actor(), window()).unwrap();
    let scene = engine.revision_view(actor()).unwrap().scene.clone();
    let geometry = engine.game.geometry_snapshot();
    let key = adapt::cell_key(
        &engine.archive.view_salt,
        actor().0,
        adapt::location(destination),
    );
    assert!(before
        .cells
        .iter()
        .find(|cell| cell.key == key)
        .unwrap()
        .ground_items
        .is_empty());
    reset_map_work_counts();
    wizard(
        &mut engine,
        "item-only",
        WizardOperation::PlaceItem {
            kind: WizardItem::Token,
            position: destination,
        },
    )
    .unwrap();
    let work = map_work_counts();
    assert!(work.observed_cells > 0 && work.appearance_changes > 0);
    assert_eq!(engine.game.geometry_snapshot(), geometry);
    assert_eq!(engine.revision_view(actor()).unwrap().scene, scene);
    assert!(!engine.map_projection_is_current(actor(), &before));
    let after = engine.explored_map(actor(), window()).unwrap();
    let cell = after.cells.iter().find(|cell| cell.key == key).unwrap();
    assert_eq!(cell.ground_items.len(), 1);
    assert_eq!(cell.ground_items[0].quantity, 1);
    assert!(engine
        .boundaries
        .back()
        .unwrap()
        .game
        .map_knowledge_is_current(sim_actor(), &after.root));
}

#[test]
fn place_hint_changes_refresh_saved_appearance_even_when_the_physical_scene_is_equal() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.explored_map(actor(), window()).unwrap();
    let scene = engine.revision_view(actor()).unwrap().scene.clone();
    let key = adapt::cell_key(
        &engine.archive.view_salt,
        actor().0,
        adapt::location(origin()),
    );
    assert!(
        !before
            .cells
            .iter()
            .find(|cell| cell.key == key)
            .unwrap()
            .place_hint
    );
    reset_map_work_counts();
    wizard(
        &mut engine,
        "hint-only",
        WizardOperation::SetPlaceHint {
            position: origin(),
            present: true,
        },
    )
    .unwrap();
    let work = map_work_counts();
    assert!(work.observed_cells > 0 && work.appearance_changes > 0);
    assert_eq!(engine.revision_view(actor()).unwrap().scene, scene);
    assert!(!engine.map_projection_is_current(actor(), &before));
    let after = engine.explored_map(actor(), window()).unwrap();
    assert!(
        after
            .cells
            .iter()
            .find(|cell| cell.key == key)
            .unwrap()
            .place_hint
    );
}

#[test]
fn repeated_observation_and_map_reads_preserve_authority_and_knowledge_ownership() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let root = engine.game.map_knowledge_root(sim_actor());
    let game = engine.game.clone();
    let archive = serde_json::to_value(&engine.archive).unwrap();
    let state = engine.state(actor()).unwrap();
    let expected = engine.explored_map(actor(), window()).unwrap().cells;
    reset_map_work_counts();
    for _ in 0..3 {
        assert_eq!(engine.state(actor()).unwrap(), state);
        assert_eq!(
            engine.explored_map(actor(), window()).unwrap().cells,
            expected
        );
        assert!(engine.game.map_knowledge_is_current(sim_actor(), &root));
    }
    assert_eq!(engine.game, game);
    assert_eq!(serde_json::to_value(&engine.archive).unwrap(), archive);
    no_map_writes(map_work_counts());
}

#[test]
fn spectator_attachment_and_snapshot_reads_do_not_acquire_map_knowledge() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let expected = engine.state(actor()).unwrap();
    let mut service = crate::Service::new(engine);
    reset_map_work_counts();
    let mut client = service
        .connect(
            &crate::Account {
                user: "map-spectator".into(),
                token: "test".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([actor()]),
            },
            "headless".into(),
        )
        .unwrap();
    while client.messages.try_recv().is_ok() {}
    for (request, body) in [
        ("attach", Request::Attach { actor: actor() }),
        ("snapshot-one", Request::Snapshot),
        ("snapshot-two", Request::Snapshot),
    ] {
        service.handle(client.id, request.into(), body);
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        let snapshot = messages
            .iter()
            .find_map(|message| match message {
                ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
                _ => None,
            })
            .expect("spectator receives the actor's disclosed snapshot");
        assert_eq!(*snapshot.state, expected);
        assert!(!snapshot.has_control);
        no_map_writes(map_work_counts());
    }
}

#[test]
fn rejected_map_output_encoding_keeps_the_same_knowledge_root_and_retry_result() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let before = engine.game.clone();
    let root = engine.game.map_knowledge_root(sim_actor());
    reset_map_work_counts();
    let prepared = engine.explored_map(actor(), window()).unwrap();
    assert!(matches!(
        tor_protocol::encode_bounded_json(&prepared.cells, 1),
        Err(tor_protocol::EncodeError::TooLarge { limit: 1 })
    ));
    assert!(engine.map_projection_is_current(actor(), &prepared));
    assert!(engine.game.map_knowledge_is_current(sim_actor(), &root));
    assert_eq!(engine.game, before);
    assert_eq!(
        engine.explored_map(actor(), window()).unwrap().cells,
        prepared.cells
    );
    no_map_writes(map_work_counts());
}

#[test]
fn late_storage_rejection_discards_candidate_map_learning_and_retry_still_works() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map-memory.db");
    let scenario = Scenario::two_room(42);
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    engine.enable_wizard().unwrap();
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open_with_policy(
        &path,
        scenario.clone(),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    engine.enable_wizard().unwrap();
    let root = engine.game.map_knowledge_root(sim_actor());
    let before = engine.game.clone();
    let revision = engine.revision(actor()).unwrap();
    let records = serde_json::to_value(&engine.archive.records).unwrap();
    let boundaries = engine.boundaries.clone();
    reset_map_work_counts();
    let failure = wizard(
        &mut engine,
        "retry-item",
        WizardOperation::PlaceItem {
            kind: WizardItem::Token,
            position: origin(),
        },
    )
    .unwrap_err();
    assert_eq!(failure.code, ErrorCode::StorageFailure);
    assert!(
        map_work_counts().appearance_changes > 0,
        "rejected candidate really learned new appearance"
    );
    assert!(engine.game.map_knowledge_is_current(sim_actor(), &root));
    assert_eq!(engine.game, before);
    assert_eq!(engine.revision(actor()).unwrap(), revision);
    assert_eq!(
        serde_json::to_value(&engine.archive.records).unwrap(),
        records
    );
    assert_eq!(engine.boundaries.len(), boundaries.len());
    assert!(engine
        .boundaries
        .iter()
        .zip(&boundaries)
        .all(|(a, b)| Arc::ptr_eq(a, b)));
    assert!(!engine
        .receipts
        .contains_key(&("map-test".into(), "retry-item".into())));
    drop(engine);
    let mut reopened = Engine::open(&path, scenario).unwrap();
    reopened.enable_wizard().unwrap();
    assert_eq!(reopened.game, before);
    let old = reopened.explored_map(actor(), window()).unwrap();
    let result = wizard(
        &mut reopened,
        "retry-item",
        WizardOperation::PlaceItem {
            kind: WizardItem::Token,
            position: origin(),
        },
    )
    .unwrap();
    assert!(!result.duplicate);
    assert!(!reopened.map_projection_is_current(actor(), &old));
    reopened.flush().unwrap();
}
