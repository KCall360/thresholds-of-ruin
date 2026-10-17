//! Starting a private local server for one play session, as the desktop
//! launcher does: a fresh save, a fresh token, and the server's own address.
use std::{
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

fn token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub struct Credentials {
    pub owner: String,
    pub spectator: String,
}
impl Credentials {
    pub fn new() -> Self {
        let owner = token();
        let spectator = loop {
            let candidate = token();
            if candidate != owner {
                break candidate;
            }
        };
        Self { owner, spectator }
    }
}

/// Only this process is owned. Never kills a name-matched process or a tree.
pub struct OwnedChild {
    child: Child,
}
impl OwnedChild {
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn stop(&mut self) -> Result<(), std::io::Error> {
        stop_child(&mut self.child)
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn stop_child(child: &mut Child) -> Result<(), std::io::Error> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    if let Err(error) = child.kill() {
        return if child.try_wait()?.is_some() {
            Ok(())
        } else {
            Err(error)
        };
    }
    child.wait()?;
    Ok(())
}

pub struct Server {
    pub child: OwnedChild,
    pub address: SocketAddr,
    pub save: PathBuf,
}

fn reserve_save(root: &Path, stamp: &str) -> Result<PathBuf, Error> {
    std::fs::create_dir_all(root)?;
    for suffix in 0..10_000 {
        let dir = root.join(format!("game-{stamp}-{suffix}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir.join("game.db")),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("Could not reserve a fresh save directory".into())
}

pub fn start(
    server_bin: &Path,
    scenario: &Path,
    save: Option<PathBuf>,
    seed: Option<u64>,
    save_root: &Path,
    credentials: &Credentials,
) -> Result<Server, Error> {
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
            reserve_save(save_root, &stamp.to_string())?
        }
    };
    let logs = save.parent().unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&logs)?;
    let mut command = private_command(server_bin, scenario, &save, seed, credentials);
    command.stderr(std::fs::File::create(logs.join("server.stderr.log"))?);
    let mut child = OwnedChild {
        child: command.spawn()?,
    };
    let address = server_ready(&mut child.child, &logs)?;
    Ok(Server {
        child,
        address,
        save,
    })
}

fn private_command(
    server_bin: &Path,
    scenario: &Path,
    save: &Path,
    seed: Option<u64>,
    credentials: &Credentials,
) -> Command {
    let mut command = Command::new(server_bin);
    command
        .args(["--listen", "127.0.0.1:0", "--scenario"])
        .arg(scenario)
        .arg("--save")
        .arg(save)
        .env("TOR_SERVER_TOKEN", &credentials.owner)
        .env("TOR_SPECTATOR_TOKEN", &credentials.spectator)
        .env_remove("TOR_WIZARD_TOKEN")
        .stdout(Stdio::piped());
    if let Some(seed) = seed {
        command.args(["--seed", &seed.to_string()]);
    }
    command
}

fn server_ready(child: &mut Child, logs: &Path) -> Result<SocketAddr, Error> {
    let result = read_ready(child, logs);
    if result.is_err() {
        stop_child(child).map_err(|error| {
            format!("Startup failed and the private server could not be stopped: {error}")
        })?;
    }
    result
}

fn read_ready(child: &mut Child, logs: &Path) -> Result<SocketAddr, Error> {
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
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn private_command_uses_separate_owner_and_spectator_credentials_without_wizard_access() {
        let credentials = Credentials {
            owner: "owner-test".into(),
            spectator: "spectator-test".into(),
        };
        let command = private_command(
            Path::new("server"),
            Path::new("scenario"),
            Path::new("save"),
            None,
            &credentials,
        );
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        use std::ffi::OsStr;
        assert_eq!(
            environment[OsStr::new("TOR_SERVER_TOKEN")],
            Some(OsStr::new("owner-test"))
        );
        assert_eq!(
            environment[OsStr::new("TOR_SPECTATOR_TOKEN")],
            Some(OsStr::new("spectator-test"))
        );
        assert_eq!(environment[OsStr::new("TOR_WIZARD_TOKEN")], None);
    }

    #[test]
    #[ignore = "subprocess fixture invoked only by the owning lifecycle test"]
    fn malformed_ready_fixture() {
        println!("fixture-ready");
        println!("not-json");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
    }

    #[test]
    fn malformed_startup_reaps_its_owned_server() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "launch::tests::malformed_ready_fixture",
                "--ignored",
                "--nocapture",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut prefix = Vec::new();
        while !prefix.ends_with(b"fixture-ready\n") {
            let mut byte = [0];
            stdout.read_exact(&mut byte).unwrap();
            prefix.push(byte[0]);
            assert!(
                prefix.len() < 4096,
                "fixture did not reach its controlled startup boundary"
            );
        }
        child.stdout = Some(stdout);
        let result = server_ready(&mut child, &repo_root().join(".local"));
        let reaped = child.try_wait().unwrap().is_some();
        // The first-red case must clean up its own subprocess too.
        let _ = child.kill();
        let _ = child.wait();
        assert!(result.is_err());
        assert!(
            reaped,
            "a malformed ready message must not leave the private server running"
        );
    }

    #[test]
    fn default_sessions_with_the_same_clock_stamp_keep_separate_saves() {
        let root = repo_root().join(".local/3d-launch-tests").join(token());
        let first = reserve_save(&root, "same-stamp").unwrap();
        std::fs::write(&first, b"prior save").unwrap();
        let second = reserve_save(&root, "same-stamp").unwrap();
        assert_ne!(
            first, second,
            "a fresh launch must not resume a previous session"
        );
        assert_eq!(std::fs::read(first).unwrap(), b"prior save");
        assert!(!second.exists());
    }
}
