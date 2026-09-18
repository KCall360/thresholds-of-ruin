use std::{
    io::{self, BufRead, Write},
    net::SocketAddr,
    time::Duration,
};
use tokio::{sync::mpsc, time::timeout};
use tor_client_common::Connection;
use tor_client_text::{describe, history, inventory, parse, safe, Input, HELP};
use tor_protocol::*;

mod adventure_ui;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Text client: {}", safe(&error.to_string()));
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Error> {
    let mut address: SocketAddr = "127.0.0.1:4000".parse()?;
    let mut actor = ActorId(1);
    let mut observe = false;
    let mut script = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("tor-client-text [--connect 127.0.0.1:4000] [--actor 1] [--observe] [--script]\nSet TOR_SERVER_TOKEN to the server's token. Enter one command per line.\n{}\n\n--script selects the development scripting interface. Use help session in the game for connection and history tools.", tor_client_text::adventure::HELP);
                return Ok(());
            }
            "--connect" => address = args.next().ok_or("Missing --connect address")?.parse()?,
            "--actor" => actor = ActorId(args.next().ok_or("Missing --actor ID")?.parse()?),
            "--observe" => observe = true,
            "--script" => script = true,
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let token = std::env::var("TOR_SERVER_TOKEN")
        .map_err(|_| "Set TOR_SERVER_TOKEN before starting the client")?;
    let mut connection = Connection::connect(address, token, actor, "text").await?;
    if !script {
        return adventure_ui::run(connection, observe).await;
    }
    println!("Attached to actor {}. Type help for commands.", actor.0);
    println!("{}", describe(connection.state.state()));
    println!("{}", inventory(connection.state.state()));
    for entry in connection.state.history() {
        println!("{}", history(entry));
    }
    if connection.role() == AccessRole::Spectator {
        println!("Spectator access is read-only. Live actions, look, inventory, sync and history are available.");
    } else if !observe {
        transact(&mut connection, Request::AcquireControl).await?;
    }
    ready()?;

    // A dedicated blocking reader avoids keeping Tokio's runtime alive on stdin
    // after a network disconnect. The bounded channel also supports piped scripts.
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
                present(&connection, &message?);
                io::stdout().flush()?;
            },
            line = rx.recv() => {
                let Some(line) = line else { break; };
                let line = line?;
                if line.trim().is_empty() { ready()?; continue; }
                match parse(&line, connection.state.state()) {
                    Ok(Input::Quit) => break,
                    Ok(Input::Look) => println!("{}", describe(connection.state.state())),
                    Ok(Input::Inventory) => println!("{}", inventory(connection.state.state())),
                    Ok(Input::Help) => println!("{HELP}"),
                    Ok(Input::Request(request)) => transact(&mut connection, request).await?,
                    Ok(Input::Command(command)) => {
                        if connection.role() == AccessRole::Spectator {
                            println!("Spectator access is read-only.");
                        } else if matches!(command, Command::Wizard { .. }) && connection.role() != AccessRole::Wizard {
                            println!("Wizard authority is required.");
                        } else if matches!(command, Command::Act { .. }) && !connection.state.has_control() {
                            println!("You are observing. Use control to request control.");
                        } else {
                            let request = Request::Command { branch: connection.state.branch().clone(), command };
                            transact(&mut connection, request).await?;
                        }
                    },
                    Err(error) => println!("{}", safe(&error)),
                }
                ready()?;
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    connection.close().await;
    println!("Goodbye.");
    Ok(())
}

fn ready() -> io::Result<()> {
    println!("Ready.");
    io::stdout().flush()
}

async fn transact(connection: &mut Connection, request: Request) -> Result<(), Error> {
    if !connection.role().permits(&request) {
        println!("Spectator access is read-only.");
        return Ok(());
    }
    let id = connection.request(request).await?;
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            present(connection, &message);
            let complete = match &message {
                ServerMessage::Ack { request_id, .. } | ServerMessage::Snapshot { request_id, .. } | ServerMessage::History { request_id, .. } => request_id == &id,
                ServerMessage::Error { request_id, .. } => request_id.as_ref() == Some(&id),
                _ => false,
            };
            if complete { return Ok::<(), Error>(()); }
        }
    }).await.map_err(|_| "Server response timed out; command outcome may be unknown. Reconnect and inspect history before retrying.")??;
    Ok(())
}

fn present(connection: &Connection, message: &ServerMessage) {
    match message {
        ServerMessage::Update { update } => match &update.body {
            UpdateBody::Travel { .. } => {}
            UpdateBody::Observation { event, .. } => {
                if let Some(entry) = event {
                    println!("{}", history(entry));
                }
                println!("{}", describe(connection.state.state()));
            }
            UpdateBody::Annotation { entry } => println!("{}", history(entry)),
            UpdateBody::Control { has_control } => println!(
                "Control: {}.",
                if *has_control { "yours" } else { "observing" }
            ),
        },
        ServerMessage::Snapshot { request_id, .. } => {
            println!("{}", describe(connection.state.state()));
            println!("{}", inventory(connection.state.state()));
            println!("Branch: {}", safe(&connection.state.branch().0));
            if request_id.is_empty() {
                if let Some(entry) = connection
                    .state
                    .history()
                    .last()
                    .filter(|entry| matches!(entry.content, HistoryContent::Wizard { .. }))
                {
                    println!("{}", history(entry));
                }
            }
        }
        ServerMessage::History { page, .. } => {
            if page.entries.is_empty() {
                println!("History: empty.");
            }
            for entry in &page.entries {
                println!("{}", history(entry));
            }
            if let Some(before) = &page.older_before {
                println!("Older entries: history {}", safe(&before.0));
            }
        }
        ServerMessage::Error { code, message, .. } => {
            println!("Server error {code:?}: {}", safe(message))
        }
        ServerMessage::Ack { .. } => println!("Done."),
        ServerMessage::Welcome { .. } => {}
    }
}
