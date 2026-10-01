use tor_client_common::ClientState;
use tor_client_hack::chart_shift;
use tor_protocol::*;

fn snapshot(cells: serde_json::Value, revision: u64, branch: &str) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor": 1,
        "branch": branch,
        "cursor": {"sequence": 0, "tick": revision},
        "has_control": true,
        "history": {"entries": [], "older_before": null},
        "state": {
            "wizard_game": false,
            "revision": revision,
            "observation": {
                "actor": 1,
                "tick": revision,
                "position": {"x": 0, "y": 0, "z": 0},
                "places": [],
                "visible_cells": cells,
                "ground_items": [],
                "inventory": [],
                "visible_actors": [],
                "ready": true
            }
        }
    }))
    .unwrap()
}

fn cells(pairs: &[(&str, i32)]) -> serde_json::Value {
    pairs
        .iter()
        .map(|(key, x)| {
            serde_json::json!({
                "key": key,
                "position": {"x": x, "y": 0, "z": 0},
                "wall": false,
                "stairs_up": false,
                "stairs_down": false,
                "place_hint": false
            })
        })
        .collect()
}

#[test]
fn chart_shift_matches_the_cell_map_memory_moves() {
    let first = snapshot(cells(&[("anchor", 0), ("old", 8)]), 0, "map");
    let second = snapshot(cells(&[("anchor", -1)]), 1, "map");
    let before = first.state.observation.clone();
    let after = second.state.observation.clone();
    let mut client = ClientState::from_snapshot(first).unwrap();
    client
        .apply(StreamUpdate {
            actor: ActorId(1),
            branch: BranchId("map".into()),
            cursor: StreamCursor {
                sequence: 1,
                tick: 1,
            },
            body: UpdateBody::Observation {
                state: Box::new(second.state),
                event: None,
            },
        })
        .unwrap();
    assert_eq!(chart_shift(&before, &after), Some((-1, 0, 0)));
    let moved = client.map_memory().find(|cell| cell.key == "old").unwrap();
    assert_eq!(moved.position, Position { x: 7, y: 0, z: 0 });

    let branched = snapshot(cells(&[("anchor", -1)]), 0, "other");
    client.replace_snapshot(branched).unwrap();
    assert!(client.map_memory().all(|cell| cell.key != "old"));
}
