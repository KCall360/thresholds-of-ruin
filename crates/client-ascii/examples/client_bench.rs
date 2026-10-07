//! Version 1 disclosed client workload: historical memory, bounded chart and bursts.
use std::{collections::BTreeSet, hint::black_box, sync::Arc, time::Instant};
use tor_client_ascii::{render::Canvas, App};
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(count: usize) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"client-bench","cursor":{"sequence":"0","tick":"0"},
        "intentions":[],"travel":null,"has_control":true,"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":"0","observation":{
            "actor":"1","tick":"0","position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":(0..count).map(|i| serde_json::json!({
                "key":i.to_string(),"position":{"x":(i%49) as i32-24,"y":((i/49)%25) as i32-12,"z":(i/1225) as i32},
                "wall":i%7==0,"stairs_up":false,"stairs_down":false,"place_hint":false
            })).collect::<Vec<_>>(),"ground_items":[],"inventory":[],"visible_actors":[],"ready":true
        }}
    })).unwrap()
}

fn clone_owned(state: &StateView, readers: usize) -> (f64, usize) {
    let started = Instant::now();
    let retained: Vec<_> = (0..readers).map(|_| black_box(state).clone()).collect();
    black_box(&retained);
    let elapsed = started.elapsed().as_secs_f64() * 1000.;
    let objects = retained
        .iter()
        .map(std::ptr::from_ref)
        .collect::<BTreeSet<_>>()
        .len();
    assert_eq!(objects, readers);
    assert!(retained.iter().all(|copy| copy == state));
    (elapsed, objects)
}

fn clone_shared(state: &Arc<StateView>, readers: usize) -> (f64, usize) {
    let started = Instant::now();
    let retained: Vec<_> = (0..readers).map(|_| Arc::clone(black_box(state))).collect();
    black_box(&retained);
    let elapsed = started.elapsed().as_secs_f64() * 1000.;
    let objects = retained
        .iter()
        .map(Arc::as_ptr)
        .collect::<BTreeSet<_>>()
        .len();
    assert_eq!(objects, 1);
    assert_eq!(Arc::strong_count(state), readers + 1);
    assert!(retained.iter().all(|copy| Arc::ptr_eq(copy, state)));
    (elapsed, objects)
}

fn ownership_diagnostic() {
    for cells in [64, 4096, 20_956] {
        let state = snapshot(cells).state;
        state.validate().unwrap();
        let state_bytes = serde_json::to_vec(state.as_ref()).unwrap().len();
        for readers in [1, 8, 32] {
            for sample in 0..100 {
                // Alternate paired methods; release retained objects after timing
                // and verify identity/content outside the measured interval.
                let methods = if sample % 2 == 0 {
                    ["owned_clone", "shared_handle"]
                } else {
                    ["shared_handle", "owned_clone"]
                };
                for method in methods {
                    let (clone_ms, retained_objects) = if method == "owned_clone" {
                        clone_owned(&state, readers)
                    } else {
                        clone_shared(&state, readers)
                    };
                    println!(
                        "{}",
                        serde_json::json!({
                            "diagnostic":"observation_ownership", "version":1,
                            "cells":cells, "readers":readers, "sample":sample, "method":method,
                            "state_bytes":state_bytes, "retained_objects":retained_objects,
                            "distinct_state_serialized_bytes":state_bytes * retained_objects,
                            "clone_ms":clone_ms, "verified":true
                        })
                    );
                }
            }
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let ownership = args.iter().any(|arg| arg == "--ownership");
    let narration = args.iter().any(|arg| arg == "--narration");
    assert!(
        !(ownership && narration),
        "Choose either --ownership or --narration"
    );
    assert!(
        args.iter()
            .all(|arg| matches!(arg.as_str(), "--ownership" | "--narration")),
        "Unknown client workload argument"
    );
    if ownership {
        ownership_diagnostic();
        return;
    }
    // Keep the original workload unchanged; opt into disclosed actor/door churn.
    for cells in [64, 20_956] {
        for burst in [1, 64] {
            let mut app = App::new();
            app.set_state(ClientState::from_snapshot(snapshot(cells)).unwrap());
            let mut view = app.state.as_ref().unwrap().state().clone();
            view.observation.visible_cells.truncate(64);
            if narration {
                view.observation.visible_cells[0].door = Some(DoorView {
                    asset: None,
                    id: 1,
                    name: "wooden door".into(),
                    description: String::new(),
                    open: false,
                    reachable: true,
                    approaches: Vec::new(),
                });
            }
            let mut canvas = Canvas::default();
            for sample in 0..20 {
                let started = Instant::now();
                for _ in 0..burst {
                    let client = app.state.as_mut().unwrap();
                    view.revision += 1;
                    view.observation.tick += 1;
                    if narration {
                        let open = view.revision % 2 == 1;
                        view.observation.visible_cells[0]
                            .door
                            .as_mut()
                            .unwrap()
                            .open = open;
                        view.observation.visible_actors = if open {
                            vec![ActorView {
                                asset: None,
                                id: ActorId(2),
                                name: "figure".into(),
                                description: String::new(),
                                position: Position { x: 1, y: 0, z: 0 },
                            }]
                        } else {
                            Vec::new()
                        };
                    }
                    client
                        .apply(StreamUpdate {
                            context: fixture_context(),
                            actor: ActorId(1),
                            branch: client.branch().clone(),
                            cursor: StreamCursor {
                                sequence: client.cursor().sequence + 1,
                                tick: view.observation.tick,
                            },
                            body: UpdateBody::Observation {
                                state: view.clone().into(),
                                event: None,
                            },
                        })
                        .unwrap();
                }
                let apply_ms = started.elapsed().as_secs_f64() * 1000.;
                let started = Instant::now();
                canvas.draw(black_box(&app));
                black_box(&canvas.pixels);
                let render_ms = started.elapsed().as_secs_f64() * 1000.;
                let mut result = serde_json::json!({"version":if narration {2} else {1},"cells":cells,"burst":burst,"sample":sample,
                    "memory":app.state.as_ref().unwrap().memory().count(),
                    "chart":app.state.as_ref().unwrap().map_memory().count(),"apply_ms":apply_ms,"render_ms":render_ms});
                if narration {
                    let count = app.state.as_ref().unwrap().narration().len();
                    assert!(count > 0);
                    result["narration_count"] = count.into();
                }
                println!("{result}");
            }
        }
    }
}

/// Context for one synthetic attachment used by this fixture/workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn workload_snapshot_matches_the_current_protocol() {
        for count in [64, 4096, 20_956] {
            let snapshot = super::snapshot(count);
            assert!(snapshot.intentions.is_empty());
            assert!(snapshot.travel.is_none());
            assert_eq!(snapshot.state.observation.visible_cells.len(), count);
            snapshot.state.validate().unwrap();
        }
    }
}
