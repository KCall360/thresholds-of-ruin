//! Shared helpers for the server integration tests: playing through an
//! [`Engine`] the way the session does, locating scenario packages, and
//! reading or forging save files.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tor_protocol::{Action, ActorId};
use tor_server::journal::Command;
use tor_server::{scenario_package, CommandResult, Engine, Failure, Scenario};

/// The character most tests play.
pub const CHARACTER: ActorId = ActorId(1);

/// A scenario package by name: a shipped package (`scenarios/<name>`) or a
/// test package (`scenarios/tests/<name>`).
pub fn package(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    if root.join(name).join("scenario.toml").exists() {
        root.join(name)
    } else {
        root.join("tests").join(name)
    }
}

/// Load a validated package for its default character.
pub fn load(name: &str, seed: u64) -> Scenario {
    scenario_package::load(&package(name), seed, None, false)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every scenario package: the shipped ones and the test packages.
pub fn all_packages() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let mut packages: Vec<PathBuf> = [root.clone(), root.join("tests")]
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("scenario.toml").exists())
        .collect();
    packages.sort();
    packages
}

/// Submit a command for `actor` under a fresh request id.
pub fn submit(
    engine: &mut Engine,
    actor: ActorId,
    command: Command,
) -> Result<CommandResult, Failure> {
    let branch = engine.branch().clone();
    let request = uuid::Uuid::new_v4().to_string();
    engine.command("player", "test", actor, &request, &branch, command)
}

/// Act for `actor` at its current revision.
pub fn act_as(
    engine: &mut Engine,
    actor: ActorId,
    action: Action,
) -> Result<CommandResult, Failure> {
    let expected_revision = engine.revision(actor)?;
    submit(
        engine,
        actor,
        Command::Act {
            expected_revision,
            action,
        },
    )
}

/// Act as the character at its current revision.
pub fn act(engine: &mut Engine, action: Action) -> Result<CommandResult, Failure> {
    act_as(engine, CHARACTER, action)
}

/// Run a developer command for `actor`, in the text client's `wizard` form.
/// The game must allow wizard commands ([`Engine::enable_wizard`]).
pub fn wizard(
    engine: &mut Engine,
    actor: ActorId,
    operation: &str,
) -> Result<CommandResult, Failure> {
    let command = Command::from_wire(&tor_protocol::Command::Wizard {
        expected_revision: engine.revision(actor)?,
        operation: operation.into(),
    })?;
    submit(engine, actor, command)
}

/// Take every AI turn that is due before a controlled actor's, as the session
/// runs until it needs client input. Returns the number of turns taken.
pub fn run_ai_turns(engine: &mut Engine) -> usize {
    let mut turns = 0;
    while let Some(actor) = engine.next_actor().filter(|id| engine.is_ai(*id)) {
        engine.advance_ai(actor).unwrap();
        turns += 1;
        assert!(
            turns <= 10_000,
            "AI turns never returned to a controlled actor"
        );
    }
    turns
}

/// One turn of play: the AI turns that are due, then the character's action.
pub fn play(engine: &mut Engine, action: Action) -> Result<CommandResult, Failure> {
    run_ai_turns(engine);
    act(engine, action)
}

/// Fixture encoder intentionally independent of production decoding.
pub fn frame(sequence: u64, value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(if sequence == 0 { b"TORB" } else { b"TORJ" });
    bytes.extend_from_slice(&6u16.to_le_bytes());
    bytes.extend_from_slice(&(if sequence == 0 { 0u16 } else { 1u16 }).to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    let payload = serde_json::to_vec(value).unwrap();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let mut crc = !0u32;
    for b in bytes[4..].iter().chain(&payload) {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_le_bytes());
    bytes.extend(payload);
    bytes
}
pub fn read(path: impl AsRef<Path>) -> Value {
    tor_server::inspect_save(path).unwrap()
}
pub fn write(path: impl AsRef<Path>, mut archive: Value) {
    let mut conn = rusqlite::Connection::open(path).unwrap();
    let base: Vec<u8> = conn
        .query_row("SELECT frame FROM journal WHERE sequence=0", [], |r| {
            r.get(0)
        })
        .unwrap();
    let base: Value = serde_json::from_slice(&base[24..]).unwrap();
    let id = &base["save_id"];
    let records = archive["records"].take();
    archive["records"] = json!([]);
    let tx = conn.transaction().unwrap();
    tx.execute("DELETE FROM journal", []).unwrap();
    tx.execute(
        "INSERT INTO journal VALUES (0,?1)",
        [frame(
            0,
            &json!({"save_id":id,"generation":0,"archive":archive}),
        )],
    )
    .unwrap();
    for (i, record) in records.as_array().unwrap().iter().enumerate() {
        let seq = i as u64 + 1;
        tx.execute(
            "INSERT INTO journal VALUES (?1,?2)",
            rusqlite::params![
                seq as i64,
                frame(seq, &json!({"save_id":id,"generation":0,"record":record}))
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

/// Copy a package: its manifest, region files, index and certificate.
pub fn copy_package(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.join("regions")).unwrap();
    for name in ["scenario.toml", "index.json", "validation.json"] {
        if from.join(name).exists() {
            std::fs::copy(from.join(name), to.join(name)).unwrap();
        }
    }
    for entry in std::fs::read_dir(from.join("regions")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), to.join("regions").join(entry.file_name())).unwrap();
    }
}

/// Replace `from` with `to` in the one region file of a package holding it.
pub fn edit_region(package: &Path, from: &str, to: &str) {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(package.join("regions")).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains(from) {
            found.push((path, text));
        }
    }
    assert_eq!(found.len(), 1, "Expected one region file holding {from}");
    let (path, text) = found.pop().unwrap();
    std::fs::write(path, text.replacen(from, to, 1)).unwrap();
}
