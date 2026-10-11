use super::creature::{content, stats};
use tor_protocol::*;
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
    let stats = stats();
    CreatureInspectionView {
        actor: ActorId(1),
        tick: 5,
        name: "Subject".into(),
        faction: "Blue".into(),
        species: content("fixture_species"),
        natural_attack: attack(),
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
            training: vec![content("fixture_skill")],
            attribute: None,
            talent: Some(content("fixture_talent")),
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
    for case in 0..13 {
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
            11 => {
                value.natural_attack.damage.components[0].amount = DamageAmountView::Rolled {
                    count: 65,
                    sides: 6,
                    bonus: 0,
                }
            }
            12 => value.stats.skills[0].skill.id = "invalid id".into(),
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
