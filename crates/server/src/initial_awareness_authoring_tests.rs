//! Initial equipment belongs to construction, not a later concealment action.
use super::*;
use serde_json::{json, Value};
use tor_simulation::{ActorId, EquipmentSlotId};

fn fixture(equipped: bool) -> Package {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let original = read_package(&root).unwrap();
    let mut manifest = serde_json::to_value(&original.manifest).unwrap();
    manifest["archetypes"] = json!({"stealth": {
        "name":"stealth ring", "class":"ring", "stackable":false,
        "equipment":{"slot":"ring","modifiers":[{"scope":"always","op":{
            "kind":"perception","value":{"kind":"continual_stealth","edge":1}}}]}}});
    manifest["appearance_pools"] = json!({});
    manifest["factions"] = json!({"neutral":["foe"],"foe":["neutral"]});
    manifest["ai_profiles"] = json!({"watcher":{"idle":"wait"}});
    let mut stealth_species = manifest["creatures"]["species"]["figure"].clone();
    stealth_species["modifiers"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "scope":"always","op":{"kind":"set","stat":{"kind":"attribute",
        "attribute":"speed"},"value":100}}));
    manifest["creatures"]["species"]["sneaky"] = stealth_species;
    manifest["characters"][0]["creature"]["species"] = json!("sneaky");
    let mut loadout = json!({"archetype":"stealth","quantity":1});
    if equipped {
        loadout["equipped_slot"] = json!(2);
    }
    manifest["characters"][0]["loadout"] = json!([loadout]);
    let mut target = manifest["characters"][0]["creature"].clone();
    target["species"] = json!("figure");
    target["name"] = json!("watcher");
    target["faction"] = json!("foe");
    let region = serde_json::from_value(json!({"id":1,"name":"Initial cues",
        "size":[7,5,2],"chamber":true,"anchors":{"start":[1,2,0]},
        "actors":[{"id":2,"at":[4,2,0],"creature":target,
                    "controller":"ai","ai":"watcher"}]}))
    .unwrap();
    Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &original.rules_source,
    )
    .unwrap()
}

fn facts(game: &Game) -> (Value, Value) {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let table = checkpoint["actors"].as_u64().unwrap() as usize;
    let shared = serde_json::to_value(shared).unwrap();
    (checkpoint, shared["actors"][table].clone())
}

#[test]
fn starting_equipment_precedes_initial_encounter_receipts_and_rng() {
    let package = fixture(true);
    let index = package.index(42).unwrap();
    let regions = package.region_defs().unwrap();
    let regions: Vec<_> = regions.iter().collect();
    // Mirror the loaded construction boundary: actors/AI and run factions
    // exist before selected loadout and the first authoritative publication.
    let content = package.start(42).unwrap().content().clone();
    // The declared shell belongs to the streaming host; use an empty authored
    // world here to isolate the same entity/loadout publication boundary.
    let mut game = Game::new(World::new(vec![], vec![]).unwrap(), 42, content);
    game.defer_initial_awareness().unwrap();
    package.add_geometry(&mut game, &regions).unwrap();
    package
        .add_entities(&mut game, 42, &index, &regions)
        .unwrap();
    // Unlike Package::start this isolated world has no unbuilt actor directory;
    // configure_run therefore requires the selected actor to be installed.
    package.configure_run(&mut game, &index).unwrap();
    let constructed = game.clone();
    assert!(
        game.defer_initial_awareness().is_err(),
        "spawning closes setup admission"
    );
    assert_eq!(
        game, constructed,
        "rejected setup entry must not mutate an actor-bearing game"
    );
    let (before, actors) = facts(&game);
    assert_eq!(
        before["rules_rng"], 42,
        "construction cannot spend encounter RNG"
    );
    assert_eq!(actors["2"]["awareness"]["contacts"], json!([]));
    package
        .initialize_selected_character_with_index(&mut game, &index)
        .unwrap();
    assert_eq!(facts(&game).0["rules_rng"], 42);
    game.initialize_discovery().unwrap();
    let (published, actors) = facts(&game);
    assert_ne!(published["rules_rng"], 42);
    assert_eq!(
        actors["2"]["awareness"]["contacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(actors["2"]["awareness"]["contacts"][0]["detected"], false);
    let unchanged = facts(&game);
    game.initialize_discovery().unwrap();
    assert_eq!(
        facts(&game),
        unchanged,
        "publication/query cannot retry the encounter"
    );
    let published_game = game.clone();
    assert!(
        game.defer_initial_awareness().is_err(),
        "a live game cannot reenter setup"
    );
    assert_eq!(game, published_game, "rejected live setup entry is atomic");
    let complete = package.build(42, true).unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let saved = complete.checkpoint(&mut shared);
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(
        Game::restore_checkpoint(saved, &shared, complete.content().clone()).unwrap(),
        complete,
        "completed construction cannot retain an unpersisted setup gate"
    );
    assert_eq!(
        facts(&complete).1["2"]["awareness"]["contacts"][0]["detected"],
        false
    );
}

#[test]
fn equipping_after_initial_detection_does_not_erase_a_live_receipt() {
    let mut game = fixture(false).build(42, true).unwrap();
    assert_eq!(
        facts(&game).1["2"]["awareness"]["contacts"][0]["detected"],
        true
    );
    let item = game.observe(ActorId(1)).unwrap().inventory[0].id;
    game.equip_authored(ActorId(1), item, EquipmentSlotId(2))
        .unwrap();
    assert_eq!(
        facts(&game).1["2"]["awareness"]["contacts"][0]["detected"],
        true
    );
}
