use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(cell_id: u64, tick: u64, revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"first","cursor":{"sequence":0,"tick":tick},
        "has_control":false,"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision,"observation":{
            "actor":1,"tick":tick,"position":{"x":1,"y":1,"z":0},
            "visible_cells":[{"key":cell_id.to_string(),"stairs_up":false,"stairs_down":false,"position":{"x":1,"y":1,"z":0},"wall":false,"place_hint":false}],"ground_items":[],"inventory":[],"visible_actors":[],
            "ready":true
        }}
    }))
    .unwrap()
}

fn update(next: Snapshot, sequence: u64) -> StreamUpdate {
    StreamUpdate {
        actor: next.actor,
        branch: next.branch,
        cursor: StreamCursor {
            sequence,
            tick: next.cursor.tick,
        },
        body: UpdateBody::Observation {
            state: Box::new(next.state),
            event: None,
        },
    }
}

#[test]
fn place_hints_stay_stale_until_seen_again_and_do_not_survive_rewind() {
    let mut first = snapshot(1, 0, 0);
    first.state.observation.visible_cells[0].place_hint = true;
    let mut client = ClientState::from_snapshot(first).unwrap();
    client.apply(update(snapshot(2, 100, 1), 1)).unwrap();
    assert!(
        client
            .memory()
            .find(|cell| cell.key == "1")
            .unwrap()
            .place_hint
    );
    client.replace_snapshot(snapshot(2, 100, 1)).unwrap();
    assert!(
        client
            .memory()
            .find(|cell| cell.key == "1")
            .unwrap()
            .place_hint
    );
    client.apply(update(snapshot(1, 200, 2), 1)).unwrap();
    assert!(
        !client
            .memory()
            .find(|cell| cell.key == "1")
            .unwrap()
            .place_hint
    );
    let mut marked = snapshot(1, 300, 3);
    marked.state.observation.visible_cells[0].place_hint = true;
    client.apply(update(marked, 2)).unwrap();
    let mut rewind = snapshot(2, 0, 0);
    rewind.branch = BranchId("new".into());
    client.replace_snapshot(rewind).unwrap();
    assert!(client.memory().all(|cell| !cell.place_hint));
}

#[test]
fn only_received_views_are_remembered_and_revisits_replace_stale_contents() {
    let mut first = snapshot(1, 0, 0);
    first.state.observation.ground_items.push(GroundItemView {
        reachable: false,
        item: ItemView {
            id: 7,
            name: "token".into(),
        },
        position: first.state.observation.position,
    });
    let mut client = ClientState::from_snapshot(first).unwrap();
    assert_eq!(client.memory().count(), 1);
    client.apply(update(snapshot(2, 100, 1), 1)).unwrap();
    assert_eq!(client.state().observation.visible_cells[0].key, "2");
    assert!(client.state().observation.ground_items.is_empty());
    let remembered = client.memory().find(|view| view.key == "1").unwrap();
    assert_eq!(remembered.ground_items[0].item.id, 7);
    assert_eq!(remembered.last_seen_tick, 0);
    assert_eq!(remembered.last_seen_revision, 0);
    client.apply(update(snapshot(1, 200, 2), 2)).unwrap();
    assert_eq!(client.memory().count(), 2);
    assert!(client
        .memory()
        .find(|view| view.key == "1")
        .unwrap()
        .ground_items
        .is_empty());
}

#[test]
fn snapshots_preserve_same_branch_memory_but_clear_abandoned_futures() {
    let mut client = ClientState::from_snapshot(snapshot(1, 0, 0)).unwrap();
    client.apply(update(snapshot(2, 100, 1), 1)).unwrap();
    client.replace_snapshot(snapshot(2, 100, 1)).unwrap();
    assert_eq!(client.memory().count(), 2);
    let mut rewind = snapshot(1, 0, 0);
    rewind.branch = BranchId("rewound".into());
    client.replace_snapshot(rewind).unwrap();
    assert_eq!(client.memory().count(), 1);
    assert_eq!(client.memory().next().unwrap().key, "1");
    // A fresh connection has no access to the old client's memories.
    let fresh = ClientState::from_snapshot(snapshot(2, 100, 1)).unwrap();
    assert_eq!(fresh.memory().count(), 1);
}

#[test]
fn elevation_views_are_separate_and_invalid_inputs_leave_memory_unchanged() {
    let mut client = ClientState::from_snapshot(snapshot(1, 0, 0)).unwrap();
    let mut upstairs = snapshot(2, 100, 1);
    upstairs.state.observation.position.z = 1;
    upstairs.state.observation.visible_cells[0].position.z = 1;
    client.apply(update(upstairs, 1)).unwrap();
    assert_eq!(
        client
            .memory()
            .map(|view| view.position.z)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let before = client.clone();
    assert!(client.apply(update(snapshot(3, 200, 2), 3)).is_err());
    let mut wrong_actor = snapshot(3, 200, 2);
    wrong_actor.actor = ActorId(2);
    wrong_actor.state.observation.actor = ActorId(2);
    assert!(client.replace_snapshot(wrong_actor).is_err());
    let mut invalid = snapshot(3, 200, 2);
    invalid.cursor.tick = 0;
    assert!(client.replace_snapshot(invalid).is_err());
    assert_eq!(client, before);
}

#[test]
fn partially_seen_rooms_retain_unseen_cells_but_clear_visible_empty_cells() {
    let mut first = snapshot(1, 0, 0);
    let distant = Position { x: 4, y: 1, z: 0 };
    first.state.observation.visible_cells.push(CellView {
        key: "distant".into(),
        stairs_up: false,
        stairs_down: false,
        position: distant,
        wall: false,
        place_hint: false,
    });
    first.state.observation.ground_items.push(GroundItemView {
        reachable: false,
        item: ItemView {
            id: 8,
            name: "distant token".into(),
        },
        position: distant,
    });
    let mut client = ClientState::from_snapshot(first).unwrap();
    client.apply(update(snapshot(1, 100, 1), 1)).unwrap();
    let memory = client
        .memory()
        .find(|cell| cell.position == distant)
        .unwrap();
    assert_eq!(memory.last_seen_tick, 0);
    assert_eq!(memory.ground_items.len(), 1);
    let mut revisit = snapshot(1, 200, 2);
    revisit.state.observation.visible_cells.push(CellView {
        key: "distant".into(),
        stairs_up: false,
        stairs_down: false,
        position: distant,
        wall: false,
        place_hint: false,
    });
    client.apply(update(revisit, 2)).unwrap();
    let memory = client
        .memory()
        .find(|cell| cell.position == distant)
        .unwrap();
    assert_eq!(memory.last_seen_tick, 200);
    assert!(memory.ground_items.is_empty());
}
