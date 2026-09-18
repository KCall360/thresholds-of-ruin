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
    let mut save = PathBuf::from("saves/game.json");
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("tor-server [--listen 127.0.0.1:4000] [--seed 0] [--save saves/game.json]\nSet TOR_SERVER_TOKEN to an authentication token of at least 16 characters.\nOnly loopback connections are supported. Existing saves retain their original seed.");
                return Ok(());
            }
            "--listen" => listen = args.next().ok_or("Missing --listen value")?.parse()?,
            "--seed" => seed = args.next().ok_or("Missing --seed value")?.parse()?,
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
    let engine = Engine::open(save, Scenario::two_room(seed))?;
    let account = Account {
        user: "local".into(),
        token,
        actors: engine.actors().into_iter().collect::<BTreeSet<_>>(),
    };
    let service = Arc::new(Mutex::new(Service::new(engine)));
    let listener = TcpListener::bind(listen).await?;
    println!(
        "{}",
        serde_json::json!({ "address": listener.local_addr()?.to_string(), "protocol": tor_protocol::PROTOCOL_VERSION })
    );
    io::stdout().flush()?;
    serve(listener, service, vec![account], async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
