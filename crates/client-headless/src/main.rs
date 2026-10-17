//! JSON-lines frontend for scripted play and disclosed-state acceptance tests.
use serde::Deserialize;
use std::{
    io::{self, BufRead, Write},
    net::SocketAddr,
    time::Duration,
};
use tokio::{sync::mpsc, time::timeout};
use tor_client_common::Connection;
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// Presentation-only projection; the shared client always retains full memory.
#[derive(Clone, Copy, Default)]
struct DiagnosticOutput {
    omit_memory: bool,
}

impl DiagnosticOutput {
    fn project(
        self,
        mut fields: serde_json::Value,
        state: &tor_client_common::ClientState,
    ) -> serde_json::Value {
        if !self.omit_memory {
            fields["memory"] = diagnostic_memory(state);
        }
        fields["map_hydration"] = serde_json::json!(state.map_hydration());
        fields
    }
}

fn diagnostic_memory(state: &tor_client_common::ClientState) -> serde_json::Value {
    let cells: Vec<_> = state
        .memory()
        .map(|cell| {
            let mut value = serde_json::json!(cell);
            // Preserve the harmless diagnostic item wrapper, without stale targets
            // or affordances. The item's position is its owning cell's last sighting.
            value["ground_items"] = serde_json::json!(cell
                .ground_items
                .iter()
                .map(|item| serde_json::json!({"item":item,"position":cell.position}))
                .collect::<Vec<_>>());
            value
        })
        .collect();
    serde_json::json!(cells)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    /// The same preview/confirm navigation used by the ordinary Text/ASCII UI.
    Advancement {
        command: String,
    },
    ResumeIntention {},
    /// An omitted or null step cancels in place; a direction requests atomic escape.
    CancelIntention {
        step: Option<Direction>,
    },
    Act {
        action: Action,
    },
    /// A developer command, in the text client's `wizard` form; the current
    /// branch and revision are supplied, as for `act`.
    Wizard {
        command: String,
    },
    Request {
        request: Request,
    },
    Inspect,
    Quit,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        // JSON escaping keeps diagnostics safe even for malformed remote text.
        eprintln!(
            "{}",
            serde_json::json!({"type":"fatal", "error":error.to_string()})
        );
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Error> {
    let mut address: SocketAddr = "127.0.0.1:4000".parse()?;
    let mut actor = ActorId(1);
    let mut observe = false;
    let mut output = DiagnosticOutput::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("tor-client-headless [--connect 127.0.0.1:4000] [--actor 1] [--observe] [--omit-memory]\nSet TOR_SERVER_TOKEN. Send JSON lines: advancement, act, resume_intention, cancel_intention, wizard, request, inspect, quit. cancel_intention accepts an optional step direction after a completed strike.\n--omit-memory omits only accumulated memory from diagnostic JSON; current observations and client state are unchanged.\nSee docs/reference/headless.md for the input and output contract.");
                return Ok(());
            }
            "--connect" => address = args.next().ok_or("Missing --connect address")?.parse()?,
            "--actor" => actor = ActorId(args.next().ok_or("Missing --actor ID")?.parse()?),
            "--observe" => observe = true,
            "--omit-memory" => output.omit_memory = true,
            _ => return Err("Unknown command-line option".into()),
        }
    }
    let token = std::env::var("TOR_SERVER_TOKEN")
        .map_err(|_| "Set TOR_SERVER_TOKEN before starting the client")?;
    let mut connection = Connection::connect(address, token, actor, "headless").await?;
    let error = if connection.role() != AccessRole::Spectator && !observe {
        transact(&mut connection, Request::AcquireControl, output).await?
    } else {
        None
    };
    emit(&connection, "ready", None, error.as_deref(), output)?;

    let (tx, mut rx) = mpsc::channel(16);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let failed = line.is_err();
            if tx.blocking_send(line).is_err() || failed {
                break;
            }
        }
    });
    loop {
        tokio::select! {
            biased;
            ready = tor_client_common::server_before_local(connection.next(), rx.recv()) => {
                match ready {
                    tor_client_common::FirstReady::Server(message) => {
                        emit(&connection, "update", Some(&message?), None, output)?;
                    }
                    tor_client_common::FirstReady::Local(line) => {
                        let Some(line) = line else { break; };
                        let line = line?;
                        let input = serde_json::from_str::<Input>(&line);
                        if !connection.is_synchronized() && !matches!(input, Ok(Input::Quit | Input::Inspect)) {
                            emit(&connection, "ready", None, Some("Resynchronizing; input was not sent"), output)?;
                            continue;
                        }
                        let result = match input {
                            Ok(Input::Quit) => break,
                            Ok(Input::Inspect) => None,
                            Ok(Input::Advancement { command }) => {
                                match tor_client_common::advancement::parse(&command).and_then(|input| connection.advancement_pages.request(input, &connection.state)) {
                                    Ok(Some(request)) => transact(&mut connection, request, output).await?,
                                    Ok(None) => None,
                                    Err(error) => Some(error),
                                }
                            }
                            Ok(Input::ResumeIntention {}) => change_intention(&mut connection, true, None, output).await?,
                            Ok(Input::CancelIntention { step }) => change_intention(&mut connection, false, step, output).await?,
                            Ok(Input::Act { action }) => {
                                if connection.role() == AccessRole::Spectator {
                                    Some("Spectator access is read-only".into())
                                } else if !connection.state.has_control() {
                                    Some("Actor control is required".into())
                                } else if !connection.state.can_admit_intention() {
                                    Some("The server is not accepting another action".into())
                                } else {
                                    let request = connection.state.command_request(Command::Act {
                                        expected_revision: connection.state.state().revision,
                                        action,
                                    });
                                    transact(&mut connection, request, output).await?
                                }
                            }
                            Ok(Input::Wizard { command }) => {
                                // A developer command must not run into StaleRevision if an
                                // autonomous actor moved between observation delivery and
                                // applying it. Rats and travel can move the revision
                                // between this client's last look and the packet
                                // arriving, so resubmit that same unapplied operation
                                // at the revision the rejection disclosed.
                                let mut result = None;
                                for _ in 0..32 {
                                    let request = connection.state.command_request(Command::Wizard {
                                        expected_revision: connection.state.state().revision,
                                        operation: command.clone(),
                                    });
                                    result = transact(&mut connection, request, output).await?;
                                    if !wizard_still_unapplied(result.as_deref()) {
                                        break;
                                    }
                                }
                                result
                            }
                            Ok(Input::Request { request }) => transact(&mut connection, request, output).await?,
                            Err(_) => Some(
                                "Invalid input; expected a JSON advancement, act, resume_intention, cancel_intention, wizard, request, inspect or quit"
                                    .into(),
                            ),
                        };
                        emit(&connection, "ready", None, result.as_deref(), output)?;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    connection.close().await?;
    Ok(())
}

async fn change_intention(
    connection: &mut Connection,
    resume: bool,
    step: Option<Direction>,
    output: DiagnosticOutput,
) -> Result<Option<String>, Error> {
    if connection.role() == AccessRole::Spectator {
        return Ok(Some("Spectator access is read-only".into()));
    }
    if !connection.state.has_control() {
        return Ok(Some("Actor control is required".into()));
    }
    let request = if resume {
        connection.state.resume_intention_request()
    } else if let Some(step) = step {
        connection.state.cancel_intention_with_step_request(step)
    } else {
        connection.state.cancel_intention_request()
    };
    if let Some(request) = request {
        transact(connection, request, output).await
    } else {
        Ok(Some(
            if resume {
                "No suspended action to resume"
            } else if step.is_some() {
                "Escape requires a completed strike and cancellation permission"
            } else {
                "No queued action to cancel"
            }
            .into(),
        ))
    }
}

/// True only when the wizard operation was rejected and not applied.
/// A committed rewind, or any other error, must not be sent again.
fn wizard_still_unapplied(error: Option<&str>) -> bool {
    error.is_some_and(|error| error.starts_with("StaleRevision"))
}

async fn transact(
    connection: &mut Connection,
    request: Request,
    output: DiagnosticOutput,
) -> Result<Option<String>, Error> {
    if !connection.role().permits(&request) {
        return Ok(Some("Spectator access is read-only".into()));
    }
    let id = connection.request(request).await?;
    let mut pending = tor_client_common::PendingRequest::new(id);
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            emit(connection, "response", Some(&message), None, output)?;
            match pending.observe(connection, &message) {
                Some(tor_client_common::RequestCompletion::Reply(tor_client_common::ConfirmedReply::Rejected { code, message })) =>
                    return Ok(Some(format!("{code:?}: {message}"))),
                Some(tor_client_common::RequestCompletion::Reply(_)) => return Ok(None),
                Some(tor_client_common::RequestCompletion::Unknown) => return Ok(Some(
                    "State resynchronized; request outcome may be unknown. Inspect history before retrying.".into())),
                None => {},
            }
        }
    }).await.map_err(|_| "Server response timed out; outcome may be unknown. Inspect history after reconnecting before retrying.")?
}

fn emit(
    connection: &Connection,
    kind: &str,
    message: Option<&ServerMessage>,
    error: Option<&str>,
    output: DiagnosticOutput,
) -> Result<(), Error> {
    static TIMING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let timing = *TIMING.get_or_init(|| std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some());
    let started = timing.then(std::time::Instant::now);
    let fields = serde_json::json!({
        "type": kind,
        "advancement": connection.advancement_pages.display(&connection.state.state().observation),
        "advancement_page": connection.advancement_pages.page(),
        "synchronized": connection.is_synchronized(),
        "role": connection.role(),
        "capabilities": connection.capabilities(),
        "state": connection.state.state(),
        "branch": connection.state.branch(),
        "cursor": connection.state.cursor(),
        "has_control": connection.state.has_control(),
        "readiness": connection.state.readiness(),
        "input_context": connection.state.input_context(),
        "history": connection.state.history(),
        "travel": connection.state.travel(),
        "intentions": connection.state.intentions(),
        "palette": &connection.palette,
        "message": message,
        "error": error,
    });
    let output = output.project(fields, &connection.state);
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output)?;
    writeln!(stdout)?;
    stdout.flush()?;
    drop(stdout);
    if let Some(started) = started {
        let request_id = match message {
            Some(ServerMessage::Ack { request_id, .. }) => Some(request_id),
            _ => None,
        };
        eprintln!(
            "{}",
            serde_json::json!({"timing_version":1,"event":"headless_report",
            "kind":kind,"request_id":request_id,"duration_ms":started.elapsed().as_secs_f64()*1000.,
            "unix_ns":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()})
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    fn diagnostic_state(remembered_cells: i32) -> tor_client_common::ClientState {
        let mut observation = tor_client_common::test_support::observation();
        observation.visible_cells = (0..remembered_cells)
            .map(|x| CellView {
                key: format!("disclosed-{x}"),
                position: Position { x, y: 0, z: 0 },
                wall: false,
                stairs_up: false,
                stairs_down: false,
                place_hint: false,
                door: None,
                material: "stone".into(),
                asset: None,
            })
            .collect();
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "actor":"1", "branch":"first", "cursor":{"sequence":"0","tick":"0"},
            "context":{"stream":"3b7523b8-893a-4ea9-8b09-0a3887a7e6a1","epoch":"0"},
            "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},
            "state":{"wizard_game":false,"revision":"0","observation":observation},
            "has_control":false, "intentions":[], "travel":null,
            "history":{"entries":[],"older_before":null}
        }))
        .unwrap();
        let mut state = tor_client_common::ClientState::from_snapshot(snapshot).unwrap();
        let mut current = state.snapshot();
        current.context.epoch += 1;
        std::sync::Arc::make_mut(&mut current.state)
            .observation
            .visible_cells
            .truncate(1);
        state.replace_snapshot(current).unwrap();
        state
    }

    fn retained_diagnostic_fields(state: &tor_client_common::ClientState) -> serde_json::Value {
        serde_json::json!({
            "type":"response", "state":state.state(), "branch":state.branch(),
            "cursor":state.cursor(), "readiness":state.readiness(),
            "input_context":state.input_context(), "history":state.history(),
            "intentions":state.intentions(), "travel":state.travel(),
            "has_control":state.has_control(), "role":"player", "synchronized":true,
            "advancement":["Retained owner display"], "advancement_page":{"total":1},
            "capabilities":ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 8),
            "palette":tor_client_common::Palette::default(),
            "message":{"fixture":{"memory":"Retain nested fields too"}}, "error":null
        })
    }

    #[test]
    fn creature_catalog_reply_is_preserved_by_both_headless_projection_modes() {
        let state = diagnostic_state(2);
        let message: ServerMessage = serde_json::from_value(serde_json::json!({
            "type":"creature_catalog", "context":state.reply_context(), "request_id":"catalog",
            "page":{"depth":null,"entries":[{"selector":"fixture:imp","name":"Imp","details":"Authored base"}],"next":null}
        })).unwrap();
        let fields = serde_json::json!({"type":"response", "message":message});
        for omit_memory in [false, true] {
            let result = DiagnosticOutput { omit_memory }.project(fields.clone(), &state);
            assert_eq!(result["message"], fields["message"]);
            assert_eq!(result["message"]["request_id"], "catalog");
        }
    }

    #[test]
    fn diagnostic_projection_preserves_default_fields_and_accumulated_client_memory() {
        let state = diagnostic_state(512);
        let before = state.clone();
        let retained = retained_diagnostic_fields(&state);
        let full = DiagnosticOutput::default().project(retained.clone(), &state);
        let projected = DiagnosticOutput { omit_memory: true }.project(retained.clone(), &state);
        let memory = diagnostic_memory(&state);
        let mut expected_full = retained.clone();
        expected_full["memory"] = memory;
        expected_full["map_hydration"] = serde_json::json!(state.map_hydration());
        assert_eq!(full, expected_full);
        let mut expected_projected = retained.clone();
        expected_projected["map_hydration"] = serde_json::json!(state.map_hydration());
        assert_eq!(projected, expected_projected);
        assert_eq!(full["memory"].as_array().unwrap().len(), 512);
        assert_eq!(
            projected["state"]["observation"]["visible_cells"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(state, before, "Projection must not change any client state");
        assert_eq!(
            DiagnosticOutput::default().project(retained, &state),
            full,
            "A projected report must not discard future full-memory output"
        );
    }

    #[test]
    fn diagnostic_projection_serialized_bytes_exclude_only_accumulated_memory() {
        let mut projected_sizes = Vec::new();
        let mut full_sizes = Vec::new();
        for remembered_cells in [1, 512] {
            let state = diagnostic_state(remembered_cells);
            let retained = retained_diagnostic_fields(&state);
            let full = DiagnosticOutput::default().project(retained.clone(), &state);
            let projected = DiagnosticOutput { omit_memory: true }.project(retained, &state);
            let full_bytes = serde_json::to_vec(&full).unwrap();
            let projected_bytes = serde_json::to_vec(&projected).unwrap();
            assert_eq!(
                full_bytes.len() - projected_bytes.len(),
                serde_json::to_vec(&full["memory"]).unwrap().len() + b",\"memory\":".len()
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&projected_bytes).unwrap(),
                projected
            );
            full_sizes.push(full_bytes.len());
            projected_sizes.push(projected_bytes.len());
        }
        assert!(full_sizes[1] > full_sizes[0] * 2);
        assert_eq!(projected_sizes[0], projected_sizes[1]);
    }

    #[test]
    fn ordered_strikes_headless_cancel_accepts_only_a_typed_optional_escape_step() {
        for value in [
            serde_json::json!({"type":"cancel_intention"}),
            serde_json::json!({"type":"cancel_intention","step":null}),
            serde_json::json!({"type":"cancel_intention","step":"west"}),
        ] {
            assert!(
                serde_json::from_value::<super::Input>(value.clone()).is_ok(),
                "{value}"
            );
        }
        for value in [
            serde_json::json!({"type":"cancel_intention","step":"nowhere"}),
            serde_json::json!({"type":"cancel_intention","step":"west","intention":"forged"}),
            serde_json::json!({"type":"resume_intention","step":"west"}),
        ] {
            assert!(serde_json::from_value::<super::Input>(value).is_err());
        }
    }

    #[test]
    fn intention_inputs_do_not_accept_forged_actor_or_identity_fields() {
        for kind in ["resume_intention", "cancel_intention"] {
            assert!(
                serde_json::from_value::<super::Input>(serde_json::json!({"type":kind})).is_ok()
            );
            for field in ["actor", "intention", "branch"] {
                let mut forged = serde_json::json!({"type":kind});
                forged[field] = serde_json::json!("forged");
                assert!(serde_json::from_value::<super::Input>(forged).is_err());
            }
        }
    }
    use super::*;

    #[test]
    fn ordinary_advancement_input_carries_only_navigation_not_forged_authority() {
        let input: Input =
            serde_json::from_str(r#"{"type":"advancement","command":"remove 1 1"}"#).unwrap();
        assert!(matches!(input, Input::Advancement { command } if command == "remove 1 1"));
        for extra in ["actor", "basis", "price", "refund", "wizard"] {
            let mut value = serde_json::json!({"type":"advancement","command":"confirm"});
            value[extra] = serde_json::json!("forged");
            assert!(serde_json::from_value::<Input>(value).is_err());
        }
    }

    #[test]
    fn a_rejected_wizard_command_is_unapplied_and_other_results_are_final() {
        assert!(wizard_still_unapplied(Some(
            "StaleRevision: Refresh before a wizard operation"
        )));
        assert!(!wizard_still_unapplied(None));
        assert!(!wizard_still_unapplied(Some(
            "NotController: Acquire control before acting"
        )));
        assert!(!wizard_still_unapplied(Some(
            "InvalidAction: Unknown wizard operation"
        )));
    }

    #[test]
    fn wizard_input_carries_a_developer_command() {
        let input: Input =
            serde_json::from_str(r#"{"type":"wizard","command":"rewind initial"}"#).unwrap();
        assert!(matches!(input, Input::Wizard { command } if command == "rewind initial"));
        assert!(serde_json::from_str::<Input>(r#"{"type":"wizard"}"#).is_err());
        assert!(
            serde_json::from_str::<Input>(r#"{"type":"wizard","command":"x","extra":1}"#).is_err()
        );
    }
}
