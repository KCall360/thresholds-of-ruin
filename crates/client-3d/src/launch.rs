//! Starting a private local server for one play session, as the desktop
//! launcher does: a fresh save, a fresh token, and the server's own address.
use std::{
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

type Error = Box<dyn std::error::Error + Send + Sync>;

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (0..2)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(nanos);
            h.write_u32(std::process::id() ^ i);
            format!("{:016x}", h.finish())
        })
        .collect()
}

pub struct Server {
    pub child: Child,
    pub address: SocketAddr,
    pub save: PathBuf,
}

pub fn start(server_bin: &Path, scenario: &Path, save: Option<PathBuf>, seed: Option<u64>, token: &str) -> Result<Server, Error> {
    if !server_bin.is_file() {
        return Err(format!(
            "Missing server at {}. Build it with: cargo build --release -p tor-server",
            server_bin.display()
        )
        .into());
    }
    let save = match save {
        Some(save) => save,
        None => {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let dir = repo_root().join(format!(".local/client-3d/saves/game-{stamp}"));
            std::fs::create_dir_all(&dir)?;
            dir.join("game.db")
        }
    };
    let logs = save.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut command = Command::new(server_bin);
    command
        .args(["--listen", "127.0.0.1:0", "--scenario"])
        .arg(scenario)
        .arg("--save")
        .arg(&save)
        .env("TOR_SERVER_TOKEN", token)
        .env_remove("TOR_SPECTATOR_TOKEN")
        .env_remove("TOR_WIZARD_TOKEN")
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(logs.join("server.stderr.log"))?);
    if let Some(seed) = seed {
        command.args(["--seed", &seed.to_string()]);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or("Server has no stdout")?;
    let (tx, rx) = mpsc::channel();
    // Keep draining the server's output so it never blocks on a full pipe.
    std::thread::spawn(move || {
        let mut first = true;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if first {
                first = false;
                let _ = tx.send(line);
            }
        }
    });
    let line = match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(line) => line,
        Err(_) => {
            let _ = child.kill();
            return Err(format!(
                "The server did not start; see {}",
                logs.join("server.stderr.log").display()
            )
            .into());
        }
    };
    let ready: serde_json::Value = serde_json::from_str(&line)?;
    let address = ready["address"]
        .as_str()
        .ok_or("Server did not report its address")?
        .parse()?;
    Ok(Server { child, address, save })
}
