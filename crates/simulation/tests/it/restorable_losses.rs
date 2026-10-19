use super::{creature_fixture, creature_state};
use tor_simulation::attributes::Attribute;
use tor_simulation::content_ids::{ConditionId, TemplateId};
use tor_simulation::creatures::record::CreatureRecord;
use tor_simulation::creatures::{CreatureState, EquipmentSource, Template};
use tor_simulation::rules::{Modifier, ModifierOp, Scope, Stat};
use tor_simulation::{ActorId, EquipmentSlotId, ItemId};

fn strength_set(value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::Attribute {
                attribute: Attribute::Strength,
            },
            value,
        },
    }
}

#[test]
fn losses_follow_ordinary_sets_and_leave_permanent_eligibility_unchanged() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    creature
        .set_equipment(vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(42),
            socket: EquipmentSlotId(0),
            modifiers: vec![strength_set(8)],
            attack: None,
        }])
        .unwrap();
    let epoch = creature.build_epoch();
    let original_build = creature.build().clone();
    assert_eq!(
        creature
            .lose_attribute(Attribute::Strength, 3)
            .unwrap()
            .applied,
        3
    );
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 5);
    assert_eq!(
        *creature
            .restorable_losses()
            .attributes
            .get(Attribute::Strength),
        3
    );
    assert_eq!(creature.build_epoch(), epoch);
    assert_eq!(creature.build(), &original_build);
    creature.rebuild(creature.build().clone()).unwrap();
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 5);
}

#[test]
fn retained_deficits_survive_source_changes_and_partial_restoration_without_stat_gain() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    creature
        .set_equipment(vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(42),
            socket: EquipmentSlotId(0),
            modifiers: vec![strength_set(8)],
            attack: None,
        }])
        .unwrap();
    creature.lose_attribute(Attribute::Strength, 7).unwrap();
    creature.set_equipment(vec![]).unwrap();
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 0);
    assert_eq!(
        creature
            .restore_attribute(Attribute::Strength, 2)
            .unwrap()
            .applied,
        2
    );
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 0);
    assert_eq!(creature.restorable_losses().attributes.strength, 5);
    assert_eq!(
        creature
            .restore_attribute(Attribute::Strength, 100)
            .unwrap()
            .applied,
        5
    );
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 2);
}

#[test]
fn losses_do_not_rescue_invalid_undrained_attribute_or_resource_totals() {
    for stat in [
        Stat::Attribute {
            attribute: Attribute::Strength,
        },
        Stat::ResourceMax {
            resource: tor_simulation::resources::Resource::Stamina,
        },
    ] {
        let mut creature = CreatureState::new(creature_state::build()).unwrap();
        let undrained = match stat {
            Stat::Attribute { attribute } => {
                u32::from(creature.derived().attributes.get(attribute))
            }
            Stat::ResourceMax { resource } => creature.costs().resources().maximum(resource),
            _ => unreachable!(),
        };
        creature.lose_attribute(Attribute::Strength, 2).unwrap();
        creature.lose_maximum_health(1).unwrap();
        let before = creature.clone();
        assert!(creature
            .set_equipment(vec![EquipmentSource {
                weapon_profile: None,
                item: ItemId(42),
                socket: EquipmentSlotId(0),
                modifiers: vec![creature_fixture::add(
                    stat,
                    1_000_001 - i64::from(undrained)
                )],
                attack: None,
            }])
            .is_err());
        assert_eq!(creature, before);
    }
}

#[test]
fn strength_and_maximum_health_loss_preserve_injury_and_can_kill_without_damage_activity() {
    for strength in [true, false] {
        let mut creature = CreatureState::new(creature_state::build()).unwrap();
        creature.damage(u32::try_from(creature.health().maximum() - 1).unwrap());
        let injury = creature.health().injury();
        let quiet = creature.received_health_loss_remaining();
        let outcome = if strength {
            creature.lose_attribute(Attribute::Strength, 1)
        } else {
            creature.lose_maximum_health(1)
        }
        .unwrap();
        assert_eq!(outcome.applied, 1);
        assert!(outcome.rebuild.died);
        assert!(creature.health().dead());
        assert_eq!(creature.health().injury(), injury);
        assert_eq!(creature.received_health_loss_remaining(), quiet);
        let dead = creature.clone();
        assert_eq!(
            creature
                .restore_attribute(Attribute::Strength, 100)
                .unwrap()
                .applied,
            0
        );
        assert_eq!(creature.restore_maximum_health(100).unwrap().applied, 0);
        assert_eq!(creature, dead);
    }
}

#[test]
fn zero_attributes_have_no_invented_instant_death_and_loss_is_explicitly_capped() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    assert_eq!(
        creature
            .lose_attribute(Attribute::Strength, 1_000_000)
            .unwrap()
            .applied,
        2
    );
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 0);
    assert!(!creature.health().dead());
    assert_eq!(
        creature
            .lose_attribute(Attribute::Strength, 1)
            .unwrap()
            .applied,
        0
    );
    let maximum = creature.health().maximum();
    assert_eq!(
        u64::from(creature.lose_maximum_health(1_000_000).unwrap().applied),
        maximum
    );
    assert!(creature.health().dead());
}

#[test]
fn ordinary_healing_waiting_conditions_and_respec_never_clear_losses() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    creature.lose_attribute(Attribute::Strength, 1).unwrap();
    creature.lose_maximum_health(2).unwrap();
    let losses = creature.restorable_losses();
    creature.damage(1);
    creature.heal(100);
    creature
        .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(9), 20)
        .unwrap();
    creature.advance_active(100);
    let mut template = Template::new(TemplateId::new("after_loss").unwrap(), 0);
    template
        .modifiers
        .push(creature_fixture::add(Stat::Health, 3));
    let replacement = creature_state::with_template(creature.build(), template);
    creature.rebuild(replacement).unwrap();
    assert_eq!(creature.restorable_losses(), losses);
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 1);
}

#[test]
fn restoration_preserves_injury_and_clears_only_the_actual_bounded_deficit() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    creature.damage(1);
    let maximum = creature.health().maximum();
    creature.lose_maximum_health(3).unwrap();
    assert_eq!(creature.restore_maximum_health(2).unwrap().applied, 2);
    assert_eq!(creature.health().maximum(), maximum - 1);
    assert_eq!(creature.health().injury(), 1);
    assert_eq!(
        creature.restore_maximum_health(1_000_000).unwrap().applied,
        1
    );
    assert_eq!(creature.restore_maximum_health(1).unwrap().applied, 0);
}

#[test]
fn current_records_require_complete_valid_named_losses_and_restore_before_health() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    creature.lose_attribute(Attribute::Strength, 1).unwrap();
    creature.lose_maximum_health(2).unwrap();
    creature.damage(1);
    let record = CreatureRecord::capture(&creature);
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["restorable_losses"]["attributes"]["strength"], 1);
    assert_eq!(
        record
            .restore(creature.build().content().clone(), vec![])
            .unwrap(),
        creature
    );
    for path in [
        "restorable_losses",
        "attributes",
        "strength",
        "maximum_health",
    ] {
        let mut invalid = value.clone();
        match path {
            "restorable_losses" => {
                invalid.as_object_mut().unwrap().remove(path);
            }
            "strength" => {
                invalid["restorable_losses"]["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove(path);
            }
            _ => {
                invalid["restorable_losses"]
                    .as_object_mut()
                    .unwrap()
                    .remove(path);
            }
        }
        assert!(
            serde_json::from_value::<CreatureRecord>(invalid).is_err(),
            "missing {path}"
        );
    }
    for (field, amount) in [("maximum_health", 1_000_001), ("strength", 1001)] {
        let mut invalid = value.clone();
        if field == "strength" {
            invalid["restorable_losses"]["attributes"][field] = amount.into();
        } else {
            invalid["restorable_losses"][field] = amount.into();
        }
        assert!(serde_json::from_value::<CreatureRecord>(invalid).is_err());
    }
}

#[test]
fn a_reduced_source_record_retains_a_deficit_larger_than_its_current_capacity() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    creature
        .set_equipment(vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(42),
            socket: EquipmentSlotId(0),
            modifiers: vec![strength_set(8)],
            attack: None,
        }])
        .unwrap();
    creature.lose_attribute(Attribute::Strength, 7).unwrap();
    creature.set_equipment(vec![]).unwrap();
    let record = CreatureRecord::capture(&creature);
    let mut restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
    restored.restore_attribute(Attribute::Strength, 2).unwrap();
    assert_eq!(restored.restorable_losses().attributes.strength, 5);
    assert_eq!(restored.derived().attributes.get(Attribute::Strength), 0);
}

#[test]
fn invalid_loss_operands_reject_atomically_and_strict_records_reject_extra_or_positional_data() {
    let mut creature = CreatureState::new(creature_state::build()).unwrap();
    let before = creature.clone();
    assert!(creature
        .lose_attribute(Attribute::Strength, 1_000_001)
        .is_err());
    assert!(creature
        .restore_attribute(Attribute::Strength, 1_000_001)
        .is_err());
    assert!(creature.lose_maximum_health(1_000_001).is_err());
    assert!(creature.restore_maximum_health(1_000_001).is_err());
    assert_eq!(creature, before);
    let value = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    for losses in [
        serde_json::json!([0, 0]),
        serde_json::json!({
            "attributes": {"strength":0, "speed":0, "intellect":0, "willpower":0, "awareness":0, "presence":0},
            "maximum_health": 0, "source": 7,
        }),
    ] {
        let mut invalid = value.clone();
        invalid["restorable_losses"] = losses;
        assert!(serde_json::from_value::<CreatureRecord>(invalid).is_err());
    }
}

#[test]
fn mutable_strength_deficit_does_not_disable_an_owned_permanent_attribute_gate() {
    let mut talent = creature_fixture::talent("fixture_strength_gate", vec![]);
    talent.requirement = tor_simulation::rules::Requirement::AttributeMinimum {
        attribute: Attribute::Strength,
        minimum: 2,
    };
    let talent_id = talent.id.clone();
    let mut build = creature_fixture::with_talents(creature_state::build(), [talent]);
    build
        .select_talent(tor_simulation::HdIndex(0), talent_id.clone())
        .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    assert!(creature.derived().active_talents.contains(&talent_id));
    creature.lose_attribute(Attribute::Strength, 2).unwrap();
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 0);
    assert!(creature.derived().active_talents.contains(&talent_id));
    assert!(!creature.derived().dormant_talents.contains(&talent_id));
}
