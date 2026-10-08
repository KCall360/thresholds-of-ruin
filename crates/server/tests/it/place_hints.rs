use tempfile::tempdir;
use tor_protocol::*;
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

fn setup(
    engine: &mut Engine,
    id: &str,
    operation: &str,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    let command = crate::support::decode_command(
        engine,
        ActorId(1),
        &tor_protocol::Command::Wizard {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            operation: operation.into(),
        },
    )?;
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
fn durable_places_are_disclosed_once_renamed_freely_replayed_and_rewound() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("places.db");
    let mut engine = Engine::open_with_policy(
        &path,
        Scenario::two_room(42),
        tor_server::SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )
    .unwrap();
    engine.enable_wizard().unwrap();
    let initial = engine.state(ActorId(1)).unwrap();
    assert_eq!(initial.observation.places.len(), 2);
    let place = initial.observation.places[0].clone();
    let command = Command::RenamePlace {
        expected_revision: initial.revision,
        key: place.key.clone(),
        name: "Hearth of Echoes".into(),
    };
    let branch = engine.branch().clone();
    let renamed = engine
        .command(
            "player",
            "text",
            ActorId(1),
            "rename",
            &branch,
            command.clone(),
        )
        .unwrap();
    assert!(
        engine
            .command("player", "text", ActorId(1), "rename", &branch, command)
            .unwrap()
            .duplicate
    );
    let state = engine.state(ActorId(1)).unwrap();
    assert_eq!(state.observation.tick, initial.observation.tick);
    assert_eq!(state.revision, initial.revision + 1);
    assert_eq!(state.observation.places[0].name, "Hearth of Echoes");
    for (id, key, name) in [
        ("unknown", "unseen", "Name"),
        ("blank", place.key.as_str(), " "),
        ("control", place.key.as_str(), "Bad\nName"),
    ] {
        let result = engine.command(
            "player",
            "text",
            ActorId(1),
            id,
            &branch,
            Command::RenamePlace {
                expected_revision: state.revision,
                key: key.into(),
                name: name.into(),
            },
        );
        assert!(result.is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    }
    setup(&mut engine, "room", "room 3 5 3 1 Secret author name").unwrap();
    setup(&mut engine, "hidden", "place 3 2 1 0 on").unwrap();
    assert_eq!(
        engine.observation(ActorId(1)).unwrap().places,
        state.observation.places
    );
    setup(&mut engine, "visit", "teleport 1 3 1 1 0").unwrap();
    let discovered = engine.observation(ActorId(1)).unwrap();
    assert_eq!(discovered.places.len(), 3);
    assert!(!serde_json::to_string(&discovered)
        .unwrap()
        .contains("Secret author name"));
    setup(&mut engine, "away", "teleport 1 1 1 1 0").unwrap();
    setup(&mut engine, "remove", "place 3 2 1 0 off").unwrap();
    assert_eq!(
        engine.observation(ActorId(1)).unwrap().places,
        discovered.places
    );
    let saved = engine.state(ActorId(1)).unwrap();
    engine.flush().unwrap();
    assert!(engine.save_status().checkpoint_bytes > 0);
    drop(engine);
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), saved);
    engine.enable_wizard().unwrap();
    setup(
        &mut engine,
        "rewind-name",
        &format!("rewind {}", renamed.entry.id.0),
    )
    .unwrap();
    assert_eq!(
        engine.observation(ActorId(1)).unwrap().places,
        state.observation.places
    );
    setup(&mut engine, "rewind-initial", "rewind initial").unwrap();
    assert_eq!(
        engine.observation(ActorId(1)).unwrap().places,
        initial.observation.places
    );
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
