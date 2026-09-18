use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(region: u64, tick: u64, revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"first","cursor":{"sequence":0,"tick":tick},
        "has_control":false,"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision,"observation":{
            "actor":1,"tick":tick,"position":{"region":region,"x":1,"y":1,"z":0},
            "region":{"id":region,"name":format!("Room {region}"),"width":5,"depth":3,"height":2},
            "ground_items":[],"inventory":[],"visible_actors":[],"exits":[],
            "known_places":[{"id":99,"name":"A known name is not a view"}],"ready":true
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
fn only_received_views_are_remembered_and_revisits_replace_stale_contents() {
    let mut first = snapshot(1, 0, 0);
    first.state.observation.ground_items.push(GroundItemView {
        item: ItemView {
            id: 7,
            name: "token".into(),
        },
        position: first.state.observation.position,
    });
    let mut client = ClientState::from_snapshot(first).unwrap();
    assert_eq!(client.memory().count(), 1);
    client.apply(update(snapshot(2, 100, 1), 1)).unwrap();
    assert_eq!(client.state().observation.region.id, 2);
    assert!(client.state().observation.ground_items.is_empty());
    let remembered = client.memory().find(|view| view.region.id == 1).unwrap();
    assert_eq!(remembered.ground_items[0].item.id, 7);
    assert_eq!(remembered.last_seen_tick, 0);
    assert_eq!(remembered.last_seen_revision, 0);
    client.apply(update(snapshot(1, 200, 2), 2)).unwrap();
    assert_eq!(client.memory().count(), 2);
    assert!(client
        .memory()
        .find(|view| view.region.id == 1)
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
    assert_eq!(client.memory().next().unwrap().region.id, 1);
    // A fresh connection has no access to the old client's memories.
    let fresh = ClientState::from_snapshot(snapshot(2, 100, 1)).unwrap();
    assert_eq!(fresh.memory().count(), 1);
}

#[test]
fn elevation_views_are_separate_and_invalid_inputs_leave_memory_unchanged() {
    let mut client = ClientState::from_snapshot(snapshot(1, 0, 0)).unwrap();
    let mut upstairs = snapshot(1, 100, 1);
    upstairs.state.observation.position.z = 1;
    client.apply(update(upstairs, 1)).unwrap();
    assert_eq!(
        client
            .memory()
            .map(|view| view.elevation)
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
