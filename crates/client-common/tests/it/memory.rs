use std::sync::Arc;
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(cell_id: u64, tick: u64, revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},"context":super::stream_context(0),
        "actor":"1","branch":"first","cursor":{"sequence":"0","tick":tick.to_string()},
        "has_control":false, "intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision.to_string(),"observation":{
            "actor":"1","tick":tick.to_string(),"position":{"x":1,"y":1,"z":0},
            "places":[],"visible_cells":[{"key":cell_id.to_string(),"stairs_up":false,"stairs_down":false,"position":{"x":1,"y":1,"z":0},"wall":false,"place_hint":false}],"ground_items":[],"inventory":[],"visible_actors":[],
            "ready":true
        }}
    }))
    .unwrap()
}

fn update(client: &ClientState, next: Snapshot, sequence: u64) -> StreamUpdate {
    StreamUpdate {
        context: client.context().clone(),
        actor: next.actor,
        branch: next.branch,
        cursor: StreamCursor {
            sequence,
            tick: next.cursor.tick,
        },
        body: UpdateBody::Observation {
            state: next.state,
            event: None,
        },
    }
}

#[test]
fn free_rename_updates_and_authoritative_place_snapshots_survive_reconnect() {
    let mut client = ClientState::from_snapshot(snapshot(1, 0, 0)).unwrap();
    let mut next = snapshot(1, 0, 1);
    Arc::make_mut(&mut next.state)
        .observation
        .places
        .push(PlaceView {
            key: "offscreen".into(),
            name: "Quiet Reverie".into(),
            origin: PlaceNameOrigin::Authored,
        });
    let mut change = update(&client, next.clone(), 1);
    if let UpdateBody::Observation { event, .. } = &mut change.body {
        *event = Some(Box::new(HistoryEntry {
            id: EntryId("rename".into()),
            branch: next.branch.clone(),
            actor: ActorId(1),
            tick: 0,
            author: Author::User {
                user: "player".into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::PlaceRenamed {
                key: "offscreen".into(),
                name: "Quiet Reverie".into(),
            },
        }));
    }
    client.apply(change).unwrap();
    assert_eq!(
        client.state().observation.places,
        next.state.observation.places
    );
    assert!(client.memory().all(|cell| cell.key != "offscreen"));
    assert_eq!(
        ClientState::from_snapshot(next)
            .unwrap()
            .state()
            .observation
            .places,
        client.state().observation.places
    );
    let mut rewind = snapshot(1, 0, 0);
    rewind.branch = BranchId("rewound".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
    assert!(client.state().observation.places.is_empty());
}

#[test]
fn solid_cell_memory_is_stale_until_refreshed_and_clears_on_rewind() {
    // Floors and ceilings are ordinary solid cells, remembered like any other.
    let mut first = snapshot(1, 0, 0);
    let ceiling = &mut Arc::make_mut(&mut first.state).observation.visible_cells[0];
    ceiling.wall = true;
    ceiling.material = "granite".into();
    let mut client = ClientState::from_snapshot(first).unwrap();
    client
        .apply(update(&client, snapshot(2, 100, 1), 1))
        .unwrap();
    let remembered = client.memory().find(|c| c.key == "1").unwrap();
    assert!(remembered.wall);
    assert_eq!(remembered.material, "granite");
    super::reset_snapshot(&mut client, snapshot(1, 100, 1)).unwrap();
    let refreshed = client.memory().find(|c| c.key == "1").unwrap();
    assert!(!refreshed.wall);
    assert_ne!(refreshed.material, "granite");
    let mut rewind = snapshot(2, 0, 0);
    rewind.branch = BranchId("rewound".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
    assert!(client.memory().all(|c| c.key != "1"));
}

#[test]
fn place_hints_stay_stale_until_seen_again_and_do_not_survive_rewind() {
    let mut first = snapshot(1, 0, 0);
    Arc::make_mut(&mut first.state).observation.visible_cells[0].place_hint = true;
    let mut client = ClientState::from_snapshot(first).unwrap();
    client
        .apply(update(&client, snapshot(2, 100, 1), 1))
        .unwrap();
    assert!(
        client
            .memory()
            .find(|cell| cell.key == "1")
            .unwrap()
            .place_hint
    );
    super::reset_snapshot(&mut client, snapshot(2, 100, 1)).unwrap();
    assert!(
        client
            .memory()
            .find(|cell| cell.key == "1")
            .unwrap()
            .place_hint
    );
    client
        .apply(update(&client, snapshot(1, 200, 2), 1))
        .unwrap();
    assert!(
        !client
            .memory()
            .find(|cell| cell.key == "1")
            .unwrap()
            .place_hint
    );
    let mut marked = snapshot(1, 300, 3);
    Arc::make_mut(&mut marked.state).observation.visible_cells[0].place_hint = true;
    client.apply(update(&client, marked, 2)).unwrap();
    let mut rewind = snapshot(2, 0, 0);
    rewind.branch = BranchId("new".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
    assert!(client.memory().all(|cell| !cell.place_hint));
}

#[test]
fn only_received_views_are_remembered_and_revisits_replace_stale_contents() {
    let mut first = snapshot(1, 0, 0);
    let position = first.state.observation.position;
    Arc::make_mut(&mut first.state)
        .observation
        .ground_items
        .push(GroundItemView {
            reachable: false,
            item: ItemView {
                asset: None,
                quantity: 1,
                appearance: String::new(),
                identified: true,
                description: String::new(),
                id: 7,
                name: "token".into(),
            },
            position,
        });
    let mut client = ClientState::from_snapshot(first).unwrap();
    assert_eq!(client.memory().count(), 1);
    client
        .apply(update(&client, snapshot(2, 100, 1), 1))
        .unwrap();
    assert_eq!(client.state().observation.visible_cells[0].key, "2");
    assert!(client.state().observation.ground_items.is_empty());
    let remembered = client.memory().find(|view| view.key == "1").unwrap();
    assert_eq!(remembered.ground_items[0].item.id, 7);
    assert_eq!(remembered.last_seen_tick, 0);
    assert_eq!(remembered.last_seen_revision, 0);
    client
        .apply(update(&client, snapshot(1, 200, 2), 2))
        .unwrap();
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
    client
        .apply(update(&client, snapshot(2, 100, 1), 1))
        .unwrap();
    super::reset_snapshot(&mut client, snapshot(2, 100, 1)).unwrap();
    assert_eq!(client.memory().count(), 2);
    let mut rewind = snapshot(1, 0, 0);
    rewind.branch = BranchId("rewound".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
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
    Arc::make_mut(&mut upstairs.state).observation.position.z = 1;
    Arc::make_mut(&mut upstairs.state).observation.visible_cells[0]
        .position
        .z = 1;
    client.apply(update(&client, upstairs, 1)).unwrap();
    assert_eq!(
        client
            .memory()
            .map(|view| view.position.z)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let before = client.clone();
    assert!(client
        .apply(update(&client, snapshot(3, 200, 2), 3))
        .is_err());
    let mut wrong_actor = snapshot(3, 200, 2);
    wrong_actor.actor = ActorId(2);
    Arc::make_mut(&mut wrong_actor.state).observation.actor = ActorId(2);
    assert!(super::reset_snapshot(&mut client, wrong_actor).is_err());
    let mut invalid = snapshot(3, 200, 2);
    invalid.cursor.tick = 0;
    assert!(super::reset_snapshot(&mut client, invalid).is_err());
    assert_eq!(client, before);
}

#[test]
fn partially_seen_rooms_retain_unseen_cells_but_clear_visible_empty_cells() {
    let mut first = snapshot(1, 0, 0);
    let distant = Position { x: 4, y: 1, z: 0 };
    Arc::make_mut(&mut first.state)
        .observation
        .visible_cells
        .push(CellView {
            asset: None,
            door: None,
            material: "stone".into(),
            key: "distant".into(),
            stairs_up: false,
            stairs_down: false,
            position: distant,
            wall: false,
            place_hint: false,
        });
    Arc::make_mut(&mut first.state)
        .observation
        .ground_items
        .push(GroundItemView {
            reachable: false,
            item: ItemView {
                asset: None,
                quantity: 1,
                appearance: String::new(),
                identified: true,
                description: String::new(),
                id: 8,
                name: "distant token".into(),
            },
            position: distant,
        });
    let mut client = ClientState::from_snapshot(first).unwrap();
    client
        .apply(update(&client, snapshot(1, 100, 1), 1))
        .unwrap();
    let memory = client
        .memory()
        .find(|cell| cell.position == distant)
        .unwrap();
    assert_eq!(memory.last_seen_tick, 0);
    assert_eq!(memory.ground_items.len(), 1);
    let mut revisit = snapshot(1, 200, 2);
    Arc::make_mut(&mut revisit.state)
        .observation
        .visible_cells
        .push(CellView {
            asset: None,
            door: None,
            material: "stone".into(),
            key: "distant".into(),
            stairs_up: false,
            stairs_down: false,
            position: distant,
            wall: false,
            place_hint: false,
        });
    client.apply(update(&client, revisit, 2)).unwrap();
    let memory = client
        .memory()
        .find(|cell| cell.position == distant)
        .unwrap();
    assert_eq!(memory.last_seen_tick, 200);
    assert!(memory.ground_items.is_empty());
}

#[test]
fn remembered_doors_stay_stale_until_seen_and_rewind_clears_them() {
    let mut first = snapshot(1, 0, 0);
    Arc::make_mut(&mut first.state).observation.visible_cells[0].door = Some(DoorView {
        asset: None,
        id: 4,
        name: "wooden door".into(),
        description: "wood".into(),
        open: true,
        reachable: true,
        approaches: vec![],
    });
    let mut client = ClientState::from_snapshot(first.clone()).unwrap();
    client
        .apply(update(&client, snapshot(2, 100, 1), 1))
        .unwrap();
    assert!(
        client
            .memory()
            .find(|c| c.key == "1")
            .unwrap()
            .door
            .as_ref()
            .unwrap()
            .open
    );
    Arc::make_mut(&mut first.state).observation.visible_cells[0]
        .door
        .as_mut()
        .unwrap()
        .open = false;
    super::reset_snapshot(&mut client, first).unwrap();
    assert!(
        !client
            .memory()
            .find(|c| c.key == "1")
            .unwrap()
            .door
            .as_ref()
            .unwrap()
            .open
    );
    let mut rewind = snapshot(2, 0, 0);
    rewind.branch = BranchId("new".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
    assert!(client.memory().all(|c| c.door.is_none()));
}

#[test]
fn updates_and_same_branch_snapshots_retain_unseen_allocations() {
    let mut client = ClientState::from_snapshot(snapshot(1, 0, 0)).unwrap();
    let address = client.memory().next().unwrap().key.as_ptr();
    client
        .apply(update(&client, snapshot(2, 100, 1), 1))
        .unwrap();
    assert_eq!(
        client.memory().find(|c| c.key == "1").unwrap().key.as_ptr(),
        address
    );
    super::reset_snapshot(&mut client, snapshot(2, 100, 1)).unwrap();
    assert_eq!(
        client.memory().find(|c| c.key == "1").unwrap().key.as_ptr(),
        address
    );
    let before = client.clone();
    let mut invalid = update(&client, snapshot(3, 200, 2), 1);
    if let UpdateBody::Observation { state, .. } = &mut invalid.body {
        Arc::make_mut(state).observation.actor = ActorId(2);
    }
    assert!(client.apply(invalid).is_err());
    assert_eq!(client, before);
}
