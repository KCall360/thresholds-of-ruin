use std::collections::BTreeSet;
use tor_simulation::attributes::{Attributes, ManaBinding};
use tor_simulation::costs::{CostError, ResourceCost, StartCost};
use tor_simulation::creatures::{CreatureBuild, CreatureState, Species, Template};
use tor_simulation::dice::DicePool;
use tor_simulation::progression::{Class, CreatureType, HdLedger, HdSource};
use tor_simulation::resources::Resource;
use tor_simulation::{ActorId, AnatomySpec, IntentionId};

#[test]
fn cost_preparation_checks_are_readonly_and_reject_dead_actors() {
    let mut creature = CreatureState::new(build()).unwrap();
    let cost = ResourceCost {
        resource: Resource::Mana,
        start: 1,
        resolution: 1,
    };
    let before = creature.clone();
    assert_eq!(
        creature.validate_cost(IntentionId(1), cost),
        Ok(StartCost::Started)
    );
    assert_eq!(creature, before);
    creature.start_cost(IntentionId(1), cost).unwrap();
    let before = creature.clone();
    assert_eq!(
        creature.validate_cost(IntentionId(1), cost),
        Ok(StartCost::Resumed)
    );
    assert_eq!(creature, before);
    creature.kill();
    let before = creature.clone();
    assert_eq!(
        creature.validate_cost(IntentionId(1), cost),
        Err(CostError::Invalid)
    );
    assert_eq!(
        creature.start_cost(IntentionId(1), cost),
        Err(CostError::Invalid)
    );
    assert_eq!(creature, before);
}

#[test]
fn next_timer_change_combines_recovery_and_expiry_and_stops_at_death() {
    let mut creature = CreatureState::new(build()).unwrap();
    assert_eq!(creature.next_change_in(), None);
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
    creature.apply_fear(ActorId(9), 50).unwrap();
    assert_eq!(creature.next_change_in(), Some(50));
    creature.advance_active(49);
    assert_eq!(creature.next_change_in(), Some(1));
    creature.advance_active(1);
    assert_eq!(creature.next_change_in(), Some(50));
    creature.advance_active(50);
    assert_eq!(creature.next_change_in(), None);
    creature.apply_fear(ActorId(9), 50).unwrap();
    creature.kill();
    assert_eq!(creature.next_change_in(), None);
    assert!(!creature.needs_active_time());
}

pub(super) fn build() -> CreatureBuild {
    CreatureBuild::new(
        Species {
            id: "state_subject".into(),
            kind: CreatureType::Humanoid,
            subtypes: BTreeSet::new(),
            default_attributes: Attributes::new([2, 1, 2, 2, 1, 1]).unwrap(),
            anatomy: AnatomySpec::humanoid(),
            melee: tor_simulation::attacks::MeleeAttack::new(
                tor_simulation::attributes::Skill::HeavyWeaponry,
                0,
                60,
                40,
                {
                    let component = tor_simulation::damage::DamageComponent::rolled(
                        tor_simulation::combat::DamageType::Impact,
                        None,
                        DicePool::new(1, 6, 0).unwrap(),
                    );
                    let primary = component.key();
                    tor_simulation::damage::DamageSpec::new(vec![component], Some(primary)).unwrap()
                },
            )
            .unwrap(),
            grants: vec![],
        },
        HdLedger::seeded(
            vec![
                HdSource::Class(Class::Warrior),
                HdSource::Class(Class::Mage),
            ],
            42,
        )
        .unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap()
}

#[test]
fn rebuilding_preserves_injury_and_balances_and_clears_newly_immune_fear() {
    let mut creature = CreatureState::new(build()).unwrap();
    creature.damage(5);
    creature.apply_fear(ActorId(9), 300).unwrap();
    let maximum = creature.health().maximum();
    creature
        .start_cost(
            IntentionId(1),
            ResourceCost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 1,
            },
        )
        .unwrap();
    let balance = creature.costs().resources().balance(Resource::Stamina);
    let mut transformed = creature.build().clone();
    transformed
        .set_templates(vec![Template::zombified(0)])
        .unwrap();
    let outcome = creature.rebuild(transformed).unwrap();
    assert!(outcome.fear_cleared);
    assert!(!outcome.died);
    assert_eq!(creature.health().injury(), 5);
    assert_eq!(creature.health().maximum(), maximum + 2);
    assert_eq!(
        creature.costs().resources().balance(Resource::Stamina),
        balance
    );
    assert!(creature.fear().sources().is_empty());
    let mut restored = creature.build().clone();
    restored.set_templates(vec![]).unwrap();
    creature.rebuild(restored).unwrap();
    assert_eq!(creature.health().maximum(), maximum);
    assert_eq!(creature.health().current(), maximum - 5);
    assert!(creature.fear().sources().is_empty());
}

#[test]
fn reduced_resource_capacity_cancels_unfunded_holds_without_refunding_paid_costs() {
    let mut creature = CreatureState::new(build()).unwrap();
    let cost = ResourceCost {
        resource: Resource::Mana,
        start: 1,
        resolution: 3,
    };
    assert_eq!(
        creature.start_cost(IntentionId(7), cost),
        Ok(StartCost::Started)
    );
    let mut transformed = creature.build().clone();
    transformed
        .set_templates(vec![Template::zombified(0)])
        .unwrap();
    let outcome = creature.rebuild(transformed).unwrap();
    assert_eq!(outcome.canceled, vec![IntentionId(7)]);
    assert_eq!(creature.costs().resources().balance(Resource::Mana), 2);
    assert!(creature.costs().reservations().is_empty());
    let mut restored = creature.build().clone();
    restored.set_templates(vec![]).unwrap();
    creature.rebuild(restored).unwrap();
    assert_eq!(creature.costs().resources().balance(Resource::Mana), 2);
}

#[test]
fn zero_hd_death_cancels_holds_and_never_revives_or_recovers() {
    let original = build();
    let mut creature = CreatureState::new(original.clone()).unwrap();
    creature.apply_fear(ActorId(9), 300).unwrap();
    creature
        .start_cost(
            IntentionId(7),
            ResourceCost {
                resource: Resource::Focus,
                start: 1,
                resolution: 1,
            },
        )
        .unwrap();
    let mut drained = original.clone();
    while drained.remove_latest().is_some() {}
    let outcome = creature.rebuild(drained).unwrap();
    assert!(outcome.died);
    assert_eq!(outcome.canceled, vec![IntentionId(7)]);
    assert!(creature.fear().sources().is_empty());
    creature.rebuild(original).unwrap();
    assert!(creature.health().dead());
    assert_eq!(creature.heal(100), 0);
    let frozen = creature.clone();
    creature.advance_active(10_000);
    assert_eq!(creature, frozen);
    assert!(creature
        .start_cost(
            IntentionId(8),
            ResourceCost {
                resource: Resource::Focus,
                start: 1,
                resolution: 1
            }
        )
        .is_err());
}

#[test]
fn creature_checkpoint_preserves_paid_costs_recovery_fear_and_next_outcomes() {
    let mut original = CreatureState::new(build()).unwrap();
    original.damage(5);
    original.apply_fear(ActorId(9), 300).unwrap();
    let cost = ResourceCost {
        resource: Resource::Focus,
        start: 1,
        resolution: 1,
    };
    original.start_cost(IntentionId(7), cost).unwrap();
    original.advance_active(99);
    let data = serde_json::to_vec(&original).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&data).unwrap();
    assert!(value.get("derived").is_none());
    assert!(value.get("maximum_health").is_none());
    let mut restored: CreatureState = serde_json::from_slice(&data).unwrap();
    assert_eq!(restored, original);
    assert_eq!(
        restored.start_cost(IntentionId(7), cost),
        Ok(StartCost::Resumed)
    );
    for ticks in [201, 1, 299] {
        original.advance_active(ticks);
        restored.advance_active(ticks);
        assert_eq!(restored, original);
    }
    assert_eq!(
        restored.finish_cost(IntentionId(7)),
        original.finish_cost(IntentionId(7))
    );
    assert_eq!(restored.damage(u32::MAX), original.damage(u32::MAX));
    assert_eq!(restored, original);
}

#[test]
fn forged_cross_field_state_rejects_instead_of_clamping_or_repairing() {
    let value = serde_json::to_value(CreatureState::new(build()).unwrap()).unwrap();
    for (field, forged) in [
        ("injury", serde_json::json!(1_000_000)),
        ("balances", serde_json::json!([100, 100, 100])),
        ("recovery", serde_json::json!([1, 0, 0])),
        ("holds", serde_json::json!([[7, 1, 1, 100]])),
        ("fear", serde_json::json!([[9, 300], [9, 200]])),
    ] {
        let mut forged_value = value.clone();
        forged_value[field] = forged;
        assert!(
            serde_json::from_value::<CreatureState>(forged_value).is_err(),
            "{field}"
        );
    }
    let mut forged = value.clone();
    forged["dead"] = serde_json::json!(true);
    forged["fear"] = serde_json::json!([[9, 300]]);
    assert!(serde_json::from_value::<CreatureState>(forged).is_err());
    let mut forged = value;
    forged.as_object_mut().unwrap().remove("holds");
    assert!(serde_json::from_value::<CreatureState>(forged).is_err());
}
