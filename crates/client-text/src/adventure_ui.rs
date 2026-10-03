use std::{
    io::{self, BufRead, Write},
    time::Duration,
};
use tokio::sync::mpsc;
use tor_client_common::Connection;
use tor_client_text::engine::{self, turn::Link, Engine, Outcome};
use tor_protocol::*;

use super::Error;

/// Updates between turns are gathered until play has been quiet this long,
/// then told as one passage.
const GATHER: Duration = Duration::from_millis(150);

fn prompt() -> io::Result<()> {
    print!("> ");
    io::stdout().flush()
}

fn show(passage: &str) {
    if !passage.trim().is_empty() {
        println!("{passage}");
    }
}

pub async fn run(mut connection: Connection, observe: bool) -> Result<(), Error> {
    let mut engine = Engine::default();
    println!("Thresholds of Ruin\nType help for things you can try.\n");
    if connection.role() == AccessRole::Spectator {
        println!("Spectator access is read-only.");
    } else if !observe {
        // Taking control at startup isn't news to the player.
        let mut chronicler = engine::chronicle::Chronicler::default();
        let mut record = engine::narrate::Record::default();
        engine::turn::run_request(
            &mut connection,
            &mut chronicler,
            &mut record,
            Request::AcquireControl,
        )
        .await?;
    }
    engine.learn(&connection);
    println!("{}", engine.welcome(&connection));
    prompt()?;
    let (tx, mut rx) = mpsc::channel(16);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let failed = line.is_err();
            if tx.blocking_send(line).is_err() || failed {
                break;
            }
        }
    });
    let mut shown = connection.state.state().clone();
    loop {
        tokio::select! {
            biased;
            ready = tor_client_common::server_before_local(connection.next(), rx.recv()) => {
                match ready {
                    tor_client_common::FirstReady::Server(message) => {
                        // Something happened while the player was deciding.
                        let mut beats = engine.between_turns(&connection, &shown, &message?);
                        shown = connection.state.state().clone();
                        while let Some(message) = Link::next(&mut connection, GATHER).await? {
                            beats.extend(engine.between_turns(&connection, &shown, &message));
                            shown = connection.state.state().clone();
                        }
                        let passage = engine.passage(&connection, beats);
                        if !passage.trim().is_empty() {
                            println!();
                            show(&passage);
                            prompt()?;
                        }
                    }
                    tor_client_common::FirstReady::Local(line) => {
                        let Some(line) = line else { break; };
                        let outcome = engine.play(&mut connection, &line?).await?;
                        shown = connection.state.state().clone();
                        match outcome {
                            Outcome::Passage(passage) => show(&passage),
                            Outcome::Quit(passage) => {
                                show(&passage);
                                break;
                            }
                        }
                        prompt()?;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    connection.close().await?;
    println!("Goodbye.");
    Ok(())
}
