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
    Act { action: Action },
    Request { request: Request },
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
                println!("tor-client-headless [--connect 127.0.0.1:4000] [--actor 1] [--observe]\nSet TOR_SERVER_TOKEN. Send JSON lines: act, request, inspect, quit.\nSee docs/headless-client.md for the input and output contract.");
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
            message = connection.next() => {
                emit(&connection, "update", Some(&message?), None)?;
            },
            line = rx.recv() => {
                let Some(line) = line else { break; };
                let line = line?;
                let input = serde_json::from_str::<Input>(&line);
                let result = match input {
                    Ok(Input::Quit) => break,
                    Ok(Input::Inspect) => None,
                    Ok(Input::Act { action }) => {
                        if connection.role() == AccessRole::Spectator {
                            Some("Spectator access is read-only".into())
                        } else if !connection.state.has_control() {
                            Some("Actor control is required".into())
                        } else {
                            let request = Request::Command {
                                branch: connection.state.branch().clone(),
                                command: Command::Act {
                                    expected_revision: connection.state.state().revision, action,
                                },
                            };
                            transact(&mut connection, request).await?
                        }
                    },
                    Ok(Input::Request { request }) => transact(&mut connection, request).await?,
                    Err(_) => Some("Invalid input; expected a JSON act, request, inspect or quit".into()),
                };
                emit(&connection, "ready", None, result.as_deref())?;
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    connection.close().await;
    Ok(())
}

async fn transact(connection: &mut Connection, request: Request) -> Result<Option<String>, Error> {
    if !connection.role().permits(&request) {
        return Ok(Some("Spectator access is read-only".into()));
    }
    let id = connection.request(request).await?;
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            emit(connection, "response", Some(&message), None)?;
            match message {
                ServerMessage::Ack { request_id, .. }
                | ServerMessage::Snapshot { request_id, .. }
                | ServerMessage::History { request_id, .. } if request_id == id => return Ok(None),
                ServerMessage::Error { request_id: Some(request_id), code, message }
                    if request_id == id => return Ok(Some(format!("{code:?}: {message}"))),
                _ => {}
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
    let output = serde_json::json!({
        "type": kind,
        "role": connection.role(),
        "state": connection.state.state(),
        "branch": connection.state.branch(),
        "cursor": connection.state.cursor(),
        "has_control": connection.state.has_control(),
        "history": connection.state.history(),
        "memory": connection.state.memory().collect::<Vec<_>>(),
        "message": message,
        "error": error,
    });
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}
