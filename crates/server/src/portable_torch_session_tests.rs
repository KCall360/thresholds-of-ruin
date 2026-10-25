//! Admitted portable-light work and genuine compressed source-free recovery.
use super::*;

fn torch_source(root: &Path, with_other: bool) -> crate::Scenario {
    let baseline = self_device_source(root, with_other, false);
    let source = baseline.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["creatures"]["species"]["figure"]["anatomy"]["slots"]
        .as_array_mut()
        .unwrap()
        .push(json!("held_light"));
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
    let checked =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    checked.check().unwrap();
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn torch_view(snapshot: &Snapshot, item: ItemTarget) -> PortableLightView {
    nothing_affordance(snapshot, item)
        .portable_light
        .clone()
        .unwrap()
}

#[test]
fn portable_torch_paid_equipment_ignition_fuel_and_dropped_source_free_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("torch.db");
    let mut wire = Wire::new(
        Engine::open_with_policy(
            &path,
            torch_source(&root, false),
            crate::SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let initial = wire.snapshot(OWNER);
    let inventory: Vec<_> = initial
        .state
        .observation
        .inventory
        .iter()
        .filter(|row| row.class == ItemClass::Tool)
        .map(|row| row.id)
        .collect();
    assert_eq!(inventory.len(), 2);
    assert!(initial
        .state
        .observation
        .visible_cells
        .iter()
        .all(|cell| !cell.lit));
    let torch = inventory[0];
    let spare = inventory[1];
    let light = torch_view(&initial, torch);
    assert_eq!(light.remaining, 1000);
    assert!(!light.lit && !light.ignitable && !light.extinguishable);
    let slot = initial
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id == "held_light")
        .unwrap() as u16;
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
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    let illuminated = wire.snapshot(OWNER);
    assert!(illuminated
        .state
        .observation
        .visible_cells
        .iter()
        .any(|cell| cell.lit));
    let lit = torch_view(&illuminated, torch);
    assert!(lit.lit && lit.extinguishable && !lit.ignitable);
    assert!(lit.remaining > 100 && lit.remaining <= 1000);
    assert_eq!(ack(wire.send(OWNER, "ignite-once", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.act("burn-one-turn", Action::Wait);
    assert_eq!(
        torch_view(&wire.snapshot(OWNER), torch).remaining,
        lit.remaining - 100
    );
    assert_eq!(torch_view(&wire.snapshot(OWNER), spare).remaining, 1000);
    wire.act(
        "drop-lit",
        Action::Drop {
            item: torch,
            quantity: None,
        },
    );
    let dropped = wire.checkpoint();
    let item = nothing_items(&dropped)
        .values()
        .find(|item| item["fuel"]["lit"] == true)
        .unwrap();
    assert!(item["fuel"]["remaining"].as_u64().unwrap() > 0);
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(
        checkpoint(&reopened),
        dropped,
        "compare full state before Service reconnect pauses work"
    );
}

#[test]
fn portable_torch_extinguish_and_stow_preserve_fuel_without_automatic_reignition() {
    let root = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(torch_source(root.path(), false)).unwrap());
    let snapshot = wire.snapshot(OWNER);
    let torch = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Tool)
        .unwrap()
        .id;
    let slot = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id == "held_light")
        .unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    wire.act("ignite", Action::Ignite { item: torch });
    let before = wire.checkpoint();
    wire.act("extinguish", Action::Extinguish { item: torch });
    let after = wire.checkpoint();
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    let off = torch_view(&wire.snapshot(OWNER), torch);
    assert!(!off.lit && off.ignitable && !off.extinguishable);
    wire.act("off-wait", Action::Wait);
    assert_eq!(
        torch_view(&wire.snapshot(OWNER), torch).remaining,
        off.remaining
    );
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
    for _ in 0..3 {
        assert_eq!(
            torch_view(&wire.snapshot(OWNER), torch).remaining,
            held.remaining
        );
    }
    assert_eq!(
        wire.checkpoint(),
        stable,
        "queries cannot settle fuel or manufacture time"
    );
}

#[test]
fn portable_torch_pending_ignition_survives_interval_one_deleted_source_and_exact_retry() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("pending-torch.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire = Wire::new(
        Engine::open_with_policy(&path, torch_source(&root, true), policy.clone()).unwrap(),
    );
    let initial = wire.snapshot(OWNER);
    let torch = initial
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Tool)
        .unwrap()
        .id;
    let slot = initial
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id == "held_light")
        .unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let request = wire.request(OWNER, Action::Ignite { item: torch });
    let admitted = ack(wire.send(OWNER, "pending-ignite", request.clone()));
    wire.service.run_until_blocked();
    for actor in [OWNER, OTHER] {
        let messages = wire.drain(actor);
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Error { .. })),
            "pending torch publication: {messages:?}"
        );
    }
    let started = ack(wire.send(OWNER, "pending-ignite", request.clone()));
    assert!(matches!(
        started,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    assert_eq!(started.entry_id(), admitted.entry_id());
    let pending = wire.checkpoint();
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["active"], true);
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["remaining"], 100);
    let unlit = torch_view(&wire.snapshot(OWNER), torch);
    assert!(!unlit.lit);
    assert_eq!(
        unlit.remaining, 1000,
        "pending ignition has not burned fuel"
    );
    wire.service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&pending).unwrap())
        .unwrap();
    let actors = pending["game"]["actors"].as_u64().unwrap();
    let plan_path = format!("/shared/actors/{actors}/1/pending/plan");
    assert_eq!(pending.pointer(&plan_path).unwrap()["kind"], "ignite");
    for (field, value) in [
        ("actor", json!(2)),
        ("item", json!(999)),
        ("preparation", json!(0)),
        ("kind", json!("extinguish")),
        ("fuel_ticks", json!(1000)),
    ] {
        let mut forged = pending.clone();
        forged.pointer_mut(&plan_path).unwrap()[field] = value;
        assert!(
            wire.service
                .engine
                .test_restore_checkpoint(&serde_json::to_vec(&forged).unwrap())
                .is_err(),
            "forged pending {field}"
        );
    }
    for field in ["item", "actor", "preparation"] {
        let mut forged = pending.clone();
        forged
            .pointer_mut(&plan_path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            wire.service
                .engine
                .test_restore_checkpoint(&serde_json::to_vec(&forged).unwrap())
                .is_err(),
            "missing pending {field}"
        );
    }
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
    assert_eq!(
        nothing_actor(&paused, OWNER)["pending"]["plan"],
        nothing_actor(&pending, OWNER)["pending"]["plan"]
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
    ack(wire.send(OWNER, "resume-torch", resume));
    let completed = wire.finish(OWNER, "pending-ignite", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    assert!(torch_view(&wire.snapshot(OWNER), torch).lit);
    let after = wire.checkpoint();
    assert_eq!(after["game"]["rules_rng"], pending["game"]["rules_rng"]);
    assert_eq!(ack(wire.send(OWNER, "pending-ignite", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}

#[test]
fn portable_torch_source_requires_single_tool_and_real_hand_equipment() {
    let root = tempfile::tempdir().unwrap();
    let scenario = torch_source(root.path(), false);
    let source = scenario.package.as_ref().unwrap();
    source.check().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let original = serde_json::to_value(&source.manifest).unwrap();
    for (field, value) in [
        ("class", json!("weapon")),
        ("stackable", json!(true)),
        ("equipment", Value::Null),
    ] {
        let mut manifest = original.clone();
        manifest["archetypes"]["torch"][field] = value;
        let result = Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            source.region_defs().unwrap(),
            &rules,
        )
        .and_then(|package| package.check());
        assert!(result.is_err(), "invalid torch {field}");
    }
    let mut aliases = original.clone();
    aliases["archetypes"]["torch"]["identity"] = json!("fixture.shared_torch");
    aliases["archetypes"]["torch_alias"] = aliases["archetypes"]["torch"].clone();
    Package::from_parts(
        serde_json::from_value(aliases.clone()).unwrap(),
        source.region_defs().unwrap(),
        &rules,
    )
    .unwrap()
    .check()
    .unwrap();
    for (field, value) in [("radius", json!(5)), ("fuel_ticks", json!(999))] {
        let mut mismatch = aliases.clone();
        mismatch["archetypes"]["torch_alias"]["held_light"][field] = value;
        assert!(
            Package::from_parts(
                serde_json::from_value(mismatch).unwrap(),
                source.region_defs().unwrap(),
                &rules
            )
            .and_then(|package| package.check())
            .is_err(),
            "identity mismatched {field}"
        );
    }
    let mut mismatch = aliases;
    mismatch["archetypes"]["torch_alias"]["held_light"] = Value::Null;
    assert!(
        Package::from_parts(
            serde_json::from_value(mismatch).unwrap(),
            source.region_defs().unwrap(),
            &rules
        )
        .and_then(|package| package.check())
        .is_err(),
        "identity mismatched capability presence"
    );
    // Quantity is checked by the ordinary item compiler, not the metadata-only
    // Package::check. Exercise that same full boundary for carried and ground
    // placement, establishing each valid single-item baseline first.
    for carried_by in [Some(OWNER.0), None] {
        let mut regions = source.region_defs().unwrap();
        let mut region = serde_json::to_value(&regions[0]).unwrap();
        region["items"][0]["carried_by"] = json!(carried_by);
        region["items"][0]["quantity"] = json!(1);
        regions[0] = serde_json::from_value(region.clone()).unwrap();
        Package::from_parts(source.manifest.clone(), regions.clone(), &rules)
            .unwrap()
            .build(42, false)
            .unwrap();
        region["items"][0]["quantity"] = json!(2);
        regions[0] = serde_json::from_value(region).unwrap();
        let invalid = Package::from_parts(source.manifest.clone(), regions, &rules).unwrap();
        assert!(invalid.build(42, false).is_err());
        if carried_by.is_some() {
            // Keep the original invalid carried payload and prove the supported
            // public source checker rejects it too; torch_source already passed
            // this checker with the otherwise identical unit-quantity source.
            let invalid_source = tempfile::tempdir().unwrap();
            crate::scenario_package::write_package(
                invalid_source.path(),
                &invalid.manifest,
                &invalid.region_defs().unwrap(),
                &rules,
            )
            .unwrap();
            assert!(crate::scenario_package::check(invalid_source.path()).is_err());
        }
    }
}

fn blind_portal_torch_source(root: &Path, cold_frontier: bool) -> crate::Scenario {
    let baseline = torch_source(root, false);
    let source = baseline.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    let figure = manifest["creatures"]["species"]["figure"].clone();
    manifest["creatures"]["species"]["blind_figure"] = figure;
    manifest["creatures"]["species"]["blind_figure"]["modifiers"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "scope":"always", "op":{"kind":"perception","value":{"kind":"sight","enabled":false}}
        }));
    manifest["characters"][0]["creature"]["species"] = json!("blind_figure");
    let mut remote_creature = manifest["characters"][0]["creature"].clone();
    remote_creature["species"] = json!("figure");
    if cold_frontier {
        // This is deliberately unusual but lawful authored geometry: the
        // carrier origin/emitter is eight cells ahead of its eye. Existing
        // conservative eye dependencies therefore do not preload the far room.
        manifest["characters"][0]["body"] = json!({
            "cells":(-8..=0).map(|x| [x,0,0]).collect::<Vec<_>>(),
            "eye":[-8,0,0], "mass":80
        });
    }
    let origin_x = if cold_frontier { 22 } else { 28 };
    let mut local = serde_json::to_value(&source.region_defs().unwrap()[0]).unwrap();
    local["size"] = json!([32, 5, 2]);
    local["anchors"]["start"] = json!([origin_x, 2, 0]);
    local["anchors"]["east"] = json!([31, 2, 0]);
    local["portals"] =
        json!([{"at":[31,2,0],"direction":"east","to":"2/west","kind":"portal","height":2}]);
    for item in local["items"].as_array_mut().unwrap() {
        item["at"] = json!([origin_x, 2, 0]);
    }
    let remote = json!({"id":2,"name":"Unobserved torch dependency","size":[5,5,2],"chamber":true,"lit":false,
        "anchors":{"west":[0,2,0]},
        "portals":[{"at":[0,2,0],"direction":"west","to":"1/east","kind":"portal","height":2}],
        "actors":[{"id":3,"at":[0,2,0],"turn_ticks":100,"controller":"ai","creature":remote_creature}]
    });
    let mut regions = vec![serde_json::from_value(local).unwrap()];
    if cold_frontier {
        // Explicit maximum supported radius for this offset-eye fixture only.
        manifest["archetypes"]["torch"]["held_light"]["radius"] = json!(16);
        let middle = json!({"id":2,"name":"Load-only intermediate","size":[3,5,2],"chamber":true,"lit":false,
            "anchors":{"west":[0,2,0],"east":[2,2,0]},
            "portals":[
                {"at":[0,2,0],"direction":"west","to":"1/east","kind":"portal","height":2},
                {"at":[2,2,0],"direction":"east","to":"3/west","kind":"portal","height":2}]
        });
        let mut far = remote;
        far["id"] = json!(3);
        far["portals"][0]["to"] = json!("2/east");
        regions.push(serde_json::from_value(middle).unwrap());
        regions.push(serde_json::from_value(far).unwrap());
    } else {
        regions.push(serde_json::from_value(remote).unwrap());
    }
    let checked =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    checked.check().unwrap();
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    let mut scenario = crate::scenario_package::load(root, 42, None, true).unwrap();
    // The witness concerns optical dependencies, not configured portal-hop activation.
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

#[test]
fn portable_torch_optical_dependency_lights_frozen_ai_without_observer_activation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("frozen-torch.db");
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(
            &path,
            blind_portal_torch_source(&root, false),
            crate::SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            },
        )
        .unwrap(),
        [OWNER],
    );
    let initial = wire.checkpoint();
    let world = room_light_world(&initial);
    let mut light_path = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 28, y: 2, z: 0 },
    };
    for _ in 0..4 {
        light_path = world
            .physics_neighbor(light_path, tor_world::Direction::East)
            .unwrap()
            .0;
        assert!(world.walkable(light_path));
    }
    assert_eq!(
        light_path,
        tor_world::Location {
            region: tor_world::RegionId(2),
            position: tor_world::Position { x: 0, y: 2, z: 0 }
        }
    );
    let frozen_actor = nothing_actor(&initial, ActorId(3)).clone();
    assert!(
        frozen_actor.is_object(),
        "linked source is genuinely prepared"
    );
    assert_eq!(frozen_actor["ready_at"], 0);
    assert_eq!(frozen_actor["active_since"], 0);
    let assert_frozen = |saved: &Value| {
        let index = saved["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert!(saved["shared"]["lifecycles"][index]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(2)));
        assert_eq!(saved["shared"]["lifecycles"][index]["stamps"]["3"], 0);
        assert_eq!(nothing_actor(saved, ActorId(3)), &frozen_actor);
    };
    assert_frozen(&initial);
    let remote_before = wire.service.engine.observation(ActorId(3)).unwrap();
    assert!(remote_before.visible_cells.iter().all(|cell| !cell.lit));
    assert_eq!(
        wire.checkpoint(),
        initial,
        "direct observation never installs a reference point"
    );
    let snapshot = wire.snapshot(OWNER);
    let torch = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Tool)
        .unwrap()
        .id;
    let slot = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id == "held_light")
        .unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    wire.act("ignite", Action::Ignite { item: torch });
    let after = wire.checkpoint();
    assert_frozen(&after);
    let owner = wire.snapshot(OWNER);
    assert!(torch_view(&owner, torch).lit);
    assert!(
        owner
            .state
            .observation
            .visible_cells
            .iter()
            .all(|cell| cell.position.x.abs() <= 1 && cell.position.y.abs() <= 1),
        "lawful blindness keeps the remote light outside the ordinary observer scene"
    );
    let remote_after = wire.service.engine.observation(ActorId(3)).unwrap();
    assert!(
        remote_after
            .visible_cells
            .iter()
            .any(|cell| cell.position.x == 0
                && cell.position.y == 0
                && cell.position.z == 0
                && cell.lit),
        "actual remote body cell receives portable light"
    );
    assert_eq!(wire.checkpoint(), after);
    let world = room_light_world(&after);
    assert_eq!(
        world.is_lit(tor_world::Location {
            region: tor_world::RegionId(2),
            position: tor_world::Position { x: 0, y: 2, z: 0 }
        }),
        Some(false),
        "portable light is not ambient World mutation"
    );
    wire.service.engine.flush().unwrap();
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), after);
    assert!(reopened
        .observation(ActorId(3))
        .unwrap()
        .visible_cells
        .iter()
        .any(|cell| cell.position.x == 0
            && cell.position.y == 0
            && cell.position.z == 0
            && cell.lit));
    assert_eq!(checkpoint(&reopened), after);
}

#[test]
fn portable_torch_missing_optical_frontier_is_prepared_load_only_before_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("cold-torch.db");
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(
            &path,
            blind_portal_torch_source(&root, true),
            crate::SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            },
        )
        .unwrap(),
        [OWNER],
    );
    let initial = wire.checkpoint();
    assert_eq!(wire.service.engine.region_counts().unwrap().unbuilt, 1);
    assert!(nothing_actor(&initial, ActorId(3)).is_null());
    let owner = nothing_actor(&initial, OWNER);
    assert_eq!(owner["location"]["position"], json!({"x":22,"y":2,"z":0}));
    assert_eq!(owner["body"]["eye"], json!([-8, 0, 0]));
    assert_eq!(
        owner["body"]["cells"],
        json!((-8..=0).map(|x| [x, 0, 0]).collect::<Vec<_>>())
    );
    let initial_world = room_light_world(&initial);
    for x in 14..=22 {
        assert!(
            initial_world.walkable(tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x, y: 2, z: 0 }
            }),
            "every authored body cell fits the actual initial World"
        );
    }
    let eye = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 14, y: 2, z: 0 },
    };
    assert_eq!(
        initial_world
            .illuminated_eye_scene_regions(
                eye,
                0,
                u8::try_from(tor_simulation::limits::SIGHT_RANGE).unwrap()
            )
            .unwrap(),
        vec![tor_world::RegionId(1), tor_world::RegionId(2)],
        "historical conservative eye dependencies stop before the optical frontier"
    );
    let far = tor_world::Location {
        region: tor_world::RegionId(3),
        position: tor_world::Position { x: 0, y: 2, z: 0 },
    };
    assert_eq!(room_light_world(&initial).is_lit(far), None);
    let snapshot = wire.snapshot(OWNER);
    let torch = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Tool)
        .unwrap()
        .id;
    let slot = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id == "held_light")
        .unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    assert_eq!(
        wire.service.engine.region_counts().unwrap().unbuilt,
        1,
        "unlit equipment does not acquire the optical frontier"
    );
    let before = wire.checkpoint();
    let request = wire.request(OWNER, Action::Ignite { item: torch });
    ack(wire.send(OWNER, "cold-ignite", request.clone()));
    let receipt = wire.finish(OWNER, "cold-ignite", &request);
    let after = wire.checkpoint();
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(wire.service.engine.region_counts().unwrap().unbuilt, 0);
    assert_eq!(wire.service.engine.region_counts().unwrap().active, 1);
    let world = room_light_world(&after);
    let mut cursor = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 22, y: 2, z: 0 },
    };
    for _ in 0..13 {
        cursor = world
            .physics_neighbor(cursor, tor_world::Direction::East)
            .unwrap()
            .0;
        assert!(world.walkable(cursor));
    }
    assert_eq!(cursor, far);
    let actor = nothing_actor(&after, ActorId(3)).clone();
    assert!(actor.is_object());
    assert_eq!(actor["ready_at"], 0);
    assert_eq!(actor["active_since"], 0);
    let index = after["game"]["lifecycle"].as_u64().unwrap() as usize;
    assert!(after["shared"]["lifecycles"][index]["frozen"]
        .as_array()
        .unwrap()
        .contains(&json!(3)));
    assert_eq!(after["shared"]["lifecycles"][index]["stamps"]["3"], 0);
    let owner = wire.snapshot(OWNER);
    assert!(
        owner
            .state
            .observation
            .visible_cells
            .iter()
            .all(|cell| (-9..=1).contains(&cell.position.x) && cell.position.y.abs() <= 1),
        "offset body's tactile disclosure remains local despite actual remote light"
    );
    let light = torch_view(&owner, torch);
    assert!(light.lit);
    assert_eq!(
        light.remaining, 1000,
        "fuel starts at completed ignition, not geometry preparation"
    );
    assert!(nothing_items(&after)
        .values()
        .all(|item| item["holds"].as_array().unwrap().is_empty()));
    let observation = wire.service.engine.observation(ActorId(3)).unwrap();
    assert!(observation
        .visible_cells
        .iter()
        .any(|cell| cell.position.x == 0
            && cell.position.y == 0
            && cell.position.z == 0
            && cell.lit));
    assert_eq!(wire.checkpoint(), after);
    assert_eq!(ack(wire.send(OWNER, "cold-ignite", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.act("fuel-only-wait", Action::Wait);
    let waited = wire.checkpoint();
    assert_eq!(torch_view(&wire.snapshot(OWNER), torch).remaining, 900);
    assert_eq!(nothing_actor(&waited, ActorId(3)), &actor);
    let index = waited["game"]["lifecycle"].as_u64().unwrap() as usize;
    assert_eq!(waited["shared"]["lifecycles"][index]["stamps"]["3"], 0);
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), waited);
    assert!(reopened
        .observation(ActorId(3))
        .unwrap()
        .visible_cells
        .iter()
        .any(|cell| cell.position.x == 0
            && cell.position.y == 0
            && cell.position.z == 0
            && cell.lit));
    assert_eq!(checkpoint(&reopened), waited);
}

#[test]
fn portable_torch_engine_fuel_only_wait_skips_map_work_and_matches_conservative_acquisition() {
    let root = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(torch_source(root.path(), true)).unwrap());
    let snapshot = wire.snapshot(OWNER);
    let torch = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Tool)
        .unwrap()
        .id;
    let slot = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id == "held_light")
        .unwrap() as u16;
    wire.act("hold", Action::Equip { item: torch, slot });
    wire.act("ignite", Action::Ignite { item: torch });
    let before = torch_view(&wire.snapshot(OWNER), torch);
    assert!(before.lit);
    let seeded = wire.service.engine.test_conservative_map_reacquisition();
    assert!(seeded.observed_cells > 0);
    let before_tick = wire.checkpoint()["game"]["tick"].as_u64().unwrap();
    tor_simulation::reset_map_work_counts();
    wire.act("steady-fuel-wait", Action::Wait);
    // Immediate Wait completes the owner's action before the other external
    // actor has supplied its same-tick decision. Drive that legal peer once.
    assert_eq!(wire.service.engine.next_actor(), Some(OTHER));
    let peer_wait = wire.request(OTHER, Action::Wait);
    ack(wire.send(OTHER, "steady-fuel-peer-wait", peer_wait.clone()));
    wire.finish(OTHER, "steady-fuel-peer-wait", &peer_wait);
    let optimized = tor_simulation::map_work_counts();
    assert_eq!(
        wire.checkpoint()["game"]["tick"].as_u64().unwrap(),
        before_tick + 100
    );
    assert_eq!(optimized.observed_cells, 0);
    assert_eq!(optimized.placement_proposals, 0);
    assert_eq!(optimized.appearance_changes, 0);
    assert_eq!(optimized.appearance_payload_copies, 0);
    assert_eq!(optimized.neighbor_changes, 0);
    assert_eq!(optimized.witness_changes, 0);
    assert_eq!(optimized.owner_copies, 0);
    assert_eq!(optimized.directory_entries_copied, 0);
    assert_eq!(optimized.bucket_entries_copied, 0);
    let after = torch_view(&wire.snapshot(OWNER), torch);
    assert!(after.lit);
    assert_eq!(after.remaining, before.remaining - 100);
    let checkpoint = wire.checkpoint();
    let conservative = wire.service.engine.test_conservative_map_reacquisition();
    assert!(conservative.observed_cells > optimized.observed_cells);
    assert_eq!(
        wire.checkpoint(),
        checkpoint,
        "oracle never mutates the live engine"
    );
}
