use tempfile::tempdir;
use tor_protocol::*;
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

fn setup(
    engine: &mut Engine,
    id: &str,
    operation: &str,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    let command = Command::from_wire(&tor_protocol::Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: operation.into(),
    })?;
    let branch = engine.branch().clone();
    engine.command("developer", "text", ActorId(1), id, &branch, command)
}

fn hints(engine: &Engine) -> Vec<serde_json::Value> {
    let view = serde_json::to_value(engine.observation(ActorId(1)).unwrap()).unwrap();
    view["visible_cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|cell| cell["place_hint"] == true)
        .cloned()
        .collect()
}

#[test]
fn unnamed_hints_are_visible_only_with_their_cells_and_restore_on_replay_and_rewind() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("hints.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    assert_eq!(hints(&engine).len(), 2);
    assert_eq!(
        setup(&mut engine, "denied", "place 1 0 0 0 on")
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    setup(&mut engine, "room", "room 3 5 3 1 Internal name").unwrap();
    let before = engine.observation(ActorId(1)).unwrap();
    setup(&mut engine, "hidden", "place 3 2 1 0 on").unwrap();
    assert_eq!(engine.observation(ActorId(1)).unwrap(), before);
    setup(&mut engine, "visit", "teleport 1 3 1 1 0").unwrap();
    let marked = hints(&engine);
    assert_eq!(marked.len(), 1);
    assert_eq!(
        marked[0]["position"],
        serde_json::json!({"x":1,"y":0,"z":0})
    );
    assert!(marked[0].get("name").is_none());
    setup(&mut engine, "cover", "wall 3 2 1 0 closed").unwrap();
    assert!(hints(&engine).is_empty());
    setup(&mut engine, "uncover", "wall 3 2 1 0 open").unwrap();
    assert_eq!(hints(&engine), marked);
    let bytes = std::fs::read(&path).unwrap();
    assert!(setup(&mut engine, "invalid", "place 3 99 1 0 on").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let state = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    engine.enable_wizard().unwrap();
    setup(&mut engine, "clear", "place 3 2 1 0 off").unwrap();
    assert!(hints(&engine).is_empty());
    setup(&mut engine, "rewind", "rewind initial").unwrap();
    assert_eq!(hints(&engine).len(), 2);
}

#[test]
fn older_rules_do_not_gain_hints_or_allow_hint_mutations() {
    for ruleset in ["two-room-v1", "portal-sight-v2", "observer-scene-v3"] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("old.json");
        drop(Engine::open(&path, Scenario::two_room(42)).unwrap());
        let mut archive: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        archive["ruleset"] = ruleset.into();
        std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
        let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert!(hints(&engine).is_empty());
        engine.enable_wizard().unwrap();
        assert!(setup(&mut engine, "unsupported", "place 1 1 1 0 on").is_err());
    }
}

#[test]
fn rotated_and_repeated_occurrences_keep_opaque_anchor_identity() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    setup(&mut engine, "room", "room 3 5 3 2 Private").unwrap();
    setup(&mut engine, "hint", "place 3 1 2 1 on").unwrap();
    setup(&mut engine, "link", "connect 1 1 0 0 north 3 0 2 1 1").unwrap();
    let remote = hints(&engine)
        .into_iter()
        .find(|c| c["position"] == serde_json::json!({"x":0,"y":-3,"z":0}))
        .unwrap();
    setup(&mut engine, "occlude", "wall 1 1 0 0 closed").unwrap();
    assert!(!hints(&engine).iter().any(|c| c["key"] == remote["key"]));
    setup(&mut engine, "visit", "teleport 1 3 1 2 1").unwrap();
    assert!(hints(&engine)
        .iter()
        .any(|c| c["key"] == remote["key"]
            && c["position"] == serde_json::json!({"x":0,"y":0,"z":0})));
    setup(&mut engine, "loop-room", "room 4 3 3 1 Private loop").unwrap();
    setup(&mut engine, "loop-hint", "place 4 1 1 0 on").unwrap();
    setup(&mut engine, "loop", "join 4 2 0 0 east 4 0 0 0 0 3 1").unwrap();
    setup(&mut engine, "loop-visit", "teleport 1 4 1 1 0").unwrap();
    let repeated = hints(&engine);
    assert!(repeated.len() >= 3);
    assert!(repeated.iter().all(|c| c["key"] == repeated[0]["key"]));
    let encoded = serde_json::to_string(&engine.observation(ActorId(1)).unwrap()).unwrap();
    for forbidden in ["region", "portal", "Private", "quarter_turns"] {
        assert!(!encoded.contains(forbidden));
    }
}
