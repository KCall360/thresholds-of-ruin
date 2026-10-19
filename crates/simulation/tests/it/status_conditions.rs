//! Generic status timing contracts, written before the implementation.
//! Renamed content proves these are mechanics, not condition-ID exceptions.
use serde_json::{json, Value};
use tor_simulation::{
    conditions::ConditionUpdate,
    content_ids::ConditionId,
    creatures::{record::CreatureRecord, Content, CreatureBuild, CreatureState},
    rules::{ConditionDef, Ruleset, RulesetDocument},
    ActorId,
};

pub(super) fn definition(name: &str, protection: Option<u64>) -> Value {
    json!({
        "id":name,"family":name,"lifecycle":"timed","name":name,
        "modifiers":[],"stacking":{"kind":"keep"},"source_scoped":false,
        "descriptors":[],"capacity":"reject",
        "timing":{"kind":"actor_active","on_expiry":protection.map_or(
            json!({"kind":"remove"}), |duration| json!({"kind":"protect","duration":duration}))},
        "restrictions":["voluntary_actions"],"preparation":"pause"
    })
}

fn creature(definitions: Vec<Value>) -> CreatureState {
    creature_with(definitions, |_| {})
}
fn creature_with(definitions: Vec<Value>, configure: impl FnOnce(&mut Value)) -> CreatureState {
    let original = super::creature_fixture::build(super::creature_fixture::species());
    let mut document = serde_json::to_value(original.ruleset().document()).unwrap();
    document["conditions"]
        .as_array_mut()
        .unwrap()
        .extend(definitions);
    configure(&mut document);
    let rules =
        Ruleset::validate(serde_json::from_value(document).expect("typed condition semantics"))
            .unwrap();
    let content = Content::new(rules, original.content().catalog().clone()).unwrap();
    CreatureState::new(
        CreatureBuild::new(
            content,
            original.species_id().clone(),
            original.ledger().clone(),
            original.binding(),
            *original.starting_skills(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn denial_and_protection_share_elapsed_time_without_stopping_other_actor_clocks() {
    let id = ConditionId::new("fixture_lock").unwrap();
    let mut subject = creature_with(vec![definition(id.as_str(), Some(400))], |document| {
        document["nutrition"] = json!({"kind":"enabled","initial":8,"maximum":12,
            "hungry_below":7,"weak_below":3,"starve_at":-3,"unit_ticks":100,
            "hungry_recovery":{"kind":"slowed","multiplier":2},
            "weak_recovery":{"kind":"stopped"},"regeneration_cost":{"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0},
        "empty_recovery":{"kind":"stopped"}});
        document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "basic_melee")
            .unwrap()["cooldown"] = json!(700);
    });
    subject
        .configure_nutrition(tor_simulation::nutrition::NutritionRequirement::Required)
        .unwrap();
    subject.damage(5);
    let ability = tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap();
    subject.start_cooldown(&ability, 700).unwrap();
    let mut control = subject.clone();
    subject.apply_condition(&id, ActorId(7), 200).unwrap();
    for elapsed in [199, 1, 399, 1] {
        subject.advance_active(elapsed);
        control.advance_active(elapsed);
        assert_eq!(subject.nutrition(), control.nutrition());
        assert_eq!(subject.health(), control.health());
        assert_eq!(subject.health_recovery(), control.health_recovery());
        assert_eq!(subject.costs(), control.costs());
        assert_eq!(subject.cooldowns(), control.cooldowns());
    }
    // Expiry removes the condition, not its non-reusable instance generation.
    assert!(subject.conditions().is_empty());
    assert!(control.conditions().is_empty());
    let mut subject_record = record(&subject);
    let mut control_record = record(&control);
    assert_eq!(subject_record["next_condition_generation"], 1);
    assert_eq!(control_record["next_condition_generation"], 0);
    subject_record
        .as_object_mut()
        .unwrap()
        .remove("next_condition_generation");
    control_record
        .as_object_mut()
        .unwrap()
        .remove("next_condition_generation");
    assert_eq!(subject_record, control_record);
    assert_eq!(restored(&subject), subject);
    assert_eq!(restored(&control), control);
}

fn record(creature: &CreatureState) -> Value {
    serde_json::to_value(CreatureRecord::capture(creature)).unwrap()
}

fn restored(creature: &CreatureState) -> CreatureState {
    serde_json::from_value::<CreatureRecord>(record(creature))
        .unwrap()
        .restore(creature.build().content().clone(), vec![])
        .unwrap()
}

#[test]
fn actor_active_status_transitions_once_and_cannot_refresh_from_another_causer() {
    let id = ConditionId::new("fixture_lock").unwrap();
    let mut subject = creature(vec![definition(id.as_str(), Some(400))]);
    assert_eq!(
        subject.apply_condition(&id, ActorId(7), 200),
        Ok(ConditionUpdate::Applied)
    );
    subject.advance_active(199);
    assert_eq!(record(&subject)["conditions"][0]["remaining"], 1);
    let before = subject.clone();
    for causer in [7, 8, 99] {
        subject.apply_condition(&id, ActorId(causer), 999).unwrap();
        assert_eq!(
            subject, before,
            "active applications neither refresh nor replace provenance"
        );
    }
    subject.advance_active(1);
    assert_eq!(record(&subject)["conditions"][0]["phase"], "protection");
    assert_eq!(record(&subject)["conditions"][0]["remaining"], 400);
    assert_eq!(record(&subject)["conditions"][0]["source"], 7);
    subject = restored(&subject);
    subject.advance_active(399);
    let before = subject.clone();
    for causer in [7, 8, 99] {
        subject.apply_condition(&id, ActorId(causer), 999).unwrap();
        assert_eq!(
            subject, before,
            "protection is target-wide and nonrefreshable"
        );
    }
    subject.advance_active(1);
    assert!(subject.conditions().is_empty());
    assert_eq!(
        subject.apply_condition(&id, ActorId(8), 200),
        Ok(ConditionUpdate::Applied)
    );
    assert_eq!(record(&subject)["conditions"][0]["source"], 8);
}

#[test]
fn bulk_active_time_matches_each_boundary_without_a_second_timer() {
    let id = ConditionId::new("fixture_lock").unwrap();
    let mut initial = creature(vec![definition(id.as_str(), Some(400))]);
    initial.apply_condition(&id, ActorId(7), 200).unwrap();
    for elapsed in [0, 1, 199, 200, 201, 599, 600, 601, 1_000_000] {
        let mut bulk = initial.clone();
        bulk.advance_active(elapsed);
        let mut split = initial.clone();
        split.advance_active(elapsed.min(200));
        split.advance_active(elapsed.saturating_sub(200).min(400));
        split.advance_active(elapsed.saturating_sub(600));
        assert_eq!(bulk, split, "elapsed {elapsed}");
    }
}

#[test]
fn fainting_style_status_expires_without_acquiring_ice_protection() {
    let id = ConditionId::new("fixture_stupor").unwrap();
    let mut subject = creature(vec![definition(id.as_str(), None)]);
    subject.apply_condition(&id, ActorId(7), 75).unwrap();
    subject.advance_active(74);
    assert_eq!(record(&subject)["conditions"][0]["phase"], "active");
    subject.advance_active(1);
    assert!(subject.conditions().is_empty());
    assert_eq!(
        subject.apply_condition(&id, ActorId(8), 75),
        Ok(ConditionUpdate::Applied)
    );
}

#[test]
fn timing_and_restriction_inputs_require_explicit_closed_named_semantics() {
    let valid = definition("fixture_lock", Some(400));
    let parsed: ConditionDef = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), valid);
    for (path, value) in [
        ("/timing", json!(null)),
        ("/timing", json!(["actor_active", {"kind":"remove"}])),
        (
            "/timing",
            json!({"kind":"wall_clock","on_expiry":{"kind":"remove"}}),
        ),
        (
            "/timing",
            json!({"kind":"actor_active","on_expiry":{"kind":"remove"},"extra":0}),
        ),
        ("/timing/on_expiry", json!({"kind":"remove","duration":400})),
        ("/timing/on_expiry", json!({"kind":"protect","duration":0})),
        (
            "/timing/on_expiry",
            json!({"kind":"protect","duration":1_000_001}),
        ),
        ("/restrictions", json!(["unknown"])),
        ("/preparation", json!("discard")),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<ConditionDef>(invalid).is_err(),
            "{path}"
        );
    }
    for field in ["timing", "restrictions", "preparation"] {
        let mut invalid = valid.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ConditionDef>(invalid).is_err(),
            "missing {field}"
        );
    }
    for invalid in [
        r#"{"kind":"actor_active","kind":"actor_active","on_expiry":{"kind":"remove"}}"#,
        r#"{"kind":"actor_active","on_expiry":{"kind":"protect","duration":400,"duration":401}}"#,
    ] {
        let raw = serde_json::to_string(&valid).unwrap();
        let timing = serde_json::to_string(&valid["timing"]).unwrap();
        assert!(serde_json::from_str::<ConditionDef>(&raw.replace(&timing, invalid)).is_err());
    }
}

#[test]
fn saved_phase_must_match_its_definition_and_cannot_hide_renewable_protection() {
    let id = ConditionId::new("fixture_lock").unwrap();
    let mut subject = creature(vec![definition(id.as_str(), Some(400))]);
    subject.apply_condition(&id, ActorId(7), 200).unwrap();
    subject.advance_active(200);
    for (field, value) in [
        ("phase", json!("unknown")),
        ("remaining", json!(401)),
        ("remaining", json!(0)),
        ("source", json!(0)),
        ("stacks", json!(2)),
        ("reserved_by", json!(7)),
    ] {
        let mut invalid = record(&subject);
        invalid["conditions"][0][field] = value;
        let parsed = serde_json::from_value::<CreatureRecord>(invalid);
        assert!(
            parsed.is_err()
                || parsed
                    .unwrap()
                    .restore(subject.build().content().clone(), vec![])
                    .is_err(),
            "{field}"
        );
    }
    let mut invalid = record(&subject);
    invalid["conditions"][0]
        .as_object_mut()
        .unwrap()
        .remove("phase");
    assert!(serde_json::from_value::<CreatureRecord>(invalid).is_err());
}

#[test]
fn ordinary_keep_modifiers_are_distinct_from_protected_action_denial() {
    let mut ordinary = definition("fixture_keep", None);
    ordinary["restrictions"] = json!([]);
    ordinary["preparation"] = json!("preserve");
    ordinary["modifiers"] =
        json!([{"scope":"always","op":{"kind":"add","stat":{"kind":"check_edge"},"value":1}}]);
    let mut subject = creature(vec![ordinary]);
    let id = ConditionId::new("fixture_keep").unwrap();
    subject.apply_condition(&id, ActorId(7), 20).unwrap();
    subject.advance_active(10);
    let before = subject.clone();
    subject.apply_condition(&id, ActorId(8), 100).unwrap();
    assert_eq!(subject, before);
    assert_eq!(record(&subject)["conditions"][0]["remaining"], 10);
}

#[test]
fn duplicate_operation_restrictions_are_rejected_by_semantic_validation() {
    let original = super::creature_fixture::ruleset();
    let mut document = serde_json::to_value(original.document()).unwrap();
    let mut invalid = definition("fixture_lock", Some(400));
    invalid["restrictions"] = json!(["ordinary_movement", "ordinary_movement"]);
    document["conditions"].as_array_mut().unwrap().push(invalid);
    let parsed: RulesetDocument = serde_json::from_value(document).unwrap();
    assert!(Ruleset::validate(parsed).is_err());
}

#[test]
fn active_to_protection_keeps_its_full_book_slot_and_cannot_be_capacity_evicted() {
    let mut ordinary = definition("fixture_ordinary", None);
    ordinary["restrictions"] = json!([]);
    ordinary["preparation"] = json!("preserve");
    ordinary["source_scoped"] = json!(true);
    ordinary["capacity"] = json!("oldest");
    let mut subject = creature(vec![definition("fixture_lock", Some(400)), ordinary]);
    let lock = ConditionId::new("fixture_lock").unwrap();
    let ordinary = ConditionId::new("fixture_ordinary").unwrap();
    subject.apply_condition(&lock, ActorId(7), 200).unwrap();
    for source in 1..=127 {
        subject
            .apply_condition(&ordinary, ActorId(source), 1000)
            .unwrap();
    }
    assert_eq!(subject.conditions().instances().count(), 128);
    for elapsed in [0, 200] {
        subject.advance_active(elapsed);
        subject
            .apply_condition(&ordinary, ActorId(128 + elapsed), 1000)
            .unwrap();
        assert_eq!(subject.conditions().instances().count(), 128);
        assert_eq!(
            subject
                .conditions()
                .instances()
                .filter(|entry| entry.condition == lock)
                .count(),
            1
        );
    }
    assert_eq!(
        record(&restored(&subject))["conditions"][0]["phase"],
        "protection"
    );
}

#[test]
fn repeatable_expiring_restrictions_keep_their_slot_under_capacity_pressure() {
    let lock = ConditionId::new("fixture_repeatable_lock").unwrap();
    let ordinary = ConditionId::new("fixture_ordinary").unwrap();
    let mut repeated = definition(lock.as_str(), None);
    repeated["stacking"] = json!({"kind":"extend"});
    let mut filler = definition(ordinary.as_str(), None);
    filler["restrictions"] = json!([]);
    filler["preparation"] = json!("preserve");
    filler["source_scoped"] = json!(true);
    filler["capacity"] = json!("oldest");
    let mut subject = creature(vec![repeated, filler]);
    subject.apply_condition(&lock, ActorId(7), 200).unwrap();
    for source in 1..=127 {
        subject
            .apply_condition(&ordinary, ActorId(source), 1000)
            .unwrap();
    }
    assert_eq!(subject.conditions().instances().count(), 128);
    subject.advance_active(50);
    assert_eq!(
        subject.apply_condition(&lock, ActorId(8), 100),
        Ok(ConditionUpdate::Refreshed),
        "a repeated restriction extends its existing slot even in a full book"
    );
    for source in [128, 129] {
        assert_eq!(
            subject.apply_condition(&ordinary, ActorId(source), 1000),
            Ok(ConditionUpdate::Evicted)
        );
        assert_eq!(subject.conditions().instances().count(), 128);
        let entry = subject
            .conditions()
            .instances()
            .find(|entry| entry.condition == lock)
            .expect("the oldest restriction survives ordinary capacity eviction");
        assert_eq!(
            (entry.remaining, entry.stacks, entry.source),
            (250, 1, ActorId(7))
        );
        assert!(subject
            .condition_restricts(tor_simulation::rules::ConditionRestriction::VoluntaryActions));
        let reloaded = restored(&subject);
        assert_eq!(reloaded, subject);
        subject = reloaded;
    }
    subject.advance_active(250);
    assert!(
        !subject.condition_restricts(tor_simulation::rules::ConditionRestriction::VoluntaryActions)
    );
    assert!(!subject
        .conditions()
        .instances()
        .any(|entry| entry.condition == lock));
    assert_eq!(
        subject.apply_condition(&lock, ActorId(9), 100),
        Ok(ConditionUpdate::Applied),
        "expiry leaves no follow-up protection"
    );
}

#[test]
fn a_full_protected_book_rejects_new_admission_without_changing_any_entry() {
    let mut definitions: Vec<_> = (0..128)
        .map(|index| definition(&format!("fixture_lock_{index}"), Some(400)))
        .collect();
    let mut incoming = definition("fixture_incoming", None);
    incoming["restrictions"] = json!([]);
    incoming["preparation"] = json!("preserve");
    incoming["capacity"] = json!("oldest");
    definitions.push(incoming);
    let mut subject = creature(definitions);
    for index in 0..128 {
        subject
            .apply_condition(
                &ConditionId::new(format!("fixture_lock_{index}")).unwrap(),
                ActorId(7),
                200,
            )
            .unwrap();
    }
    for elapsed in [0, 200] {
        subject.advance_active(elapsed);
        let before = subject.clone();
        assert_eq!(
            subject.apply_condition(
                &ConditionId::new("fixture_incoming").unwrap(),
                ActorId(8),
                20
            ),
            Err(tor_simulation::conditions::ConditionError::Capacity)
        );
        assert_eq!(subject, before);
        assert_eq!(restored(&subject), before);
    }
}

#[test]
fn protected_semantics_require_modifier_free_target_wide_nonrefreshing_definitions() {
    for (field, value) in [
        ("source_scoped", json!(true)),
        ("stacking", json!({"kind":"refresh"})),
        ("stacking", json!({"kind":"longest"})),
        ("stacking", json!({"kind":"stack","maximum":2})),
        ("lifecycle", json!("next_check")),
        (
            "modifiers",
            json!([{"scope":"always","op":{"kind":"add",
            "stat":{"kind":"check_edge"},"value":1}}]),
        ),
    ] {
        let mut document =
            serde_json::to_value(super::creature_fixture::ruleset().document()).unwrap();
        let mut definition = definition("fixture_lock", Some(400));
        definition[field] = value;
        document["conditions"]
            .as_array_mut()
            .unwrap()
            .push(definition);
        assert!(
            Ruleset::validate(serde_json::from_value(document).unwrap()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn gaining_and_losing_descriptor_immunity_ends_effect_once_without_erasing_protection() {
    use tor_simulation::{
        creatures::EquipmentSource,
        rules::{Modifier, ModifierOp, Scope, Selector},
    };
    let mut locked = definition("fixture_lock", Some(400));
    locked["descriptors"] = json!(["cold"]);
    let mut subject = creature(vec![locked]);
    let id = ConditionId::new("fixture_lock").unwrap();
    subject.apply_condition(&id, ActorId(7), 200).unwrap();
    subject.advance_active(50);
    let equipment = EquipmentSource {
        weapon_profile: None,
        item: tor_simulation::ItemId(1),
        socket: tor_simulation::EquipmentSlotId(0),
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: tor_simulation::content_ids::DescriptorId::new("cold").unwrap(),
                },
            },
        }],
        attack: None,
    };
    subject.set_equipment(vec![equipment.clone()]).unwrap();
    assert_eq!(record(&subject)["conditions"][0]["phase"], "protection");
    assert_eq!(record(&subject)["conditions"][0]["remaining"], 400);
    subject.advance_active(75);
    for equipment in [vec![], vec![equipment.clone()], vec![], vec![equipment]] {
        subject.set_equipment(equipment).unwrap();
        assert_eq!(record(&subject)["conditions"][0]["remaining"], 325);
        subject.apply_condition(&id, ActorId(8), 200).unwrap();
        assert_eq!(record(&subject)["conditions"][0]["remaining"], 325);
    }
}
