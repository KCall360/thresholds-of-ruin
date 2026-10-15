//! Actual periodic gains pay an explicit cost; direct restoration does not.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use tor_simulation::{
    costs::ResourceCost,
    creatures::{record::CreatureRecord, Content, CreatureBuild, CreatureState},
    nutrition::NutritionRequirement,
    resources::Resource,
    rules::{Modifier, ModifierOp, Ruleset, Scope, Stat},
    IntentionId,
};

fn policy(cost: Value) -> Value {
    json!({"kind":"enabled", "initial":40, "maximum":50,
        "hungry_below":20, "weak_below":10, "starve_at":-3, "unit_ticks":100,
        "hungry_recovery":{"kind":"slowed", "multiplier":2},
        "weak_recovery":{"kind":"stopped"}, "empty_recovery":{"kind":"stopped"},
        "regeneration_cost":cost})
}

fn cost(denominator: u32, health: u32, stamina: u32, focus: u32, mana: u32) -> Value {
    json!({"denominator":denominator, "health":health, "stamina":stamina,
        "focus":focus, "mana":mana})
}

fn creature(policy: Value) -> CreatureState {
    creature_with(policy, |_| {})
}

fn creature_with(policy: Value, edit: impl FnOnce(&mut Value)) -> CreatureState {
    let mut species = fixture::species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Flag {
            flag: tor_simulation::content_ids::FlagId::new("magical").unwrap(),
        },
    });
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::HealthRecoveryPeriod,
            value: 10,
        },
    });
    let original = fixture::build(species);
    let mut document = serde_json::to_value(original.ruleset().document()).unwrap();
    document["nutrition"] = policy;
    for resource in document["resources"].as_array_mut().unwrap() {
        resource["recovery_period"] = json!(10);
        resource["capacity"]["base"] = json!(10);
    }
    edit(&mut document);
    let rules = Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let content = Content::new(rules, original.content().catalog().clone()).unwrap();
    let build = CreatureBuild::new(
        content,
        original.species_id().clone(),
        original.ledger().clone(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    creature
        .configure_nutrition(NutritionRequirement::Required)
        .unwrap();
    for resource in Resource::ALL {
        assert_eq!(creature.costs().resources().maximum(resource), 10);
    }
    creature
}

fn spend(creature: &mut CreatureState, resource: Resource, amount: u32) {
    let id = IntentionId(resource as u64 + 1);
    creature
        .start_cost(
            id,
            ResourceCost {
                resource,
                start: amount,
                resolution: 0,
            },
        )
        .unwrap();
    creature.finish_cost(id).unwrap();
}

fn nutrition(creature: &CreatureState) -> Value {
    serde_json::to_value(creature.nutrition()).unwrap()
}

#[test]
fn periodic_actual_gains_charge_each_resource_without_capped_excess_or_direct_healing() {
    let mut subject = creature(policy(cost(1, 2, 3, 4, 5)));
    subject.damage(1);
    for resource in Resource::ALL {
        spend(&mut subject, resource, 1);
    }
    let outcome = subject.advance_active(10);
    assert_eq!(outcome.regenerated.health, 1);
    assert_eq!(outcome.regenerated.resources.stamina, 1);
    assert_eq!(outcome.regenerated.resources.focus, 1);
    assert_eq!(outcome.regenerated.resources.mana, 1);
    assert_eq!(outcome.nutrition_spent, 14);
    assert_eq!(outcome.active_ticks, 10);
    assert_eq!(outcome.nutrition_time_demand, 0);
    assert_eq!(outcome.nutrition_regeneration_demand, 14);
    assert_eq!(nutrition(&subject)["reserve"], 26);
    let saturated = subject.advance_active(89);
    assert_eq!(saturated.regenerated, Default::default());
    assert_eq!(saturated.nutrition_spent, 0);
    assert_eq!(saturated.nutrition_time_demand, 0);
    assert_eq!(saturated.nutrition_regeneration_demand, 0);
    assert_eq!(nutrition(&subject)["reserve"], 26);
    subject.advance_active(1);
    assert_eq!(nutrition(&subject)["reserve"], 25);
    subject.damage(2);
    for resource in Resource::ALL {
        spend(&mut subject, resource, 1);
        assert_eq!(subject.restore_resource(resource, u32::MAX), 1);
    }
    assert_eq!(subject.heal(u32::MAX), 2);
    subject.advance_active(10);
    assert_eq!(nutrition(&subject)["reserve"], 25);
}

#[test]
fn simultaneous_regeneration_band_crossing_and_expiry_rephase_once() {
    let mut authored = policy(cost(1, 0, 2, 0, 0));
    authored["initial"] = json!(20);
    authored["hungry_recovery"] = json!({"kind":"slowed", "multiplier":3});
    let mut subject = creature_with(authored, |document| {
        let mut condition = document["conditions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == "braced")
            .unwrap()
            .clone();
        condition["id"] = json!("fixture_regeneration_expiry");
        condition["family"] = json!("fixture_regeneration_expiry");
        condition["modifiers"] = json!([
            {"scope":"always", "op":{"kind":"set",
             "stat":{"kind":"health_recovery_period_percent"}, "value":30}},
            {"scope":"always", "op":{"kind":"add",
             "stat":{"kind":"resource_max", "resource":"stamina"}, "value":5}}]);
        document["conditions"]
            .as_array_mut()
            .unwrap()
            .push(condition);
    });
    subject
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("fixture_regeneration_expiry").unwrap(),
            tor_simulation::ActorId(1),
            10,
        )
        .unwrap();
    assert_eq!(subject.costs().resources().maximum(Resource::Stamina), 15);
    assert_eq!(subject.restore_resource(Resource::Stamina, 5), 5);
    subject.damage(7);
    spend(&mut subject, Resource::Stamina, 1);
    assert_eq!(subject.costs().resources().balance(Resource::Stamina), 14);
    let mut split = subject.clone();
    let whole = subject.advance_active(10);
    let mut gross_health = 0;
    let mut gross_nutrition = 0;
    for _ in 0..10 {
        let outcome = split.advance_active(1);
        gross_health += outcome.regenerated.health;
        gross_nutrition += outcome.nutrition_spent;
    }
    assert_eq!(subject, split);
    assert_eq!(whole.regenerated.health, 3);
    assert_eq!(
        whole.regenerated.resources.stamina, 1,
        "charge gross gain before the cap shrinks"
    );
    assert_eq!(subject.costs().resources().balance(Resource::Stamina), 10);
    assert_eq!(gross_health, whole.regenerated.health);
    assert_eq!(gross_nutrition, whole.nutrition_spent);
    assert_eq!(nutrition(&subject)["reserve"], 18);
    assert_eq!(subject.health_recovery().period(), 30);
    assert_eq!(
        subject.health_recovery().elapsed(),
        10,
        "floor(1 * 30 / 3), not floor(1 * 10 / 3) * 3"
    );
}

#[test]
fn restored_fraction_is_required_bounded_and_cleared_on_death() {
    let mut subject = creature(policy(cost(3, 1, 1, 1, 1)));
    subject.damage(1);
    subject.advance_active(10);
    let original = serde_json::to_value(CreatureRecord::capture(&subject)).unwrap();
    let rejects = |value| {
        !serde_json::from_value::<CreatureRecord>(value).is_ok_and(|record| {
            record
                .restore(subject.build().content().clone(), vec![])
                .is_ok()
        })
    };
    let mut missing = original.clone();
    missing["nutrition"]
        .as_object_mut()
        .unwrap()
        .remove("regeneration_remainder");
    assert!(rejects(missing));
    for fraction in [json!(3), json!(u64::MAX), json!(-1), json!([])] {
        let mut invalid = original.clone();
        invalid["nutrition"]["regeneration_remainder"] = fraction;
        assert!(rejects(invalid));
    }
    subject.kill();
    assert_eq!(nutrition(&subject)["regeneration_remainder"], 0);
    let mut invalid = serde_json::to_value(CreatureRecord::capture(&subject)).unwrap();
    invalid["nutrition"]["regeneration_remainder"] = json!(1);
    assert!(!serde_json::from_value::<CreatureRecord>(invalid)
        .unwrap()
        .restore(subject.build().content().clone(), vec![])
        .is_ok());
}

#[test]
fn cross_resource_fraction_survives_record_restore_and_is_never_charged_twice() {
    let mut subject = creature(policy(cost(3, 1, 1, 1, 1)));
    subject.damage(1);
    spend(&mut subject, Resource::Stamina, 1);
    subject.advance_active(10);
    assert_eq!(nutrition(&subject)["reserve"], 40);
    assert_eq!(nutrition(&subject)["regeneration_remainder"], 2);
    let record = serde_json::to_value(CreatureRecord::capture(&subject)).unwrap();
    let mut restored = serde_json::from_value::<CreatureRecord>(record)
        .unwrap()
        .restore(subject.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, subject);
    for actor in [&mut subject, &mut restored] {
        spend(actor, Resource::Focus, 1);
        actor.advance_active(10);
        assert_eq!(nutrition(actor)["reserve"], 39);
        assert_eq!(nutrition(actor)["regeneration_remainder"], 0);
    }
    assert_eq!(restored, subject);
}

#[test]
fn zero_cost_control_preserves_time_drain_and_still_regenerates() {
    let mut subject = creature(policy(cost(7, 0, 0, 0, 0)));
    subject.damage(2);
    spend(&mut subject, Resource::Mana, 2);
    subject.advance_active(100);
    assert_eq!(subject.health().injury(), 0);
    assert_eq!(nutrition(&subject)["reserve"], 39);
    assert_eq!(nutrition(&subject)["regeneration_remainder"], 0);
}

#[test]
fn exempt_actors_keep_periodic_recovery_without_nutrition_enrollment() {
    let enrolled = creature(policy(cost(1, 1, 1, 1, 1)));
    let mut exempt = CreatureState::new(enrolled.build().clone()).unwrap();
    exempt.damage(1);
    spend(&mut exempt, Resource::Mana, 1);
    let outcome = exempt.advance_active(10);
    assert_eq!(outcome.regenerated.health, 1);
    assert_eq!(outcome.regenerated.resources.mana, 1);
    assert_eq!(outcome.nutrition_spent, 0);
    assert_eq!(nutrition(&exempt), json!({"kind":"exempt"}));
}

#[test]
fn large_and_partitioned_settlement_agree_through_caps_bands_and_starvation() {
    let mut authored = policy(cost(3, 11, 13, 17, 19));
    authored["initial"] = json!(21);
    let mut whole = creature(authored);
    whole.damage(7);
    for resource in Resource::ALL {
        spend(&mut whole, resource, 3);
    }
    let mut partitioned = whole.clone();
    let whole_flow = whole.advance_active(u64::MAX);
    let mut parts = [0_u64; 8];
    let totals = |flow: &tor_simulation::creatures::ActiveTimeOutcome| {
        [
            flow.active_ticks,
            u64::try_from(flow.regenerated.health).unwrap(),
            u64::from(flow.regenerated.resources.stamina),
            u64::from(flow.regenerated.resources.focus),
            u64::from(flow.regenerated.resources.mana),
            u64::from(flow.nutrition_spent),
            flow.nutrition_time_demand,
            flow.nutrition_regeneration_demand,
        ]
    };
    for _ in 0..10_000 {
        let flow = partitioned.advance_active(1);
        for (sum, value) in parts.iter_mut().zip(totals(&flow)) {
            *sum += value;
        }
        if partitioned.health().dead() {
            break;
        }
    }
    assert!(whole.health().dead());
    assert_eq!(parts, totals(&whole_flow));
    assert_eq!(partitioned, whole);
    let dead = whole.clone();
    whole.advance_active(u64::MAX);
    assert_eq!(whole, dead);
}

#[test]
fn indexed_recovery_deadline_precedes_cost_driven_death_and_metrics_stop_there() {
    let mut authored = policy(cost(1, 0, 25, 0, 0));
    authored["initial"] = json!(21);
    let mut subject = creature(authored);
    spend(&mut subject, Resource::Stamina, 3);
    assert_eq!(subject.next_change_in(), Some(10));
    let outcome = subject.advance_active(u64::MAX);
    assert!(outcome.died);
    assert_eq!(outcome.regenerated.resources.stamina, 1);
    assert_eq!(outcome.nutrition_spent, 24);
    assert_eq!(outcome.active_ticks, 10);
    assert_eq!(outcome.nutrition_time_demand, 0);
    assert_eq!(outcome.nutrition_regeneration_demand, 25);
    assert_eq!(subject.next_change_in(), None);
    let dead = subject.advance_active(u64::MAX);
    assert_eq!(dead.regenerated, Default::default());
    assert_eq!(dead.nutrition_spent, 0);
    assert_eq!(dead.active_ticks, 0);
}

#[test]
fn explicit_regeneration_cost_rejects_missing_unknown_invalid_and_duplicate_fields() {
    let good = policy(cost(3, 1, 2, 3, 4));
    for field in ["denominator", "health", "stamina", "focus", "mana"] {
        let mut invalid = good.clone();
        invalid["regeneration_cost"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<tor_simulation::nutrition::NutritionPolicy>(invalid).is_err()
        );
    }
    for invalid_cost in [
        json!([]),
        json!({}),
        cost(0, 1, 1, 1, 1),
        cost(1_000_001, 0, 0, 0, 0),
        cost(1, 1_000_001, 0, 0, 0),
    ] {
        let mut invalid = good.clone();
        invalid["regeneration_cost"] = invalid_cost;
        assert!(
            !serde_json::from_value::<tor_simulation::nutrition::NutritionPolicy>(invalid)
                .is_ok_and(|policy| policy.valid())
        );
    }
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("regeneration_cost");
    assert!(serde_json::from_value::<tor_simulation::nutrition::NutritionPolicy>(missing).is_err());
    let mut extra = good.clone();
    extra["regeneration_cost"]["extra"] = json!(0);
    assert!(serde_json::from_value::<tor_simulation::nutrition::NutritionPolicy>(extra).is_err());
    let raw = serde_json::to_string(&good)
        .unwrap()
        .replace("\"health\":1", "\"health\":1,\"health\":1");
    assert!(serde_json::from_str::<tor_simulation::nutrition::NutritionPolicy>(&raw).is_err());
}
