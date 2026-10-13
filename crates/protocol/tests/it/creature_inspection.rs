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
        selected_attack: SelectedAttackView {
            category: "equipment".into(),
            source: content("fixture_weapon"),
            socket: Some(0),
            attack: attack(),
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
            11 => {
                value.natural_attack.damage.components[0].amount = DamageAmountView::Rolled {
                    count: 65,
                    sides: 6,
                    bonus: 0,
                }
            }
            12 => value.stats.skills[0].skill.id = "invalid id".into(),
            13 => value.selected_attack.attack.bonus = 1001,
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
