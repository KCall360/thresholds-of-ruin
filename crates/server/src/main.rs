use std::collections::BTreeSet;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tor_server::{serve, Account, Engine, Scenario, Service};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listen: SocketAddr = "127.0.0.1:4000".parse()?;
    let mut seed = 0;
    let mut wizard = false;
    let mut save = PathBuf::from("saves/game.json");
    let mut regions = 2u64;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("tor-server [--listen 127.0.0.1:4000] [--seed 0] [--save saves/game.json]\nSet TOR_SERVER_TOKEN to an authentication token of at least 16 characters.\nOptionally set a different TOR_SPECTATOR_TOKEN for read-only access.\n--wizard with distinct TOR_WIZARD_TOKEN permanently marks a new or existing game and enables development commands.\nOnly loopback connections are supported. Existing saves retain their original seed.");
                return Ok(());
            }
            "--wizard" => wizard = true,
            "--listen" => listen = args.next().ok_or("Missing --listen value")?.parse()?,
            "--seed" => seed = args.next().ok_or("Missing --seed value")?.parse()?,
            "--regions" => regions = args.next().ok_or("Missing --regions value")?.parse()?,
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
    let mut scenario = Scenario::two_room(seed);
    scenario.regions = regions.max(2);
    let mut engine = Engine::open(save, scenario)?;
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
