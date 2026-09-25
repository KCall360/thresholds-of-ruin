//! Version 1 disclosed client workload: historical memory, bounded chart and bursts.
use std::{hint::black_box, time::Instant};
use tor_client_ascii::{render::Canvas, App};
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(count: usize) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"client-bench","cursor":{"sequence":0,"tick":0},
        "has_control":true,"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":0,"observation":{
            "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},
            "visible_cells":(0..count).map(|i| serde_json::json!({
                "key":i.to_string(),"position":{"x":(i%49) as i32-24,"y":((i/49)%25) as i32-12,"z":(i/1225) as i32},
                "wall":i%7==0,"stairs_up":false,"stairs_down":false,"place_hint":false
            })).collect::<Vec<_>>(),"ground_items":[],"inventory":[],"visible_actors":[],"ready":true
        }}
    })).unwrap()
}

fn main() {
    for cells in [64, 20_956] {
        for burst in [1, 64] {
            let mut app = App::new();
            app.set_state(ClientState::from_snapshot(snapshot(cells)).unwrap());
            let mut view = app.state.as_ref().unwrap().state().clone();
            view.observation.visible_cells.truncate(64);
            let mut canvas = Canvas::default();
            for sample in 0..20 {
                let started = Instant::now();
                for _ in 0..burst {
                    let client = app.state.as_mut().unwrap();
                    view.revision += 1;
                    view.observation.tick += 1;
                    client
                        .apply(StreamUpdate {
                            actor: ActorId(1),
                            branch: client.branch().clone(),
                            cursor: StreamCursor {
                                sequence: client.cursor().sequence + 1,
                                tick: view.observation.tick,
                            },
                            body: UpdateBody::Observation {
                                state: Box::new(view.clone()),
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
                println!(
                    "{}",
                    serde_json::json!({"version":1,"cells":cells,"burst":burst,"sample":sample,
                    "memory":app.state.as_ref().unwrap().memory().count(),
                    "chart":app.state.as_ref().unwrap().map_memory().count(),"apply_ms":apply_ms,"render_ms":render_ms})
                );
            }
        }
    }
}
