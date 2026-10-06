use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(cells: &[(&str, i32, i32)], revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":0,"admission":false,"resume":[],"cancel":[]},"context":super::stream_context(0),
        "actor":1,"branch":"map","cursor":{"sequence":0,"tick":revision},
        "has_control":true, "intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision,"observation":{
            "actor":1,"tick":revision,"position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":cells.iter().map(|(key,x,y)| serde_json::json!({
                "key":key,"position":{"x":x,"y":y,"z":0},"wall":false,
                "stairs_up":false,"stairs_down":false,"place_hint":false
            })).collect::<Vec<_>>(),
            "ground_items":[],"inventory":[],"visible_actors":[],"ready":true
        }}
    }))
    .unwrap()
}

fn advance(client: &mut ClientState, next: Snapshot) {
    client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.actor,
            branch: next.branch,
            cursor: StreamCursor {
                sequence: client.cursor().sequence + 1,
                tick: next.cursor.tick,
            },
            body: UpdateBody::Observation {
                state: Box::new(next.state),
                event: None,
            },
        })
        .unwrap();
}

#[test]
fn narration_rejects_gaps_atomically_and_resets_on_snapshot() {
    let mut client = ClientState::from_snapshot(snapshot(&[("a", 0, 0)], 0)).unwrap();
    let mut seen = snapshot(&[("a", 0, 0)], 1);
    seen.state.observation.visible_actors.push(ActorView {
        asset: None,
        id: ActorId(2),
        name: "figure".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    advance(&mut client, seen);
    assert_eq!(client.narration(), ["You notice a figure."]);
    let unchanged = client.clone();
    let mut next = snapshot(&[("a", 0, 0)], 2);
    assert!(client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.actor,
            branch: next.branch.clone(),
            cursor: StreamCursor {
                sequence: 3,
                tick: 2
            },
            body: UpdateBody::Observation {
                state: Box::new(next.state.clone()),
                event: None
            },
        })
        .is_err());
    assert_eq!(client, unchanged);
    super::reset_snapshot(&mut client, next.clone()).unwrap();
    assert!(client.narration().is_empty());
    next.branch = BranchId("rewound".into());
    super::reset_snapshot(&mut client, next).unwrap();
    assert!(client.narration().is_empty());
}

#[test]
fn map_aligns_every_update_and_refreshes_items_without_retaining_actors() {
    let mut initial = snapshot(&[("a", 0, 0), ("b", 1, 0), ("item", 3, 0)], 0);
    initial.state.observation.ground_items.push(GroundItemView {
        item: ItemView {
            asset: None,
            quantity: 1,
            appearance: String::new(),
            identified: true,
            id: 7,
            name: "token".into(),
            description: String::new(),
        },
        position: Position { x: 3, y: 0, z: 0 },
        reachable: false,
    });
    initial.state.observation.visible_actors.push(ActorView {
        asset: None,
        id: ActorId(2),
        position: Position { x: 3, y: 0, z: 0 },
        name: "figure".into(),
        description: String::new(),
    });
    let mut client = ClientState::from_snapshot(initial).unwrap();
    advance(&mut client, snapshot(&[("a", -1, 0), ("b", 0, 0)], 1));
    advance(&mut client, snapshot(&[("b", -1, 0), ("c", 0, 0)], 2));
    let remembered = client.map_memory().find(|c| c.key == "item").unwrap();
    assert_eq!(remembered.position, Position { x: 1, y: 0, z: 0 });
    assert_eq!(remembered.ground_items.len(), 1);
    assert!(remembered.visible_actors.is_empty());
    // The historical memory contract remains unchanged.
    assert_eq!(
        client
            .memory()
            .find(|c| c.key == "item")
            .unwrap()
            .position
            .x,
        3
    );
    advance(&mut client, snapshot(&[("c", 0, 0), ("item", 1, 0)], 3));
    assert!(client
        .map_memory()
        .find(|c| c.key == "item")
        .unwrap()
        .ground_items
        .is_empty());
}

#[test]
fn snapshots_preserve_aligned_map_but_rewind_and_unalignable_views_reset_it() {
    let mut client =
        ClientState::from_snapshot(snapshot(&[("a", 0, 0), ("old", 1, 0)], 0)).unwrap();
    super::reset_snapshot(&mut client, snapshot(&[("a", 0, 0)], 1)).unwrap();
    assert_eq!(client.map_memory().count(), 2);
    super::reset_snapshot(&mut client, snapshot(&[("elsewhere", 0, 0)], 2)).unwrap();
    assert_eq!(client.map_memory().count(), 1);
    let mut rewind = snapshot(&[("a", 0, 0)], 0);
    rewind.branch = BranchId("new".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
    assert_eq!(client.map_memory().count(), 1);
    assert_eq!(client.map_memory().next().unwrap().key, "a");
}

#[test]
fn conflicting_or_ambiguous_occurrences_do_not_guess_a_global_map() {
    let mut client =
        ClientState::from_snapshot(snapshot(&[("a", 0, 0), ("b", 1, 0), ("old", 2, 0)], 0))
            .unwrap();
    advance(&mut client, snapshot(&[("a", 0, 0), ("b", 0, 1)], 1));
    assert!(client.map_memory().all(|c| c.key != "old"));
    let mut client =
        ClientState::from_snapshot(snapshot(&[("a", 0, 0), ("a", 3, 0), ("old", 2, 0)], 0))
            .unwrap();
    advance(&mut client, snapshot(&[("a", 0, 0), ("a", 3, 0)], 1));
    assert!(client.map_memory().all(|c| c.key != "old"));
}

#[test]
fn map_tracks_elevation_and_rejects_bad_updates_atomically() {
    let first = snapshot(&[("anchor", 0, 0), ("old", 1, 0)], 0);
    let mut client = ClientState::from_snapshot(first).unwrap();
    let mut upstairs = snapshot(&[("anchor", 0, 0)], 1);
    upstairs.state.observation.visible_cells[0].position.z = -1;
    advance(&mut client, upstairs);
    assert_eq!(
        client
            .map_memory()
            .find(|c| c.key == "old")
            .unwrap()
            .position
            .z,
        -1
    );
    let before = client.clone();
    let next = snapshot(&[("anchor", 0, 0)], 2);
    assert!(client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.actor,
            branch: next.branch,
            cursor: StreamCursor {
                sequence: 99,
                tick: 2
            },
            body: UpdateBody::Observation {
                state: Box::new(next.state),
                event: None
            }
        })
        .is_err());
    assert_eq!(client, before);
}

#[test]
fn chart_size_and_extreme_translations_are_bounded() {
    let mut initial = snapshot(&[("anchor", 0, 0)], 0);
    let template = initial.state.observation.visible_cells[0].clone();
    for x in 1..5000 {
        let mut cell = template.clone();
        cell.key = format!("cell-{x}");
        cell.position.x = x;
        initial.state.observation.visible_cells.push(cell);
    }
    let mut client = ClientState::from_snapshot(initial).unwrap();
    assert_eq!(client.map_memory().count(), 4096);
    advance(&mut client, snapshot(&[("anchor", i32::MAX, 0)], 1));
    assert_eq!(client.map_memory().count(), 1);
}

fn cells(row: &[(String, i32, i32)]) -> Vec<(&str, i32, i32)> {
    row.iter().map(|(k, x, y)| (k.as_str(), *x, *y)).collect()
}

fn delta_update(client: &ClientState, delta: StateDelta) -> StreamUpdate {
    StreamUpdate {
        context: client.context().clone(),
        actor: ActorId(1),
        branch: client.branch().clone(),
        cursor: StreamCursor {
            sequence: client.cursor().sequence + 1,
            tick: delta.tick,
        },
        body: UpdateBody::ObservationDelta {
            base: client.observation_base(),
            state: Box::new(delta),
            event: None,
        },
    }
}

#[test]
fn deltas_rebuild_the_full_view_and_reject_another_base_atomically() {
    let row = |ox: i32| -> Vec<(String, i32, i32)> {
        (0..8).map(|x| (format!("{x}"), x - ox, 0)).collect()
    };
    let first = row(0);
    let mut client = ClientState::from_snapshot(snapshot(&cells(&first), 0)).unwrap();
    // One step east: every retained cell moves one place west.
    let mut next = snapshot(&cells(&row(1)[1..]), 1).state;
    next.observation.visible_cells[0].wall = true;
    let delta = StateDelta::between(client.state(), &next).unwrap();
    assert_eq!(delta.cells.shift, Position { x: -1, y: 0, z: 0 });
    client.apply(delta_update(&client, delta)).unwrap();
    assert_eq!(client.state(), &next);

    let unchanged = client.clone();
    let stale = StateDelta::between(&snapshot(&cells(&first), 0).state, &next);
    let mut stale = stale.unwrap();
    stale.revision = 2;
    stale.tick = 2;
    assert_eq!(
        client.apply(delta_update(&client, stale)),
        Err(tor_client_common::StreamError::InconsistentState)
    );
    assert_eq!(client, unchanged);
}

#[test]
fn overflowing_delta_preserves_the_entire_client_model() {
    for (coordinate, shift) in [(i32::MAX, 1), (i32::MIN, -1)] {
        let row: Vec<_> = (0..8)
            .map(|y| (format!("edge-{y}"), coordinate, y))
            .collect();
        let initial = snapshot(&cells(&row), 0);
        let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
        let mut next = initial.state;
        next.revision = 1;
        next.observation.tick = 1;
        let mut delta = StateDelta::between(client.state(), &next).unwrap();
        delta.cells.shift.x = shift;
        let unchanged = client.clone();
        assert_eq!(
            client.apply(delta_update(&client, delta)),
            Err(tor_client_common::StreamError::InconsistentState)
        );
        assert_eq!(client, unchanged);
        super::reset_snapshot(&mut client, snapshot(&[("recovered", 0, 0)], 1)).unwrap();
        assert_eq!(client.state().revision, 1);
    }
}
