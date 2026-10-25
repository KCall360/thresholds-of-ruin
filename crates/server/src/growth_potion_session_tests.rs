//! Imported permanent growth potions through ordinary admitted Service work.
use super::*;

fn growth_source(root: &Path, peer: bool) -> crate::Scenario {
    let baseline = imported_nothing_source(root);
    let source = baseline.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    assert_eq!(
        manifest["archetypes"]["rogue.gain_strength"]["consumable"]["effects"],
        json!([{"kind":"gain_strength"}])
    );
    assert_eq!(
        manifest["archetypes"]["rogue.raise_level"]["consumable"]["effects"],
        json!([{"kind":"raise_level"}])
    );
    let figure = manifest["creatures"]["species"]["rogue.human"].clone();
    manifest["creatures"]["species"]["fixture.growth"] = figure;
    manifest["creatures"]["species"]["fixture.growth"]["modifiers"] = json!([
        {"scope":"always","op":{"kind":"set","stat":{"kind":"attribute","attribute":"speed"},"value":0}}
    ]);
    let owner = &mut manifest["characters"][0];
    owner["turn_ticks"] = json!(100);
    owner["creature"]["species"] = json!("fixture.growth");
    owner["creature"]["attributes"] =
        json!({"strength":3,"speed":2,"intellect":2,"willpower":2,"awareness":1,"presence":1});
    assert_eq!(owner["creature"]["future_growth"]["kind"], "disabled");
    let creature = owner["creature"].clone();
    manifest["archetypes"]["fixture.strength_loss"] = json!({"name":"strength loss fixture","class":"potion","stackable":true,
        "consumable":{"discovery":"on_use","effects":[{"kind":"lose_attribute","attribute":"strength","amount":{"kind":"fixed","amount":1}}]}});
    manifest["archetypes"]["fixture.strength_ring"] = json!({"name":"strength ring fixture","class":"ring","stackable":false,
        "equipment":{"slot":"ring","attack":null,"modifiers":[{"scope":"always","op":{"kind":"add","stat":{"kind":"attribute","attribute":"strength"},"value":2}}]}});
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["items"] = json!([
        {"id":30,"at":[2,2,0],"archetype":"rogue.gain_strength","quantity":8,"carried_by":1},
        {"id":31,"at":[2,2,0],"archetype":"rogue.raise_level","quantity":3,"carried_by":1},
        {"id":32,"at":[2,2,0],"archetype":"fixture.strength_loss","carried_by":1},
        {"id":33,"at":[2,2,0],"archetype":"fixture.strength_ring","carried_by":1}
    ]);
    if peer {
        region["actors"] = json!([{"id":2,"at":[4,2,0],"turn_ticks":100,"controller":"external","creature":creature}]);
    }
    regions[0] = serde_json::from_value(region).unwrap();
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
fn creature(saved: &Value) -> &Value {
    &nothing_actor(saved, OWNER)["combat"]["creature"]
}
fn strength(wire: &mut Wire) -> u16 {
    wire.snapshot(OWNER)
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .attributes
        .strength
}
fn item(wire: &mut Wire, physical: u64) -> ItemTarget {
    let snapshot = wire.snapshot(OWNER);
    snapshot.state.observation.inventory.iter().find(|row| {
        matches!(wire.service.engine.decode_action(OWNER, &Action::Drop { item: row.id, quantity: None }),
            Ok(crate::actions::Action::Drop { item, .. }) if item == physical)
    }).unwrap().id
}
fn advancement(wire: &mut Wire, query: Value) -> tor_protocol::AdvancementPage {
    let snapshot = wire.snapshot(OWNER);
    let request = Request::Command {
        context: snapshot.reply_context().input,
        branch: snapshot.branch.clone(),
        command: Command::Advancement {
            expected_revision: snapshot.state.revision,
            query: serde_json::from_value(query).unwrap(),
        },
    };
    let messages = wire.send(OWNER, "growth-overview", request);
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Error { .. })),
        "{messages:?}"
    );
    messages
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::AdvancementPage { page, .. } => Some(*page),
            _ => None,
        })
        .unwrap()
}
fn acquired_overview(wire: &mut Wire, expected: u8) {
    let page = advancement(wire, json!({"kind":"overview"}));
    let [tor_protocol::AdvancementEntry::Overview { value }] = page.entries.as_slice() else {
        panic!("actual owner overview")
    };
    assert_eq!(value.strength_acquired, expected);
}
fn prerequisite(wire: &mut Wire) {
    let saved = wire.checkpoint();
    let owner = creature(&saved);
    assert_eq!(owner["build"]["initial_attributes"]["strength"], 3);
    assert_eq!(owner["build"]["strength_acquired"], 0);
    assert_eq!(owner["build"]["hd"].as_array().unwrap().len(), 1);
    assert_eq!(owner["earned_progress"]["held_xp"], 0);
    assert_eq!(owner["earned_progress"]["highest_applied_tier"], 1);
    assert_eq!(owner["dead"], false);
    assert_eq!(strength(wire), 3);
    acquired_overview(wire, 0);
    assert_eq!(
        wire.snapshot(OWNER)
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
}

#[test]
fn imported_strength_restores_exclusively_then_reaches_combined_cap_independent_of_ring() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(
        Engine::open(
            &directory.path().join("save.db"),
            growth_source(&directory.path().join("source"), false),
        )
        .unwrap(),
    );
    prerequisite(&mut wire);
    let potion = item(&mut wire, 30);
    let loss = item(&mut wire, 32);
    let ring = item(&mut wire, 33);
    let initial = wire.checkpoint();
    wire.act("actual-strength-loss", Action::Drink { item: loss });
    assert_eq!(strength(&mut wire), 2);
    wire.act("restore-one-only", Action::Drink { item: potion });
    assert_eq!(strength(&mut wire), 3);
    assert_eq!(
        creature(&wire.checkpoint())["build"]["strength_acquired"],
        0
    );
    let view = wire.snapshot(OWNER);
    let slot = view
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|row| row.id == "ring")
        .unwrap() as u16;
    wire.act("equip-ring", Action::Equip { item: ring, slot });
    assert_eq!(strength(&mut wire), 5);
    for acquired in 1..=4 {
        wire.act(
            &format!("acquire-{acquired}"),
            Action::Drink { item: potion },
        );
        assert_eq!(strength(&mut wire), 5 + acquired);
        assert_eq!(
            creature(&wire.checkpoint())["build"]["strength_acquired"],
            acquired
        );
    }
    wire.act("cap-dose", Action::Drink { item: potion });
    assert_eq!(strength(&mut wire), 9);
    let capped = wire.checkpoint();
    assert_eq!(creature(&capped)["build"]["strength_acquired"], 4);
    assert_eq!(
        creature(&capped)["build"]["initial_attributes"],
        creature(&initial)["build"]["initial_attributes"]
    );
    assert_eq!(
        creature(&capped)["build"]["hd"],
        creature(&initial)["build"]["hd"],
        "boons are not refundable purchases"
    );
    assert_eq!(nothing_items(&capped)["30"]["quantity"], 2);
    assert_eq!(capped["game"]["rules_rng"], initial["game"]["rules_rng"]);
    wire.act("remove-ring", Action::Unequip { item: ring });
    assert_eq!(strength(&mut wire), 7);
    acquired_overview(&mut wire, 4);
    let page = advancement(&mut wire, json!({"kind":"overview"}));
    let before_preview = wire.checkpoint();
    let preview = advancement(
        &mut wire,
        json!({"kind":"preview", "basis":page.basis,
        "proposal":{"kind":"purchase","purchase":{"owner":"0","pool":"advancement","selection":{"kind":"skill_rank","skill":"lore"}}},
        "section":"summary","offset":0}),
    );
    let [tor_protocol::AdvancementEntry::Summary { value }] = preview.entries.as_slice() else {
        panic!("actual preview summary")
    };
    assert_eq!(value.strength_acquired, 4);
    assert_eq!(value.refund_count, 0);
    assert_eq!(
        wire.checkpoint(),
        before_preview,
        "preview preserves actual permanent authority"
    );
    assert!(nothing_identified(&wire.snapshot(OWNER), potion));
}

#[test]
fn imported_raise_repeated_blocked_doses_fund_successive_levels_and_reopen_without_sources() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("save.db");
    let scenario = growth_source(&root, false);
    let rules = scenario.package.as_ref().unwrap().rules().unwrap();
    let curve = &rules.document().growth;
    let first = curve.threshold(tor_simulation::HdCount(1)).unwrap();
    let second = curve.threshold(tor_simulation::HdCount(2)).unwrap();
    assert!(first > 0 && second > 0);
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire = Wire::new(Engine::open_with_policy(&path, scenario, policy.clone()).unwrap());
    prerequisite(&mut wire);
    let potion = item(&mut wire, 31);
    let initial = wire.checkpoint();
    wire.act("fund-first", Action::Drink { item: potion });
    assert_eq!(
        creature(&wire.checkpoint())["earned_progress"]["held_xp"],
        first
    );
    let request = wire.request(OWNER, Action::Drink { item: potion });
    let admitted = ack(wire.send(OWNER, "fund-second", request.clone()));
    let completed = wire.finish(OWNER, "fund-second", &request);
    assert_eq!(admitted.entry_id(), completed.entry_id());
    let funded = wire.checkpoint();
    let total = first.checked_add(second).unwrap();
    for field in ["held_xp", "earned_xp"] {
        assert_eq!(creature(&funded)["earned_progress"][field], total);
    }
    assert_eq!(creature(&funded)["earned_progress"]["spent_xp"], 0);
    assert_eq!(
        creature(&funded)["earned_progress"]["highest_applied_tier"],
        1
    );
    assert_eq!(
        creature(&funded)["build"],
        creature(&initial)["build"],
        "banking does not bypass disabled growth"
    );
    assert_eq!(nothing_items(&funded)["31"]["quantity"], 1);
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy).unwrap();
    assert_eq!(checkpoint(&engine), funded);
    let mut wire = Wire::new(engine);
    wire.serial = serial;
    let before_retry = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "fund-second", request)), completed);
    assert_eq!(wire.checkpoint(), before_retry);
    assert_eq!(
        creature(&wire.checkpoint())["earned_progress"]["held_xp"],
        total
    );
    // Spend the bank only through the existing player-owned growth-source action.
    let page = advancement(&mut wire, json!({"kind":"overview"}));
    wire.act(
        "choose-normal-growth",
        Action::ChooseGrowthSource {
            basis: Box::new(page.basis),
            source: serde_json::from_value(json!({"kind":"class","class":"warrior"})).unwrap(),
        },
    );
    let grown = wire.checkpoint();
    let hd = creature(&grown)["build"]["hd"].as_array().unwrap().len() as u64;
    assert!(
        (2..=3).contains(&hd),
        "normal growth advances only funded ordinals"
    );
    let spent = curve
        .threshold_sum(tor_simulation::HdCount(1), tor_simulation::HdCount(hd))
        .unwrap();
    assert_eq!(creature(&grown)["earned_progress"]["earned_xp"], total);
    assert_eq!(creature(&grown)["earned_progress"]["spent_xp"], spent);
    assert_eq!(
        creature(&grown)["earned_progress"]["held_xp"],
        total - spent
    );
}

#[test]
fn imported_strength_pending_interval_one_reopen_keeps_unexecuted_boon_and_exact_retry() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("save.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire = Wire::new(
        Engine::open_with_policy(&path, growth_source(&root, true), policy.clone()).unwrap(),
    );
    prerequisite(&mut wire);
    let potion = item(&mut wire, 30);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let request = wire.request(OWNER, Action::Drink { item: potion });
    let admitted = ack(wire.send(OWNER, "pending-strength", request.clone()));
    wire.service.run_until_blocked();
    for actor in [OWNER, OTHER] {
        let messages = wire.drain(actor);
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Error { .. })),
            "{messages:?}"
        );
    }
    assert!(matches!(
        ack(wire.send(OWNER, "pending-strength", request.clone())),
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    let pending = wire.checkpoint();
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["active"], true);
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["remaining"], 100);
    assert_eq!(creature(&pending)["build"]["strength_acquired"], 0);
    assert_eq!(nothing_items(&pending)["30"]["quantity"], 8);
    wire.service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&pending).unwrap())
        .unwrap();
    let index = pending["game"]["actors"].as_u64().unwrap();
    let path_to_boon = format!("/shared/actors/{index}/1/combat/creature/build/strength_acquired");
    for value in [json!(5), json!(8), json!(-1), Value::Null, json!("1")] {
        let mut malformed = pending.clone();
        *malformed.pointer_mut(&path_to_boon).unwrap() = value;
        assert!(wire
            .service
            .engine
            .test_restore_checkpoint(&serde_json::to_vec(&malformed).unwrap())
            .is_err());
    }
    let mut missing = pending.clone();
    missing
        .pointer_mut(&format!("/shared/actors/{index}/1/combat/creature/build"))
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("strength_acquired");
    assert!(wire
        .service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&missing).unwrap())
        .is_err());
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy).unwrap();
    assert_eq!(checkpoint(&engine), pending);
    let mut wire = Wire::new(engine);
    wire.serial = serial;
    assert_eq!(
        nothing_actor(&wire.checkpoint(), OWNER)["pending"]["active"],
        false
    );
    let snapshot = wire.snapshot(OWNER);
    assert_eq!(snapshot.readiness.resume.len(), 1);
    let resume = Request::Command {
        context: snapshot.reply_context().input,
        branch: snapshot.branch.clone(),
        command: Command::ResumeIntention {
            expected_revision: snapshot.state.revision,
            intention: snapshot.readiness.resume[0].clone(),
        },
    };
    ack(wire.send(OWNER, "resume-strength", resume));
    let completed = wire.finish(OWNER, "pending-strength", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    assert_eq!(strength(&mut wire), 4);
    acquired_overview(&mut wire, 1);
    let after = wire.checkpoint();
    assert_eq!(creature(&after)["build"]["strength_acquired"], 1);
    assert_eq!(nothing_items(&after)["30"]["quantity"], 7);
    assert_eq!(
        ack(wire.send(OWNER, "pending-strength", request)),
        completed
    );
    assert_eq!(wire.checkpoint(), after);
    // Also restore the completed nonzero boon, not only the pre-release record.
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    let engine = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&engine), after);
    let mut wire = Wire::new(engine);
    assert_eq!(strength(&mut wire), 4);
    acquired_overview(&mut wire, 1);
}
