//! Admitted portable-light work and genuine compressed source-free recovery.
use super::*;

fn torch_source(root: &Path, with_other: bool) -> crate::Scenario {
    let baseline = self_device_source(root, with_other, false);
    let source = baseline.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["creatures"]["species"]["figure"]["anatomy"]["slots"]
        .as_array_mut().unwrap().push(json!("held_light"));
    manifest["archetypes"]["torch"] = json!({
        "name":"fixture torch", "class":"tool", "stackable":false,
        "held_light":{"radius":4,"fuel_ticks":1000},
        "equipment":{"slot":"held_light","attack":null,"modifiers":[]}
    });
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["lit"] = json!(false);
    region["lighting"] = json!([]);
    region["items"] = json!([
        {"id":40,"at":[1,2,0],"archetype":"torch","carried_by":1},
        {"id":41,"at":[1,2,0],"archetype":"torch","carried_by":1}
    ]);
    regions[0] = serde_json::from_value(region).unwrap();
    let checked = Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    checked.check().unwrap();
    crate::scenario_package::write_package(root, &checked.manifest, &checked.region_defs().unwrap(), &rules).unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn torch_view(snapshot: &Snapshot, item: ItemTarget) -> PortableLightView {
    nothing_affordance(snapshot, item).portable_light.clone().unwrap()
}

#[test]
fn portable_torch_paid_equipment_ignition_fuel_and_dropped_source_free_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("torch.db");
    let mut wire = Wire::new(Engine::open_with_policy(&path, torch_source(&root, false), crate::SavePolicy {
        checkpoint_interval: 1, ..Default::default()
    }).unwrap());
    let initial = wire.snapshot(OWNER);
    let inventory: Vec<_> = initial.state.observation.inventory.iter().filter(|row| row.class == ItemClass::Tool).map(|row| row.id).collect();
    assert_eq!(inventory.len(), 2);
    assert!(initial.state.observation.visible_cells.iter().all(|cell| !cell.lit));
    let torch = inventory[0];
    let spare = inventory[1];
    let light = torch_view(&initial, torch);
    assert_eq!(light.remaining, 1000);
    assert!(!light.lit && !light.ignitable && !light.extinguishable);
    let slot = initial.state.observation.interactions.as_ref().unwrap().slots.iter().position(|slot| slot.id == "held_light").unwrap() as u16;
    let request = wire.request(OWNER, Action::Ignite { item: torch });
    let untouched = wire.checkpoint();
    rejected(wire.send(OWNER, "unheld-ignite", request));
    assert_eq!(wire.checkpoint(), untouched);
    wire.act("hold-torch", Action::Equip { item: torch, slot });
    let equipped = wire.snapshot(OWNER);
    assert_eq!(torch_view(&equipped, torch).remaining, 1000);
    assert!(!torch_view(&equipped, torch).lit, "equipping never ignites");
    assert!(torch_view(&equipped, torch).ignitable);
    let before = wire.checkpoint();
    let request = wire.request(OWNER, Action::Ignite { item: torch });
    ack(wire.send(OWNER, "ignite-once", request.clone()));
    let receipt = wire.finish(OWNER, "ignite-once", &request);
    let after = wire.checkpoint();
    assert_eq!(after["game"]["tick"].as_u64().unwrap(), before["game"]["tick"].as_u64().unwrap() + 100);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    let illuminated = wire.snapshot(OWNER);
    assert!(illuminated.state.observation.visible_cells.iter().any(|cell| cell.lit));
    let lit = torch_view(&illuminated, torch);
    assert!(lit.lit && lit.extinguishable && !lit.ignitable);
    assert!(lit.remaining > 100 && lit.remaining <= 1000);
    assert_eq!(ack(wire.send(OWNER, "ignite-once", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.act("burn-one-turn", Action::Wait);
    assert_eq!(torch_view(&wire.snapshot(OWNER), torch).remaining, lit.remaining - 100);
    assert_eq!(torch_view(&wire.snapshot(OWNER), spare).remaining, 1000);
    wire.act("drop-lit", Action::Drop { item: torch, quantity: None });
    let dropped = wire.checkpoint();
    let item = nothing_items(&dropped).values().find(|item| item["fuel"]["lit"] == true).unwrap();
    assert!(item["fuel"]["remaining"].as_u64().unwrap() > 0);
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), dropped, "compare full state before Service reconnect pauses work");
}

#[test]
fn portable_torch_extinguish_and_stow_preserve_fuel_without_automatic_reignition() {
    let root = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(torch_source(root.path(), false)).unwrap());
    let snapshot = wire.snapshot(OWNER);
    let torch = snapshot.state.observation.inventory.iter().find(|row| row.class == ItemClass::Tool).unwrap().id;
    let slot = snapshot.state.observation.interactions.as_ref().unwrap().slots.iter().position(|slot| slot.id == "held_light").unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    wire.act("ignite", Action::Ignite { item: torch });
    let before = wire.checkpoint();
    wire.act("extinguish", Action::Extinguish { item: torch });
    let after = wire.checkpoint();
    assert_eq!(after["game"]["tick"].as_u64().unwrap(), before["game"]["tick"].as_u64().unwrap() + 100);
    let off = torch_view(&wire.snapshot(OWNER), torch);
    assert!(!off.lit && off.ignitable && !off.extinguishable);
    wire.act("off-wait", Action::Wait);
    assert_eq!(torch_view(&wire.snapshot(OWNER), torch).remaining, off.remaining);
    wire.act("relight", Action::Ignite { item: torch });
    wire.act("stow-lit", Action::Unequip { item: torch });
    let stowed = torch_view(&wire.snapshot(OWNER), torch);
    assert!(!stowed.lit && !stowed.ignitable && !stowed.extinguishable);
    assert!(stowed.remaining > 0 && stowed.remaining < off.remaining);
    wire.act("hold-again", Action::Equip { item: torch, slot });
    let held = torch_view(&wire.snapshot(OWNER), torch);
    assert!(!held.lit && held.ignitable);
    assert_eq!(held.remaining, stowed.remaining);
    let stable = wire.checkpoint();
    for _ in 0..3 { assert_eq!(torch_view(&wire.snapshot(OWNER), torch).remaining, held.remaining); }
    assert_eq!(wire.checkpoint(), stable, "queries cannot settle fuel or manufacture time");
}

#[test]
fn portable_torch_pending_ignition_survives_interval_one_deleted_source_and_exact_retry() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("pending-torch.db");
    let policy = crate::SavePolicy { checkpoint_interval: 1, ..Default::default() };
    let mut wire = Wire::new(Engine::open_with_policy(&path, torch_source(&root, true), policy.clone()).unwrap());
    let initial = wire.snapshot(OWNER);
    let torch = initial.state.observation.inventory.iter().find(|row| row.class == ItemClass::Tool).unwrap().id;
    let slot = initial.state.observation.interactions.as_ref().unwrap().slots.iter().position(|slot| slot.id == "held_light").unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let request = wire.request(OWNER, Action::Ignite { item: torch });
    let admitted = ack(wire.send(OWNER, "pending-ignite", request.clone()));
    wire.service.run_until_blocked();
    wire.drain(OWNER);
    wire.drain(OTHER);
    let started = ack(wire.send(OWNER, "pending-ignite", request.clone()));
    assert!(matches!(started, RequestReceipt::Admitted { phase: IntentionPhase::Started, .. }));
    assert_eq!(started.entry_id(), admitted.entry_id());
    let pending = wire.checkpoint();
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["active"], true);
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["remaining"], 100);
    let unlit = torch_view(&wire.snapshot(OWNER), torch);
    assert!(!unlit.lit);
    assert_eq!(unlit.remaining, 1000, "pending ignition has not burned fuel");
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy).unwrap();
    assert_eq!(checkpoint(&engine), pending);
    let mut wire = Wire::new(engine);
    wire.serial = serial;
    let paused = wire.checkpoint();
    assert_eq!(nothing_actor(&paused, OWNER)["pending"]["active"], false);
    assert_eq!(nothing_actor(&paused, OWNER)["pending"]["plan"], nothing_actor(&pending, OWNER)["pending"]["plan"]);
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
    ack(wire.send(OWNER, "resume-torch", resume));
    let completed = wire.finish(OWNER, "pending-ignite", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    assert!(torch_view(&wire.snapshot(OWNER), torch).lit);
    let after = wire.checkpoint();
    assert_eq!(after["game"]["rules_rng"], pending["game"]["rules_rng"]);
    assert_eq!(ack(wire.send(OWNER, "pending-ignite", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}
