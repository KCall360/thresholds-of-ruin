use crate::support;
use std::path::Path;
use tor_protocol::ActorId;
use tor_server::journal::Action;
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
    support::act(engine, action).unwrap();
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
        assert_eq!(
            split,
            engine
                .target_scope(ActorId(1))
                .item(tor_simulation::ItemId(23))
        );
        act(
            &mut engine,
            Action::Take {
                item: 11,
                quantity: None,
            },
        );
        support::act_disclosed(
            &mut engine,
            ActorId(1),
            tor_protocol::Action::Drop {
                item: split,
                quantity: Some(2),
            },
        )
        .unwrap();
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
                .find(|i| i.item.id
                    == engine
                        .target_scope(ActorId(1))
                        .item(tor_simulation::ItemId(22)))
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
                .find(|i| i.item.id
                    == engine
                        .target_scope(ActorId(1))
                        .item(tor_simulation::ItemId(21)))
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
    let regions = std::fs::read_to_string(root.join("regions/1.toml")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("regions")).unwrap();
    for (m, r) in [
        (manifest.clone(), regions.replacen("quantity = 10", "quantity = 0", 1)),
        (manifest.replacen("stackable = true", "stackable = false", 1), regions.clone()),
        (manifest.replace("default_character = 1", "default_character = 1\nobjective = { anchor = \"1/start\", item = 10, disclosed = true, continue_play = false }"), regions.clone()),
        (manifest.clone(), regions.replace("quality = \"fine\"", "\"\" = \"fine\"")),
    ] {
        std::fs::write(dir.path().join("scenario.toml"), m).unwrap();
        std::fs::write(dir.path().join("regions/1.toml"), r).unwrap();
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
            .find(|i| i.item.id
                == unknown
                    .target_scope(ActorId(1))
                    .item(tor_simulation::ItemId(20)))
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
            .find(|i| i.item.id
                == known
                    .target_scope(ActorId(2))
                    .item(tor_simulation::ItemId(20)))
            .unwrap()
            .item
            .identified
    );
    let dir = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    crate::support::copy_package(&root, dir.path());
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
        let a_engine = load();
        let b_engine = load();
        let a = a_engine.state(ActorId(1)).unwrap().observation;
        let mut b = b_engine.state(ActorId(1)).unwrap().observation;
        // Normalize only privacy references for these authored identities.
        // All item data, collection order and projected positions remain compared.
        for id in [10, 11, 12, 20, 21, 22] {
            let a_target = a_engine
                .target_scope(ActorId(1))
                .item(tor_simulation::ItemId(id));
            let b_target = b_engine
                .target_scope(ActorId(1))
                .item(tor_simulation::ItemId(id));
            for ground in &mut b.ground_items {
                if ground.item.id == b_target {
                    ground.item.id = a_target;
                }
            }
            for held in &mut b.inventory {
                if held.id == b_target {
                    held.id = a_target;
                }
            }
        }
        assert_eq!(a.ground_items, b.ground_items);
        assert_eq!(a.inventory, b.inventory);
        let appearance = |id: u64| {
            a.ground_items
                .iter()
                .find(|i| {
                    i.item.id
                        == a_engine
                            .target_scope(ActorId(1))
                            .item(tor_simulation::ItemId(id))
                })
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

#[test]
fn queued_take_retry_keeps_original_quantity_after_consumption_and_recovery() {
    for checkpoint_interval in [1, 100] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("retry.db");
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, scenario(None), policy.clone()).unwrap();
        let actor = ActorId(1);
        let branch = engine.branch().clone();
        let before = engine.state(actor).unwrap();
        let command = Command::AdmitIntention {
            expected_revision: before.revision,
            action: Action::Take {
                item: 10,
                quantity: None,
            },
        };
        let admitted = engine
            .command(
                "player",
                "test",
                actor,
                "take-original",
                &branch,
                command.clone(),
            )
            .unwrap();
        assert_eq!(
            engine.state(actor).unwrap(),
            before,
            "admission must not execute gameplay"
        );
        engine.execute_next_intention().unwrap().unwrap();
        let after = engine.state(actor).unwrap();
        assert!(!after
            .observation
            .ground_items
            .iter()
            .any(|entry| entry.item.id
                == engine
                    .target_scope(ActorId(1))
                    .item(tor_simulation::ItemId(10))));
        assert_eq!(after.observation.inventory[0].quantity, 10);
        let retry = engine
            .command(
                "player",
                "other-client",
                actor,
                "take-original",
                &branch,
                command.clone(),
            )
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, admitted.entry);
        assert_eq!(engine.state(actor).unwrap(), after);
        // An explicit count producing the same effect is a different request.
        let conflict = Command::AdmitIntention {
            expected_revision: before.revision,
            action: Action::Take {
                item: 10,
                quantity: Some(10),
            },
        };
        assert_eq!(
            engine
                .command("player", "test", actor, "take-original", &branch, conflict)
                .unwrap_err()
                .code,
            tor_protocol::ErrorCode::RequestConflict
        );
        engine.flush().unwrap();
        drop(engine);
        let mut recovered = Engine::open_with_policy(&path, scenario(None), policy).unwrap();
        assert_eq!(recovered.state(actor).unwrap(), after);
        let retry = recovered
            .command(
                "player",
                "reconnected",
                actor,
                "take-original",
                &branch,
                command,
            )
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, admitted.entry);
        assert_eq!(recovered.state(actor).unwrap(), after);
        assert!(recovered.execute_next_intention().unwrap().is_none());
    }
}
