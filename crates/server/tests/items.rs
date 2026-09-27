use std::path::Path;
use tor_protocol::{Action, ActorId};
use tor_server::journal::{Command, WizardOperation};
use tor_server::{scenario_package, Engine, SavePolicy, Scenario};

fn scenario(character: Option<u64>) -> Scenario {
    scenario_package::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items"),
        42,
        character,
        false,
    )
    .unwrap()
}
fn act(engine: &mut Engine, action: Action) {
    let actor = ActorId(1);
    let revision = engine.revision(actor).unwrap();
    engine
        .command(
            "player",
            "test",
            actor,
            &format!("action-{revision}"),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: revision,
                action,
            },
        )
        .unwrap();
}
fn wizard(engine: &mut Engine, operation: WizardOperation) {
    let actor = ActorId(1);
    let revision = engine.revision(actor).unwrap();
    engine
        .command(
            "wizard",
            "test",
            actor,
            &format!("wizard-{revision}"),
            &engine.branch().clone(),
            Command::Wizard {
                expected_revision: revision,
                operation,
            },
        )
        .unwrap();
}

#[test]
fn quantities_disclosure_restart_and_rewind_use_ordinary_packages() {
    for checkpoint_interval in [1, 100] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("items.db");
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, scenario(None), policy.clone()).unwrap();
        let initial_observation = engine.state(ActorId(1)).unwrap().observation;
        let initial = serde_json::to_string(&engine.state(ActorId(1)).unwrap()).unwrap();
        assert!(
            !initial.contains("healing")
                && !initial.contains("poison")
                && !initial.contains("quality")
        );
        act(
            &mut engine,
            Action::Take {
                item: 10,
                quantity: Some(3),
            },
        );
        let split = engine.state(ActorId(1)).unwrap().observation.inventory[0].id;
        assert_eq!(split, 23);
        act(
            &mut engine,
            Action::Take {
                item: 11,
                quantity: None,
            },
        );
        act(
            &mut engine,
            Action::Drop {
                item: split,
                quantity: Some(2),
            },
        );
        assert_eq!(
            engine.state(ActorId(1)).unwrap().observation.inventory[0].quantity,
            6
        );
        engine.enable_wizard().unwrap();
        let before_knowledge = engine.state(ActorId(1)).unwrap();
        wizard(
            &mut engine,
            WizardOperation::IdentifyItem {
                actor: ActorId(1),
                item: 20,
            },
        );
        let known = engine.state(ActorId(1)).unwrap();
        let history =
            serde_json::to_string(&engine.history(ActorId(1), "player", None, 50).unwrap())
                .unwrap();
        assert!(
            !history.contains("poison")
                && !history.contains("healing")
                && !history.contains("quality")
        );
        assert_eq!(
            known
                .observation
                .ground_items
                .iter()
                .find(|i| i.item.id == 22)
                .unwrap()
                .item
                .name,
            "potion of healing"
        );
        assert_eq!(
            known
                .observation
                .ground_items
                .iter()
                .find(|i| i.item.id == 21)
                .unwrap()
                .item
                .name,
            "red potion"
        );
        act(
            &mut engine,
            Action::Take {
                item: 20,
                quantity: None,
            },
        );
        act(
            &mut engine,
            Action::Drop {
                item: 20,
                quantity: None,
            },
        );
        let expected = engine.state(ActorId(1)).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let mut resumed = Engine::open_with_policy(&path, Scenario::two_room(99), policy).unwrap();
        assert_eq!(resumed.state(ActorId(1)).unwrap(), expected);
        resumed.enable_wizard().unwrap();
        wizard(&mut resumed, WizardOperation::Rewind { target: None });
        let rewound = resumed.state(ActorId(1)).unwrap();
        assert_eq!(rewound.observation, initial_observation);
        assert!(rewound.observation.inventory.is_empty());
        assert!(!rewound
            .observation
            .ground_items
            .iter()
            .any(|i| i.item.name.contains("healing")));
        assert!(before_knowledge.wizard_game);
    }
}

#[test]
fn package_validation_rejects_bad_counts_properties_and_objective_stacks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let manifest = std::fs::read_to_string(root.join("scenario.toml")).unwrap();
    let regions = std::fs::read_to_string(root.join("regions.toml")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (m, r) in [
        (manifest.clone(), regions.replacen("quantity = 10", "quantity = 0", 1)),
        (manifest.replacen("stackable = true", "stackable = false", 1), regions.clone()),
        (manifest.replace("default_character = 1", "default_character = 1\nobjective = { anchor = \"1/start\", item = 10, disclosed = true, continue_play = false }"), regions.clone()),
        (manifest.clone(), regions.replace("quality = \"fine\"", "\"\" = \"fine\"")),
    ] {
        std::fs::write(dir.path().join("scenario.toml"), m).unwrap();
        std::fs::write(dir.path().join("regions.toml"), r).unwrap();
        assert!(scenario_package::validate(dir.path()).is_err());
    }
}

#[test]
fn selected_character_knowledge_and_seed_mapping_are_deterministic() {
    let unknown = Engine::memory(scenario(None)).unwrap();
    let known = Engine::memory(scenario(Some(2))).unwrap();
    assert!(
        !unknown
            .state(ActorId(1))
            .unwrap()
            .observation
            .ground_items
            .iter()
            .find(|i| i.item.id == 20)
            .unwrap()
            .item
            .identified
    );
    assert!(
        known
            .state(ActorId(2))
            .unwrap()
            .observation
            .ground_items
            .iter()
            .find(|i| i.item.id == 20)
            .unwrap()
            .item
            .identified
    );
    let dir = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    for file in ["scenario.toml", "regions.toml"] {
        std::fs::copy(root.join(file), dir.path().join(file)).unwrap();
    }
    let manifest = dir.path().join("scenario.toml");
    let content = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        content
            .replace("[\"red potion\"]", "[\"red potion\", \"blue potion\"]")
            .replace("confounding = true", "confounding = false"),
    )
    .unwrap();
    scenario_package::validate(dir.path()).unwrap();
    let mut mappings = std::collections::BTreeSet::new();
    for seed in 0..16 {
        let load = || {
            Engine::memory(scenario_package::load(dir.path(), seed, None, false).unwrap()).unwrap()
        };
        let a = load().state(ActorId(1)).unwrap().observation;
        let b = load().state(ActorId(1)).unwrap().observation;
        // Per-game opaque cell keys intentionally use a fresh disclosure salt.
        // Seed determinism concerns the item assignment, not those keys.
        assert_eq!(a.ground_items, b.ground_items);
        assert_eq!(a.inventory, b.inventory);
        let appearance = |id| {
            a.ground_items
                .iter()
                .find(|i| i.item.id == id)
                .unwrap()
                .item
                .appearance
                .clone()
        };
        assert_ne!(appearance(20), appearance(21));
        assert_eq!(appearance(20), appearance(22));
        mappings.insert(appearance(20));
    }
    assert_eq!(mappings.len(), 2);
    for invalid in [
        content.replace("confounding = true", "confounding = false"),
        content.replace(
            "known_identities = [\"healing\"]",
            "known_identities = [\"missing\"]",
        ),
    ] {
        std::fs::write(&manifest, invalid).unwrap();
        assert!(scenario_package::validate(dir.path()).is_err());
    }
}
