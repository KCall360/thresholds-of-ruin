//! Imported Hallucination through the real encoded Service and retained writer.
//! Production effects/catalogs are untouched; only finite stock, a speed-zero
//! external peer and this lit chamber are fixture setup.
use super::*;

const CONDITION: &str = "rogue.hallucinating";

fn source(root: &Path) -> crate::Scenario {
    let baseline = imported_nothing_source(root);
    let original = baseline.package.as_ref().unwrap();
    let rules = toml::to_string(original.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&original.manifest).unwrap();
    assert_eq!(
        manifest["archetypes"]["rogue.hallucination"]["consumable"],
        json!({
            "discovery":"on_use", "descriptors":[], "effects":[{"kind":"apply_condition_for_turns",
                "condition":CONDITION, "duration":{"kind":"rolled",
                    "dice":{"count":1,"sides":85,"bonus":807}}}]
        })
    );
    let figure = manifest["creatures"]["species"]["rogue.human"].clone();
    manifest["creatures"]["species"]["fixture.hallucination"] = figure;
    manifest["creatures"]["species"]["fixture.hallucination"]["modifiers"] = json!([
        {"scope":"always","op":{"kind":"set","stat":{"kind":"attribute","attribute":"speed"},"value":0}}
    ]);
    let mut body_package = manifest["creatures"]["body_packages"]["rogue.body.human"].clone();
    body_package["species"] = json!("fixture.hallucination");
    manifest["creatures"]["body_packages"]["fixture.hallucination.body"] = body_package;
    let owner = &mut manifest["characters"][0];
    owner["turn_ticks"] = json!(100);
    owner["creature"]["species"] = json!("fixture.hallucination");
    owner["creature"]["body_package"] = json!("fixture.hallucination.body");
    owner["known_identities"] = json!(["rogue.gold"]);
    let mut peer = owner["creature"].clone();
    peer["name"] = json!("actual visible peer");
    let body = owner["body"].clone();
    let region: crate::scenario_package::RegionDef = serde_json::from_value(json!({
        "id":1,"name":"Lit Hallucination laboratory","size":[7,5,2],"chamber":true,
        "lit":true,"anchors":{"start":[2,2,0]},
        "actors":[{"id":2,"at":[4,2,0],"turn_ticks":100,"controller":"external",
            "creature":peer,"body":body}],
        "items":[
            {"id":30,"at":[2,2,0],"archetype":"rogue.hallucination","quantity":6,"carried_by":1},
            {"id":31,"at":[2,2,0],"archetype":"rogue.extra_healing","quantity":3,"carried_by":1},
            {"id":32,"at":[2,2,0],"archetype":"rogue.blindness","quantity":4,"carried_by":1},
            {"id":33,"at":[2,2,0],"archetype":"rogue.healing","quantity":2,"carried_by":1},
            {"id":34,"at":[2,2,0],"archetype":"rogue.see_invisible","quantity":2,"carried_by":1},
            {"id":40,"at":[3,2,0],"archetype":"rogue.gold","quantity":7}
        ]
    }))
    .unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &rules,
    )
    .unwrap();
    package.build(42, false).unwrap();
    crate::scenario_package::write_package(
        root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn item(wire: &mut Wire, physical: u64) -> ItemTarget {
    let snapshot = wire.snapshot(OWNER);
    snapshot.state.observation.inventory.iter().find(|row| {
        matches!(wire.service.engine.decode_action(OWNER, &Action::Drop { item: row.id, quantity: None }),
            Ok(crate::actions::Action::Drop { item, .. }) if item == physical)
    }).unwrap().id
}

fn condition(wire: &mut Wire, id: &str) -> Option<PersonalCondition> {
    wire.snapshot(OWNER)
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .find(|row| row.condition.id == id)
        .cloned()
}

fn cadence(saved: &Value) -> &Value {
    &nothing_actor(saved, OWNER)["hallucination"]
}

fn ready(wire: &mut Wire, actor: ActorId) {
    for _ in 0..32 {
        let snapshot = wire.snapshot(actor);
        if snapshot.has_control
            && snapshot.readiness.admission
            && wire.service.engine.next_actor() == Some(actor)
        {
            return;
        }
        let next = wire.service.engine.next_actor().unwrap();
        wire.serial += 1;
        let id = format!("hallucination-ready-{}", wire.serial);
        let request = wire.request(next, Action::Wait);
        ack(wire.send(next, &id, request.clone()));
        wire.finish(next, &id, &request);
    }
    panic!("lawful external peer did not reach requested actor");
}

fn drink(wire: &mut Wire, physical: u64, id: &str) -> ItemTarget {
    ready(wire, OWNER);
    let item = item(wire, physical);
    wire.act(id, Action::Drink { item });
    item
}

fn true_peer(view: &Observation) -> &ActorView {
    view.visible_actors
        .iter()
        .find(|row| row.id != view.self_target)
        .unwrap()
}

fn ground(view: &Observation) -> &GroundItemView {
    assert_eq!(view.ground_items.len(), 1);
    &view.ground_items[0]
}

fn assert_sample(view: &Observation, phase: u64) {
    // Frozen values independently evaluated from the documented integer recipe
    // against the actual production 26-creature/98-object cosmetic roster.
    // Seed42, observer1, peer2 / item40. Not a probabilistic inequality.
    let (name, asset, object, object_asset, class) = match phase {
        0 => (
            "emu",
            "creature.bird.emu",
            "gold wand",
            Some("item.wand"),
            ItemClass::Wand,
        ),
        1 => (
            "bat",
            "creature.bat",
            "cedar staff",
            Some("item.wand"),
            ItemClass::Wand,
        ),
        _ => panic!("unlisted literal phase witness"),
    };
    let peer = true_peer(view);
    assert_eq!(peer.name, name);
    assert_eq!(peer.asset.as_deref(), Some(asset));
    assert!(peer.description.is_empty());
    let floor = ground(view);
    assert_eq!(floor.item.name, "object");
    assert_eq!(floor.item.appearance, "object");
    assert!(floor.item.description.is_empty());
    assert!(floor.item.asset.is_none() && floor.item.corpse.is_none());
    assert_eq!(floor.item.class, ItemClass::Coin);
    assert_eq!(floor.item.quantity, 7);
    assert_eq!(floor.item.known_currency, Some(CurrencyKind::Gold));
    let presentation = floor.presentation.as_ref().unwrap();
    assert_eq!(presentation.name, object);
    assert_eq!(presentation.asset.as_deref(), object_asset);
    assert_eq!(presentation.kind, ObjectPresentationKind::Item { class });
    assert!(view
        .visible_cells
        .iter()
        .all(|cell| !cell.stairs_up && !cell.stairs_down));
}

#[test]
fn paid_production_dose_selects_exact_visual_cosmetics_without_query_rng_or_authority_changes() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(source(directory.path())).unwrap());
    let initial = wire.snapshot(OWNER);
    assert_eq!(
        true_peer(&initial.state.observation).name,
        "actual visible peer"
    );
    assert_eq!(ground(&initial.state.observation).item.name, "gold coin");
    assert!(ground(&initial.state.observation).presentation.is_none());
    assert_eq!(
        initial
            .state
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .attributes
            .speed,
        0
    );
    let potion = drink(&mut wire, 30, "first-hallucination");
    let after = wire.checkpoint();
    assert_eq!(cadence(&after)["phase"], 0);
    assert_eq!(cadence(&after)["period"], 100);
    assert_eq!(cadence(&after)["remaining_to_shift"], 100);
    let current = wire.snapshot(OWNER);
    assert_sample(&current.state.observation, 0);
    assert_eq!(
        ground(&current.state.observation).item.id,
        ground(&initial.state.observation).item.id
    );
    assert_eq!(
        true_peer(&current.state.observation).id,
        true_peer(&initial.state.observation).id
    );
    assert_eq!(quantity(&current, potion), 5);
    assert!(nothing_identified(&current, potion));
    assert!((80800..=89200).contains(
        &condition(&mut wire, CONDITION)
            .unwrap()
            .remaining_ticks
            .unwrap()
    ));
    for _ in 0..4 {
        assert_eq!(wire.snapshot(OWNER).state, current.state);
        assert_eq!(
            wire.checkpoint(),
            after,
            "queries neither settle cadence nor draw rules RNG"
        );
    }
    let peer = wire.snapshot(OTHER);
    assert!(peer
        .state
        .observation
        .ground_items
        .iter()
        .all(|row| row.presentation.is_none()));
    assert!(peer
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .all(|row| row.condition.id != CONDITION));
}

fn follower(wire: &mut Wire) -> (Connection, StateView) {
    let mut connection = wire
        .service
        .connect(
            &Account {
                user: "hallucination-follower".into(),
                token: "test-only".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "encoded-hallucination".into(),
        )
        .unwrap();
    while connection.messages.try_recv_frame().is_ok() {}
    wire.service.handle(
        connection.id,
        "hallucination-follow".into(),
        Request::Attach { actor: OWNER },
    );
    let mut state = None;
    while let Ok(frame) = connection.messages.try_recv_frame() {
        match decode_response(&frame.text).unwrap() {
            ServerMessage::Snapshot { snapshot, .. } => state = Some((*snapshot.state).clone()),
            ServerMessage::Error { .. } => panic!("follower attach failed"),
            _ => {}
        }
    }
    (connection, state.unwrap())
}

fn apply_updates(state: &mut StateView, messages: Vec<ServerMessage>, shifted: &mut usize) {
    for message in messages {
        assert!(
            !matches!(message, ServerMessage::Error { .. }),
            "{message:?}"
        );
        let ServerMessage::Update { update } = message else {
            continue;
        };
        let encoded = serde_json::to_string(&update).unwrap();
        for private in [
            "remaining_to_shift",
            "presentation_catalog",
            "palette_index",
            "private_index",
        ] {
            assert!(
                !encoded.contains(private),
                "private cadence/catalog leaked: {private}"
            );
        }
        match update.body {
            UpdateBody::Observation { state: next, .. } => *state = (*next).clone(),
            UpdateBody::ObservationDelta { state: delta, .. } => {
                *state = delta.apply(state).unwrap()
            }
            _ => continue,
        }
        if state.observation.tick == 200 {
            assert_sample(&state.observation, 1);
            *shifted += 1;
        }
    }
}

#[test]
fn peer_driven_exact_base_period_publishes_unsolicited_owner_and_followed_visual_updates() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(source(directory.path())).unwrap());
    drink(&mut wire, 30, "publish-dose");
    let initial = wire.snapshot(OWNER);
    assert_eq!(initial.state.observation.tick, 100);
    assert_sample(&initial.state.observation, 0);
    let initial_seed = true_peer(&initial.state.observation).appearance_seed;
    let mut owner_state = (*initial.state).clone();
    let (mut follower, mut follower_state) = follower(&mut wire);
    assert_sample(&follower_state.observation, 0);
    let request = Request::Command {
        context: initial.reply_context().input,
        branch: initial.branch.clone(),
        command: Command::Act {
            expected_revision: initial.state.revision,
            action: Action::Wait,
        },
    };
    ack(wire.send(OWNER, "owner-clock-wait", request));
    let mut owner_updates = 0;
    let mut follower_updates = 0;
    for _ in 0..8 {
        wire.service.run_until_blocked();
        apply_updates(&mut owner_state, wire.drain(OWNER), &mut owner_updates);
        let frames = std::iter::from_fn(|| follower.messages.try_recv_frame().ok())
            .map(|frame| decode_response(&frame.text).unwrap())
            .collect();
        apply_updates(&mut follower_state, frames, &mut follower_updates);
        wire.drain(OTHER);
        let saved = wire.checkpoint();
        if cadence(&saved)["phase"] == 1 {
            assert_eq!(saved["game"]["tick"], 200);
            assert_eq!(cadence(&saved)["remaining_to_shift"], 100);
            break;
        }
        assert_eq!(
            saved["game"]["tick"], 100,
            "no assumed Wait-to-tick progression"
        );
        assert_eq!(wire.service.engine.next_actor(), Some(OTHER));
        let request = wire.request(OTHER, Action::Wait);
        ack(wire.send(OTHER, "peer-clock-wait", request));
    }
    assert!(
        owner_updates > 0,
        "stationary owner received no phase publication"
    );
    assert!(
        follower_updates > 0,
        "followed observer received no phase publication"
    );
    assert_eq!(
        true_peer(&owner_state.observation).appearance_seed,
        initial_seed,
        "cosmetic seed is neutral, not phase, subject identity or catalog index"
    );
    assert_eq!(
        owner_state.observation.visible_actors,
        follower_state.observation.visible_actors
    );
    assert_eq!(
        owner_state.observation.ground_items,
        follower_state.observation.ground_items
    );
    assert_eq!(
        owner_state.observation.visible_cells,
        follower_state.observation.visible_cells
    );
    assert_eq!(owner_state, wire.service.engine.state(OWNER).unwrap());
}

#[test]
fn first_blind_dose_identifies_without_disclosing_eye_only_cosmetics() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(source(directory.path())).unwrap());
    let potion = item(&mut wire, 30);
    assert!(!nothing_identified(&wire.snapshot(OWNER), potion));
    drink(&mut wire, 32, "blind-before-first-dose");
    assert!(condition(&mut wire, "rogue.blindness").is_some());
    drink(&mut wire, 30, "first-blind-dose");
    let seen = wire.snapshot(OWNER);
    assert!(nothing_identified(&seen, potion));
    assert_eq!(quantity(&seen, potion), 5);
    assert!(condition(&mut wire, CONDITION).is_some());
    assert!(condition(&mut wire, "rogue.blindness").is_some());
    assert!(seen
        .state
        .observation
        .ground_items
        .iter()
        .all(|row| row.presentation.is_none()));
    assert!(seen
        .state
        .observation
        .visible_cells
        .iter()
        .all(|cell| cell.object_presentation.is_none()));
    assert!(seen
        .state
        .observation
        .visible_actors
        .iter()
        .all(
            |row| row.id == seen.state.observation.self_target || row.name == "actual visible peer"
        ));
}

#[test]
fn repeat_preserves_generation_and_cadence_blind_dose_identifies_and_only_extra_healing_cures() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(source(directory.path())).unwrap());
    drink(&mut wire, 30, "first-dose");
    let first = wire.checkpoint();
    let first_expiry = first["game"]["tick"].as_u64().unwrap()
        + condition(&mut wire, CONDITION)
            .unwrap()
            .remaining_ticks
            .unwrap();
    drink(&mut wire, 30, "extend-dose");
    let extended = wire.checkpoint();
    assert_eq!(
        cadence(&extended)["generation"],
        cadence(&first)["generation"]
    );
    assert!(
        cadence(&extended)["phase"].as_u64().unwrap() > 0,
        "Extend does not reset phase0"
    );
    let second_expiry = extended["game"]["tick"].as_u64().unwrap()
        + condition(&mut wire, CONDITION)
            .unwrap()
            .remaining_ticks
            .unwrap();
    assert!((80800..=89200).contains(&(second_expiry - first_expiry)));
    drink(&mut wire, 32, "blind-before-repeat");
    assert!(condition(&mut wire, "rogue.blindness").is_some());
    let blind_potion = drink(&mut wire, 30, "blind-repeat-dose");
    let blind = wire.snapshot(OWNER);
    assert!(nothing_identified(&blind, blind_potion));
    assert!(condition(&mut wire, "rogue.blindness").is_some());
    assert!(condition(&mut wire, CONDITION).is_some());
    assert!(blind
        .state
        .observation
        .ground_items
        .iter()
        .all(|row| row.presentation.is_none()));
    assert!(blind
        .state
        .observation
        .visible_cells
        .iter()
        .all(|cell| cell.object_presentation.is_none()));
    drink(&mut wire, 33, "ordinary-healing-control");
    assert!(condition(&mut wire, CONDITION).is_some());
    drink(&mut wire, 32, "blind-before-sight");
    drink(&mut wire, 34, "see-invisible-control");
    assert!(condition(&mut wire, CONDITION).is_some());
    drink(&mut wire, 32, "blind-before-extra");
    assert!(condition(&mut wire, "rogue.blindness").is_some());
    drink(&mut wire, 31, "blind-extra-healing");
    assert!(condition(&mut wire, CONDITION).is_none());
    assert!(condition(&mut wire, "rogue.blindness").is_none());
    assert!(cadence(&wire.checkpoint()).is_null());
    let cured = wire.snapshot(OWNER);
    assert_eq!(
        true_peer(&cured.state.observation).name,
        "actual visible peer"
    );
    assert!(ground(&cured.state.observation).presentation.is_none());
    assert_eq!(ground(&cured.state.observation).item.name, "gold coin");
    drink(&mut wire, 30, "fresh-reapply");
    let reapplied = wire.checkpoint();
    assert_ne!(
        cadence(&reapplied)["generation"],
        cadence(&first)["generation"]
    );
    assert_eq!(cadence(&reapplied)["phase"], 0);
    assert_eq!(cadence(&reapplied)["remaining_to_shift"], 100);
}

#[test]
fn pending_paid_dose_and_active_palette_reopen_interval_one_without_sources_before_service_pause() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("hallucination.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire =
        Wire::new(Engine::open_with_policy(&path, source(&root), policy.clone()).unwrap());
    let potion = item(&mut wire, 30);
    let before = wire.checkpoint();
    let request = wire.request(OWNER, Action::Drink { item: potion });
    let admitted = ack(wire.send(OWNER, "pending-hallucination", request.clone()));
    wire.service.run_until_blocked();
    for actor in [OWNER, OTHER] {
        wire.drain(actor);
    }
    let started = ack(wire.send(OWNER, "pending-hallucination", request.clone()));
    assert!(matches!(
        started,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    assert_eq!(started.entry_id(), admitted.entry_id());
    let pending = wire.checkpoint();
    assert!(cadence(&pending).is_null());
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["active"], true);
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["remaining"], 100);
    assert_eq!(pending["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(nothing_items(&pending)["30"]["quantity"], 6);
    let pending_view = wire.service.engine.state(OWNER).unwrap();
    wire.service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&pending).unwrap())
        .unwrap();
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let engine =
        Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy.clone()).unwrap();
    assert_eq!(
        checkpoint(&engine),
        pending,
        "exact reconstruction before Service reconnect pause"
    );
    assert_eq!(engine.state(OWNER).unwrap(), pending_view);
    let mut wire = Wire::new(engine);
    wire.serial = serial;
    let paused = wire.checkpoint();
    assert_eq!(nothing_actor(&paused, OWNER)["pending"]["active"], false);
    assert_eq!(
        nothing_actor(&paused, OWNER)["pending"]["plan"],
        nothing_actor(&pending, OWNER)["pending"]["plan"]
    );
    let snapshot = wire.snapshot(OWNER);
    assert_eq!(snapshot.readiness.resume.len(), 1);
    ack(wire.send(
        OWNER,
        "resume-hallucination",
        Request::Command {
            context: snapshot.reply_context().input,
            branch: snapshot.branch.clone(),
            command: Command::ResumeIntention {
                expected_revision: snapshot.state.revision,
                intention: snapshot.readiness.resume[0].clone(),
            },
        },
    ));
    let completed = wire.finish(OWNER, "pending-hallucination", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    assert_eq!(quantity(&wire.snapshot(OWNER), potion), 5);
    assert!(nothing_identified(&wire.snapshot(OWNER), potion));
    assert_sample(&wire.snapshot(OWNER).state.observation, 0);
    let active = wire.checkpoint();
    assert_eq!(
        ack(wire.send(OWNER, "pending-hallucination", request)),
        completed
    );
    assert_eq!(wire.checkpoint(), active);
    let active_view = wire.service.engine.state(OWNER).unwrap();
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(123), policy).unwrap();
    assert_eq!(checkpoint(&engine), active);
    assert_eq!(engine.state(OWNER).unwrap(), active_view);
    assert_sample(&engine.state(OWNER).unwrap().observation, 0);
}

#[test]
fn real_current_and_retained_clock_records_reject_independent_corruption_after_valid_restore() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(source(directory.path())).unwrap());
    drink(&mut wire, 30, "restore-valid-dose");
    drink(&mut wire, 30, "restore-valid-extension");
    let good = wire.checkpoint();
    wire.service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&good).unwrap())
        .unwrap();
    let current = good["game"]["actors"].as_u64().unwrap() as usize;
    let retained = good["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["game"]["actors"].as_u64().unwrap() as usize)
        .find(|&index| {
            index != current && !good["shared"]["actors"][index]["1"]["hallucination"].is_null()
        })
        .expect("a genuine retained hallucinating boundary");
    for index in [current, retained] {
        for field in [
            "generation",
            "period",
            "remaining_to_shift",
            "phase",
            "missing",
            "unknown",
        ] {
            let mut bad = good.clone();
            let clock = &mut bad["shared"]["actors"][index]["1"]["hallucination"];
            match field {
                "generation" => clock[field] = json!(clock[field].as_u64().unwrap() + 1),
                "period" | "remaining_to_shift" => clock[field] = json!(0),
                "phase" => clock[field] = json!(u64::MAX),
                "missing" => {
                    clock.as_object_mut().unwrap().remove("period");
                }
                "unknown" => clock["private_index"] = json!(0),
                _ => unreachable!(),
            }
            assert!(
                wire.service
                    .engine
                    .test_restore_checkpoint(&serde_json::to_vec(&bad).unwrap())
                    .is_err(),
                "{field} in shared actor root {index}"
            );
        }
    }
}

// Genuine source-owned door and paid observation grant isolate historical
// acquisition. They are fixture additions, not production abilities or stock.
fn historical_source(root: &Path) -> crate::Scenario {
    let baseline = source(root);
    let original = baseline.package.as_ref().unwrap();
    let mut rules = original.rules().unwrap().document().clone();
    rules.abilities.push(serde_json::from_value(json!({
        "id":"fixture.hallucination_plan", "name":"Observe historical plan", "tags":[],
        "basic_attack":false,"invocation":"actor_target","cost":null,
        "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null},
        "reach":{"kind":"sight","range":6},
        "strikes":{"kind":"authored","strikes":[{"offset":0,"unlock":null,"check":null,
            "on_success":[{"kind":"observe_public_plan"}],"on_failure":[],"descriptors":[],"riders":[]}]},
        "interaction":"non_hostile",
        "ai":{"priority":0,"hostile":false,"minimum_resource_percent":0,"range_preference":"any"}
    })).unwrap());
    let mut manifest = serde_json::to_value(&original.manifest).unwrap();
    manifest["creatures"]["species"]["fixture.hallucination"]["modifiers"].as_array_mut().unwrap()
        .push(json!({"scope":"always","op":{"kind":"grant_ability","ability":"fixture.hallucination_plan"}}));
    let mut regions = original.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["actors"][0]["at"] = json!([5, 2, 0]);
    region["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == 40)
        .unwrap()["at"] = json!([4, 2, 0]);
    region["walls"] = json!((0..5)
        .filter(|&y| y != 2)
        .flat_map(|y| (0..2).map(move |z| [3, y, z]))
        .collect::<Vec<_>>());
    region["doors"] = json!([{"id":1,"at":[3,2,0],"open":true,"height":2}]);
    regions[0] = serde_json::from_value(region).unwrap();
    let rules = toml::to_string(&rules).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.build(42, false).unwrap();
    crate::scenario_package::write_package(
        root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn remembered_gold(engine: &Engine) -> RememberedMapCell {
    engine
        .explored_map(OWNER, MapWindow::normal())
        .unwrap()
        .cells
        .into_iter()
        .find(|cell| {
            cell.ground_items
                .iter()
                .any(|item| item.class == ItemClass::Coin && item.quantity == 7)
        })
        .expect("a genuinely acquired gold cell, not a synthesized memory")
}

fn public_plan(engine: &Engine) -> PublicPlanObservationView {
    engine
        .state(OWNER)
        .unwrap()
        .observation
        .combat
        .as_ref()
        .unwrap()
        .public_plan
        .clone()
        .expect("the actual paid observation captured a public-plan record")
}

#[test]
fn acquired_map_and_paid_public_plan_keep_historical_cosmetics_after_door_cure_and_source_free_reopen(
) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("historical.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire = Wire::new(
        Engine::open_with_policy(&path, historical_source(&root), policy.clone()).unwrap(),
    );
    let initial = wire.snapshot(OWNER);
    assert_eq!(
        true_peer(&initial.state.observation).name,
        "actual visible peer"
    );
    assert_eq!(ground(&initial.state.observation).item.name, "gold coin");
    assert!(initial
        .state
        .observation
        .visible_cells
        .iter()
        .any(|cell| cell.door.as_ref().is_some_and(|door| door.open)));
    drink(&mut wire, 30, "history-dose");
    assert_sample(&wire.snapshot(OWNER).state.observation, 0);
    ready(&mut wire, OWNER);
    let target = true_peer(&wire.snapshot(OWNER).state.observation).id;
    wire.act(
        "paid-historical-plan",
        Action::UseAbility {
            id: "fixture.hallucination_plan".into(),
            invocation: AbilityInvocation::actor_target(target),
            supplies: SupplySelection::Carried,
        },
    );
    let plan = public_plan(&wire.service.engine);
    assert_eq!(plan.observed_at, 200);
    assert_eq!(
        plan.subject_name, "bat",
        "capture samples phase1 once at actual paid acquisition"
    );
    assert_eq!(plan.state, PublicPlanStateView::NoPublicPlan);
    ready(&mut wire, OWNER);
    let before_close = wire.snapshot(OWNER);
    let acquired = remembered_gold(&wire.service.engine);
    let actual = ground(&before_close.state.observation);
    assert_eq!(acquired.ground_items[0].presentation, actual.presentation);
    assert!(acquired.ground_items[0].presentation.is_some());
    assert_ne!(acquired.ground_items[0].name, "gold coin");
    let door = before_close
        .state
        .observation
        .visible_cells
        .iter()
        .find_map(|cell| cell.door.as_ref())
        .unwrap()
        .id;
    wire.act(
        "conceal-acquired-scene",
        Action::SetDoor { door, open: false },
    );
    let hidden = wire.snapshot(OWNER);
    assert!(hidden.state.observation.ground_items.is_empty());
    assert!(hidden
        .state
        .observation
        .visible_cells
        .iter()
        .all(|cell| cell.key != acquired.key));
    assert_eq!(
        remembered_gold(&wire.service.engine),
        acquired,
        "closing a lawful door leaves the last visual sample, not the current phase"
    );
    assert_eq!(public_plan(&wire.service.engine), plan);
    drink(&mut wire, 31, "cure-out-of-view");
    assert!(condition(&mut wire, CONDITION).is_none());
    assert!(wire
        .snapshot(OWNER)
        .state
        .observation
        .ground_items
        .is_empty());
    assert_eq!(
        remembered_gold(&wire.service.engine),
        acquired,
        "cure does not retroactively identify an unseen historical appearance"
    );
    assert_eq!(public_plan(&wire.service.engine), plan);
    let before_reopen = wire.checkpoint();
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(1001), policy).unwrap();
    assert_eq!(
        checkpoint(&engine),
        before_reopen,
        "exact result before Service reconnect pause"
    );
    assert_eq!(remembered_gold(&engine), acquired);
    assert_eq!(public_plan(&engine), plan);
    assert!(engine
        .state(OWNER)
        .unwrap()
        .observation
        .ground_items
        .is_empty());
}
