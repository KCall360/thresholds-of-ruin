use std::collections::BTreeSet;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tor_server::{serve, Account, Engine, SavePolicy, Scenario, Service};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listen: SocketAddr = "127.0.0.1:4000".parse()?;
    let mut seed = 0;
    let mut wizard = false;
    let mut save = PathBuf::from("saves/game.db");
    let mut regions = None;
    let mut actors = 1usize;
    let mut save_policy = SavePolicy::default();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("Background saves: --save-target-ms 30000 --save-max-ms 60000 --save-idle-ms 750 --save-queue-bytes 8388608. Ordinary acknowledgements may be lost after a crash; explicit save and clean shutdown wait for storage.");
                println!("Checkpoints: --checkpoint-interval 1024 journal entries (0 disables). Retains all history; bounds simulation replay after the latest committed checkpoint.");
                println!("Diagnostic fixture: --regions 1..=256 [--actors 1..=8] selects performance trace version 1.");
                println!("tor-server [--listen 127.0.0.1:4000] [--seed 0] [--save saves/game.db]\nSet TOR_SERVER_TOKEN to an authentication token of at least 16 characters.\nOptionally set a different TOR_SPECTATOR_TOKEN for read-only access.\n--wizard with distinct TOR_WIZARD_TOKEN permanently marks a new or existing game and enables development commands.\nOnly loopback connections are supported. Existing saves retain their original seed.");
                return Ok(());
            }
            "--checkpoint-interval" => {
                save_policy.checkpoint_interval =
                    args.next().ok_or("Missing checkpoint interval")?.parse()?;
            }
            "--save-target-ms" => {
                save_policy.target_interval = std::time::Duration::from_millis(
                    args.next().ok_or("Missing save target")?.parse()?,
                )
            }
            "--save-max-ms" => {
                save_policy.max_unsaved_age = std::time::Duration::from_millis(
                    args.next().ok_or("Missing save maximum")?.parse()?,
                )
            }
            "--save-idle-ms" => {
                save_policy.idle_interval = std::time::Duration::from_millis(
                    args.next().ok_or("Missing save idle interval")?.parse()?,
                )
            }
            "--save-queue-bytes" => {
                save_policy.max_pending_bytes =
                    args.next().ok_or("Missing save queue size")?.parse()?
            }
            "--wizard" => wizard = true,
            "--listen" => listen = args.next().ok_or("Missing --listen value")?.parse()?,
            "--seed" => seed = args.next().ok_or("Missing --seed value")?.parse()?,
            "--regions" => regions = Some(args.next().ok_or("Missing --regions value")?.parse()?),
            "--actors" => actors = args.next().ok_or("Missing --actors value")?.parse()?,
            "--save" => save = args.next().ok_or("Missing --save value")?.into(),
            _ => return Err(format!("Unknown argument: {argument}").into()),
        }
    }
    if !listen.ip().is_loopback() {
        return Err("Only loopback listeners are supported".into());
    }
    let token = std::env::var("TOR_SERVER_TOKEN")
        .map_err(|_| "Set TOR_SERVER_TOKEN before starting the server")?;
    if token.trim().len() < 16 || token.len() > 1024 || token.chars().any(char::is_control) {
        return Err("TOR_SERVER_TOKEN must contain 16–1024 non-control characters".into());
    }
    let spectator_token = match std::env::var("TOR_SPECTATOR_TOKEN") {
        Ok(value) => {
            if value.trim().len() < 16 || value.len() > 1024 || value.chars().any(char::is_control)
            {
                return Err(
                    "TOR_SPECTATOR_TOKEN must contain 16-1024 non-control characters".into(),
                );
            }
            if value == token {
                return Err("Player and spectator tokens must differ".into());
            }
            Some(value)
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => return Err("TOR_SPECTATOR_TOKEN must be Unicode".into()),
    };
    let wizard_token = match std::env::var("TOR_WIZARD_TOKEN") {
        Ok(value) => {
            if !wizard {
                return Err("TOR_WIZARD_TOKEN requires --wizard".into());
            }
            if value.trim().len() < 16
                || value.len() > 1024
                || value.chars().any(char::is_control)
                || value == token
                || spectator_token.as_ref() == Some(&value)
            {
                return Err(
                    "TOR_WIZARD_TOKEN must be distinct and contain 16-1024 non-control characters"
                        .into(),
                );
            }
            Some(value)
        }
        Err(std::env::VarError::NotPresent) if !wizard => None,
        Err(_) => return Err("--wizard requires TOR_WIZARD_TOKEN".into()),
    };
    let scenario = match regions {
        Some(regions) => Scenario::performance(seed, regions, actors)?,
        None if actors == 1 => Scenario::two_room(seed),
        None => return Err("--actors requires --regions".into()),
    };
    save_policy.validate()?;
    let mut engine = Engine::open_with_policy(save, scenario, save_policy)?;
    if wizard {
        engine.enable_wizard()?;
    }
    let account = Account {
        role: tor_protocol::AccessRole::Player,
        user: "local".into(),
        token,
        actors: engine.actors().into_iter().collect::<BTreeSet<_>>(),
    };
    let mut accounts = vec![account];
    if let Some(token) = spectator_token {
        accounts.push(Account {
            role: tor_protocol::AccessRole::Spectator,
            user: "spectator".into(),
            token,
            actors: accounts[0].actors.clone(),
        });
    }
    if let Some(token) = wizard_token {
        accounts.push(Account {
            role: tor_protocol::AccessRole::Wizard,
            user: "wizard".into(),
            token,
            actors: accounts[0].actors.clone(),
        });
    }
    let service = Arc::new(Mutex::new(Service::new(engine)));
    let listener = TcpListener::bind(listen).await?;
    println!(
        "{}",
        serde_json::json!({ "address": listener.local_addr()?.to_string(), "protocol": tor_protocol::PROTOCOL_VERSION })
    );
    io::stdout().flush()?;
    serve(listener, service, accounts, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
