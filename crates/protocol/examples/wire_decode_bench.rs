//! Same-build diagnostic: typed Serde parsing versus the bounded wire decoder.
//! Includes parsing/DTO construction, excludes validation, drop, network and UI.
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::{fmt::Debug, hint::black_box, time::Instant};
use tor_protocol::*;

fn measure<T: DeserializeOwned + Serialize + PartialEq + Debug>(
    case: &str,
    expected: &T,
    bounded: impl Fn(&str) -> Result<T, DecodeError>,
    samples: usize,
    cells: usize,
) {
    let text = serde_json::to_string(expected).unwrap();
    for sample in 0..samples {
        // Alternate which method runs first to expose order/cache effects.
        for guarded in if sample % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = Instant::now();
            let parsed = if guarded {
                bounded(black_box(&text)).unwrap()
            } else {
                serde_json::from_str::<T>(black_box(&text)).unwrap()
            };
            let decode_ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(&parsed, expected, "{case}: decoder changed the payload");
            black_box(parsed); // Equality and DTO destruction are outside the interval.
            println!(
                "{}",
                serde_json::json!({
                    "kind":"wire_decode_sample", "version":1, "case":case,
                    "sample":sample, "method":if guarded {"bounded"} else {"serde"},
                    "wire_bytes":text.len(), "visible_cells":cells, "decode_ms":decode_ms,
                })
            );
        }
    }
}

fn snapshot(cells: usize) -> ServerMessage {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/wire-v30.json")).unwrap();
    assert_eq!(fixtures["protocol"], PROTOCOL_VERSION);
    let fixture = fixtures["server"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["type"] == "snapshot")
        .unwrap()
        .clone();
    let ServerMessage::Snapshot {
        request_id,
        mut snapshot,
    } = serde_json::from_value::<ServerMessage>(fixture).unwrap()
    else {
        unreachable!()
    };
    let mut cell = snapshot.state.observation.visible_cells[0].clone();
    cell.door = None;
    // Synthetic disclosed positions, not a world geometry or topology benchmark.
    Arc::make_mut(&mut snapshot.state).observation.visible_cells = (0..cells)
        .map(|index| {
            let mut cell = cell.clone();
            cell.key = format!("decode-cell-{index}");
            cell.position = Position {
                x: index.try_into().unwrap(),
                y: 0,
                z: 0,
            };
            cell
        })
        .collect();
    ServerMessage::Snapshot {
        request_id,
        snapshot,
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() <= 1,
        "usage: wire_decode_bench [samples-per-method]"
    );
    let samples: usize = args
        .first()
        .map(|arg| arg.parse().expect("positive sample count"))
        .unwrap_or(200);
    assert!(
        (1..=10_000).contains(&samples),
        "sample count must be 1..=10000"
    );
    println!(
        "{}",
        serde_json::json!({
            "kind":"wire_decode_header", "version":1, "protocol":PROTOCOL_VERSION,
            "samples_per_method":samples, "max_depth":MAX_JSON_DEPTH,
            "scope":"typed decode including DTO construction; validation/drop/network/UI excluded",
            "comparison":"same executable and payload; Serde reference versus bounded decoder",
        })
    );
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: "sample-not-a-secret".into(),
        frontend: "decode-bench".into(),
    };
    measure("hello", &hello, decode_request, samples, 0);
    let mut large = hello.clone();
    let overhead = serde_json::to_string(&large).unwrap().len();
    if let ClientMessage::Hello { token, .. } = &mut large {
        token.push_str(&"x".repeat(MAX_REQUEST_BYTES - overhead));
    }
    assert_eq!(
        serde_json::to_string(&large).unwrap().len(),
        MAX_REQUEST_BYTES
    );
    // This exercises the codec ceiling, not account/frontend authorization.
    measure("request-byte-ceiling", &large, decode_request, samples, 0);
    for cells in [8, 256, 4096] {
        measure(
            &format!("snapshot-{cells}"),
            &snapshot(cells),
            decode_response,
            samples,
            cells,
        );
    }
    println!(
        "{}",
        serde_json::json!({"kind":"wire_decode_end", "version":1,
        "cases":5, "samples":samples * 2 * 5})
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_disclosed_payloads_round_trip_both_decoders_and_fit_response_limits() {
        for cells in [8, 256, 4096] {
            let expected = snapshot(cells);
            let text = serde_json::to_string(&expected).unwrap();
            assert!(text.len() < MAX_RESPONSE_BYTES);
            assert_eq!(decode_response(&text).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<ServerMessage>(&text).unwrap(),
                expected
            );
            let ServerMessage::Snapshot { snapshot, .. } = expected else {
                unreachable!()
            };
            assert_eq!(snapshot.state.observation.visible_cells.len(), cells);
            assert!(snapshot
                .state
                .observation
                .visible_cells
                .windows(2)
                .all(|pair| pair[0].position < pair[1].position));
        }
    }
}
