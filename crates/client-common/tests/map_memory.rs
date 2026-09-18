use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(cells: &[(&str, i32, i32)], revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"map","cursor":{"sequence":0,"tick":revision},
        "has_control":true,"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision,"observation":{
            "actor":1,"tick":revision,"position":{"x":0,"y":0,"z":0},
            "visible_cells":cells.iter().map(|(key,x,y)| serde_json::json!({
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
fn map_aligns_every_update_and_refreshes_items_without_retaining_actors() {
    let mut initial = snapshot(&[("a", 0, 0), ("b", 1, 0), ("item", 3, 0)], 0);
    initial.state.observation.ground_items.push(GroundItemView {
        item: ItemView {
            id: 7,
            name: "token".into(),
            description: String::new(),
        },
        position: Position { x: 3, y: 0, z: 0 },
        reachable: false,
    });
    initial.state.observation.visible_actors.push(ActorView {
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
    client
        .replace_snapshot(snapshot(&[("a", 0, 0)], 1))
        .unwrap();
    assert_eq!(client.map_memory().count(), 2);
    client
        .replace_snapshot(snapshot(&[("elsewhere", 0, 0)], 2))
        .unwrap();
    assert_eq!(client.map_memory().count(), 1);
    let mut rewind = snapshot(&[("a", 0, 0)], 0);
    rewind.branch = BranchId("new".into());
    client.replace_snapshot(rewind).unwrap();
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
