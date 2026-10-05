//! Versioned place discovery, rename, delivery and recovery diagnostics.
use serde::Deserialize;
use serde_json::json;
use std::time::Instant;
use tor_client_ascii::{render::Canvas, App};
use tor_client_common::ClientState;
use tor_protocol::*;
use tor_server::{journal, Engine, SavePolicy, Scenario};

#[derive(Deserialize)]
struct Spec {
    version: u32,
    seed: u64,
    extra_rooms: Vec<u64>,
    hints: Vec<[i32; 3]>,
    samples: usize,
    checkpoint_interval: u64,
}
fn wizard(engine: &mut Engine, id: &str, operation: String) -> tor_server::CommandProfile {
    let command = journal::Command::from_wire(&Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation,
    })
    .unwrap();
    engine
        .command_profiled(
            "bench",
            "places",
            ActorId(1),
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
        .1
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn main() {
    let spec: Spec =
        serde_json::from_str(include_str!("../fixtures/place-knowledge-v1.json")).unwrap();
    assert_eq!(spec.version, 1);
    let directory = tempfile::tempdir().unwrap();
    for extra_rooms in &spec.extra_rooms {
        let mut engine = Engine::memory(Scenario::two_room(spec.seed)).unwrap();
        engine.enable_wizard().unwrap();
        for room in 0..*extra_rooms {
            let region = room + 3;
            wizard(
                &mut engine,
                &format!("room-{room}"),
                format!("room {region} 5 5 1 Private"),
            );
            for (index, [x, y, z]) in spec.hints.iter().enumerate() {
                wizard(
                    &mut engine,
                    &format!("hint-{room}-{index}"),
                    format!("place {region} {x} {y} {z} on"),
                );
            }
            let profile = wizard(
                &mut engine,
                &format!("visit-{room}"),
                format!("teleport 1 {region} 0 0 0"),
            );
            println!(
                "{}",
                json!({"version":1,"kind":"discovery","extra_rooms":extra_rooms,"room":room,
                "places":engine.observation(ActorId(1)).unwrap().places.len(),
                "command_ms":profile.authoritative_total.as_secs_f64()*1000.,
                "navigation_ms":profile.navigation_refresh.as_secs_f64()*1000.})
            );
        }
        wizard(&mut engine, "return", "teleport 1 1 1 1 0".into());
        let count = engine.observation(ActorId(1)).unwrap().places.len();
        assert_eq!(count, 2 + *extra_rooms as usize * spec.hints.len());
        let path = directory.path().join(format!("places-{extra_rooms}.db"));
        let mut engine = engine
            .attach_profile_save_with_policy(
                &path,
                SavePolicy {
                    checkpoint_interval: spec.checkpoint_interval,
                    ..SavePolicy::default()
                },
            )
            .unwrap();
        let state = engine.state(ActorId(1)).unwrap();
        let key = state.observation.places.last().unwrap().key.clone();
        let mut app = App::new();
        app.set_state(
            ClientState::from_snapshot(Snapshot {
                intentions: Vec::new(),
                actor: ActorId(1),
                branch: engine.branch().clone(),
                cursor: StreamCursor {
                    sequence: 0,
                    tick: state.observation.tick,
                },
                state,
                has_control: true,
                history: HistoryPage {
                    entries: vec![],
                    older_before: None,
                },
                travel: None,
            })
            .unwrap(),
        );
        app.places_open = true;
        let mut canvas = Canvas::default();
        for sample in 0..spec.samples {
            let rename = sample % 2 == 0;
            let revision = engine.revision(ActorId(1)).unwrap();
            let command = if rename {
                journal::Command::RenamePlace {
                    expected_revision: revision,
                    key: key.clone(),
                    name: format!("Reverie {sample}"),
                }
            } else {
                journal::Command::Act {
                    expected_revision: revision,
                    action: Action::Wait,
                }
            };
            let (entry, profile) = engine
                .command_profiled(
                    "bench",
                    "places",
                    ActorId(1),
                    &format!("sample-{sample}"),
                    &engine.branch().clone(),
                    command,
                )
                .unwrap();
            let start = Instant::now();
            let state = engine.state(ActorId(1)).unwrap();
            let observation_ms = ms(start);
            let update = StreamUpdate {
                actor: ActorId(1),
                branch: engine.branch().clone(),
                cursor: StreamCursor {
                    sequence: sample as u64 + 1,
                    tick: state.observation.tick,
                },
                body: UpdateBody::Observation {
                    state: Box::new(state),
                    event: Some(Box::new(
                        entry
                            .entry
                            .disclosed()
                            .expect("completed command has history"),
                    )),
                },
            };
            let start = Instant::now();
            let wire_bytes = serde_json::to_vec(&update).unwrap().len();
            let encode_ms = ms(start);
            let start = Instant::now();
            app.update(update).unwrap();
            let apply_ms = ms(start);
            let start = Instant::now();
            canvas.draw(&app);
            let render_ms = ms(start);
            println!(
                "{}",
                json!({"version":1,"kind":"sample","extra_rooms":extra_rooms,"places":count,"sample":sample,
                "label":if rename {"rename"} else {"wait"},"command_ms":profile.authoritative_total.as_secs_f64()*1000.,
                "observation_ms":observation_ms,"encode_ms":encode_ms,"apply_ms":apply_ms,"render_ms":render_ms,
                "wire_bytes":wire_bytes,"records":profile.records_serialized,"navigation_refreshes":profile.navigation_refreshes})
            );
        }
        engine.flush().unwrap();
        let expected = engine.state(ActorId(1)).unwrap();
        let checkpoint_bytes = engine.save_status().checkpoint_bytes;
        let save_bytes = std::fs::metadata(&path).unwrap().len();
        drop(engine);
        let start = Instant::now();
        let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
        let restart_ms = ms(start);
        assert_eq!(expected, restored.state(ActorId(1)).unwrap());
        println!(
            "{}",
            json!({"version":1,"kind":"recovery","extra_rooms":extra_rooms,"places":count,
            "checkpoint_bytes":checkpoint_bytes,"save_bytes":save_bytes,"restart_ms":restart_ms,"exact":true,
            "profile":if cfg!(debug_assertions) {"debug"} else {"release"},"platform":std::env::consts::OS})
        );
    }
}
