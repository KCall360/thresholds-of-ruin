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

fn projectile_stack_package(root: &Path) -> Scenario {
    std::fs::create_dir_all(root.join("regions")).unwrap();
    std::fs::copy(
        support::package("items").join("rules.toml"),
        root.join("rules.toml"),
    )
    .unwrap();
    std::fs::write(root.join("scenario.toml"), r#"
format = 28
id = "projectile-stack-identity"
version = "1.0"
ruleset = "rules.toml"
default_character = 1
characters = [{ id = 1, anchor = "1/start" }]

[archetypes.arrow]
class = "weapon"
name = "fixture arrow"
stackable = true
projectile = { family = "fixture_bow", recovery = "consumed_on_hit", damage = { components = [{ category = "keen", amount = { type = "fixed", value = 2 } }], primary = { category = "keen" } } }
"#).unwrap();
    std::fs::write(
        root.join("regions/1.toml"),
        r#"
id = 1
name = "Ammunition stacks"
size = [3, 3, 2]
chamber = true
anchors = { start = [1, 1, 0] }
items = [
    { id = 10, at = [1, 1, 0], archetype = "arrow", quantity = 3 },
    { id = 11, at = [1, 1, 0], archetype = "arrow", quantity = 2 }
]
"#,
    )
    .unwrap();
    scenario_package::validate(root).unwrap();
    scenario_package::load(root, 42, None, false).unwrap()
}

#[test]
fn projectile_take_drop_merges_publish_fresh_disclosed_identity_and_survive_retry_reopen() {
    use tor_protocol::{Action as WireAction, Event, HistoryContent};
    let actor = ActorId(1);
    for checkpoint_interval in [1, 4096] {
        for taking in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source");
            let scenario = projectile_stack_package(&source);
            let database = directory.path().join("projectiles.db");
            let policy = SavePolicy {
                checkpoint_interval,
                ..Default::default()
            };
            let mut engine = Engine::open_with_policy(&database, scenario, policy.clone()).unwrap();
            let initial = engine.state(actor).unwrap();
            assert_eq!(initial.observation.ground_items.len(), 2);
            let ground = initial
                .observation
                .ground_items
                .iter()
                .find(|row| row.item.quantity == 3)
                .unwrap()
                .item
                .id;
            let carried = initial
                .observation
                .ground_items
                .iter()
                .find(|row| row.item.quantity == 2)
                .unwrap()
                .item
                .id;
            support::act_disclosed(
                &mut engine,
                actor,
                WireAction::Take {
                    item: carried,
                    quantity: None,
                },
            )
            .unwrap();
            let setup = engine.state(actor).unwrap();
            assert_eq!(setup.observation.inventory.len(), 1);
            assert_eq!(
                (
                    setup.observation.inventory[0].id,
                    setup.observation.inventory[0].quantity
                ),
                (carried, 2)
            );
            let (old_source, old_destination, merged_total, remainder) = if taking {
                (ground, carried, 3, 2)
            } else {
                (carried, ground, 4, 1)
            };
            let action = if taking {
                WireAction::Take {
                    item: old_source,
                    quantity: Some(1),
                }
            } else {
                WireAction::Drop {
                    item: old_source,
                    quantity: Some(1),
                }
            };
            // Keep the exact decoded action and request identity for retry checks.
            let command = Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: engine.decode_action(actor, &action).unwrap(),
            };
            let branch = engine.branch().clone();
            let committed = engine
                .command(
                    "player",
                    "test",
                    actor,
                    "merge-ammunition",
                    &branch,
                    command.clone(),
                )
                .unwrap();
            assert!(!committed.duplicate);
            let expected = engine.state(actor).unwrap();
            let history = engine.history(actor, "player", None, 50).unwrap();
            let entry = history
                .entries
                .iter()
                .find(|entry| entry.id == committed.entry.id)
                .unwrap();
            let HistoryContent::Action {
                action: disclosed_action,
                event,
            } = &entry.content
            else {
                panic!("ordinary disclosed action history")
            };
            assert_eq!(disclosed_action, &action);
            let (source_item, result, transferred) = match event.as_ref() {
                Event::Taken {
                    item,
                    result,
                    quantity,
                } if taking => (*item, *result, *quantity),
                Event::Dropped {
                    item,
                    result,
                    quantity,
                } if !taking => (*item, *result, *quantity),
                other => panic!("expected the matching transfer event, got {other:?}"),
            };
            assert_eq!(source_item, old_source);
            assert_eq!(
                transferred, 1,
                "event quantity is the transferred amount, not the merged total"
            );
            assert_ne!(
                result, old_source,
                "ammunition merge allocates a fresh result"
            );
            assert_ne!(
                result, old_destination,
                "ammunition merge cannot reuse destination identity"
            );
            let all_items: Vec<_> = expected
                .observation
                .inventory
                .iter()
                .chain(
                    expected
                        .observation
                        .ground_items
                        .iter()
                        .map(|row| &row.item),
                )
                .collect();
            assert!(
                !all_items.iter().any(|item| item.id == old_destination),
                "the old destination disappears from both disclosed locations"
            );
            assert_eq!(
                all_items
                    .iter()
                    .find(|item| item.id == old_source)
                    .unwrap()
                    .quantity,
                remainder
            );
            assert_eq!(
                all_items
                    .iter()
                    .find(|item| item.id == result)
                    .unwrap()
                    .quantity,
                merged_total
            );
            assert_eq!(all_items.iter().map(|item| item.quantity).sum::<u64>(), 5);
            if taking {
                assert_eq!(expected.observation.inventory.len(), 1);
                assert_eq!(expected.observation.inventory[0].id, result);
            } else {
                assert_eq!(expected.observation.ground_items.len(), 1);
                assert_eq!(expected.observation.ground_items[0].item.id, result);
            }
            let stale = if taking {
                WireAction::Drop {
                    item: old_destination,
                    quantity: Some(1),
                }
            } else {
                WireAction::Take {
                    item: old_destination,
                    quantity: Some(1),
                }
            };
            assert_eq!(
                support::act_disclosed(&mut engine, actor, stale.clone())
                    .unwrap_err()
                    .code,
                tor_protocol::ErrorCode::InvalidAction
            );
            assert_eq!(
                engine.state(actor).unwrap(),
                expected,
                "stale destination cannot retarget or advance state"
            );
            assert_eq!(engine.history(actor, "player", None, 50).unwrap(), history);
            let duplicate = engine
                .command(
                    "player",
                    "another-client",
                    actor,
                    "merge-ammunition",
                    &branch,
                    command.clone(),
                )
                .unwrap();
            assert!(duplicate.duplicate);
            assert_eq!(duplicate.entry, committed.entry);
            assert_eq!(engine.state(actor).unwrap(), expected);
            assert_eq!(engine.history(actor, "player", None, 50).unwrap(), history);
            engine.flush().unwrap();
            drop(engine);
            // The authored source is unavailable; the saved closure supplies ProjectileSpec.
            std::fs::remove_dir_all(&source).unwrap();
            let mut reopened =
                Engine::open_with_policy(&database, Scenario::two_room(999), policy).unwrap();
            if checkpoint_interval == 4096 {
                assert!(
                    reopened.recovery_profile().records_replayed > 0,
                    "fresh result must survive actual journal replay"
                );
            }
            assert_eq!(reopened.state(actor).unwrap(), expected);
            assert_eq!(
                reopened.history(actor, "player", None, 50).unwrap(),
                history
            );
            let duplicate = reopened
                .command(
                    "player",
                    "reopened",
                    actor,
                    "merge-ammunition",
                    &branch,
                    command,
                )
                .unwrap();
            assert!(duplicate.duplicate);
            assert_eq!(duplicate.entry, committed.entry);
            assert_eq!(
                reopened.state(actor).unwrap(),
                expected,
                "retry preserves the exact fresh result"
            );
            assert_eq!(
                support::act_disclosed(&mut reopened, actor, stale)
                    .unwrap_err()
                    .code,
                tor_protocol::ErrorCode::InvalidAction
            );
            assert_eq!(reopened.state(actor).unwrap(), expected);
            assert_eq!(
                reopened.history(actor, "player", None, 50).unwrap(),
                history
            );
        }
    }
}

#[test]
fn package_validation_rejects_bad_counts_properties_and_objective_stacks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let manifest = std::fs::read_to_string(root.join("scenario.toml")).unwrap();
    let regions = std::fs::read_to_string(root.join("regions/1.toml")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("regions")).unwrap();
    std::fs::copy(root.join("rules.toml"), dir.path().join("rules.toml")).unwrap();
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
