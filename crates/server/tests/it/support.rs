//! Shared helpers for the server integration tests: playing through an
//! [`Engine`] the way the session does, locating scenario packages, and
//! reading or forging save files.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tor_protocol::ActorId;
use tor_server::journal::Action;
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

#[path = "../../test_support/packages.rs"]
mod packages;
pub use packages::all_packages;

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

/// Decode fresh wire input for a trusted Engine test driver. Session authority
/// remains covered by the actual Service/WebSocket tests.
pub fn decode_command(
    engine: &Engine,
    actor: ActorId,
    command: &tor_protocol::Command,
) -> Result<Command, Failure> {
    engine.resolve_command(
        actor,
        engine.branch(),
        tor_server::wire_adapter::decode_command(command)?,
    )
}

/// Drive the existing immediate simulation test path using a disclosed target.
pub fn act_disclosed(
    engine: &mut Engine,
    actor: ActorId,
    action: tor_protocol::Action,
) -> Result<CommandResult, Failure> {
    let action = engine.decode_action(actor, &action)?;
    act_as(engine, actor, action)
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
    let command = decode_command(
        engine,
        actor,
        &tor_protocol::Command::Wizard {
            expected_revision: engine.revision(actor)?,
            operation: operation.into(),
        },
    )?;
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

#[test]
fn copy_package_preserves_modular_regionless_source_closure() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    let copied = directory.path().join("copied");
    std::fs::create_dir_all(original.join("catalog/nested")).unwrap();
    std::fs::create_dir_all(original.join("rules/nested")).unwrap();
    std::fs::write(
        original.join("scenario.toml"),
        r#"
format = 17
id = "copy-compact"
version = "1.0"
ruleset = "rules/nested/base.toml"
modules = ["catalog/nested/world.toml"]
default_character = 1
[[characters]]
id = 1
anchor = "caves.depths/1/start"
turn_ticks = 100
"#,
    )
    .unwrap();
    std::fs::write(
        original.join("rules/nested/base.toml"),
        "import = \"interactions-v41\"\n",
    )
    .unwrap();
    std::fs::write(
        original.join("catalog/nested/world.toml"),
        r#"
namespace = "caves"
[generation_recipes."caves.rooms"]
kind = "grid_rooms"
version = 1
room_width = [4, 6]
room_height = [3, 5]
extra_connections = [0, 1]
[procedural_worlds."caves.depths"]
kind = "grid_chain"
version = 1
base_region = 1
grid = [1, 1]
size = [12, 10, 2]
recipe = "caves.rooms"
domain = { kind = "finite", count = 2 }
"#,
    )
    .unwrap();
    let expected = scenario_package::validate(&original).unwrap();
    assert!(!original.join("regions").exists());
    assert!(original.join(".compiled").exists());
    std::fs::write(original.join("unrelated.txt"), "not an authored input").unwrap();
    copy_package(&original, &copied);
    for file in [
        "scenario.toml",
        "catalog/nested/world.toml",
        "rules/nested/base.toml",
        "index.json",
        "validation.json",
    ] {
        assert_eq!(
            std::fs::read(original.join(file)).unwrap(),
            std::fs::read(copied.join(file)).unwrap(),
            "{file}"
        );
    }
    assert!(!copied.join("regions").exists());
    assert!(!copied.join(".compiled").exists());
    assert!(!copied.join("unrelated.txt").exists());
    assert_eq!(scenario_package::check(&copied).unwrap(), expected);
}

/// Copy authored inputs and optional index/certificate, never the compiled cache.
/// Module membership comes from the production resolver. Region inputs follow
/// its flat `regions/*.toml` layout; a compact package need not have that directory.
pub fn copy_package(from: &Path, to: &Path) {
    use std::collections::BTreeSet;
    use std::path::Component;

    let resolved = scenario_package::resolve_source(from).unwrap();
    let mut files: BTreeSet<_> = resolved.source_hashes.into_keys().collect();
    files.insert(resolved.manifest.ruleset);
    for name in ["index.json", "validation.json"] {
        if from.join(name).exists() {
            files.insert(name.to_owned());
        }
    }
    let regions = from.join("regions");
    match std::fs::symlink_metadata(&regions) {
        Ok(metadata) => {
            assert!(metadata.is_dir(), "regions must be a regular directory");
            for entry in std::fs::read_dir(&regions).unwrap() {
                let path = entry.unwrap().path();
                if path
                    .extension()
                    .is_some_and(|extension| extension == "toml")
                {
                    files.insert(format!(
                        "regions/{}",
                        path.file_name().unwrap().to_str().unwrap()
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("Cannot inspect region sources: {error}"),
    }
    assert!(
        files.len() <= 65_536 + 128,
        "Test source closure is too large"
    );
    let mut bytes = 0_u64;
    for name in files {
        let relative = Path::new(&name);
        assert!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(value) if value != ".compiled")),
            "Unsafe source path: {name}"
        );
        let mut source = from.to_path_buf();
        for part in relative.components() {
            source.push(part);
            let metadata = std::fs::symlink_metadata(&source).unwrap();
            assert!(!metadata.file_type().is_symlink(), "Symlink source: {name}");
        }
        let metadata = std::fs::symlink_metadata(&source).unwrap();
        assert!(metadata.is_file(), "Source must be a regular file: {name}");
        bytes = bytes
            .checked_add(metadata.len())
            .expect("Source byte overflow");
        assert!(
            bytes <= 256 * 1024 * 1024,
            "Test source closure is too large"
        );
        let destination = to.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(source, destination).unwrap();
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
