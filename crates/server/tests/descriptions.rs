use tempfile::tempdir;
use tor_protocol::*;
use tor_server::{Engine, Scenario};

#[test]
fn appearance_is_disclosed_with_objects_and_survives_replay_without_changing_rules() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let view = engine.observation(ActorId(1)).unwrap();
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["visible_cells"][0]["material"], "stone");
    assert!(view
        .ground_items
        .iter()
        .all(|i| !i.item.description.is_empty()));
    assert!(view
        .ground_items
        .iter()
        .find(|i| i.item.name == "copper token")
        .unwrap()
        .item
        .description
        .contains("copper"));
    assert!(!json.to_string().contains("Gallery"));
    drop(engine);
    let resumed = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(resumed.observation(ActorId(1)).unwrap(), view);
    let archive: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(archive["ruleset"], "material-rims-v10");
}
