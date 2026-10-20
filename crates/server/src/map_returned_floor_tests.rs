//! A fresh lower-floor chart recovers the explored upper floor after ordinary stairs.
use super::*;
use crate::{SavePolicy, Scenario, Streaming};
use tor_client_common::ClientState;

#[test]
fn returned_floor_rehydrates_unseen_walls_and_stairs_after_source_free_resume() {
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
    // All geometry is authored before play. The second upper stair and its
    // neighboring wall are 18 cells from the return stair, beyond sight but
    // inside the normal remembered-map window.
    std::fs::write(
        source.join("regions/1.toml"),
        "id = 1\nname = \"Upper floor\"\nsize = [27, 5, 1]\nchamber = true\n\
         anchors = { stair = [1, 1, 0], distant = [19, 1, 0] }\n",
    )
    .unwrap();
    let manifest = source.join("scenario.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    assert!(text.contains("upper = \"2/down\""));
    std::fs::write(
        manifest,
        text.replace("upper = \"2/down\"", "upper = \"1/distant\""),
    )
    .unwrap();
    crate::scenario_package::validate(&source).unwrap();
    let mut scenario = crate::scenario_package::load(&source, 42, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let save = directory.path().join("returned-floor.db");
    let policy = SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut engine = Engine::open_with_policy(&save, scenario, policy.clone()).unwrap();
    let actor = ActorId(1);
    let origin = Position { x: 0, y: 0, z: 0 };
    let start = engine.state(actor).unwrap();
    let start_key = start
        .observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == origin)
        .unwrap()
        .key
        .clone();
    let walk = |engine: &mut Engine, direction| {
        let revision = engine.revision(actor).unwrap();
        let branch = engine.branch().clone();
        engine
            .command(
                "player",
                "test",
                actor,
                &format!("walk-{revision}"),
                &branch,
                crate::journal::Command::Act {
                    expected_revision: revision,
                    action: crate::journal::Action::Move { direction },
                },
            )
            .unwrap();
    };
    let appearance = |cell: &CellView, position| RememberedMapCell {
        key: cell.key.clone(),
        position,
        wall: cell.wall,
        material: cell.material.clone(),
        asset: cell.asset.clone(),
        place_hint: cell.place_hint,
        stairs_up: cell.stairs_up,
        stairs_down: cell.stairs_down,
        trap: cell.trap.as_ref().map(|trap| MapTrapAppearance {
            family: trap.family,
        }),
        door: None,
        ground_items: vec![],
    };
    let mut known_upper = BTreeMap::new();
    let mut offset = 0;
    // Include every physically observed upper-floor cell, not only the two
    // witnesses. Shell cells include floor/ceiling planes; artificial landings
    // are open cells off the upper floor. Expected values never come from hydration.
    for step in 0..=36 {
        if step > 0 {
            let direction = if step <= 18 {
                offset += 1;
                crate::journal::Direction::East
            } else {
                offset -= 1;
                crate::journal::Direction::West
            };
            walk(&mut engine, direction);
        }
        let state = engine.state(actor).unwrap();
        assert_eq!(state.observation.tick, step * 100);
        assert!(state.observation.ground_items.is_empty());
        for cell in state
            .observation
            .visible_cells
            .iter()
            .filter(|cell| cell.position.z == 0 || cell.wall)
        {
            assert!(cell.door.is_none());
            let position = Position {
                x: cell.position.x + offset,
                ..cell.position
            };
            known_upper.insert(position, appearance(cell, position));
        }
    }
    let stair = Position { x: 18, y: 0, z: 0 };
    let wall = Position { x: 18, y: -2, z: 0 };
    assert!(known_upper[&stair].stairs_down);
    assert!(known_upper[&wall].wall);
    walk(&mut engine, crate::journal::Direction::Down);
    let lower = engine.state(actor).unwrap();
    assert_ne!(
        lower
            .observation
            .visible_cells
            .iter()
            .find(|cell| cell.position == origin)
            .unwrap()
            .key,
        start_key
    );
    engine.flush().unwrap();
    drop(engine);
    std::fs::remove_dir_all(&source).unwrap();
    let engine = Engine::open_with_policy(&save, Scenario::two_room(0), policy).unwrap();
    assert_eq!(engine.state(actor).unwrap(), lower);

    // A new client starts with only the lower-floor chart. No upper-floor
    // presentation cache survives the disconnect or the source-free restore.
    let mut service = Service::with_outbound_limits(
        engine,
        crate::OutboundLimits {
            // The ordinary live floor transition needs a complete observation
            // update. Small-frame map pagination has its own Service witness.
            frame_bytes: MAX_MAP_PAGE_BYTES,
            client_bytes: 16 * 1024 * 1024,
            total_bytes: 32 * 1024 * 1024,
        },
    )
    .unwrap();
    let mut client = service
        .connect(
            &Account {
                role: AccessRole::Player,
                user: "player".into(),
                token: "test".into(),
                actors: BTreeSet::from([actor]),
            },
            "headless".into(),
        )
        .unwrap();
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    service.handle(client.id, "control".into(), Request::AcquireControl);
    service.handle(client.id, "lower".into(), Request::Snapshot);
    let snapshot = std::iter::from_fn(|| client.messages.try_recv().ok())
        .filter_map(|message| match message {
            ServerMessage::Snapshot { snapshot, .. } => Some(*snapshot),
            ServerMessage::Error { code, message, .. } => panic!("{code:?}: {message}"),
            _ => None,
        })
        .last()
        .unwrap();
    let mut model = ClientState::from_snapshot(snapshot).unwrap();
    for position in [wall, stair] {
        assert!(!model
            .memory()
            .any(|cell| cell.key == known_upper[&position].key));
    }
    service.handle(
        client.id,
        "return-up".into(),
        model.command_request(Command::Act {
            expected_revision: model.state().revision,
            action: Action::Move {
                direction: Direction::Up,
            },
        }),
    );
    service.run_until_blocked();
    while let Ok(message) = client.messages.try_recv() {
        match message {
            ServerMessage::Update { update } => model.apply(*update).unwrap(),
            ServerMessage::Snapshot { snapshot, .. } => model.replace_snapshot(*snapshot).unwrap(),
            ServerMessage::Error { code, message, .. } => panic!("{code:?}: {message}"),
            _ => {}
        }
    }
    assert!(service.clients.contains_key(&client.id));
    assert!(client.close.borrow().is_none());
    assert!(service.save_warning.is_none());
    assert_eq!(model.state().observation.tick, lower.observation.tick + 100);
    assert_eq!(model.state(), &service.engine.state(actor).unwrap());
    assert!(model.readiness().admission);
    assert_eq!(model.map_cell(origin).unwrap().key, start_key);
    for position in [wall, stair] {
        let key = &known_upper[&position].key;
        assert!(!model
            .state()
            .observation
            .visible_cells
            .iter()
            .any(|cell| &cell.key == key));
        assert!(
            model.map_cell(position).is_none(),
            "upper chart is not locally recoverable"
        );
    }
    let live = model.state().clone();
    let readiness = model.readiness().clone();
    // Live disclosure wins even at the artificial landing's projected offset.
    for cell in &live.observation.visible_cells {
        assert!(cell.door.is_none());
        known_upper.insert(cell.position, appearance(cell, cell.position));
    }
    for _ in 0..MAX_MAP_TRANSFER_PAGES {
        let Some(query) = model.next_map_query() else {
            break;
        };
        assert_eq!(query.window, MapWindow::normal());
        let request_id = format!(
            "map-{}",
            query.cursor.as_ref().map_or(0, |cursor| cursor.page)
        );
        model.bind_map_query(&query, &request_id).unwrap();
        service.handle(client.id, request_id, Request::Map { query });
        let frame = client.messages.try_recv_frame().unwrap();
        assert!(frame.following.is_empty());
        assert!(frame.text.len() <= MAX_MAP_PAGE_BYTES);
        let ServerMessage::MapPage {
            context,
            request_id,
            page,
        } = decode_response(&frame.text).unwrap()
        else {
            panic!("expected one correlated map page")
        };
        assert!(page.complete && !page.ambiguous && !page.limited);
        assert!(page.cells.len() <= MAX_MAP_PAGE_CELLS);
        model
            .accept_map_page(&context, &request_id, &page, frame.text.len())
            .unwrap();
        if page.next.is_some() {
            for position in [wall, stair] {
                assert!(
                    model.map_cell(position).is_none(),
                    "partial pages must not publish"
                );
            }
        }
    }
    let status = model.map_hydration();
    assert!(
        status.complete
            && !status.pending
            && !status.failed
            && !status.limited
            && !status.ambiguous
    );
    assert!(status.pages > 0 && status.pages <= MAX_MAP_TRANSFER_PAGES);
    assert!(status.cells <= MAX_MAP_TRANSFER_CELLS && status.bytes <= MAX_MAP_TRANSFER_BYTES);
    assert!(
        model.next_map_query().is_none(),
        "complete coverage must not loop"
    );
    for (position, expected) in known_upper {
        assert_eq!(
            model.map_cell(position).as_ref(),
            Some(&expected),
            "known upper cell {position:?}"
        );
    }
    // The wire appearances cannot smuggle actors, door/item action handles,
    // reachability, or navigation affordances into the remembered presentation.
    for position in [wall, stair] {
        let appearance = model.map_cell(position).unwrap();
        let value = serde_json::to_value(&appearance).unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "key",
                "position",
                "wall",
                "material",
                "asset",
                "place_hint",
                "stairs_up",
                "stairs_down",
                "trap",
                "door",
                "ground_items"
            ])
        );
        assert!(
            appearance.trap.is_none()
                && appearance.door.is_none()
                && appearance.ground_items.is_empty()
        );
    }
    assert_eq!(model.state(), &live);
    assert_eq!(model.readiness(), &readiness);
}
