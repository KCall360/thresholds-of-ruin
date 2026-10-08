//! Same-build full-versus-selected observation diagnostic. Synthetic disclosed
//! collections vary individually; this does not measure world topology or UI.
use std::sync::Arc;
use std::{hint::black_box, time::Instant};
use tor_protocol::*;

// Synthetic disclosed identities for wire-only tests; not server target derivation.
fn synthetic_digest(index: u64) -> [u8; 32] {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&index.to_le_bytes());
    digest
}

fn views(count: usize, case: &str) -> (StateView, StateView) {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/wire-v32.json")).unwrap();
    assert_eq!(fixture["protocol"], PROTOCOL_VERSION);
    let message = fixture["server"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["type"] == "snapshot")
        .unwrap()
        .clone();
    let ServerMessage::Snapshot { snapshot, .. } = serde_json::from_value(message).unwrap() else {
        unreachable!()
    };
    let mut base = Arc::unwrap_or_clone(snapshot.state);
    base.revision = 1;
    let o = &mut base.observation;
    o.tick = 1;
    o.combat = None;
    o.motion = None;
    o.places.clear();
    o.inventory.clear();
    o.ground_items.clear();
    o.visible_actors.clear();
    for index in 0..count {
        let item = ItemView {
            class: Default::default(),
            quantity: 1,
            appearance: "disclosed appearance".repeat(4),
            identified: true,
            description: "disclosed description".repeat(4),
            id: ItemTarget::from_digest(synthetic_digest(index as u64 + 1)),
            name: format!("item {index}"),
            asset: None,
        };
        o.inventory.push(item.clone());
        o.ground_items.push(GroundItemView {
            reachable: false,
            item: ItemView {
                id: ItemTarget::from_digest(synthetic_digest(index as u64 + 10_000)),
                ..item
            },
            position: Position {
                x: index as i32,
                y: 1,
                z: 0,
            },
        });
        o.visible_actors.push(ActorView {
            name: format!("actor {index}"),
            description: "disclosed actor".repeat(4),
            id: ActorTarget::from_digest(synthetic_digest(index as u64 + 10)),
            position: Position {
                x: index as i32,
                y: 2,
                z: 0,
            },
            asset: None,
        });
        o.places.push(PlaceView {
            key: format!("opaque-place-{index}"),
            name: "remembered place".repeat(4),
            origin: PlaceNameOrigin::Invented,
        });
    }
    base.validate().unwrap();
    let mut next = base.clone();
    next.revision += 1;
    next.observation.tick += 1;
    match case {
        "unchanged" => {}
        "sparse" => {
            for index in [0, count / 2, count - 1] {
                next.observation.inventory[index].quantity += 1;
                next.observation.ground_items[index].reachable = true;
                next.observation.visible_actors[index].description.push('!');
                next.observation.places[index].name.push('!');
            }
        }
        "projected_movement" => {
            for item in &mut next.observation.ground_items {
                item.position.x -= 1;
            }
            for actor in &mut next.observation.visible_actors {
                actor.position.x -= 1;
            }
        }
        "reorder" => {
            next.observation.inventory.rotate_left(count / 2);
            next.observation.ground_items.rotate_left(count / 2);
            next.observation.visible_actors.rotate_left(count / 2);
            next.observation.places.rotate_left(count / 2);
        }
        _ => panic!("unknown case"),
    }
    next.validate().unwrap();
    (base, next)
}

fn message(state: StateView) -> ServerMessage {
    ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: StreamContext {
                stream: StreamId("diagnostic".into()),
                epoch: 1,
            },
            actor: state.observation.actor,
            branch: BranchId("diagnostic".into()),
            cursor: StreamCursor {
                sequence: 2,
                tick: state.observation.tick,
            },
            body: UpdateBody::Observation {
                state: state.into(),
                event: None,
            },
        }),
    }
}

fn reconstruct(message: ServerMessage, base: &StateView) -> StateView {
    let ServerMessage::Update { update } = message else {
        panic!("update")
    };
    let state = match update.body {
        UpdateBody::Observation { state, .. } => Arc::unwrap_or_clone(state),
        UpdateBody::ObservationDelta { state, .. } => state.apply(base).unwrap(),
        _ => panic!("observation"),
    };
    state.validate().unwrap();
    state
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() <= 1,
        "usage: observation_wire_bench [samples-per-method]"
    );
    let samples: usize = args
        .first()
        .map(|arg| arg.parse().expect("sample count"))
        .unwrap_or(100);
    assert!((1..=10_000).contains(&samples));
    println!(
        "{}",
        serde_json::json!({
            "kind":"observation_wire_header", "version":1, "protocol":PROTOCOL_VERSION,
            "samples_per_method":samples, "collections_per_case":4,
            "scope":"complete encoding; bounded decode; reconstruction and validation; network/context tracking/UI excluded",
            "comparison":"same executable and observations; full response versus selected response",
        })
    );
    for count in [16, 256, 4096] {
        for case in ["unchanged", "sparse", "projected_movement", "reorder"] {
            let (base, next) = views(count, case);
            let expected = message(next.clone());
            let identity = ObservationBase {
                cursor: StreamCursor {
                    sequence: 1,
                    tick: base.observation.tick,
                },
                revision: base.revision,
            };
            let candidate = StateDelta::between(&base, &next).unwrap();
            let inserted = candidate
                .inventory
                .iter()
                .map(|e| e.insert.len())
                .sum::<usize>()
                + candidate
                    .ground_items
                    .iter()
                    .map(|e| e.insert.len())
                    .sum::<usize>()
                + candidate
                    .visible_actors
                    .iter()
                    .map(|e| e.insert.len())
                    .sum::<usize>()
                + candidate
                    .places
                    .iter()
                    .map(|e| e.insert.len())
                    .sum::<usize>();
            for sample in 0..samples {
                for selected in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = Instant::now();
                    let encoded = encode_response(
                        black_box(&expected),
                        selected.then_some((identity, &base)),
                        MAX_RESPONSE_BYTES,
                    )
                    .unwrap();
                    let encode_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let start = Instant::now();
                    let decoded = decode_response(black_box(&encoded.text)).unwrap();
                    let decode_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let start = Instant::now();
                    let actual = reconstruct(decoded, &base);
                    let apply_validate_ms = start.elapsed().as_secs_f64() * 1000.0;
                    assert_eq!(actual, next);
                    black_box(actual);
                    println!(
                        "{}",
                        serde_json::json!({
                            "kind":"observation_wire_sample", "version":1, "case":case,
                            "entries_per_collection":count, "sample":sample,
                            "method":if selected {"selected"} else {"full"},
                            "delta":encoded.observation == Some(ObservationEncoding::Delta),
                            "wire_bytes":encoded.text.len(), "candidate_inserted_values":inserted,
                            "encode_ms":encode_ms, "decode_ms":decode_ms,
                            "apply_validate_ms":apply_validate_ms,
                        })
                    );
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"kind":"observation_wire_end", "version":1,
        "cases":12, "samples":samples * 2 * 12})
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn individual_collection_cases_fit_and_reconstruct_through_the_shared_codec() {
        for count in [16, 256, 4096] {
            for case in ["unchanged", "sparse", "projected_movement", "reorder"] {
                let (base, next) = views(count, case);
                for selected in [false, true] {
                    let previous = ObservationBase {
                        cursor: StreamCursor {
                            sequence: 1,
                            tick: base.observation.tick,
                        },
                        revision: base.revision,
                    };
                    let encoded = encode_response(
                        &message(next.clone()),
                        selected.then_some((previous, &base)),
                        MAX_RESPONSE_BYTES,
                    )
                    .unwrap();
                    assert!(encoded.text.len() <= MAX_RESPONSE_BYTES);
                    assert_eq!(
                        reconstruct(decode_response(&encoded.text).unwrap(), &base),
                        next
                    );
                }
            }
        }
    }
}
