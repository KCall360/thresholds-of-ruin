//! Actor-active cooldown state; action preparation wiring is a separate layer.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use tor_simulation::{
    content_ids::{AbilityId, ConditionId, TalentId},
    creatures::{record::CreatureRecord, Content, CreatureBuild, CreatureState, EquipmentSource},
    progression::HdSource,
    rules::{Modifier, ModifierOp, Ruleset, Scope, Stat},
    ActorId, EquipmentSlotId, ItemId,
};

fn subject() -> CreatureState {
    CreatureState::new(fixture::build(fixture::species())).unwrap()
}

fn recorded(creature: &CreatureState) -> Value {
    serde_json::to_value(CreatureRecord::capture(creature)).unwrap()
}

fn restore(value: Value, original: &CreatureState) -> Result<CreatureState, String> {
    serde_json::from_value::<CreatureRecord>(value)
        .map_err(|error| error.to_string())?
        .restore(original.build().content().clone(), vec![])
        .map_err(|error| format!("{error:?}"))
}

fn with_cooldown(original: &CreatureState, ability: &str, remaining: u64) -> CreatureState {
    let mut value = recorded(original);
    value["cooldowns"] = json!([{"ability": ability, "remaining": remaining}]);
    restore(value, original).unwrap()
}

#[test]
fn stored_cooldowns_drive_active_time_and_expire() {
    let original = subject();
    let mut value = recorded(&original);
    value["cooldowns"] = json!([
        {"ability": "magic_bolt", "remaining": 10},
        {"ability": "basic_melee", "remaining": 4}
    ]);
    let mut restored = restore(value, &original).expect("valid current cooldown rows");
    assert!(restored.needs_active_time());
    assert_eq!(restored.next_change_in(), Some(4));
    restored.advance_active(3);
    assert_eq!(restored.next_change_in(), Some(1));
    assert_eq!(
        recorded(&restored)["cooldowns"],
        json!([
            {"ability": "basic_melee", "remaining": 1},
            {"ability": "magic_bolt", "remaining": 7}
        ])
    );
    restored.advance_active(1);
    assert_eq!(restored.next_change_in(), Some(6));
    assert_eq!(
        recorded(&restored)["cooldowns"],
        json!([
            {"ability": "magic_bolt", "remaining": 6}
        ])
    );
    let mut reloaded = restore(recorded(&restored), &original).unwrap();
    assert_eq!(restored, reloaded);
    restored.advance_active(6);
    reloaded.advance_active(u64::MAX);
    assert_eq!(restored, reloaded);
    assert_eq!(recorded(&restored)["cooldowns"], json!([]));
    assert_eq!(restored.next_change_in(), None);
    assert!(!restored.needs_active_time());
}

#[test]
fn current_creature_records_require_explicit_cooldowns_even_when_empty() {
    let original = subject();
    let mut value = recorded(&original);
    assert_eq!(value.get("cooldowns"), Some(&json!([])));
    assert_eq!(restore(value.clone(), &original).unwrap(), original);
    value.as_object_mut().unwrap().remove("cooldowns");
    assert!(
        restore(value, &original).is_err(),
        "missing field defaulted"
    );
}

#[test]
fn stored_cooldowns_require_known_unique_positive_bounded_named_rows() {
    let original = subject();
    let mut valid = recorded(&original);
    valid["cooldowns"] = json!([{"ability": "magic_bolt", "remaining": 1}]);
    assert!(
        restore(valid.clone(), &original).is_ok(),
        "positive control"
    );
    for rows in [
        json!([{"ability": "missing", "remaining": 1}]),
        json!([{"ability": "bad id", "remaining": 1}]),
        json!([{"ability": "magic_bolt", "remaining": 0}]),
        json!([{"ability": "magic_bolt", "remaining": 1_000_001}]),
        json!([{"ability": "magic_bolt", "remaining": u64::MAX}]),
        json!([{"ability": "magic_bolt", "remaining": -1}]),
        json!([{"ability": "magic_bolt", "remaining": 1.5}]),
        json!([{"ability": "magic_bolt"}]),
        json!([{"remaining": 1}]),
        json!([{"ability": "magic_bolt", "remaining": 1, "elapsed": 0}]),
        json!([["magic_bolt", 1]]),
        json!([
            {"ability": "magic_bolt", "remaining": 1},
            {"ability": "magic_bolt", "remaining": 2}
        ]),
        json!({"magic_bolt": 1}),
        Value::Null,
    ] {
        let mut invalid = valid.clone();
        invalid["cooldowns"] = rows.clone();
        assert!(restore(invalid, &original).is_err(), "accepted {rows}");
    }
    let mut too_many = valid;
    too_many["cooldowns"] = Value::Array(vec![
        json!({"ability": "magic_bolt", "remaining": 1});
        tor_simulation::limits::CATALOG_ENTRIES + 1
    ]);
    let bytes = serde_json::to_vec(&too_many).unwrap();
    assert!(serde_json::from_slice::<CreatureRecord>(&bytes).is_err());
}

#[test]
fn duplicate_stored_cooldown_fields_reject_in_the_streaming_reader() {
    let original = subject();
    let mut value = recorded(&original);
    value["cooldowns"] = json!([{"ability": "magic_bolt", "remaining": 1}]);
    let encoded = serde_json::to_string(&value).unwrap();
    assert!(serde_json::from_str::<CreatureRecord>(&encoded).is_ok());
    for (from, to) in [
        ("\"remaining\":1", "\"remaining\":1,\"remaining\":2"),
        (
            "\"ability\":\"magic_bolt\"",
            "\"ability\":\"magic_bolt\",\"ability\":\"basic_melee\"",
        ),
        ("\"cooldowns\":[", "\"cooldowns\":[],\"cooldowns\":["),
    ] {
        let duplicate = encoded.replace(from, to);
        assert_ne!(duplicate, encoded, "exercise {from}");
        assert!(serde_json::from_str::<CreatureRecord>(&duplicate).is_err());
    }
}

#[test]
fn cooldowns_survive_permanent_drain_and_reacquisition_without_rephasing() {
    let mut build = fixture::build_with(
        fixture::species(),
        vec![],
        &[HdSource::Racial, HdSource::Racial],
    );
    let ability = AbilityId::new("magic_bolt").unwrap();
    let talent = TalentId::new("hardiness").unwrap();
    let mut document = build.ruleset().document().clone();
    document
        .talents
        .iter_mut()
        .find(|entry| entry.id == talent)
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ability.clone(),
            },
        });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    build = tor_simulation::creatures::record::BuildRecord::capture(&build)
        .restore(content)
        .unwrap();
    build.select_talent(1, talent).unwrap();
    let original = CreatureState::new(build.clone()).unwrap();
    assert!(original.derived().abilities.contains(&ability));
    let mut creature = with_cooldown(&original, "magic_bolt", 100);
    creature.damage(2);
    creature.advance_active(17);
    let expected = json!([{"ability": "magic_bolt", "remaining": 83}]);
    let mut drained = build.clone();
    drained.remove_latest().unwrap();
    creature.rebuild(drained).unwrap();
    assert!(!creature.derived().abilities.contains(&ability));
    assert_eq!(creature.health().injury(), 2);
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    creature = restore(recorded(&creature), &creature).unwrap();
    creature.rebuild(build).unwrap();
    assert!(creature.derived().abilities.contains(&ability));
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    creature.advance_active(82);
    assert_eq!(creature.next_change_in(), Some(1));
    creature.advance_active(1);
    assert_eq!(recorded(&creature)["cooldowns"], json!([]));
}

#[test]
fn cooldowns_survive_temporary_and_equipment_grant_changes() {
    let base = subject();
    let ability = AbilityId::new("magic_bolt").unwrap();
    let condition = ConditionId::new("fear").unwrap();
    let grant = Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: ability.clone(),
        },
    };
    let mut document = base.build().ruleset().document().clone();
    document
        .conditions
        .iter_mut()
        .find(|entry| entry.id == condition)
        .unwrap()
        .modifiers
        .push(grant.clone());
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        base.build().content().catalog().clone(),
    )
    .unwrap();
    let build = tor_simulation::creatures::record::BuildRecord::capture(base.build())
        .restore(content)
        .unwrap();
    let mut creature = with_cooldown(&CreatureState::new(build).unwrap(), "magic_bolt", 100);
    assert!(!creature.derived().abilities.contains(&ability));
    creature
        .apply_condition(&condition, ActorId(1), 10)
        .unwrap();
    assert!(creature.derived().abilities.contains(&ability));
    creature.advance_active(10);
    assert!(!creature.derived().abilities.contains(&ability));
    let expected = json!([{"ability": "magic_bolt", "remaining": 90}]);
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    creature
        .set_equipment(vec![EquipmentSource {
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            modifiers: vec![
                grant,
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::HealthRecoveryPeriod,
                        value: -100,
                    },
                },
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Attribute {
                            attribute: tor_simulation::attributes::Attribute::Speed,
                        },
                        value: 10,
                    },
                },
            ],
            attack: None,
        }])
        .unwrap();
    assert!(creature.derived().abilities.contains(&ability));
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    creature.set_equipment(vec![]).unwrap();
    assert!(!creature.derived().abilities.contains(&ability));
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    creature
        .apply_condition(&condition, ActorId(1), 10)
        .unwrap();
    assert_eq!(recorded(&creature)["cooldowns"], expected);
}

#[test]
fn cooldown_expiry_coexists_with_condition_resource_and_health_recovery() {
    use tor_simulation::{costs::ResourceCost, resources::Resource, IntentionId};
    let mut creature = with_cooldown(&subject(), "magic_bolt", 7);
    creature.damage(3);
    creature
        .start_cost(
            IntentionId(1),
            ResourceCost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 0,
            },
        )
        .unwrap();
    creature.finish_cost(IntentionId(1)).unwrap();
    creature
        .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(1), 4)
        .unwrap();
    assert_eq!(creature.next_change_in(), Some(4));
    creature.advance_active(4);
    assert!(creature.conditions().is_empty());
    assert_eq!(creature.next_change_in(), Some(3));
    assert_eq!(creature.health_recovery().elapsed(), 4);
    let mut restored = restore(recorded(&creature), &creature).unwrap();
    for state in [&mut creature, &mut restored] {
        state.advance_active(3);
        assert_eq!(recorded(state)["cooldowns"], json!([]));
        assert_eq!(state.next_change_in(), Some(93));
        state.advance_active(93);
        assert_eq!(state.next_change_in(), Some(1_100));
        state.advance_active(1_100);
        assert_eq!(state.health().injury(), 2);
        assert_eq!(state.health_recovery().elapsed(), 0);
    }
    assert_eq!(creature, restored);
}

#[test]
fn death_freezes_retained_cooldowns_through_rebuild_and_restore() {
    let mut creature = with_cooldown(&subject(), "magic_bolt", 100);
    creature.advance_active(17);
    creature.kill();
    let expected = json!([{"ability": "magic_bolt", "remaining": 83}]);
    creature.advance_active(u64::MAX);
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    assert_eq!(creature.next_change_in(), None);
    assert!(!creature.needs_active_time());
    let mut larger: CreatureBuild = creature.build().clone();
    larger.add_hit_die(HdSource::Racial, 42).unwrap();
    creature.rebuild(larger).unwrap();
    assert!(creature.health().dead());
    assert_eq!(recorded(&creature)["cooldowns"], expected);
    assert_eq!(restore(recorded(&creature), &creature).unwrap(), creature);
}

#[test]
fn stopped_health_recovery_does_not_stop_cooldowns() {
    let mut creature = with_cooldown(&subject(), "magic_bolt", 100);
    creature.damage(2);
    creature
        .set_recovery_nutrition(tor_simulation::health_recovery::RecoveryNutrition::Stopped)
        .unwrap();
    assert_eq!(creature.next_change_in(), Some(100));
    creature.advance_active(99);
    assert_eq!(creature.next_change_in(), Some(1));
    assert_eq!(creature.health().injury(), 2);
    assert_eq!(creature.health_recovery().elapsed(), 0);
    creature.advance_active(1);
    assert_eq!(creature.next_change_in(), None);
    assert_eq!(recorded(&creature)["cooldowns"], json!([]));
}

#[test]
fn lethal_condition_expiry_stops_cooldown_advancement_at_that_boundary() {
    let base = subject();
    let condition = ConditionId::new("fear").unwrap();
    let mut document = base.build().ruleset().document().clone();
    document
        .conditions
        .iter_mut()
        .find(|entry| entry.id == condition)
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 100,
            },
        });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        base.build().content().catalog().clone(),
    )
    .unwrap();
    let build = tor_simulation::creatures::record::BuildRecord::capture(base.build())
        .restore(content)
        .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    let baseline = creature.health().current();
    creature
        .start_cooldown(&AbilityId::new("magic_bolt").unwrap(), 100)
        .unwrap();
    creature
        .apply_condition(&condition, ActorId(1), 10)
        .unwrap();
    creature.damage(baseline + 5);
    assert!(!creature.health().dead());
    assert!(creature.advance_active(1_000).died);
    assert!(creature.health().dead());
    assert_eq!(
        recorded(&creature)["cooldowns"],
        json!([{"ability": "magic_bolt", "remaining": 90}])
    );
    assert_eq!(creature.next_change_in(), None);
    assert_eq!(restore(recorded(&creature), &creature).unwrap(), creature);
}

fn catalog_subject(ability_count: usize) -> CreatureState {
    let base = subject();
    let mut document = base.build().ruleset().document().clone();
    let template = document
        .abilities
        .iter()
        .find(|entry| entry.id.as_str() == "magic_bolt")
        .unwrap()
        .clone();
    for index in document.abilities.len()..ability_count {
        let mut ability = template.clone();
        ability.id = AbilityId::new(format!("cooldown_fixture_{index}")).unwrap();
        document.abilities.push(ability);
    }
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        base.build().content().catalog().clone(),
    )
    .unwrap();
    let build = tor_simulation::creatures::record::BuildRecord::capture(base.build())
        .restore(content)
        .unwrap();
    CreatureState::new(build).unwrap()
}

#[test]
fn catalog_bound_cooldowns_prune_without_tick_by_tick_work() {
    let original = catalog_subject(tor_simulation::limits::CATALOG_ENTRIES);
    let rows: Vec<_> = original.build().ruleset().document().abilities.iter()
        .enumerate().map(|(index, ability)| {
            json!({"ability": ability.id, "remaining": if index % 2 == 0 { 1 } else { 1_000_000 }})
        }).collect();
    let mut value = recorded(&original);
    value["cooldowns"] = json!(rows);
    let mut creature = restore(value, &original).unwrap();
    assert_eq!(
        recorded(&creature)["cooldowns"].as_array().unwrap().len(),
        4096
    );
    creature.advance_active(1);
    assert_eq!(
        recorded(&creature)["cooldowns"].as_array().unwrap().len(),
        2048
    );
    assert_eq!(creature.next_change_in(), Some(999_999));
    creature.advance_active(u64::MAX);
    assert_eq!(recorded(&creature)["cooldowns"], json!([]));
    assert!(!creature.needs_active_time());
}

#[test]
fn rebuild_rejects_a_catalog_missing_retained_cooldown_ids_atomically() {
    let base = subject();
    let expanded = catalog_subject(base.build().ruleset().document().abilities.len() + 1);
    let extra = expanded
        .build()
        .ruleset()
        .document()
        .abilities
        .last()
        .unwrap()
        .id
        .as_str();
    let mut creature = with_cooldown(&expanded, extra, 100);
    creature.damage(2);
    creature.advance_active(17);
    let before = creature.clone();
    assert!(creature.rebuild(base.build().clone()).is_err());
    assert_eq!(creature, before);
}

#[test]
fn starting_a_cooldown_validates_without_renewing_or_changing_other_state() {
    use tor_simulation::cooldowns::CooldownError;
    let ability = AbilityId::new("magic_bolt").unwrap();
    let unknown = AbilityId::new("unknown_ability").unwrap();
    let mut creature = subject();
    assert!(!creature.derived().abilities.contains(&ability));
    assert!(creature.cooldowns().is_ready(&ability));
    assert_eq!(creature.cooldowns().remaining(&ability), 0);
    let before = creature.clone();
    assert_eq!(creature.validate_cooldown(&ability, 100), Ok(()));
    assert_eq!(creature, before, "read-only gate");
    for (id, duration, error) in [
        (&unknown, 100, CooldownError::UnknownAbility),
        (&ability, 0, CooldownError::InvalidDuration),
        (&ability, 1_000_001, CooldownError::InvalidDuration),
        (&ability, u64::MAX, CooldownError::InvalidDuration),
    ] {
        assert_eq!(creature.validate_cooldown(id, duration), Err(error));
        assert_eq!(creature.start_cooldown(id, duration), Err(error));
        assert_eq!(creature, before, "failed start must be atomic");
    }
    creature.start_cooldown(&ability, 100).unwrap();
    creature.advance_active(17);
    let before = creature.clone();
    assert_eq!(creature.cooldowns().remaining(&ability), 83);
    assert!(!creature.cooldowns().is_ready(&ability));
    for duration in [1, 100, 1_000_000] {
        assert_eq!(
            creature.validate_cooldown(&ability, duration),
            Err(CooldownError::AlreadyCoolingDown)
        );
        assert_eq!(
            creature.start_cooldown(&ability, duration),
            Err(CooldownError::AlreadyCoolingDown)
        );
        assert_eq!(creature, before);
    }
    creature.advance_active(83);
    assert!(creature.cooldowns().is_ready(&ability));
    creature.start_cooldown(&ability, 1_000_000).unwrap();
    creature.kill();
    let before = creature.clone();
    assert_eq!(
        creature.validate_cooldown(&ability, 1),
        Err(CooldownError::Dead)
    );
    assert_eq!(
        creature.start_cooldown(&ability, 1),
        Err(CooldownError::Dead)
    );
    assert_eq!(creature, before);
}

#[test]
fn canceling_a_resource_hold_does_not_reset_started_cooldowns() {
    use tor_simulation::{costs::ResourceCost, resources::Resource, IntentionId};
    let ability = AbilityId::new("magic_bolt").unwrap();
    let mut creature = subject();
    creature.start_cooldown(&ability, 100).unwrap();
    let cost = ResourceCost {
        resource: Resource::Stamina,
        start: 1,
        resolution: 1,
    };
    creature.start_cost(IntentionId(1), cost).unwrap();
    creature.advance_active(3);
    assert_eq!(creature.cancel_cost(IntentionId(1)), Some(cost));
    assert_eq!(creature.cooldowns().remaining(&ability), 97);
    assert_eq!(restore(recorded(&creature), &creature).unwrap(), creature);
}

#[test]
fn cooldown_work_counts_expose_bounded_linear_advancement_and_copy_on_write() {
    use tor_simulation::diagnostics::work_counts;
    let original = catalog_subject(tor_simulation::limits::CATALOG_ENTRIES);
    for count in [0, 1, 128, tor_simulation::limits::CATALOG_ENTRIES] {
        let mut value = recorded(&original);
        value["cooldowns"] = json!(original
            .build()
            .ruleset()
            .document()
            .abilities
            .iter()
            .take(count)
            .map(|ability| json!({"ability": ability.id, "remaining": 1_000_000}))
            .collect::<Vec<_>>());
        let mut creature = restore(value, &original).unwrap();
        let before = work_counts();
        let snapshot = creature.clone();
        creature.advance_active(0);
        let after = work_counts();
        assert_eq!(
            after.cooldown_entries_advanced - before.cooldown_entries_advanced,
            0
        );
        assert_eq!(
            after.cooldown_entries_copied - before.cooldown_entries_copied,
            0
        );
        let before = work_counts();
        creature.advance_active(1);
        let after = work_counts();
        assert_eq!(
            after.cooldown_entries_advanced - before.cooldown_entries_advanced,
            count
        );
        assert_eq!(
            after.cooldown_entries_copied - before.cooldown_entries_copied,
            count
        );
        let before = work_counts();
        creature.advance_active(1);
        let after = work_counts();
        assert_eq!(
            after.cooldown_entries_advanced - before.cooldown_entries_advanced,
            count
        );
        assert_eq!(
            after.cooldown_entries_copied - before.cooldown_entries_copied,
            0
        );
        let before = work_counts();
        assert_eq!(creature.next_change_in(), (count > 0).then_some(999_998));
        let after = work_counts();
        assert_eq!(
            after.cooldown_deadline_candidates - before.cooldown_deadline_candidates,
            count
        );
        for ability in original
            .build()
            .ruleset()
            .document()
            .abilities
            .iter()
            .take(count)
        {
            assert_eq!(snapshot.cooldowns().remaining(&ability.id), 1_000_000);
            assert_eq!(creature.cooldowns().remaining(&ability.id), 999_998);
        }
        let before = work_counts();
        creature.advance_active(u64::MAX);
        let after = work_counts();
        assert_eq!(
            after.cooldown_entries_advanced - before.cooldown_entries_advanced,
            count
        );
        assert_eq!(
            after.cooldown_entries_copied - before.cooldown_entries_copied,
            0
        );
        assert_eq!(recorded(&creature)["cooldowns"], json!([]));
    }
}
