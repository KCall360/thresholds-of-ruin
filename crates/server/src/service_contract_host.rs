//! Thin, explicitly selected test adapter. Assertions and fixtures live in Python.
//! Uses production Service, package construction and encoded outbound frames.
use super::*;
use crate::scenario_package::{Manifest, Package, RegionDef};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

const HOST_VERSION: u32 = 1;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_CASE_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_CASE_CONNECTIONS: usize = 8;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Create {
        manifest: Box<Manifest>,
        regions: Vec<RegionDef>,
        rules: String,
        seed: u64,
        wizard: bool,
        frame_bytes: usize,
        client_bytes: usize,
        total_bytes: usize,
    },
    Connect {
        case: u64,
        role: AccessRole,
        user: String,
        actors: Vec<u64>,
    },
    Request {
        case: u64,
        connection: u64,
        message: String,
    },
    /// Exactly one production scheduler turn; never an implicit run loop.
    Step {
        case: u64,
    },
    Drain {
        case: u64,
        connection: u64,
    },
    Disconnect {
        case: u64,
        connection: u64,
    },
    Audit {
        case: u64,
        actor: u64,
        user: String,
    },
    Close {
        case: u64,
    },
}

struct Case {
    id: u64,
    service: Service,
    connections: BTreeMap<u64, Connection>,
}

impl Drop for Case {
    fn drop(&mut self) {
        self.service.shutdown();
    }
}

#[derive(Default)]
struct Host {
    next_case: u64,
    active: Option<Case>,
}

impl Host {
    fn case(&mut self, id: u64) -> Result<&mut Case, String> {
        self.active
            .as_mut()
            .filter(|case| case.id == id)
            .ok_or_else(|| "No matching active case".into())
    }

    fn handle(&mut self, operation: Operation) -> Result<Value, String> {
        match operation {
            Operation::Create {
                manifest,
                regions,
                rules,
                seed,
                wizard,
                frame_bytes,
                client_bytes,
                total_bytes,
            } => {
                // Require explicit close: an accidental second create cannot erase
                // a failed case's evidence or leak its clients into another case.
                if self.active.is_some() {
                    return Err("Close the active case before creating another".into());
                }
                if total_bytes > MAX_CASE_OUTPUT_BYTES {
                    return Err("Host case output budget exceeds 512 KiB".into());
                }
                let package = Package::from_parts(*manifest, regions, &rules)
                    .map_err(|error| error.to_string())?;
                let mut engine = Engine::memory(crate::Scenario {
                    actors: vec![],
                    package: Some(Arc::new(package)),
                    streaming: Some(crate::Streaming::default()),
                    ..crate::Scenario::two_room(seed)
                })
                .map_err(|error| error.to_string())?;
                if wizard {
                    engine.enable_wizard().map_err(|error| error.to_string())?;
                }
                let service = Service::with_outbound_limits(
                    engine,
                    crate::OutboundLimits {
                        frame_bytes,
                        client_bytes,
                        total_bytes,
                    },
                )
                .map_err(|error| error.to_string())?;
                self.next_case = self
                    .next_case
                    .checked_add(1)
                    .ok_or("Case identity exhausted")?;
                self.active = Some(Case {
                    id: self.next_case,
                    service,
                    connections: BTreeMap::new(),
                });
                Ok(json!({"case": self.next_case}))
            }
            Operation::Connect {
                case,
                role,
                user,
                actors,
            } => {
                let case = self.case(case)?;
                if case.connections.len() >= MAX_CASE_CONNECTIONS
                    || actors.len() > MAX_CASE_CONNECTIONS
                {
                    return Err("Host case connection/actor limit exceeded".into());
                }
                if !crate::engine::valid_label(&user) {
                    return Err("Invalid host account label".into());
                }
                let connection = case
                    .service
                    .connect(
                        &Account {
                            role,
                            user,
                            token: "contract-host-only".into(),
                            actors: actors.into_iter().map(ActorId).collect(),
                        },
                        "contract-host".into(),
                    )
                    .map_err(|error| error.to_string())?;
                let id = connection.id;
                case.connections.insert(id, connection);
                Ok(json!({"connection": id}))
            }
            Operation::Request {
                case,
                connection,
                message,
            } => {
                let ClientMessage::Request {
                    request_id,
                    request,
                } = decode_request(&message).map_err(|error| error.to_string())?
                else {
                    return Err("Host request operation requires a Request envelope".into());
                };
                let case = self.case(case)?;
                if !case.service.clients.contains_key(&connection) {
                    return Err("No live connection".into());
                }
                case.service.handle(connection, request_id, *request);
                Ok(json!({}))
            }
            Operation::Step { case } => {
                let status = match self.case(case)?.service.step() {
                    Step::Progress => "progress",
                    Step::Blocked => "blocked",
                    Step::Full(_) => "full",
                };
                Ok(json!({"status": status}))
            }
            Operation::Drain { case, connection } => {
                let case = self.case(case)?;
                let connection = case
                    .connections
                    .get_mut(&connection)
                    .ok_or("Unknown connection")?;
                let mut frames = Vec::new();
                while let Ok(frame) = connection.messages.try_recv_frame() {
                    // Preserve the exact production serialization, including escaping.
                    frames.push(validated_frame(&frame.text)?);
                }
                let closing = match *connection.close.borrow() {
                    None => None,
                    Some(DisconnectMode::Abort) => Some("abort"),
                    Some(DisconnectMode::Drain) => Some("drain"),
                };
                Ok(json!({"frames": frames, "closing": closing,
                    "live": case.service.clients.contains_key(&connection.id)}))
            }
            Operation::Disconnect { case, connection } => {
                let case = self.case(case)?;
                if !case.connections.contains_key(&connection) {
                    return Err("Unknown connection".into());
                }
                case.service.disconnect(connection);
                Ok(json!({}))
            }
            Operation::Audit { case, actor, user } => {
                // Trusted test observation, not a client message or gameplay API.
                // No generic mutation or arbitrary expression evaluation is exposed.
                let engine = &self.case(case)?.service.engine;
                let state = engine
                    .state(ActorId(actor))
                    .map_err(|error| error.to_string())?;
                let history = engine
                    .history(ActorId(actor), &user, None, 100)
                    .map_err(|error| error.to_string())?;
                Ok(json!({"state": state, "history": history}))
            }
            Operation::Close { case } => {
                self.case(case)?;
                self.active = None;
                Ok(json!({}))
            }
        }
    }
}

/// Serve one JSON request/response per line. EOF drops the active isolated case.
/// Host errors are separate from ordinary protocol rejection messages.
pub fn run(mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    writeln!(
        output,
        "{}",
        json!({"host_version": HOST_VERSION,
        "protocol": PROTOCOL_VERSION, "feature": "service-contract-host",
        "package_version": env!("CARGO_PKG_VERSION")})
    )?;
    output.flush()?;
    let mut host = Host::default();
    loop {
        let mut line = Vec::new();
        let length = input
            .by_ref()
            .take((MAX_INPUT_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if length == 0 {
            break;
        }
        if length > MAX_INPUT_BYTES {
            writeln!(
                output,
                "{}",
                json!({"error": "Host input exceeds 1 MiB; active case closed"})
            )?;
            output.flush()?;
            return Ok(());
        }
        let result = serde_json::from_slice::<Operation>(&line)
            .map_err(|error| error.to_string())
            .and_then(|operation| host.handle(operation));
        let response = match result {
            Ok(value) => json!({"ok": value}),
            Err(error) => json!({"error": error}),
        };
        match encode_bounded_json(&response, MAX_OUTPUT_BYTES) {
            Ok(encoded) => writeln!(output, "{encoded}")?,
            Err(_) => {
                // Drained frames must never disappear behind a successful envelope.
                // Fail the entire host/case explicitly rather than truncate output.
                writeln!(
                    output,
                    "{}",
                    json!({"error": "Host output exceeds 2 MiB; active case closed"})
                )?;
                output.flush()?;
                return Ok(());
            }
        }
        output.flush()?;
    }
    Ok(())
}

// Decode with the real client-side protocol contract, but return the original
// serialization: re-encoding would conceal malformed envelopes and byte costs.
fn validated_frame(text: &str) -> Result<String, String> {
    tor_protocol::decode_response(text)
        .map_err(|error| format!("Invalid production outbound frame: {error}"))?;
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_operation_is_one_strict_case_scoped_request() {
        assert!(matches!(
            serde_json::from_value::<Operation>(json!({"op":"step","case":1})).unwrap(),
            Operation::Step { case: 1 }
        ));
        for value in [
            json!({"op":"step"}),
            json!({"op":"step","case":"1"}),
            json!({"op":"step","case":1,"count":2}),
            json!({"op":"step","case":1,"actor":1}),
        ] {
            assert!(serde_json::from_value::<Operation>(value).is_err());
        }
        let mut host = Host::default();
        assert_eq!(
            host.handle(Operation::Step { case: 1 }).unwrap_err(),
            "No matching active case"
        );
    }

    #[test]
    fn step_reports_outbound_full_without_draining_or_advancing_the_case() {
        let service = Service::with_outbound_limits(
            Engine::memory(crate::Scenario::two_room(42)).unwrap(),
            crate::OutboundLimits {
                frame_bytes: 16_384,
                client_bytes: 262_144,
                total_bytes: 524_288,
            },
        )
        .unwrap();
        let mut host = Host {
            next_case: 1,
            active: Some(Case {
                id: 1,
                service,
                connections: BTreeMap::new(),
            }),
        };
        let connected = host
            .handle(Operation::Connect {
                case: 1,
                role: AccessRole::Player,
                user: "player".into(),
                actors: vec![1],
            })
            .unwrap();
        let connection = connected["connection"].as_u64().unwrap();
        host.handle(Operation::Request {
            case: 1,
            connection,
            message: json!({"type":"request","request_id":"attach",
                "request":{"type":"attach","actor":"1"}})
            .to_string(),
        })
        .unwrap();
        host.handle(Operation::Drain {
            case: 1,
            connection,
        })
        .unwrap();
        let before = host
            .handle(Operation::Audit {
                case: 1,
                actor: 1,
                user: "player".into(),
            })
            .unwrap();
        assert_eq!(
            host.handle(Operation::Step { case: 2 }).unwrap_err(),
            "No matching active case"
        );
        // Small read-only replies reach mailbox headroom without exhausting the
        // independently bounded byte budget or disconnecting this client.
        for index in 0..QUEUE - HEADROOM + 1 {
            host.handle(Operation::Request {
                case: 1,
                connection,
                message: json!({"type":"request","request_id":format!("fill-{index}"),
                    "request":{"type":"history","before":null,"limit":1}})
                .to_string(),
            })
            .unwrap();
        }
        for _ in 0..2 {
            assert_eq!(
                host.handle(Operation::Step { case: 1 }).unwrap(),
                json!({"status":"full"})
            );
            assert_eq!(
                host.handle(Operation::Audit {
                    case: 1,
                    actor: 1,
                    user: "player".into()
                })
                .unwrap(),
                before
            );
        }
        let drained = host
            .handle(Operation::Drain {
                case: 1,
                connection,
            })
            .unwrap();
        assert_eq!(drained["live"], true);
        assert!(drained["closing"].is_null());
        assert_eq!(
            drained["frames"].as_array().unwrap().len(),
            QUEUE - HEADROOM + 1
        );
        for frame in drained["frames"].as_array().unwrap() {
            let response: Value = serde_json::from_str(frame.as_str().unwrap()).unwrap();
            assert_eq!(response["type"], "history");
        }
        assert_eq!(
            host.handle(Operation::Step { case: 1 }).unwrap(),
            json!({"status":"blocked"})
        );
        assert_eq!(
            host.handle(Operation::Audit {
                case: 1,
                actor: 1,
                user: "player".into()
            })
            .unwrap(),
            before
        );
    }

    fn catalog() -> Value {
        json!({"type":"creature_catalog", "request_id":"query",
            "context":{"input":{"stream":{"stream":"stream", "epoch":"1"},
                "readiness_revision":"1"}, "actor":"1", "branch":"branch",
                "cursor":{"sequence":"0", "tick":"0"}, "revision":"0"},
            "page":{"depth":null, "entries":[
                {"selector":"archetype:a", "name":"A", "details":"Authored"},
                {"selector":"archetype:b", "name":"B", "details":"Authored"}],
                "next":"archetype:b"}})
    }

    #[test]
    fn validation_preserves_raw_whitespace_and_escaping() {
        let mut value = catalog();
        value["page"]["entries"][0]["name"] = json!("A \"quoted\" name");
        let raw = format!("  {}  ", serde_json::to_string_pretty(&value).unwrap());
        assert_eq!(validated_frame(&raw).unwrap(), raw);
    }

    #[test]
    fn validation_rejects_invalid_catalog_order_cursor_and_envelope() {
        let mut reversed = catalog();
        reversed["page"]["entries"]
            .as_array_mut()
            .unwrap()
            .reverse();
        let mut cursor = catalog();
        cursor["page"]["next"] = json!("archetype:a");
        let mut envelope = catalog();
        envelope["unexpected"] = json!(true);
        for invalid in [reversed, cursor, envelope] {
            assert!(validated_frame(&invalid.to_string())
                .unwrap_err()
                .contains("Invalid production outbound frame"));
        }
        assert!(validated_frame("not json").is_err());
    }
}
