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
        hit_dice: HitDiceSummary {
            total: 1,
            sources: vec![HitDiceSourceCount {
                source_kind: HitDieSourceKind::Class,
                source: content("fixture_class"),
                count: 1,
            }],
        },
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
        nutrition: tor_protocol::NutritionView::Exempt,
        restorable_losses: RestorableLossesView {
            hit_dice: 0,
            attributes: AttributeView {
                strength: 0,
                speed: 0,
                intellect: 0,
                willpower: 0,
                awareness: 0,
                presence: 0,
            },
            maximum_health: 0,
        },
        conditions: vec![],
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
        expertises: vec![],
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
            weapon_selection: false,
            cooldown_remaining: 0,
            supply_costs: vec![],
        }],
    }
}
#[test]
fn ordinary_hit_dice_are_a_strict_source_count_summary() {
    use serde_json::json;
    let summary = json!({"total":"256","sources":[
        {"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"128"},
        {"source_kind":"racial","source":{"id":"racial","name":"Racial"},"count":"128"}
    ]});
    let mut value = serde_json::to_value(stats()).unwrap();
    for valid in [
        summary.clone(),
        json!({"total":"0","sources":[]}),
        json!({"total":"257","sources":[{"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"257"}]}),
    ] {
        value["hit_dice"] = valid.clone();
        let decoded: OwnStats = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap()["hit_dice"], valid);
    }
    for invalid in [
        json!([]),
        json!([256, summary["sources"]]),
        json!({"total":"256"}),
        json!({"sources":[]}),
        json!({"total":"0","sources":[],"health_seed":42}),
        json!({"total":"0257","sources":[{"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"257"}]}),
        json!({"total":"1","sources":[]}),
        json!({"total":"0","sources":[{"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"0"}]}),
        json!({"total":"1","sources":[{"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"2"}]}),
        json!({"total":"2","sources":[
            {"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"1"},
            {"source_kind":"class","source":{"id":"mage","name":"Different label"},"count":"1"}
        ]}),
        json!({"total":"1","sources":[{"source_kind":"class","source":{"id":"mage","name":""},"count":"1"}]}),
        json!({"total":"1","sources":[["class",{"id":"mage","name":"Mage"},1]]}),
        json!({"total":"1","sources":[{"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"1","ordinal":1}]}),
        json!({"total":"2","sources":[
            {"source_kind":"class","source":{"id":"warrior","name":"Warrior"},"count":"1"},
            {"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"1"}
        ]}),
    ] {
        value["hit_dice"] = invalid.clone();
        assert!(
            serde_json::from_value::<OwnStats>(value.clone()).is_err(),
            "{invalid}"
        );
    }
    value["hit_dice"] = summary;
    let text = serde_json::to_string(&value).unwrap();
    for (field, duplicate) in [
        ("\"total\":\"256\"", "\"total\":\"256\",\"total\":\"256\""),
        ("\"count\":\"128\"", "\"count\":\"128\",\"count\":\"128\""),
    ] {
        assert!(serde_json::from_str::<OwnStats>(&text.replacen(field, duplicate, 1)).is_err());
    }
}

#[test]
fn typed_source_identity_distinguishes_a_class_named_racial_without_label_aliases() {
    use serde_json::json;
    let valid = json!({"total":"2","sources":[
        {"source_kind":"class","source":{"id":"racial","name":"Same label"},"count":"1"},
        {"source_kind":"racial","source":{"id":"racial","name":"Same label"},"count":"1"}
    ]});
    let decoded: HitDiceSummary = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), valid);
    for case in 0..4 {
        let mut invalid = valid.clone();
        match case {
            0 => invalid["sources"][1]["source"]["id"] = json!("not_racial"),
            1 => invalid["sources"][1]["source_kind"] = json!("class"),
            2 => {
                invalid["sources"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("source_kind");
            }
            3 => invalid["sources"].as_array_mut().unwrap().swap(0, 1),
            _ => unreachable!(),
        }
        assert!(serde_json::from_value::<HitDiceSummary>(invalid).is_err());
    }
}

#[test]
fn hit_die_summary_rejects_overflow_and_eagerly_bounds_distinct_sources() {
    use serde_json::json;
    let overflow = json!({"total":u64::MAX.to_string(),"sources":[
        {"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":u64::MAX.to_string()},
        {"source_kind":"racial","source":{"id":"racial","name":"Racial"},"count":"1"}
    ]});
    assert!(serde_json::from_value::<HitDiceSummary>(overflow).is_err());
    for count in [257, MAX_HIT_DIE_SOURCES, MAX_HIT_DIE_SOURCES + 1] {
        let sources: Vec<_> = (0..count)
            .map(|index| {
                json!({
                "source_kind":"class","source":{"id":format!("source_{index:05}"),"name":"A source"},"count":"256"
                })
            })
            .collect();
        let value = json!({"total":(count as u64 * 256).to_string(),"sources":sources});
        assert_eq!(
            serde_json::from_value::<HitDiceSummary>(value).is_ok(),
            count <= MAX_HIT_DIE_SOURCES,
            "distinct source count {count}"
        );
    }
}

#[test]
fn hit_die_summary_totals_and_counts_require_canonical_u64_strings() {
    use serde_json::json;
    let valid = json!({"total":"1","sources":[{
        "source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"1"
    }]});
    for path in ["/total", "/sources/0/count"] {
        for malformed in [
            json!(1),
            json!("01"),
            json!("+1"),
            json!("-1"),
            json!("1.0"),
            json!("18446744073709551616"),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(path).unwrap() = malformed;
            assert!(
                serde_json::from_value::<HitDiceSummary>(invalid).is_err(),
                "invalid {path}"
            );
        }
    }
}

#[test]
fn hit_die_source_kind_is_a_required_strict_string() {
    use serde_json::json;
    let valid = json!({"total":"1","sources":[{
        "source_kind":"class","source":{"id":"racial","name":"A class"},"count":"1"
    }]});
    serde_json::from_value::<HitDiceSummary>(valid.clone()).unwrap();
    for kind in [
        json!(null),
        json!(false),
        json!(1),
        json!([]),
        json!({}),
        json!({"class":null}),
        json!("Class"),
        json!("unknown"),
    ] {
        let mut invalid = valid.clone();
        invalid["sources"][0]["source_kind"] = kind;
        assert!(serde_json::from_value::<HitDiceSummary>(invalid).is_err());
    }
    let mut missing = valid.clone();
    missing["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("source_kind");
    assert!(serde_json::from_value::<HitDiceSummary>(missing).is_err());
    let text = serde_json::to_string(&valid).unwrap();
    assert!(serde_json::from_str::<HitDiceSummary>(&text.replace(
        "\"source_kind\":\"class\"",
        "\"source_kind\":\"class\",\"source_kind\":\"class\""
    ))
    .is_err());
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
        invocation: tor_protocol::AbilityInvocation {
            aim: tor_protocol::AbilityAim::ActorTarget {
                target: ActorTarget::from_digest([1; 32]),
            },
            weapon: tor_protocol::WeaponSelection::Default,
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

#[test]
fn personal_conditions_have_closed_bounded_shapes_without_private_provenance() {
    let mut value = serde_json::to_value(stats()).unwrap();
    let condition = serde_json::json!({"condition":{"id":"fixture_pause","name":"Fixture Pause"},
        "phase":"active","remaining_ticks":"200"});
    value["conditions"] = serde_json::json!([condition.clone()]);
    let parsed: OwnStats =
        serde_json::from_value(value.clone()).expect("personal condition projection");
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    for invalid in [
        serde_json::json!(["fixture_pause", "active", "200"]),
        serde_json::json!({"condition":{"id":"fixture_pause","name":"Fixture Pause"},"phase":"active","remaining_ticks":"200","source":7}),
        serde_json::json!({"condition":{"id":"fixture_pause","name":"Fixture Pause"},"phase":"active","remaining_ticks":"200","qualification":null}),
        serde_json::json!({"condition":{"id":"fixture_pause","name":"Fixture Pause"},"remaining_ticks":"200"}),
    ] {
        value["conditions"] = serde_json::json!([invalid]);
        assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    }
    value["conditions"] = serde_json::Value::Array(vec![condition; 129]);
    assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    value.as_object_mut().unwrap().remove("conditions");
    assert!(serde_json::from_value::<OwnStats>(value).is_err());
}

#[test]
fn restorable_losses_are_required_bounded_named_personal_facts() {
    let mut value = serde_json::to_value(stats()).unwrap();
    let losses = serde_json::json!({
        "attributes":{"strength":2,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "hit_dice":"0","maximum_health":3
    });
    value["restorable_losses"] = losses.clone();
    let parsed: OwnStats = serde_json::from_value(value.clone()).expect("current loss facts");
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!([losses["attributes"], 3]),
        serde_json::json!({"attributes":[2,0,0,0,0,0],"hit_dice":"0","maximum_health":3}),
        serde_json::json!({"attributes":losses["attributes"]}),
        serde_json::json!({"attributes":losses["attributes"],"hit_dice":"0","maximum_health":3,"source":"hidden"}),
        serde_json::json!({"attributes":losses["attributes"],"hit_dice":"0","maximum_health":1_000_001}),
    ] {
        value["restorable_losses"] = invalid;
        assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    }
    for attribute in [
        "strength",
        "speed",
        "intellect",
        "willpower",
        "awareness",
        "presence",
    ] {
        value["restorable_losses"] = losses.clone();
        value["restorable_losses"]["attributes"][attribute] = 1001.into();
        assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
        value["restorable_losses"]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove(attribute);
        assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    }
    value.as_object_mut().unwrap().remove("restorable_losses");
    assert!(serde_json::from_value::<OwnStats>(value).is_err());
    let duplicate = r#"{"hit_dice":"0","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},"maximum_health":1,"maximum_health":2}"#;
    assert!(serde_json::from_str::<RestorableLossesView>(duplicate).is_err());
}

#[test]
fn restorable_hit_dice_are_required_exact_and_never_a_second_saved_history() {
    let mut value = serde_json::to_value(stats()).unwrap();
    for count in [256_u64, 257, 65_536, (1_u64 << 53) + 1, u64::MAX] {
        value["restorable_losses"]["hit_dice"] = count.to_string().into();
        let parsed: OwnStats = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(parsed.restorable_losses.hit_dice, count);
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    for invalid in [
        serde_json::json!(257),
        serde_json::json!("0257"),
        serde_json::json!("-1"),
        serde_json::json!("18446744073709551616"),
    ] {
        value["restorable_losses"]["hit_dice"] = invalid;
        assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    }
    value["restorable_losses"]
        .as_object_mut()
        .unwrap()
        .remove("hit_dice");
    assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    value["restorable_losses"]["hit_dice"] = "1".into();
    value["restorable_losses"]["removed_ordinals"] = serde_json::json!([]);
    assert!(serde_json::from_value::<OwnStats>(value).is_err());
}

#[test]
fn directional_ray_catalog_discloses_only_bounded_sight_range_and_direction_kind() {
    let mut value = stats().abilities.pop().unwrap();
    value.target_kind = AbilityTargetKind::DirectionalRay;
    value.reach = AbilityReach::Sight { range: 6 };
    value.basic_attack = false;
    assert!(value.valid());
    for reach in [
        AbilityReach::SelfOnly,
        AbilityReach::Melee,
        AbilityReach::Sight { range: 0 },
        AbilityReach::Sight { range: 17 },
    ] {
        let mut invalid = value.clone();
        invalid.reach = reach;
        assert!(!invalid.valid());
    }
    value.basic_attack = true;
    assert!(!value.valid());
}

#[test]
fn expertise_snapshot_field_is_mandatory_and_eagerly_bounded() {
    let mut value = serde_json::to_value(stats()).unwrap();
    value.as_object_mut().unwrap().remove("expertises");
    assert!(serde_json::from_value::<OwnStats>(value.clone()).is_err());
    value["expertises"] = serde_json::json!([]);
    assert!(serde_json::from_value::<OwnStats>(value.clone()).is_ok());
    let row = serde_json::json!({"expertise":{"id":"spear","name":"Spear"},"status":"dormant","source_count":257});
    value["expertises"] = serde_json::json!(vec![row; MAX_ADVANCEMENT_EXPERTISE_ROWS + 1]);
    assert!(serde_json::from_value::<OwnStats>(value).is_err());
}

#[test]
fn projectile_catalog_uses_profile_reach_and_never_melee_or_ray_authority() {
    let mut value = serde_json::to_value(stats().abilities.pop().unwrap()).unwrap();
    value["target_kind"] = serde_json::json!("directional_projectile");
    value["reach"] = serde_json::json!({"kind":"projectile"});
    value["basic_attack"] = serde_json::json!(false);
    value["weapon_selection"] = serde_json::json!(false);
    let decoded: AbilityView = serde_json::from_value(value.clone()).unwrap();
    assert!(decoded.valid());
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    for (key, invalid) in [
        ("reach", serde_json::json!({"kind":"melee"})),
        ("reach", serde_json::json!({"kind":"sight","range":6})),
        ("target_kind", serde_json::json!("directional_ray")),
        ("target_kind", serde_json::json!("actor")),
        ("weapon_selection", serde_json::json!(true)),
        ("basic_attack", serde_json::json!(true)),
    ] {
        let mut bad = value.clone();
        bad[key] = invalid;
        assert!(!serde_json::from_value::<AbilityView>(bad).unwrap().valid());
    }
    value["reach"]["range"] = serde_json::json!(6);
    assert!(serde_json::from_value::<AbilityView>(value).is_err());
}

#[test]
fn projectile_strike_summary_has_closed_delivery_without_fake_strike_rows() {
    for delivery in ["throw", "launch"] {
        let value = serde_json::json!({"kind":"projectile","delivery":delivery});
        let source: StrikeSourceInspectionView = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), value);
        for bad in [
            serde_json::json!({"kind":"projectile"}),
            serde_json::json!({"kind":"projectile","delivery":{"throw":null}}),
            serde_json::json!({"kind":"projectile","delivery":delivery,"strike_count":1}),
            serde_json::json!(["projectile", delivery]),
        ] {
            assert!(serde_json::from_value::<StrikeSourceInspectionView>(bad).is_err());
        }
    }
    assert!(serde_json::from_str::<StrikeSourceInspectionView>(
        r#"{"kind":"projectile","delivery":"throw","delivery":"launch"}"#
    )
    .is_err());
}

#[test]
fn persistent_personal_conditions_require_null_and_never_disclose_private_identity() {
    use serde_json::json;
    let condition = json!({"condition":{"id":"fixture_haste","name":"Fixture Haste"},"phase":"active","remaining_ticks":null});
    let mut value = serde_json::to_value(stats()).unwrap();
    value["conditions"] = json!([condition.clone()]);
    let parsed: OwnStats = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    for mutation in 0..5 {
        let mut invalid = condition.clone();
        match mutation {
            0 => {
                invalid.as_object_mut().unwrap().remove("remaining_ticks");
            }
            1 => invalid["remaining_ticks"] = json!(0),
            2 => invalid["phase"] = json!("unknown"),
            3 => invalid["source"] = json!("7"),
            4 => invalid["generation"] = json!("9"),
            _ => unreachable!(),
        }
        value["conditions"] = json!([invalid]);
        assert!(
            serde_json::from_value::<OwnStats>(value.clone()).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn until_obstruction_reach_is_named_closed_and_directional_only() {
    let valid = serde_json::json!({"kind":"until_obstruction"});
    let reach: tor_protocol::AbilityReach = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(serde_json::to_value(reach).unwrap(), valid);
    for bad in [
        serde_json::json!({"kind":"until_obstruction","range":6}),
        serde_json::json!({"kind":"until_obstruction","check":null}),
        serde_json::json!(["until_obstruction"]),
    ] {
        assert!(serde_json::from_value::<tor_protocol::AbilityReach>(bad).is_err());
    }
    let baseline = tor_protocol::AbilityView {
        id: "fixture.contact".into(),
        name: "Contact".into(),
        cost: None,
        reach: tor_protocol::AbilityReach::UntilObstruction,
        target_kind: tor_protocol::AbilityTargetKind::DirectionalRay,
        basic_attack: false,
        weapon_selection: false,
        cooldown_remaining: 0,
        supply_costs: vec![],
    };
    assert!(baseline.valid());
    let decoded: tor_protocol::AbilityView =
        serde_json::from_value(serde_json::to_value(&baseline).unwrap()).unwrap();
    assert_eq!(decoded, baseline);
    assert!(decoded.valid());
    for target in [
        tor_protocol::AbilityTargetKind::Actor,
        tor_protocol::AbilityTargetKind::DirectionalProjectile,
        tor_protocol::AbilityTargetKind::SelfOnly,
    ] {
        let mut invalid = baseline.clone();
        invalid.target_kind = target;
        assert!(!invalid.valid());
    }
    let mut basic = baseline.clone();
    basic.basic_attack = true;
    assert!(!basic.valid());
    let mut selected = baseline.clone();
    selected.weapon_selection = true;
    assert!(!selected.valid());
    assert!(
        baseline.valid(),
        "independent mutations must not invalidate control"
    );
}
#[test]
fn cancellation_diagnostic_is_target_only_and_closed() {
    for changed in [false, true] {
        let value = serde_json::json!({"type":"cancel_specials","target":"2","changed":changed});
        let parsed: tor_protocol::DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    for bad in [
        serde_json::json!({"type":"cancel_specials","target":"2"}),
        serde_json::json!({"type":"cancel_specials","target":"2","changed":true,"source":"1"}),
        serde_json::json!(["cancel_specials", "2", true]),
    ] {
        assert!(serde_json::from_value::<tor_protocol::DiagnosticEffect>(bad).is_err());
    }
}
