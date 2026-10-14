use super::creature::{content, stats};
use tor_protocol::*;
#[test]
fn condition_phases_are_required_closed_and_named_without_legacy_defaults() {
    use serde_json::json;
    for phase in ["active", "protection"] {
        let valid = json!({"condition":{"id":"frozen","name":"Frozen"},
            "phase":phase,"source":"1","remaining_ticks":"400"});
        let decoded: ConditionView = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), valid);
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("phase");
        assert!(serde_json::from_value::<ConditionView>(missing).is_err());
        for unknown in [
            json!(null),
            json!("unknown"),
            json!([phase]),
            json!({"phase":phase}),
            json!({"active":null}),
            json!({"protection":null}),
        ] {
            let mut bad = valid.clone();
            bad["phase"] = unknown;
            assert!(serde_json::from_value::<ConditionView>(bad).is_err());
        }
        assert!(serde_json::from_value::<ConditionView>(json!([
            valid["condition"],
            phase,
            "1",
            "400"
        ]))
        .is_err());
        let bytes = serde_json::to_string(&valid).unwrap();
        let field = format!("\"phase\":\"{phase}\"");
        assert!(serde_json::from_str::<ConditionView>(
            &bytes.replace(&field, &format!("{field},{field}"))
        )
        .is_err());
    }
}
pub(super) fn attack() -> AttackView {
    AttackView {
        skill: content("fixture_skill"),
        bonus: 2,
        wind_up: 60,
        recovery: 40,
        damage: DamageView {
            primary: DamageKeyView {
                category: DamageType::Energy,
                descriptor: Some(content("fixture_descriptor")),
                sides: Some(6),
            },
            components: vec![DamageComponentView {
                category: DamageType::Energy,
                descriptor: Some(content("fixture_descriptor")),
                amount: DamageAmountView::Rolled {
                    count: 1,
                    sides: 6,
                    bonus: 1,
                },
            }],
        },
    }
}
fn report() -> CreatureInspectionView {
    let mut stats = stats();
    stats.skills = ["first_skill", "second_skill"]
        .map(|id| SkillView {
            skill: content(id),
            rank: 1,
        })
        .to_vec();
    CreatureInspectionView {
        actor: ActorId(1),
        revision: 0,
        tick: 5,
        name: "Subject".into(),
        faction: "Blue".into(),
        species: content("fixture_species"),
        starting_skills: [content("first_skill"), content("second_skill")],
        restoration: InspectionRestoration::ChooseAgain,
        dormant_talents: vec![],
        natural_attack: NaturalAttackView {
            source: content("fixture_species"),
            skill: content("fixture_skill"),
            bonus: 2,
            wind_up: 60,
            recovery: 40,
            strike_count: 1,
        },
        ability_strikes: stats
            .abilities
            .iter()
            .map(|ability| AbilityStrikesView {
                ability: ability.id.clone(),
                source: StrikeSourceInspectionView::SelectedMelee {},
            })
            .collect(),
        selected_attack: SelectedAttackView {
            category: "equipment".into(),
            source: content("fixture_weapon"),
            socket: Some(0),
            attack: SelectedMeleeView::Weapon { attack: attack() },
            attribute: content("fixture_attribute"),
            attribute_value: 2,
            rank: 1,
            check_bonus: 5,
            abilities: vec![],
        },
        initial_attributes: stats.attributes,
        templates: vec![content("fixture_template")],
        max_hp: 10,
        hp: 8,
        injury: 2,
        dead: false,
        hit_dice: vec![InspectionHitDie {
            ordinal: 1,
            source: stats.hit_dice[0].clone(),
            health_seed: u64::MAX,
            health_die: 10,
            base_health: 10,
            point_pools: vec![InspectionPointPool {
                pool: content("advancement"),
                granted: 1,
                spent: 1,
                allowed: vec![AdvancementKind::SkillRank, AdvancementKind::Talent],
            }],
            purchases: vec![InspectionPurchase {
                pool: content("advancement"),
                selection: InspectionPurchaseSelection::Talent {
                    talent: content("fixture_talent"),
                },
            }],
            attribute: None,
        }],
        stats,
        sources: vec![InspectionSource {
            owner: None,
            category: "talent".into(),
            source: content("fixture_talent"),
            rows: vec![CombatTraceStep {
                id: "add".into(),
                name: "Add".into(),
                fields: vec![DiagnosticField {
                    id: "value".into(),
                    name: "Value".into(),
                    value: DiagnosticValue::Signed { value: 7 },
                }],
            }],
        }],
        conditions: vec![ConditionView {
            phase: ConditionPhase::Active,
            condition: content("fixture_condition"),
            source: ActorId(2),
            remaining_ticks: 20,
        }],
    }
}
#[test]
fn private_inspection_uses_disclosed_identity_generic_sources_and_named_ledger_facts() {
    let report = report();
    report.validate().unwrap();
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(encoded["hit_dice"][0]["health_seed"], u64::MAX.to_string());
    assert_eq!(
        serde_json::from_value::<CreatureInspectionView>(encoded.clone()).unwrap(),
        report
    );
    let mut forged = encoded;
    forged["sources"][0]["cached_authority"] = true.into();
    assert!(serde_json::from_value::<CreatureInspectionView>(forged).is_err());
}
#[test]
fn private_inspection_rejects_structural_inconsistency_without_reimplementing_content_rules() {
    for case in 0..17 {
        let mut value = report();
        match case {
            0 => value.actor = ActorId(0),
            1 => value.name = "x".repeat(61),
            2 => value.hp = 11,
            3 => value.injury = 1,
            4 => value.hit_dice[0].ordinal = 2,
            5 => value.hit_dice[0].health_die = 0,
            6 => value.templates.push(value.templates[0].clone()),
            7 => value.sources.push(value.sources[0].clone()),
            8 => value.conditions[0].remaining_ticks = 0,
            9 => value.stats.abilities.push(value.stats.abilities[0].clone()),
            10 => value.stats.resources[0].available = u32::MAX,
            11 => value.natural_attack.strike_count = 0,
            12 => value.stats.skills[0].skill.id = "invalid id".into(),
            13 => match &mut value.selected_attack.attack {
                SelectedMeleeView::Weapon { attack } => attack.bonus = 1001,
                SelectedMeleeView::Natural { attack } => attack.bonus = 1001,
            },
            14 => value.selected_attack.source.id = "invalid id".into(),
            15 => value.selected_attack.rank = 6,
            16 => value.selected_attack.abilities.push(CombatTraceStep {
                id: "invalid id".into(),
                name: "invalid".into(),
                fields: vec![],
            }),
            _ => unreachable!(),
        }
        assert!(value.validate().is_err(), "case {case}");
    }
}
#[test]
fn distinct_condition_sources_share_a_content_id_without_colliding() {
    let mut value = report();
    value.sources[0].category = "condition".into();
    value.sources[0].owner = Some(ActorId(1));
    let mut second = value.sources[0].clone();
    second.owner = Some(ActorId(2));
    value.sources.push(second);
    value.validate().unwrap();
}

#[test]
fn selected_attack_profiles_share_the_disclosed_content_reference_bound() {
    let mut value = report();
    let profile = CombatTraceStep {
        id: "ability_profile".into(),
        name: "Ability profile".into(),
        fields: vec![],
    };
    for count in [257, MAX_CONTENT_REFERENCES] {
        value.selected_attack.abilities = vec![profile.clone(); count];
        assert!(value.validate().is_ok(), "valid profile count {count}");
    }
    value.selected_attack.abilities.push(profile);
    assert!(value.validate().is_err());
}

#[test]
fn selected_attack_socket_uses_the_equipment_socket_bound_without_content_coupling() {
    let mut value = report();
    for socket in [None, Some(0), Some(63)] {
        value.selected_attack.socket = socket;
        value.validate().unwrap();
    }
    for socket in [64, u16::MAX] {
        value.selected_attack.socket = Some(socket);
        assert!(value.validate().is_err(), "invalid socket {socket}");
    }
}

#[test]
fn inspection_rejects_banked_health_recovery_on_saturated_or_dead_health() {
    let mut value = report();
    value.stats.health_recovery.elapsed_ticks = 400;
    assert!(value.validate().is_ok());
    value.hp = value.max_hp;
    value.injury = 0;
    assert!(value.validate().is_err());
    value.dead = true;
    value.hp = 0;
    value.injury = value.max_hp;
    assert!(value.validate().is_err());
    value.stats.health_recovery.elapsed_ticks = 0;
    value.conditions.clear();
    assert!(value.validate().is_ok());
}

#[test]
fn advancement_inspection_rejects_inconsistent_pool_ownership_and_creation_grants() {
    for case in 0..8 {
        let mut value = report();
        match case {
            0 => value.starting_skills[1] = value.starting_skills[0].clone(),
            1 => value.hit_dice[0].point_pools[0].spent = 0,
            2 => value.hit_dice[0].point_pools[0].granted = 0,
            3 => value.hit_dice[0].point_pools[0].allowed = vec![AdvancementKind::SkillRank],
            4 => value.hit_dice[0].purchases[0].pool = content("unknown_pool"),
            5 => value.hit_dice[0].point_pools[0]
                .allowed
                .push(AdvancementKind::Talent),
            6 => {
                let duplicate = value.hit_dice[0].point_pools[0].clone();
                value.hit_dice[0].point_pools.push(duplicate);
            }
            7 => value.hit_dice[0].point_pools[0].granted = 257,
            _ => unreachable!(),
        }
        assert!(value.validate().is_err(), "case {case}");
    }
}
#[test]
fn dormant_inspection_requires_reasons_for_exactly_the_disclosed_dormant_nodes() {
    let mut value = report();
    value.stats.dormant_talents.push(content("child"));
    assert!(value.validate().is_err());
    value.dormant_talents.push(InspectionDormantTalent {
        talent: content("child"),
        reasons: vec!["Requires active parent root".into()],
    });
    value.hit_dice[0].point_pools[0].granted = 2;
    value.hit_dice[0].point_pools[0].spent = 2;
    value.hit_dice[0].purchases.push(InspectionPurchase {
        pool: content("advancement"),
        selection: InspectionPurchaseSelection::Talent {
            talent: content("child"),
        },
    });
    value.validate().unwrap();
    value.dormant_talents[0].reasons.clear();
    assert!(value.validate().is_err());
}

#[test]
fn purchased_talents_must_match_the_unique_disclosed_active_and_dormant_ownership() {
    for case in 0..4 {
        let mut value = report();
        match case {
            0 => {
                let purchase = value.hit_dice[0].purchases[0].clone();
                value.hit_dice[0].purchases.push(purchase);
                value.hit_dice[0].point_pools[0].granted = 2;
                value.hit_dice[0].point_pools[0].spent = 2;
            }
            1 => value.stats.active_talents.push(content("unowned")),
            2 => {
                value.hit_dice[0].purchases[0].selection = InspectionPurchaseSelection::Talent {
                    talent: content("undisclosed"),
                }
            }
            3 => {
                value.hit_dice[0].purchases[0].selection = InspectionPurchaseSelection::Talent {
                    talent: ContentRef {
                        name: "Conflicting name".into(),
                        ..content("fixture_talent")
                    },
                }
            }
            _ => unreachable!(),
        }
        assert!(value.validate().is_err(), "invalid ownership case {case}");
    }
}

fn rank_consistent_report() -> CreatureInspectionView {
    let mut value = report();
    value.stats.skills = vec![
        SkillView {
            skill: value.starting_skills[0].clone(),
            rank: 1,
        },
        SkillView {
            skill: value.starting_skills[1].clone(),
            rank: 1,
        },
        SkillView {
            skill: content("novel_paid_skill"),
            rank: 2,
        },
        SkillView {
            skill: content("novel_zero_skill"),
            rank: 0,
        },
    ];
    for _ in 0..2 {
        add_rank_purchase(&mut value, content("novel_paid_skill"));
    }
    value
}
fn add_rank_purchase(value: &mut CreatureInspectionView, skill: ContentRef) {
    let die = &mut value.hit_dice[0];
    let pool = &mut die.point_pools[0];
    pool.granted += 1;
    pool.spent += 1;
    die.purchases.push(InspectionPurchase {
        pool: pool.pool.clone(),
        selection: InspectionPurchaseSelection::SkillRank { skill },
    });
}
#[test]
fn inspection_skill_ranks_equal_starting_and_paid_ownership_with_generic_ids() {
    let value = rank_consistent_report();
    value.validate().unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    let restored: CreatureInspectionView = serde_json::from_value(encoded).unwrap();
    assert_eq!(restored, value);
    restored.validate().unwrap();

    // The wire accepts valid server-defined vocabulary and an optional zero-rank
    // row. It must not embed the current simulation list of nineteen skills.
    let mut at_cap = value.clone();
    for _ in 0..4 {
        let skill = at_cap.starting_skills[0].clone();
        add_rank_purchase(&mut at_cap, skill);
    }
    at_cap.stats.skills[0].rank = 5;
    at_cap.validate().unwrap();
    let skill = at_cap.starting_skills[0].clone();
    add_rank_purchase(&mut at_cap, skill);
    assert!(
        at_cap.validate().is_err(),
        "starting rank plus five purchases exceeds cap"
    );
}
#[test]
fn inspection_rejects_skill_ownership_rank_and_identity_disagreements() {
    for case in 0..10 {
        let mut value = rank_consistent_report();
        match case {
            0 => {
                value.stats.skills.remove(0);
            }
            1 => value.stats.skills[0].rank = 0,
            2 => value.stats.skills[0].skill.name = "Conflicting starting name".into(),
            3 => value.starting_skills[0].name = "Conflicting grant name".into(),
            4 => value.stats.skills[2].rank = 1,
            5 => value.stats.skills[2].rank = 3,
            6 => {
                value.stats.skills.remove(2);
            }
            7 => value.stats.skills[3].rank = 1,
            8 => {
                if let InspectionPurchaseSelection::SkillRank { skill } =
                    &mut value.hit_dice[0].purchases[1].selection
                {
                    skill.name = "Conflicting purchase name".into();
                }
            }
            9 => {
                if let InspectionPurchaseSelection::SkillRank { skill } =
                    &mut value.hit_dice[0].purchases[1].selection
                {
                    *skill = content("undisclosed_paid_skill");
                }
            }
            _ => unreachable!(),
        }
        assert!(
            value.validate().is_err(),
            "invalid skill ownership case {case}"
        );
    }
}

fn ordered_strikes_source_report() -> serde_json::Value {
    let mut value = serde_json::to_value(report()).unwrap();
    value["natural_attack"]["strike_count"] = serde_json::json!(3);
    value["ability_strikes"] = serde_json::json!(report().stats.abilities.into_iter().map(|ability|
        serde_json::json!({"ability":ability.id,"source":{"kind":"authored","strike_count":3}})
    ).collect::<Vec<_>>());
    value
}

#[test]
fn ordered_strikes_private_inspection_roundtrips_authored_source_summaries_without_fake_damage() {
    let mut value = ordered_strikes_source_report();
    for natural_selected in [false, true] {
        if natural_selected {
            value["selected_attack"]["source"] = value["natural_attack"]["source"].clone();
            value["selected_attack"]["category"] = serde_json::json!("species");
            value["selected_attack"]["socket"] = serde_json::Value::Null;
            value["selected_attack"]["attack"] =
                serde_json::json!({"kind":"natural","attack":value["natural_attack"]});
        }
        let decoded: CreatureInspectionView = serde_json::from_value(value.clone()).unwrap();
        decoded.validate().unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    assert!(value["natural_attack"].get("damage").is_none());
    assert!(value["natural_attack"].get("strikes").is_none());
}

#[test]
fn ordered_strikes_private_inspection_rejects_noncanonical_and_over_bound_source_summaries() {
    for case in 0..8 {
        let mut value = ordered_strikes_source_report();
        match case {
            0 => value["natural_attack"]["strike_count"] = serde_json::json!(0),
            1 => value["natural_attack"]["strike_count"] = serde_json::json!(17),
            2 => value["natural_attack"]["strikes"] = serde_json::json!([]),
            3 => {
                value["natural_attack"]["source"] =
                    serde_json::to_value(content("wrong_species")).unwrap()
            }
            4 => value["ability_strikes"][0]["ability"] = serde_json::json!("unknown_ability"),
            5 => value["ability_strikes"][0]["source"]["strike_count"] = serde_json::json!(17),
            6 => {
                value["ability_strikes"][0]["source"]
                    .as_object_mut()
                    .unwrap()
                    .remove("strike_count");
            }
            7 => value["natural_attack"]["future_rolls"] = serde_json::json!([]),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<CreatureInspectionView>(value)
                .map_or(true, |report| report.validate().is_err()),
            "invalid source summary case {case}"
        );
    }
}

#[test]
fn ordered_strikes_main_source_summaries_match_granted_ability_order_without_duplicate_labels() {
    let mut value = serde_json::to_value(report()).unwrap();
    value["revision"] = serde_json::json!("5");
    value["natural_attack"]
        .as_object_mut()
        .unwrap()
        .remove("strikes");
    value["natural_attack"]["strike_count"] = serde_json::json!(1);
    value["ability_strikes"] = serde_json::json!(value["stats"]["abilities"].as_array().unwrap().iter().map(|ability|
        serde_json::json!({"ability":ability["id"],"source":{"kind":"selected_melee"}})).collect::<Vec<_>>());
    let decoded: CreatureInspectionView =
        serde_json::from_value(value.clone()).expect("lightweight summaries decode");
    assert!(decoded.validate().is_ok());
    let mut bad = value.clone();
    bad["ability_strikes"][0]["name"] = serde_json::json!("Repeated label");
    assert!(serde_json::from_value::<CreatureInspectionView>(bad).is_err());
    let mut bad = value.clone();
    bad["ability_strikes"][0]["ability"] = serde_json::json!("ungranted");
    assert!(serde_json::from_value::<CreatureInspectionView>(bad)
        .unwrap()
        .validate()
        .is_err());
    let mut bad = value.clone();
    bad["ability_strikes"].as_array_mut().unwrap().reverse();
    if bad["ability_strikes"] != value["ability_strikes"] {
        assert!(serde_json::from_value::<CreatureInspectionView>(bad)
            .unwrap()
            .validate()
            .is_err());
    }
    let mut bad = value;
    bad["natural_attack"]["strike_count"] = serde_json::json!(17);
    assert!(serde_json::from_value::<CreatureInspectionView>(bad)
        .unwrap()
        .validate()
        .is_err());
}
