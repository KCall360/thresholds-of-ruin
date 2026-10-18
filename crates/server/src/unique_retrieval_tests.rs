//! Source policy, whole-host transaction and save-owned branch identity contracts.
use super::*;
use tor_simulation::unique_retrieval::{RetrievalResult, UniqueRetrievalState};

pub(super) fn attach_objective(mut scenario: Scenario) -> Scenario {
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["archetypes"]["amulet"] = json!({
        "name":"Amulet of Yendor", "class":"amulet", "identity":"amulet-of-yendor"
    });
    source["unique_retrieval"] = json!({
        "version":1, "protagonist":"selected_character", "family":"dungeon",
        "minimum_depth":26, "archetype":"amulet", "exit_anchor":"dungeon/1/up"
    });
    source["floor_activation"]["protagonist"] = json!("selected_character");
    let manifest: Manifest = serde_json::from_value(source)
        .expect("admit the narrow selected-character unique retrieval policy");
    let package = Package::from_parts(manifest, vec![], &original.rules_source).unwrap();
    package.check().unwrap();
    scenario.package = Some(Arc::new(package));
    scenario
}

fn at_depth(depth: u32) -> Scenario {
    start_at(fixture(), depth)
}

fn start_at(mut scenario: Scenario, depth: u32) -> Scenario {
    let original = scenario.package.as_ref().unwrap();
    let mut manifest = original.manifest.clone();
    manifest.characters[0].anchor = format!("dungeon/{depth}/up");
    scenario.package = Some(Arc::new(
        Package::from_parts(manifest, vec![], &original.rules_source).unwrap(),
    ));
    attach_objective(scenario)
}

fn retrieval(engine: &Engine) -> &UniqueRetrievalState {
    engine.game.unique_retrieval_state().unwrap()
}

fn act(engine: &mut Engine, action: Action) -> Result<CommandResult, Failure> {
    engine.command(
        "player",
        "retrieval-test",
        ActorId(1),
        &uuid::Uuid::new_v4().to_string(),
        &engine.branch().clone(),
        Command::Act {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            action,
        },
    )
}

fn take_bound_item(engine: &mut Engine) {
    let item = retrieval(engine).item.unwrap();
    let at = match retrieval(engine).entries.last().unwrap().result {
        RetrievalResult::Created { at, .. } | RetrievalResult::Relocated { at, .. } => at,
        _ => panic!("item must have a placement"),
    };
    wizard(
        engine,
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: Position {
                region: at.region.0,
                x: at.position.x,
                y: at.position.y,
                z: at.position.z,
            },
        },
    );
    act(
        engine,
        Action::Take {
            item: item.0,
            quantity: None,
        },
    )
    .unwrap();
    assert!(engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .inventory
        .iter()
        .any(|entry| entry.id == item));
}

#[test]
fn unique_retrieval_source_admits_a_selected_branch_local_amulet_policy() {
    let scenario = at_depth(25);
    assert_eq!(
        scenario
            .package
            .as_ref()
            .unwrap()
            .manifest
            .unique_retrieval
            .as_ref()
            .unwrap()
            .minimum_depth,
        26
    );
}

#[test]
fn reserved_retrieval_definition_rejects_all_source_birth_channels() {
    let scenario = at_depth(25);
    let original = scenario.package.as_ref().unwrap();
    for change in [
        "stackable",
        "class",
        "identity_alias",
        "kit",
        "floor",
        "room_floor",
        "unused_pool",
        "version",
        "depth",
        "protagonist",
        "family",
        "exit",
        "inert",
        "short_finite",
        "short_seeded",
    ] {
        let mut source = serde_json::to_value(&original.manifest).unwrap();
        match change {
            "stackable" => source["archetypes"]["amulet"]["stackable"] = json!(true),
            "class" => source["archetypes"]["amulet"]["class"] = json!("misc"),
            "identity_alias" => {
                source["archetypes"]["duplicate"] = source["archetypes"]["amulet"].clone()
            }
            "kit" => {
                source["characters"][0]["loadout"] = json!([{"archetype":"amulet","quantity":1}])
            }
            "floor" => {
                source["floor_activation"]["profiles"]["baseline"]["stages"][0]["bands"][0]
                    ["choices"][0]["archetype"] = json!("amulet")
            }
            "room_floor" => {
                source["floor_activation"]["profiles"]["baseline"]["stages"][0] = json!({
                    "kind":"room_items", "id":"reserved-room", "version":1,
                    "chance":{"numerator":0,"denominator":1}, "quantity":1,
                    "bands":[{"depth":{"kind":"open","minimum":1},
                        "choices":[{"archetype":"amulet","weight":1}]}]
                });
            }
            "unused_pool" => {
                source["floor_activation"]["carried_item_pools"] = json!({"never-used":{"version":1,"choices":[{"archetype":"amulet","weight":1,"quantity":1}]}})
            }
            "version" => source["unique_retrieval"]["version"] = json!(2),
            "depth" => source["unique_retrieval"]["minimum_depth"] = json!(25),
            "protagonist" => source["unique_retrieval"]["protagonist"] = json!(999),
            "family" => source["unique_retrieval"]["family"] = json!("absent"),
            "exit" => source["unique_retrieval"]["exit_anchor"] = json!("dungeon/2/up"),
            "inert" => source["floor_activation"] = Value::Null,
            "short_finite" => {
                source["procedural_worlds"]["dungeon"]["domain"] =
                    json!({"kind":"finite","count":25})
            }
            "short_seeded" => {
                source["procedural_worlds"]["dungeon"]["domain"] =
                    json!({"kind":"seeded_finite","min":25,"max":27})
            }
            _ => unreachable!(),
        }
        let invalid = Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap();
        assert!(
            invalid.check().is_err(),
            "reserved policy mutation {change}"
        );
    }
}

#[test]
fn deep_creation_revisits_and_carried_entry_preserve_one_exact_identity() {
    let mut engine = Engine::memory(at_depth(25)).unwrap();
    assert!(retrieval(&engine).item.is_none());
    walk_to_port(&mut engine, 25, "down");
    step(&mut engine, Direction::Down);
    let item = retrieval(&engine).item.unwrap();
    assert_eq!(
        (
            retrieval(&engine).entry_count,
            retrieval(&engine).placement_count
        ),
        (1, 1)
    );
    walk_to_port(&mut engine, 26, "down");
    step(&mut engine, Direction::Down);
    let population = decisions(&engine);
    step(&mut engine, Direction::Up);
    assert_eq!(retrieval(&engine).item, Some(item));
    assert_eq!(
        (
            retrieval(&engine).entry_count,
            retrieval(&engine).placement_count
        ),
        (3, 3)
    );
    assert_eq!(
        decisions(&engine),
        population,
        "repeatable retrieval never repopulates"
    );
    take_bound_item(&mut engine);
    walk_to_port(&mut engine, 26, "up");
    step(&mut engine, Direction::Up);
    step(&mut engine, Direction::Down);
    assert_eq!(retrieval(&engine).item, Some(item));
    assert_eq!(
        (
            retrieval(&engine).entry_count,
            retrieval(&engine).placement_count
        ),
        (4, 3)
    );
    assert!(
        matches!(retrieval(&engine).entries[0].result, RetrievalResult::AlreadyCarried { item: exact } if exact == item)
    );
    restored_both_ways(&engine);
}

#[test]
fn selected_second_character_initial_deep_entry_happens_once_on_restore() {
    let mut scenario = at_depth(26);
    let original = scenario.package.as_ref().unwrap();
    let mut manifest = original.manifest.clone();
    let mut second = manifest.characters[0].clone();
    second.id = 2;
    manifest.characters.push(second);
    let mut package = Package::from_parts(manifest, vec![], &original.rules_source).unwrap();
    package.selected = 2;
    scenario.package = Some(Arc::new(package));
    let engine = Engine::memory(scenario).unwrap();
    assert!(
        engine.game.observe(SimActor(1)).is_err(),
        "the unselected character is omitted"
    );
    assert_eq!(retrieval(&engine).policy.protagonist, SimActor(2));
    assert_eq!(
        (
            retrieval(&engine).entry_count,
            retrieval(&engine).placement_count
        ),
        (1, 1)
    );
    assert_eq!(engine.archive.initial_retrievals.len(), 1);
    restored_both_ways(&engine);
}

#[test]
fn source_free_saved_restarts_preserve_exact_retrieval_identity() {
    for checkpoint_interval in [0, 1] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retrieval.db");
        let mut engine = Engine::open_with_policy(
            &path,
            at_depth(26),
            crate::SavePolicy {
                checkpoint_interval,
                ..Default::default()
            },
        )
        .unwrap();
        let item = retrieval(&engine).item;
        take_bound_item(&mut engine);
        walk_to_port(&mut engine, 26, "down");
        step(&mut engine, Direction::Down);
        engine.flush().unwrap();
        let expected = Engine::replay(engine.archive.clone(), None, None).unwrap();
        drop(engine);
        let reopened = Engine::open(&path, Scenario::two_room(999)).unwrap();
        assert_eq!(
            reopened.recovery_profile().checkpoint_sequence > 0,
            checkpoint_interval > 0,
            "exercise both real storage recovery paths"
        );
        assert_eq!(retrieval(&reopened).item, item);
        assert_eq!(retrieval(&reopened).placement_count, 1);
        exact_equal(&reopened, &expected);
    }
}

#[test]
fn rewind_before_creation_allocates_above_abandoned_identity_and_never_rebirths_after_creation() {
    let mut engine = Engine::memory(at_depth(25)).unwrap();
    walk_to_port(&mut engine, 25, "down");
    let before_creation = engine.archive.records.last().unwrap().entry.id.clone();
    step(&mut engine, Direction::Down);
    let first_item = retrieval(&engine).item.unwrap();
    let after_creation = engine.archive.records.last().unwrap().entry.id.clone();
    take_bound_item(&mut engine);
    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(after_creation),
        },
    );
    assert_eq!(retrieval(&engine).item, Some(first_item));
    assert!(retrieval(&engine).entries.is_empty());
    assert!(!engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .inventory
        .iter()
        .any(|item| item.id == first_item));
    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(before_creation),
        },
    );
    assert!(retrieval(&engine).item.is_none());
    step(&mut engine, Direction::Down);
    assert!(retrieval(&engine).item.unwrap().0 > first_item.0);
    assert_eq!(
        (
            retrieval(&engine).entry_count,
            retrieval(&engine).placement_count
        ),
        (1, 1)
    );
    restored_both_ways(&engine);
}

#[test]
fn artifact_allocation_exhaustion_rolls_back_population_and_nested_stock() {
    let scenario = start_at(super::carried_stock::carrier_fixture(26), 25);
    let mut engine = Engine::memory(scenario).unwrap();
    walk_to_port(&mut engine, 25, "down");
    // Both floor-item births and the actor's carried stack fit; the Amulet
    // allocation is the fourth birth and fails after the complete population.
    engine.game.reserve_identities(1, u64::MAX - 3, 1);
    let admission = admit(&mut engine, "artifact-capacity", Direction::Down);
    let before = engine.diagnostic_copy();
    for _ in 0..2 {
        let failure = engine
            .execute_next_intention()
            .expect_err("late artifact birth must fail");
        assert_eq!(failure.code, ErrorCode::ResourceLimit);
        exact_equal(&engine, &before);
        assert_eq!(authority(&engine), authority(&before));
        assert_eq!(
            engine.pending_intentions(ActorId(1))[0].entry_id,
            admission.entry.id
        );
        assert!(retrieval(&engine).item.is_none());
    }
}

#[test]
fn late_storage_rejection_discards_artifact_population_stock_and_every_store_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("artifact-storage.db");
    let mut engine = Engine::open(
        &path,
        start_at(super::carried_stock::carrier_fixture(26), 25),
    )
    .unwrap();
    walk_to_port(&mut engine, 25, "down");
    admit(&mut engine, "artifact-storage", Direction::Down);
    engine.flush().unwrap();
    drop(engine);
    let mut rejected = Engine::open_with_policy(
        &path,
        Scenario::two_room(999),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let before = rejected.diagnostic_copy();
    let rows = super::deep_atomicity::logical_store_rows(&path);
    let mut healthy = Engine::replay(rejected.archive.clone(), None, None).unwrap();
    healthy.execute_next_intention().unwrap().unwrap();
    assert!(retrieval(&healthy).item.is_some());
    assert!(rejected.execute_next_intention().is_err());
    exact_equal(&rejected, &before);
    assert_eq!(authority(&rejected), authority(&before));
    rejected.flush().unwrap();
    assert_eq!(super::deep_atomicity::logical_store_rows(&path), rows);
    drop(before);
    drop(rejected);
    let mut retry = Engine::open(&path, Scenario::two_room(999)).unwrap();
    retry.execute_next_intention().unwrap().unwrap();
    assert_eq!(retry.game, healthy.game);
    assert_eq!(retry.revisions, healthy.revisions);
    assert_eq!(retry.receipts, healthy.receipts);
    assert_eq!(
        retry.archive.records.last().unwrap().activations,
        healthy.archive.records.last().unwrap().activations
    );
    assert_eq!(
        retry.archive.records.last().unwrap().retrievals,
        healthy.archive.records.last().unwrap().retrievals
    );
    restored_both_ways(&retry);
}

fn expired_created_receipt() -> (Engine, usize) {
    let mut engine =
        Engine::memory(start_at(super::carried_stock::carrier_fixture(26), 25)).unwrap();
    walk_to_port(&mut engine, 25, "down");
    let target = engine.archive.records.last().unwrap().entry.id.clone();
    step(&mut engine, Direction::Down);
    let abandoned = engine.archive.records.len() - 1;
    let abandoned_id = engine.archive.records[abandoned].entry.id.clone();
    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(target),
        },
    );
    for _ in 0..REWIND_BOUNDARIES {
        act(&mut engine, Action::Wait).unwrap();
    }
    assert!(engine
        .boundaries
        .iter()
        .all(|boundary| boundary.id.as_ref() != Some(&abandoned_id)));
    assert!(retrieval(&engine).item.is_none());
    restored_both_ways(&engine);
    (engine, abandoned)
}

#[test]
fn expired_artifact_origins_share_the_population_and_carried_stock_identity_audit() {
    use tor_simulation::floor_activation::PlacementDecision;
    let (engine, abandoned) = expired_created_receipt();
    let stock = engine.archive.records[abandoned].activations[0]
        .decisions
        .iter()
        .flat_map(|stage| stage.result.attempts())
        .find_map(|attempt| match attempt {
            PlacementDecision::Placed { entity, .. } if entity.actor_birth().is_some() => {
                entity.item_birth()
            }
            _ => None,
        })
        .expect("abandoned population has a real nested stock birth");
    for mutation in [
        "missing",
        "duplicate",
        "tick",
        "stock_alias",
        "ordinal",
        "placement",
    ] {
        let mut archive = engine.archive.clone();
        let batch = &mut archive.records[abandoned].retrievals;
        match mutation {
            "missing" => batch.clear(),
            "duplicate" => batch.push(batch[0].clone()),
            "tick" => batch[0].tick = engine.game.tick() + 1,
            "ordinal" => batch[0].ordinal += 1,
            "stock_alias" => match &mut batch[0].result {
                RetrievalResult::Created { item, .. } => *item = stock,
                _ => unreachable!(),
            },
            "placement" => match &mut batch[0].result {
                RetrievalResult::Created { placement, .. } => *placement += 1,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        if mutation == "stock_alias" {
            archive.records[abandoned]
                .retrieval_after
                .as_mut()
                .unwrap()
                .item = Some(stock);
            archive.records[abandoned + 1]
                .retrieval_before
                .as_mut()
                .unwrap()
                .item = Some(stock);
        }
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "journal {mutation}"
        );
        let disk =
            Checkpoint::capture(&engine).encode("expired-artifact", archive.records.len() as u64);
        assert!(disk.restore(archive).is_err(), "checkpoint {mutation}");
    }
}

#[test]
fn annotations_preserve_the_current_entry_snapshot_but_never_repeat_its_receipt() {
    let mut engine = Engine::memory(at_depth(26)).unwrap();
    let original = engine.game.clone();
    let revision = engine.revision(ActorId(1)).unwrap();
    engine
        .command(
            "player",
            "retrieval-test",
            ActorId(1),
            "note",
            &engine.branch().clone(),
            Command::Annotate {
                anchor: tor_protocol::Anchor::State { revision },
                text: "An ordinary note".into(),
                source: tor_protocol::ClientSource::User,
                audience: tor_protocol::Audience::Actor,
                category: tor_protocol::AnnotationCategory::Note,
            },
        )
        .unwrap();
    assert_eq!(engine.game, original);
    assert!(engine.archive.records.last().unwrap().retrievals.is_empty());
    restored_both_ways(&engine);
}

#[test]
fn retrieval_summaries_are_required_exact_private_pre_and_post_states() {
    let (engine, abandoned) = expired_created_receipt();
    for field in ["retrieval_before", "retrieval_after"] {
        let mut missing = serde_json::to_value(&engine.archive).unwrap();
        missing["records"][abandoned]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<Archive>(missing).is_err(),
            "missing {field}"
        );
        for part in ["initialized", "entry_count", "placement_count", "item"] {
            let mut value = serde_json::to_value(&engine.archive).unwrap();
            let summary = &mut value["records"][abandoned][field];
            summary[part] = if part == "initialized" {
                json!(false)
            } else {
                json!(999_999)
            };
            let forged: Archive = serde_json::from_value(value).unwrap();
            assert!(
                Engine::replay(forged.clone(), None, None).is_err(),
                "journal {field}.{part}"
            );
            let disk = Checkpoint::capture(&engine)
                .encode("forged-retrieval-summary", forged.records.len() as u64);
            assert!(disk.restore(forged).is_err(), "checkpoint {field}.{part}");
        }
    }
}

#[test]
fn expired_shallow_ground_source_is_validated_against_its_own_bounds() {
    use tor_simulation::unique_retrieval::RetrievalSource;
    let mut engine = Engine::memory(at_depth(26)).unwrap();
    let item = retrieval(&engine).item.unwrap();
    take_bound_item(&mut engine);
    walk_to_port(&mut engine, 26, "up");
    step(&mut engine, Direction::Up);
    act(
        &mut engine,
        Action::Drop {
            item: item.0,
            quantity: None,
        },
    )
    .unwrap();
    let target = engine.archive.records.last().unwrap().entry.id.clone();
    step(&mut engine, Direction::Down);
    let abandoned = engine.archive.records.len() - 1;
    let RetrievalResult::Relocated {
        from: RetrievalSource::Ground(source),
        ..
    } = retrieval(&engine).entries[0].result
    else {
        panic!("dropped shallow source must relocate on actual deep reentry");
    };
    let family = &engine
        .archive
        .scenario
        .package
        .as_ref()
        .unwrap()
        .manifest
        .procedural_worlds["dungeon"];
    assert_eq!(family.address(source.region.0).unwrap().0, 25);
    restored_both_ways(&engine);
    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(target),
        },
    );
    for _ in 0..REWIND_BOUNDARIES {
        act(&mut engine, Action::Wait).unwrap();
    }
    restored_both_ways(&engine);
    let mut archive = engine.archive.clone();
    let RetrievalResult::Relocated {
        from: RetrievalSource::Ground(source),
        ..
    } = &mut archive.records[abandoned].retrievals[0].result
    else {
        unreachable!()
    };
    source.position.x = 1000;
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    let disk =
        Checkpoint::capture(&engine).encode("expired-shallow-source", archive.records.len() as u64);
    assert!(disk.restore(archive).is_err());
}

#[test]
fn surface_escape_requires_the_bound_item_and_an_intentional_ordinary_up() {
    let mut engine = Engine::memory(at_depth(26)).unwrap();
    let item = retrieval(&engine).item.unwrap();
    let deep = engine.game.observe(SimActor(1)).unwrap().location;
    let exit = retrieval(&engine).policy.exit.at;
    let teleport = |engine: &mut Engine, at: Location| {
        wizard(
            engine,
            WizardOperation::Teleport {
                actor: ActorId(1),
                position: Position {
                    region: at.region.0,
                    x: at.position.x,
                    y: at.position.y,
                    z: at.position.z,
                },
            },
        );
    };
    teleport(&mut engine, exit);
    assert!(!engine.game.run_outcome().terminal);
    let before = engine.diagnostic_copy();
    assert!(act(
        &mut engine,
        Action::Move {
            direction: Direction::Up
        }
    )
    .is_err());
    exact_equal(&engine, &before);
    teleport(&mut engine, deep);
    take_bound_item(&mut engine);
    teleport(&mut engine, exit);
    assert!(
        !engine.game.run_outcome().terminal,
        "arrival alone must not win"
    );
    act(&mut engine, Action::Wait).unwrap();
    assert!(!engine.game.run_outcome().terminal, "waiting must not win");
    let result = act(
        &mut engine,
        Action::Move {
            direction: Direction::Up,
        },
    )
    .unwrap();
    assert!(matches!(result.entry.content, JournalContent::Action {
        event: crate::journal::Event::Escaped { item: escaped }, ..
    } if escaped == item.0));
    assert_eq!(engine.game.run_outcome().victor, Some(SimActor(1)));
    assert!(engine.game.run_outcome().terminal);
    assert_eq!(engine.game.observe(SimActor(1)).unwrap().location, exit);
    restored_both_ways(&engine);
}

#[test]
fn wizard_birth_cannot_duplicate_a_reserved_identity_but_may_share_its_display_name() {
    for reserved_identity in [true, false] {
        let mut scenario = at_depth(25);
        let original = scenario.package.as_ref().unwrap();
        let mut manifest = original.manifest.clone();
        let amulet = manifest.archetypes.get_mut("amulet").unwrap();
        if reserved_identity {
            amulet.identity = Some("copper token".into());
        } else {
            amulet.name = Some("copper token".into());
        }
        scenario.package = Some(Arc::new(
            Package::from_parts(manifest, vec![], &original.rules_source).unwrap(),
        ));
        let mut engine = Engine::memory(scenario).unwrap();
        engine.enable_wizard().unwrap();
        let before = engine.diagnostic_copy();
        let location = engine.game.observe(SimActor(1)).unwrap().location;
        let result = engine.command(
            "player",
            "retrieval-test",
            ActorId(1),
            "wizard-birth",
            &engine.branch().clone(),
            Command::Wizard {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                operation: WizardOperation::PlaceItem {
                    kind: crate::journal::WizardItem::Token,
                    position: Position {
                        region: location.region.0,
                        x: location.position.x,
                        y: location.position.y,
                        z: location.position.z,
                    },
                },
            },
        );
        if reserved_identity {
            assert!(
                result.is_err(),
                "wizard birth must reject the exact reserved identity"
            );
            exact_equal(&engine, &before);
        } else {
            result.unwrap();
            assert!(retrieval(&engine).item.is_none());
            restored_both_ways(&engine);
        }
    }
}

#[test]
fn expired_artifact_creation_cannot_alias_a_later_wizard_item_origin() {
    let (mut engine, abandoned) = expired_created_receipt();
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let placed = wizard(
        &mut engine,
        WizardOperation::PlaceItem {
            kind: crate::journal::WizardItem::Token,
            position: Position {
                region: at.region.0,
                x: at.position.x,
                y: at.position.y,
                z: at.position.z,
            },
        },
    );
    let JournalContent::Wizard {
        result: WizardResult::ItemPlaced { item: wizard_item },
        ..
    } = placed.entry.content
    else {
        panic!("wizard must allocate an ordinary item");
    };
    restored_both_ways(&engine);
    let mut archive = engine.archive.clone();
    let alias = tor_simulation::ItemId(wizard_item);
    let RetrievalResult::Created { item, .. } =
        &mut archive.records[abandoned].retrievals[0].result
    else {
        unreachable!()
    };
    assert_ne!(*item, alias);
    *item = alias;
    archive.records[abandoned]
        .retrieval_after
        .as_mut()
        .unwrap()
        .item = Some(alias);
    archive.records[abandoned + 1]
        .retrieval_before
        .as_mut()
        .unwrap()
        .item = Some(alias);
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    let disk =
        Checkpoint::capture(&engine).encode("expired-wizard-alias", archive.records.len() as u64);
    assert!(disk.restore(archive).is_err());
}

#[test]
fn eligible_floor_visibility_preload_queries_and_restore_do_not_create_the_item() {
    let mut engine = Engine::memory(at_depth(25)).unwrap();
    walk_to_port(&mut engine, 25, "down");
    let family = &engine
        .archive
        .scenario
        .package
        .as_ref()
        .unwrap()
        .manifest
        .procedural_worlds["dungeon"];
    assert!(
        engine.game.loaded_regions().any(|region| family
            .address(region.0)
            .is_some_and(|(depth, _)| depth == 26)),
        "the negative control must publish an eligible unentered floor"
    );
    let before = engine.game.clone();
    engine.start_preloading();
    engine.settle_preloading();
    engine.state(ActorId(1)).unwrap();
    engine.history(ActorId(1), "player", None, 20).unwrap();
    engine.game.observe(SimActor(1)).unwrap();
    assert_eq!(engine.game, before);
    assert!(retrieval(&engine).item.is_none());
    assert_eq!(retrieval(&engine).entry_count, 0);
    restored_both_ways(&engine);
}

#[test]
fn one_host_action_keeps_two_physical_entries_through_late_cohort_completion() {
    use tor_simulation::floor_activation::EntryCause;
    let mut scenario = at_depth(25);
    let original = scenario.package.as_ref().unwrap();
    let mut manifest = original.manifest.clone();
    manifest.procedural_worlds.get_mut("dungeon").unwrap().grid = [1, 1];
    manifest.characters[0].turn_ticks = 1000;
    let package = Package::from_parts(manifest, vec![], &original.rules_source).unwrap();
    let family = &package.manifest.procedural_worlds["dungeon"];
    let regions: Vec<_> = (25..=27)
        .map(|depth| {
            family
                .group("dungeon", scenario.seed, depth)
                .unwrap()
                .members[0]
        })
        .collect();
    let width = family.size[0];
    scenario.package = Some(Arc::new(package));
    let at = |region, x| Position {
        region,
        x,
        y: 3,
        z: 0,
    };
    let mut engine = Engine::memory(scenario).unwrap();
    for &region in &regions {
        wizard(
            &mut engine,
            WizardOperation::SetGravity {
                region,
                vector: [0; 3],
            },
        );
        for x in 0..width {
            wizard(
                &mut engine,
                WizardOperation::SetWall {
                    position: at(region, x),
                    wall: false,
                },
            );
        }
    }
    // Physical portals need true boundary exits. The carved row has no
    // interior topology shortcut and reaches each next floor at its far edge.
    for pair in regions.windows(2) {
        wizard(
            &mut engine,
            WizardOperation::ConnectPortal {
                from: at(pair[0], width - 1),
                direction: Direction::East,
                to: at(pair[1], 0),
                rotation: 0,
                width: 1,
                height: 1,
            },
        );
    }
    wizard(
        &mut engine,
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: at(regions[0], width - 1),
        },
    );
    let spawned = wizard(
        &mut engine,
        WizardOperation::SpawnActor {
            position: at(regions[0], width - 2),
            turn_ticks: 1000,
        },
    );
    let JournalContent::Wizard {
        result: WizardResult::ActorSpawned { actor: driver },
        ..
    } = spawned.entry.content
    else {
        unreachable!()
    };
    wizard(
        &mut engine,
        WizardOperation::SetVelocity {
            actor: ActorId(1),
            velocity: [8_192, 0, 0],
        },
    );
    act(&mut engine, Action::Wait).unwrap();
    assert_eq!(engine.next_actor(), Some(driver));
    assert!(retrieval(&engine).item.is_none());
    let before = engine.archive.records.len();
    engine
        .command(
            "player",
            "retrieval-test",
            driver,
            "physical-cohort",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(driver).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    assert_eq!(engine.archive.records.len(), before + 1);
    let record = engine.archive.records.last().unwrap();
    assert_eq!(
        record.retrievals.len(),
        2,
        "every physical entry belongs to this one record"
    );
    assert_eq!(
        record
            .retrievals
            .iter()
            .map(|entry| entry.depth)
            .collect::<Vec<_>>(),
        [26, 27]
    );
    assert!(record
        .retrievals
        .iter()
        .all(|entry| entry.cause == EntryCause::Physics
            && entry.tick >= record.before_tick
            && entry.tick <= record.after_tick));
    assert!(record.retrievals[0].tick < record.retrievals[1].tick);
    assert_eq!(
        record.retrievals[0].result.item(),
        record.retrievals[1].result.item()
    );
    assert_eq!(
        (
            retrieval(&engine).entry_count,
            retrieval(&engine).placement_count
        ),
        (2, 2)
    );
    assert_eq!(retrieval(&engine).entries, record.retrievals);
    restored_both_ways(&engine);
}

#[path = "unique_retrieval_preload_tests.rs"]
mod preload;

#[test]
fn missing_retrieval_receipt_is_rejected_before_candidate_and_store_publication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing-retrieval-delta.db");
    let mut engine = Engine::open(
        &path,
        start_at(super::carried_stock::carrier_fixture(26), 25),
    )
    .unwrap();
    walk_to_port(&mut engine, 25, "down");
    engine.flush().unwrap();
    let before = engine.diagnostic_copy();
    let rows = super::deep_atomicity::logical_store_rows(&path);
    let action = Action::Move {
        direction: Direction::Down,
    };
    let mut healthy = Engine::replay(engine.archive.clone(), None, None).unwrap();
    act(&mut healthy, action.clone()).unwrap();
    assert!(retrieval(&healthy).item.is_some());

    let mut candidate = engine.capture_command_candidate(None);
    let outcome = engine
        .resolve_action_transition(&mut candidate, ActorId(1), &action, None, None)
        .unwrap();
    candidate.transition(engine.regions.as_mut(), None).unwrap();
    assert_eq!(candidate.game.retrieval_entries().len(), 1);
    assert_eq!(
        RetrievalBoundaryState::capture(&candidate.game),
        RetrievalBoundaryState::capture(&healthy.game)
    );
    assert_eq!(
        candidate.game.activations_since(0).unwrap(),
        healthy.game.activations_since(0).unwrap()
    );
    let receipt = Receipt {
        user: "player".into(),
        frontend: "retrieval-test".into(),
        request_id: "missing-delta".into(),
        actor: ActorId(1),
        branch: engine.branch().clone(),
        command: Command::Act {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            action: action.clone(),
        },
    };
    let entry = JournalEntry {
        intention_suspensions: vec![],
        intention_ends: vec![],
        id: new_id(),
        branch: engine.branch().clone(),
        actor: ActorId(1),
        tick: engine.game.tick(),
        author: Author::User {
            user: "player".into(),
        },
        audience: Audience::Actor,
        content: JournalContent::Action {
            action,
            event: adapt::event(outcome.kind),
        },
    };
    for forged_side in ["before", "after"] {
        let before_summary = RetrievalBoundaryState::capture(&engine.game);
        let after_summary = RetrievalBoundaryState::capture(&candidate.game);
        // Each forged pair folds consistently with an empty batch; only its
        // comparison against actual committed/candidate authority rejects it.
        let summary = if forged_side == "before" {
            after_summary
        } else {
            before_summary
        };
        let forged = Record {
            retrieval_before: summary.clone(),
            retrieval_after: summary,
            retrievals: vec![],
            before_tick: engine.game.tick(),
            after_tick: candidate.game.tick(),
            entry: entry.clone(),
            receipt: Some(receipt.clone()),
            action_resolution: ActionResolution::Unchanged,
            activations: candidate
                .game
                .activations_since(engine.game.activation_count())
                .unwrap(),
        };
        assert!(
            engine.admit(&forged, &candidate, None).is_err(),
            "forged {forged_side} summary must not enqueue"
        );
    }
    // Simulate an internal late reset after every gameplay effect settled.
    // The owned item/population, RNG, counters and staged records still exist.
    candidate.game.begin_retrieval_transaction();
    assert!(
        engine
            .commit_candidate(candidate, entry, Some(receipt), None)
            .is_err(),
        "a candidate with unjournaled retrieval effects must not publish"
    );
    exact_equal(&engine, &before);
    assert_eq!(authority(&engine), authority(&before));
    engine.flush().unwrap();
    assert_eq!(super::deep_atomicity::logical_store_rows(&path), rows);
    drop(before);
    drop(engine);
    let recovered = Engine::open(&path, Scenario::two_room(999)).unwrap();
    assert!(retrieval(&recovered).item.is_none());
    restored_both_ways(&recovered);
}

#[test]
fn private_retrieval_summary_requires_named_current_fields() {
    let state = RetrievalBoundaryState {
        initialized: true,
        item: None,
        entry_count: 0,
        placement_count: 0,
    };
    let named = serde_json::to_value(&state).unwrap();
    assert_eq!(
        serde_json::from_value::<RetrievalBoundaryState>(named).unwrap(),
        state
    );
    assert!(
        serde_json::from_value::<RetrievalBoundaryState>(json!([true, null, 0, 0])).is_err(),
        "positional aliases cannot bypass the named private summary schema"
    );
}
