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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    ResumeIntention {},
    CancelIntention {},
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("tor-client-headless [--connect 127.0.0.1:4000] [--actor 1] [--observe]\nSet TOR_SERVER_TOKEN. Send JSON lines: act, wizard, request, inspect, quit.\nSee docs/headless-client.md for the input and output contract.");
                return Ok(());
            }
            "--connect" => address = args.next().ok_or("Missing --connect address")?.parse()?,
            "--actor" => actor = ActorId(args.next().ok_or("Missing --actor ID")?.parse()?),
            "--observe" => observe = true,
            _ => return Err("Unknown command-line option".into()),
        }
    }
    let token = std::env::var("TOR_SERVER_TOKEN")
        .map_err(|_| "Set TOR_SERVER_TOKEN before starting the client")?;
    let mut connection = Connection::connect(address, token, actor, "headless").await?;
    let error = if connection.role() != AccessRole::Spectator && !observe {
        transact(&mut connection, Request::AcquireControl).await?
    } else {
        None
    };
    emit(&connection, "ready", None, error.as_deref())?;

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
                        emit(&connection, "update", Some(&message?), None)?;
                    }
                    tor_client_common::FirstReady::Local(line) => {
                        let Some(line) = line else { break; };
                        let line = line?;
                        let input = serde_json::from_str::<Input>(&line);
                        if !connection.is_synchronized() && !matches!(input, Ok(Input::Quit | Input::Inspect)) {
                            emit(&connection, "ready", None, Some("Resynchronizing; input was not sent"))?;
                            continue;
                        }
                        let result = match input {
                            Ok(Input::Quit) => break,
                            Ok(Input::Inspect) => None,
                            Ok(Input::ResumeIntention {} | Input::CancelIntention {}) => {
                                let resume = matches!(input, Ok(Input::ResumeIntention {}));
                                if connection.role() == AccessRole::Spectator {
                                    Some("Spectator access is read-only".into())
                                } else if !connection.state.has_control() {
                                    Some("Actor control is required".into())
                                } else {
                                    let request = if resume { connection.state.resume_intention_request() }
                                        else { connection.state.cancel_intention_request() };
                                    if let Some(request) = request { transact(&mut connection, request).await? }
                                    else { Some(if resume { "No suspended action to resume" }
                                        else { "No queued action to cancel" }.into()) }
                                }
                            }
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
                                    transact(&mut connection, request).await?
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
                                    result = transact(&mut connection, request).await?;
                                    if !wizard_still_unapplied(result.as_deref()) {
                                        break;
                                    }
                                }
                                result
                            }
                            Ok(Input::Request { request }) => transact(&mut connection, request).await?,
                            Err(_) => Some(
                                "Invalid input; expected a JSON act, resume_intention, cancel_intention, wizard, request, inspect or quit"
                                    .into(),
                            ),
                        };
                        emit(&connection, "ready", None, result.as_deref())?;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    connection.close().await?;
    Ok(())
}

/// True only when the wizard operation was rejected and not applied.
/// A committed rewind, or any other error, must not be sent again.
fn wizard_still_unapplied(error: Option<&str>) -> bool {
    error.is_some_and(|error| error.starts_with("StaleRevision"))
}

async fn transact(connection: &mut Connection, request: Request) -> Result<Option<String>, Error> {
    if !connection.role().permits(&request) {
        return Ok(Some("Spectator access is read-only".into()));
    }
    let id = connection.request(request).await?;
    let mut pending = tor_client_common::PendingRequest::new(id);
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            emit(connection, "response", Some(&message), None)?;
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
) -> Result<(), Error> {
    static TIMING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let timing = *TIMING.get_or_init(|| std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some());
    let started = timing.then(std::time::Instant::now);
    let output = serde_json::json!({
        "type": kind,
        "synchronized": connection.is_synchronized(),
        "role": connection.role(),
        "state": connection.state.state(),
        "branch": connection.state.branch(),
        "cursor": connection.state.cursor(),
        "has_control": connection.state.has_control(),
        "readiness": connection.state.readiness(),
        "input_context": connection.state.input_context(),
        "history": connection.state.history(),
        "travel": connection.state.travel(),
        "intentions": connection.state.intentions(),
        "memory": connection.state.memory().collect::<Vec<_>>(),
        "palette": &connection.palette,
        "message": message,
        "error": error,
    });
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
