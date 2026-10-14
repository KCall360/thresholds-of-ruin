use tor_protocol::*;
pub(super) fn content(id: &str) -> ContentRef {
    ContentRef {
        id: id.into(),
        name: format!("Display {id}"),
    }
}
pub(super) fn stats() -> OwnStats {
    OwnStats {
        kind: content("fixture_type"),
        subtypes: vec![content("fixture_subtype")],
        hit_dice: vec![content("fixture_class")],
        attributes: AttributeView {
            strength: 1,
            speed: 2,
            intellect: 3,
            willpower: 2,
            awareness: 1,
            presence: 0,
        },
        skills: vec![SkillView {
            skill: content("fixture_skill"),
            rank: 1,
        }],
        defenses: DefenseView {
            physical: 13,
            cognitive: 15,
            spiritual: 11,
        },
        binding: content("intellect"),
        health_recovery: tor_protocol::HealthRecoveryView {
            period_ticks: 1200,
            elapsed_ticks: 0,
            stopped: false,
            reset_percent: 100,
        },
        resources: vec![ResourceView {
            resource: content("fixture_resource"),
            balance: 4,
            maximum: 5,
            available: 3,
            reserved: 1,
        }],
        active_talents: vec![content("fixture_talent")],
        dormant_talents: vec![],
        abilities: vec![AbilityView {
            id: "fixture_ability".into(),
            name: "Novel ability".into(),
            cost: Some(AbilityCost {
                resource: content("fixture_resource"),
                start: 2,
                resolution: 1,
            }),
            reach: AbilityReach::Sight { range: 7 },
            target_kind: AbilityTargetKind::Actor,
            basic_attack: false,
            cooldown_remaining: 0,
            supply_costs: vec![],
        }],
    }
}
#[test]
fn undiscovered_content_ids_and_names_round_trip_without_client_catalogs() {
    let stats = stats();
    let value = serde_json::to_value(&stats).unwrap();
    assert_eq!(
        serde_json::from_value::<OwnStats>(value.clone()).unwrap(),
        stats
    );
    for key in ["health_seed", "reservation_owner", "sources", "actor"] {
        let mut leaked = value.clone();
        leaked[key] = 7.into();
        assert!(serde_json::from_value::<OwnStats>(leaked).is_err());
    }
    let action = Action::UseAbility {
        id: stats.abilities[0].id.clone(),
        invocation: tor_protocol::AbilityInvocation::ActorTarget {
            target: ActorTarget::from_digest([1; 32]),
        },
        supplies: Default::default(),
    };
    assert_eq!(
        serde_json::from_value::<Action>(serde_json::to_value(&action).unwrap()).unwrap(),
        action
    );
}
#[test]
fn ability_disclosures_validate_ids_labels_cost_bounds_and_target_kinds() {
    let original = stats().abilities.pop().unwrap();
    assert!(original.valid());
    for case in 0..6 {
        let mut value = original.clone();
        match case {
            0 => value.id = "not an id".into(),
            1 => value.name = "hidden\ncontrol".into(),
            2 => value.cost.as_mut().unwrap().start = u32::MAX,
            3 => value.reach = AbilityReach::Sight { range: 0 },
            4 => value.target_kind = AbilityTargetKind::SelfOnly,
            5 => value.id = "x".repeat(61),
            _ => unreachable!(),
        }
        assert!(!value.valid(), "case {case}");
    }
    let mut self_only = original;
    self_only.reach = AbilityReach::SelfOnly;
    self_only.target_kind = AbilityTargetKind::SelfOnly;
    assert!(self_only.valid());
    self_only.basic_attack = true;
    assert!(!self_only.valid());
}

#[test]
fn phase5_self_or_ally_disclosure_uses_typed_bounded_range() {
    let mut value = serde_json::to_value(stats().abilities.pop().unwrap()).unwrap();
    value["reach"] = serde_json::json!({"kind":"self_or_ally","range":{"kind":"touch"}});
    value["target_kind"] = "self_or_ally".into();
    let decoded: AbilityView = serde_json::from_value(value.clone()).unwrap();
    assert!(decoded.valid());
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
    for range in [0, 17] {
        value["reach"]["range"] = serde_json::json!({"kind":"sight","range":range});
        assert!(!serde_json::from_value::<AbilityView>(value.clone())
            .unwrap()
            .valid());
    }
    for range in [
        serde_json::json!({"kind":"touch","range":1}),
        serde_json::json!({"kind":"sight"}),
        serde_json::json!({"kind":"melee"}),
    ] {
        value["reach"]["range"] = range;
        assert!(
            serde_json::from_value::<AbilityView>(value.clone()).is_err(),
            "malformed range accepted: {}",
            value["reach"]["range"]
        );
    }
    value["reach"]["range"] = serde_json::json!({"kind":"touch"});
    value["target_kind"] = "actor".into();
    assert!(!serde_json::from_value::<AbilityView>(value)
        .unwrap()
        .valid());
}

#[test]
fn phase5_ally_eligibility_is_explicit_and_cannot_disclose_factions() {
    let value = serde_json::json!({"actor": ActorTarget::from_digest([3;32]), "hostile": false, "allied": true, "injury":"healthy"});
    let decoded: CombatActorView = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("allied");
    assert!(serde_json::from_value::<CombatActorView>(missing).is_err());
    let mut leaked = value;
    leaked["faction"] = "private_faction".into();
    assert!(serde_json::from_value::<CombatActorView>(leaked).is_err());
}
