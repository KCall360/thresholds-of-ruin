//! Admitted imported Haste doses and unchanged persistent-wand interaction.
use super::*;

fn haste_source(root: &Path) -> crate::Scenario {
    let baseline = imported_nothing_source(root);
    let source = baseline.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    assert_eq!(
        manifest["archetypes"]["rogue.haste"]["consumable"]["effects"],
        json!([{"kind":"haste_dose","condition":"rogue.hasted","rest":"rogue.haste_exhaustion"}])
    );
    let figure = manifest["creatures"]["species"]["rogue.human"].clone();
    manifest["creatures"]["species"]["fixture.haste_owner"] = figure.clone();
    manifest["creatures"]["species"]["fixture.haste_caster"] = figure;
    for species in ["fixture.haste_owner", "fixture.haste_caster"] {
        manifest["creatures"]["species"][species]["modifiers"] = json!([
            {"scope":"always","op":{"kind":"set","stat":{"kind":"attribute","attribute":"speed"},"value":0}}
        ]);
    }
    manifest["creatures"]["species"]["fixture.haste_caster"]["modifiers"].as_array_mut().unwrap().push(
        json!({"scope":"always","op":{"kind":"set","stat":{"kind":"attribute","attribute":"intellect"},"value":100}}));
    manifest["archetypes"]["fixture.masked_haste"] = json!({
        "name":"ordinary masked haste dose","class":"potion","stackable":true,
        "consumable":{"discovery":"on_use","effects":[
            {"kind":"apply_persistent_condition","condition":"rogue.hasted","cancels":null},
            {"kind":"apply_persistent_condition","condition":"rogue.slowed","cancels":null}
        ]}
    });
    for species in ["fixture.haste_owner", "fixture.haste_caster"] {
        let mut body_package = manifest["creatures"]["body_packages"]["rogue.body.human"].clone();
        body_package["species"] = json!(species);
        if species == "fixture.haste_caster" {
            // Preserve this fixture actor's prior implicit one-cell geometry.
            body_package["body"] = json!({"cells":[[0,0,0]],"eye":[0,0,0],"mass":80});
        }
        manifest["creatures"]["body_packages"][format!("{species}.body")] = body_package;
    }
    manifest["characters"][0]["creature"]["species"] = json!("fixture.haste_owner");
    manifest["characters"][0]["creature"]["body_package"] = json!("fixture.haste_owner.body");
    manifest["characters"][0]["turn_ticks"] = json!(100);
    let mut caster = manifest["characters"][0]["creature"].clone();
    caster["species"] = json!("fixture.haste_caster");
    caster["body_package"] = json!("fixture.haste_caster.body");
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["actors"] =
        json!([{"id":2,"at":[3,2,0],"turn_ticks":100,"controller":"external","creature":caster}]);
    region["items"] = json!([
        {"id":30,"at":[2,2,0],"archetype":"rogue.haste","quantity":4,"carried_by":1},
        {"id":31,"at":[3,2,0],"archetype":"rogue.wand_haste","carried_by":2},
        {"id":32,"at":[2,2,0],"archetype":"fixture.masked_haste","carried_by":1}
    ]);
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
fn items(wire: &mut Wire) -> (ItemTarget, ItemTarget) {
    let owner = wire.snapshot(OWNER);
    let caster = wire.snapshot(OTHER);
    let a = owner
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    let b = caster
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    assert_eq!(a.attributes.speed, 0);
    assert_eq!(b.attributes.speed, 0);
    assert_eq!(
        b.attributes.intellect, 100,
        "actual caster stat, not an ungranted ability bonus"
    );
    let agility = a
        .skills
        .iter()
        .find(|row| row.skill.id == "agility")
        .unwrap()
        .rank;
    let lore = b
        .skills
        .iter()
        .find(|row| row.skill.id == "lore")
        .unwrap()
        .rank;
    assert!(100 + u32::from(lore) > 20 + u32::from(agility));
    let saved = wire.checkpoint();
    let owner_at: tor_world::Location =
        serde_json::from_value(nothing_actor(&saved, OWNER)["location"].clone()).unwrap();
    let caster_at: tor_world::Location =
        serde_json::from_value(nothing_actor(&saved, OTHER)["location"].clone()).unwrap();
    assert_eq!(owner_at.region.0, 1);
    assert_eq!(
        (
            owner_at.position.x,
            owner_at.position.y,
            owner_at.position.z
        ),
        (2, 2, 0)
    );
    assert_eq!(owner_at.region, caster_at.region);
    assert!(owner.state.observation.combat.as_ref().unwrap().hp > 0);
    assert!(caster.state.observation.combat.as_ref().unwrap().hp > 0);
    assert_eq!(caster_at.position.x, owner_at.position.x + 1);
    assert_eq!(caster_at.position.y, owner_at.position.y);
    assert_eq!(caster_at.position.z, owner_at.position.z);
    (
        owner
            .state
            .observation
            .inventory
            .iter()
            .find(|row| row.class == ItemClass::Potion && row.quantity == 4)
            .unwrap()
            .id,
        caster
            .state
            .observation
            .inventory
            .iter()
            .find(|row| row.class == ItemClass::Wand)
            .unwrap()
            .id,
    )
}
fn assert_paid_dose(wire: &mut Wire, potion: ItemTarget) {
    let snapshot = wire.snapshot(OWNER);
    let item = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.id == potion)
        .unwrap();
    assert_eq!(item.quantity, 3);
    assert!(
        item.identified,
        "OnUse disclosure follows actual completed dose"
    );
    assert!(nothing_actor(&wire.checkpoint(), OWNER)["pending"].is_null());
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
        let id = format!("haste-ready-{}", wire.serial);
        let request = wire.request(next, Action::Wait);
        ack(wire.send(next, &id, request.clone()));
        wire.finish(next, &id, &request);
    }
    panic!("lawful scheduler did not reach requested actor");
}
fn wand(wire: &mut Wire, item: ItemTarget) {
    ready(wire, OTHER);
    let before = wire.checkpoint();
    let charges = nothing_items(&before)["31"]["charges"].as_u64().unwrap();
    assert!(charges >= 3);
    let request = wire.request(
        OTHER,
        Action::Zap {
            item,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    ack(wire.send(OTHER, "haste-wand", request.clone()));
    wire.finish(OTHER, "haste-wand", &request);
    assert_eq!(
        nothing_items(&wire.checkpoint())["31"]["charges"],
        charges - 1
    );
}

#[test]
fn haste_dose_then_persistent_wand_keeps_original_finite_expiry() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(
        Engine::open(
            directory.path().join("save.db"),
            haste_source(&directory.path().join("source")),
        )
        .unwrap(),
    );
    let (potion, device) = items(&mut wire);
    ready(&mut wire, OWNER);
    wire.act("first-dose", Action::Drink { item: potion });
    assert_paid_dose(&mut wire, potion);
    let finite = condition(&mut wire, "rogue.hasted").unwrap();
    assert_eq!(finite.phase, ConditionPhase::Active);
    let remaining = finite.remaining_ticks.unwrap();
    assert!((400..=700).contains(&remaining));
    let expiry = wire.checkpoint()["game"]["tick"].as_u64().unwrap() + remaining;
    wand(&mut wire, device);
    let kept = condition(&mut wire, "rogue.hasted")
        .expect("real finite recipient still active at contact");
    assert_eq!(
        wire.checkpoint()["game"]["tick"].as_u64().unwrap() + kept.remaining_ticks.unwrap(),
        expiry
    );
    assert!(condition(&mut wire, "rogue.haste_exhaustion").is_none());
}

#[test]
fn persistent_wand_then_haste_dose_is_paid_overdose_not_finite_upgrade() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(
        Engine::open(
            directory.path().join("save.db"),
            haste_source(&directory.path().join("source")),
        )
        .unwrap(),
    );
    let (potion, device) = items(&mut wire);
    wand(&mut wire, device);
    assert_eq!(
        condition(&mut wire, "rogue.hasted")
            .unwrap()
            .remaining_ticks,
        None
    );
    ready(&mut wire, OWNER);
    let request = wire.request(OWNER, Action::Drink { item: potion });
    let admitted = ack(wire.send(OWNER, "overdose", request.clone()));
    let completed = wire.finish(OWNER, "overdose", &request);
    assert_eq!(admitted.entry_id(), completed.entry_id());
    assert_paid_dose(&mut wire, potion);
    assert!(condition(&mut wire, "rogue.hasted").is_none());
    if let Some(rest) = condition(&mut wire, "rogue.haste_exhaustion") {
        assert_eq!(rest.phase, ConditionPhase::Active);
        assert!((1..=700).contains(&rest.remaining_ticks.unwrap()));
    }
    let after = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "overdose", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}

#[test]
fn pending_haste_dose_interval_one_deleted_source_reopen_preserves_payload_and_retry() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("save.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire =
        Wire::new(Engine::open_with_policy(&path, haste_source(&root), policy.clone()).unwrap());
    let (potion, _) = items(&mut wire);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before = wire.checkpoint();
    let request = wire.request(OWNER, Action::Drink { item: potion });
    let admitted = ack(wire.send(OWNER, "pending-dose", request.clone()));
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
    let started = ack(wire.send(OWNER, "pending-dose", request.clone()));
    assert!(matches!(
        started,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    assert_eq!(started.entry_id(), admitted.entry_id());
    assert!(condition(&mut wire, "rogue.hasted").is_none());
    let pending = wire.checkpoint();
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["active"], true);
    assert_eq!(nothing_actor(&pending, OWNER)["pending"]["remaining"], 100);
    assert_eq!(pending["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(nothing_items(&pending)["30"]["quantity"], 4);
    wire.service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&pending).unwrap())
        .unwrap();
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
            intention: snapshot.readiness.resume[0].clone(),
        },
    };
    ack(wire.send(OWNER, "resume-dose", resume));
    let completed = wire.finish(OWNER, "pending-dose", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    assert_paid_dose(&mut wire, potion);
    let finite = condition(&mut wire, "rogue.hasted").unwrap();
    assert!((400..=700).contains(&finite.remaining_ticks.unwrap()));
    let after = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "pending-dose", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}

#[test]
fn haste_dose_detects_ordinary_canonical_haste_even_when_slow_masks_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(
        Engine::open(
            directory.path().join("save.db"),
            haste_source(&directory.path().join("source")),
        )
        .unwrap(),
    );
    let (potion, _) = items(&mut wire);
    let starter = wire
        .snapshot(OWNER)
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.name == "ordinary masked haste dose")
        .unwrap()
        .id;
    ready(&mut wire, OWNER);
    wire.act("ordinary-haste-and-slow", Action::Drink { item: starter });
    assert_eq!(
        condition(&mut wire, "rogue.hasted")
            .unwrap()
            .remaining_ticks,
        None
    );
    assert_eq!(
        condition(&mut wire, "rogue.slowed")
            .unwrap()
            .remaining_ticks,
        None
    );
    ready(&mut wire, OWNER);
    wire.act("masked-overdose", Action::Drink { item: potion });
    assert_paid_dose(&mut wire, potion);
    assert!(condition(&mut wire, "rogue.hasted").is_none());
    assert_eq!(
        condition(&mut wire, "rogue.slowed")
            .unwrap()
            .remaining_ticks,
        None,
        "overdose does not strip unrelated Slow"
    );
}

#[test]
fn haste_dose_source_accepts_ordered_self_context_and_rejects_targeted_or_checked_context() {
    use tor_simulation::{
        abilities::InvocationKind,
        rules::{Interaction, Reach, StrikeSource},
    };
    let root = tempfile::tempdir().unwrap();
    let scenario = haste_source(root.path());
    let package = scenario.package.as_ref().unwrap();
    let mut document = package.rules().unwrap().document().clone();
    let mut ability = document
        .abilities
        .iter()
        .find(|row| row.id.as_str() == "rogue.wand_haste")
        .unwrap()
        .clone();
    let original_check = match &ability.strikes {
        StrikeSource::Authored { strikes } => strikes[0].check.clone(),
        _ => panic!("real authored wand"),
    };
    ability.id = tor_simulation::content_ids::AbilityId::new("fixture.self_haste_dose").unwrap();
    ability.invocation = InvocationKind::ActorTarget;
    ability.interaction = Interaction::NonHostile;
    ability.reach = Reach::SelfOnly;
    ability.ai.hostile = false;
    ability.tags.clear();
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    strikes[0].check = None;
    strikes[0].on_success = vec![
        tor_simulation::rules::Effect::RemoveCondition {
            condition: tor_simulation::content_ids::ConditionId::new("rogue.hasted").unwrap(),
        },
        tor_simulation::rules::Effect::HasteDose {
            condition: tor_simulation::content_ids::ConditionId::new("rogue.hasted").unwrap(),
            rest: tor_simulation::content_ids::ConditionId::new("rogue.haste_exhaustion").unwrap(),
        },
    ];
    strikes[0].on_failure.clear();
    strikes[0].riders.clear();
    document.abilities.push(ability);
    Package::from_parts(
        package.manifest.clone(),
        package.region_defs().unwrap(),
        &toml::to_string(&document).unwrap(),
    )
    .unwrap()
    .build(42, false)
    .unwrap();
    for case in ["targeted", "hostile", "checked"] {
        let mut invalid = document.clone();
        let ability = invalid.abilities.last_mut().unwrap();
        match case {
            "targeted" => ability.reach = Reach::Sight { range: 6 },
            "hostile" => {
                ability.interaction = Interaction::HostileToTarget;
                ability.ai.hostile = true;
            }
            "checked" => {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                strikes[0].check = original_check.clone();
            }
            _ => unreachable!(),
        }
        assert!(
            Package::from_parts(
                package.manifest.clone(),
                package.region_defs().unwrap(),
                &toml::to_string(&invalid).unwrap()
            )
            .and_then(|value| value.build(42, false))
            .is_err(),
            "{case}"
        );
    }
}
