use std::collections::BTreeSet;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tor_server::{
    serve, Account, Engine, OutboundLimits, SavePolicy, Scenario, Service, Simulation,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut listen: SocketAddr = "127.0.0.1:4000".parse()?;
    let mut seed = 0;
    let mut wizard = false;
    let mut save = PathBuf::from("saves/game.db");
    let mut regions = None;
    let mut package_path: Option<PathBuf> = None;
    let mut character = None;
    let mut allow_unvalidated = false;
    let mut actors = 1usize;
    let mut save_policy = SavePolicy::default();
    let mut save_reply_ms = None;
    let mut outbound = OutboundLimits::default();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("Outgoing buffers: --outbound-frame-bytes 16777216 --outbound-client-bytes 67108864 --outbound-total-bytes 268435456. Byte limits include queued and in-flight payloads. Frame <= client <= total; frame is at most 16 MiB. Each connection reserves one maximum frame; the connection limit is min(128, total/frame), 16 by default. Exhausted streams disconnect and must reconnect for a snapshot.");
                println!("Background saves: --save-target-ms 30000 --save-max-ms 60000 --save-idle-ms 750 --save-queue-bytes 8388608 --save-reply-ms 5000. Ordinary acknowledgements may be lost after a crash; explicit save and clean shutdown wait for storage, and a save not durable within the reply wait is answered save_behind.");
                println!("Checkpoints: --checkpoint-interval 1024 journal entries (0 disables). Retains all history; bounds simulation replay after the latest committed checkpoint.");
                println!("Authored packages: --scenario <directory> [--character <id>] [--allow-unvalidated]. Validate with tor-scenario validate <directory>. Saves pin their original package.");
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
            "--save-reply-ms" => {
                save_reply_ms = Some(
                    args.next()
                        .ok_or("Missing save reply wait")?
                        .parse::<u64>()?,
                )
            }
            "--save-queue-bytes" => {
                save_policy.max_pending_bytes =
                    args.next().ok_or("Missing save queue size")?.parse()?
            }
            "--outbound-frame-bytes" => {
                outbound.frame_bytes = args.next().ok_or("Missing outbound frame limit")?.parse()?
            }
            "--outbound-client-bytes" => {
                outbound.client_bytes = args
                    .next()
                    .ok_or("Missing outbound client limit")?
                    .parse()?
            }
            "--outbound-total-bytes" => {
                outbound.total_bytes = args.next().ok_or("Missing outbound total limit")?.parse()?
            }
            "--scenario" => {
                package_path = Some(args.next().ok_or("Missing scenario directory")?.into())
            }
            "--character" => character = Some(args.next().ok_or("Missing character ID")?.parse()?),
            "--allow-unvalidated" => allow_unvalidated = true,
            "--wizard" => wizard = true,
            "--listen" => listen = args.next().ok_or("Missing --listen value")?.parse()?,
            "--seed" => seed = args.next().ok_or("Missing --seed value")?.parse()?,
            "--regions" => regions = Some(args.next().ok_or("Missing --regions value")?.parse()?),
            "--actors" => actors = args.next().ok_or("Missing --actors value")?.parse()?,
            "--save" => save = args.next().ok_or("Missing --save value")?.into(),
            _ => return Err(format!("Unknown argument: {argument}").into()),
        }
    }
    if !outbound.is_valid() {
        return Err("Invalid outbound byte limits".into());
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
    if regions.is_none() && actors != 1 {
        return Err("--actors requires --regions".into());
    }
    if package_path.is_some() && regions.is_some() {
        return Err("--scenario conflicts with --regions".into());
    }
    if regions.is_some() && character.is_some() {
        return Err("--character requires an authored scenario".into());
    }
    tor_server::process_diagnostics::startup("scenario.begin");
    let scenario = if save.exists() {
        // A save owns its inputs. Region files it hasn't copied yet come from
        // its package directory; `--scenario` names it if it has moved.
        match &package_path {
            Some(path) => {
                tor_server::scenario_package::load(path, seed, character, allow_unvalidated)?
            }
            None => Scenario::two_room(seed),
        }
    } else if regions.is_none() {
        let default_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon");
        tor_server::scenario_package::load(
            package_path.as_deref().unwrap_or(&default_path),
            seed,
            character,
            allow_unvalidated,
        )?
    } else {
        match regions {
            Some(regions) => Scenario::performance(seed, regions, actors)?,
            None if actors == 1 => Scenario::two_room(seed),
            None => return Err("--actors requires --regions".into()),
        }
    };
    tor_server::process_diagnostics::startup("scenario.end");
    if scenario.package.as_ref().is_some_and(|p| !p.validated) {
        eprintln!("Warning: running an unvalidated scenario under --allow-unvalidated");
    }
    save_policy.validate()?;
    tor_server::process_diagnostics::startup("engine.begin");
    let mut engine = Engine::open_with_policy(save, scenario, save_policy)?;
    tor_server::process_diagnostics::startup("engine.end");
    if let Some(path) = std::env::var_os("TOR_SURVIVAL_DIAGNOSTICS_LOG") {
        if let Err(error) = engine.enable_survival_diagnostics(PathBuf::from(path)) {
            eprintln!("Private survival diagnostics unavailable: {error}");
        }
    }
    tor_server::process_diagnostics::startup("preloading.begin");
    engine.start_preloading();
    tor_server::process_diagnostics::startup("preloading.end");
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
    tor_server::process_diagnostics::startup("simulation.begin");
    let mut service = Service::with_outbound_limits(engine, outbound)?;
    if let Some(ms) = save_reply_ms {
        service.set_save_reply_wait(std::time::Duration::from_millis(ms))?;
    }
    let simulation = Simulation::start(service);
    tor_server::process_diagnostics::startup("simulation.end");
    tor_server::process_diagnostics::startup("listener.begin");
    let listener = TcpListener::bind(listen).await?;
    tor_server::process_diagnostics::startup("listener.end");
    println!(
        "{}",
        serde_json::json!({ "address": listener.local_addr()?.to_string(), "protocol": tor_protocol::PROTOCOL_VERSION })
    );
    io::stdout().flush()?;
    tor_server::process_diagnostics::startup("ready");
    serve(listener, simulation, accounts, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
