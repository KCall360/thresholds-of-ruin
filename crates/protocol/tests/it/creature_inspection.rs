use tor_protocol::*;

fn report() -> CreatureInspectionView {
    let skills = [
        "athletics",
        "heavy_weaponry",
        "agility",
        "light_weaponry",
        "stealth",
        "thievery",
        "crafting",
        "deduction",
        "lore",
        "medicine",
        "discipline",
        "intimidation",
        "insight",
        "perception",
        "survival",
        "deception",
        "leadership",
        "persuasion",
        "spellcasting",
    ];
    let attributes = serde_json::json!({"strength":1,"speed":1,"intellect":1,"willpower":1,"awareness":1,"presence":1});
    serde_json::from_value(serde_json::json!({
        "actor":"2", "tick":"0", "name":"test mage", "faction":"blue",
        "species":{"id":"human","kind":"humanoid","subtypes":[],"attributes":attributes,"melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","descriptor":null,"sides":6},"components":[{"category":"impact","descriptor":null,"amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}}},
        "initial_attributes":attributes, "templates":[],
        "max_hp":7, "hp":4, "injury":3, "dead":false,
        "stats":{"kind":"humanoid","subtypes":[],"hit_dice":["mage"],"attributes":attributes,
            "skills":skills.iter().map(|skill| serde_json::json!({"skill":skill,"rank":if *skill=="spellcasting" {1} else {0}})).collect::<Vec<_>>(),
            "defenses":{"physical":2,"cognitive":2,"spiritual":2},"binding":"intellect",
            "resources":[{"resource":"stamina","balance":3,"maximum":3,"available":3,"reserved":0},{"resource":"focus","balance":3,"maximum":3,"available":3,"reserved":0},{"resource":"mana","balance":2,"maximum":2,"available":2,"reserved":0}],
            "active_talents":["magic_bolt"],"dormant_talents":[],"abilities":["basic_melee","magic_bolt"]},
        "hit_dice":[{"ordinal":1,"source":"mage","health_seed":"18446744073709551615","health_die":6,"base_health":6,"training":["spellcasting"],"attribute":null,"talent":"magic_bolt"}],
        "grants":[{"source":{"type":"species","id":"human"},"grants":[]},{"source":{"type":"class","class":"mage"},"grants":[{"type":"magical"}]},{"source":{"type":"talent","talent":"magic_bolt"},"grants":[{"type":"ability","ability":"magic_bolt"}]}],
        "fear":[]
    })).unwrap()
}

#[test]
fn privileged_inspection_round_trips_large_seeds_and_explicit_owned_sources() {
    let report = report();
    assert!(report.validate().is_ok());
    assert_eq!(report.hit_dice[0].health_seed, u64::MAX);
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(
        encoded["hit_dice"][0]["health_seed"],
        "18446744073709551615"
    );
    assert_eq!(
        serde_json::from_value::<CreatureInspectionView>(encoded).unwrap(),
        report
    );
}

#[test]
fn privileged_inspection_rejects_inconsistent_ownership_health_and_grants() {
    let original = report();
    let mut malformed = original.clone();
    malformed.hit_dice[0].ordinal = 0;
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.hit_dice[0].training.push(Skill::Discipline);
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.stats.active_talents.clear();
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.hp = 5;
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.dead = true;
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.max_hp = 0;
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.grants[1]
        .grants
        .push(InspectionGrant::PhysicalDefense { modifier: 1001 });
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.grants[1].source = InspectionGrantSource::Template {
        id: "unknown".into(),
    };
    assert!(malformed.validate().is_err());
    let mut malformed = original.clone();
    malformed.grants.push(malformed.grants[0].clone());
    assert!(malformed.validate().is_err());
    let mut encoded = serde_json::to_value(original).unwrap();
    encoded["world_state"] = serde_json::json!({});
    assert!(serde_json::from_value::<CreatureInspectionView>(encoded).is_err());
}

#[test]
fn privileged_inspection_rejects_fear_that_coexists_with_immunity() {
    for descriptor in [
        InspectionDescriptor::Fear,
        InspectionDescriptor::MindAffecting,
    ] {
        let mut value = report();
        value.fear.push(InspectionFear {
            causer: ActorId(99),
            remaining_ticks: 30,
        });
        assert!(value.validate().is_ok());
        value.grants[0].grants.push(InspectionGrant::Immunity {
            selector: InspectionSelector::Descriptor { descriptor },
        });
        assert!(value.validate().is_err());
    }
}

#[test]
fn privileged_inspection_keeps_fear_with_reductions_and_unrelated_immunities() {
    let mut value = report();
    value.fear.push(InspectionFear {
        causer: ActorId(99),
        remaining_ticks: 30,
    });
    value.grants[0].grants.extend([
        InspectionGrant::Reduction {
            selector: InspectionSelector::Descriptor {
                descriptor: InspectionDescriptor::Fear,
            },
            amount: 100,
        },
        InspectionGrant::Immunity {
            selector: InspectionSelector::Category {
                category: DamageType::Spirit,
            },
        },
        InspectionGrant::Immunity {
            selector: InspectionSelector::Descriptor {
                descriptor: InspectionDescriptor::Cold,
            },
        },
    ]);
    assert!(value.validate().is_ok());
    let encoded = serde_json::to_string(&value).unwrap();
    let restored: CreatureInspectionView = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, value);
    assert!(restored.validate().is_ok());
}

#[test]
fn natural_attack_inspection_rejects_invalid_sources_and_noncanonical_components() {
    let source = report();
    for case in 0..10 {
        let mut changed = source.clone();
        let attack = &mut changed.species.melee;
        match case {
            0 => attack.skill = Skill::Spellcasting,
            1 => attack.bonus = 1001,
            2 => attack.wind_up = 0,
            3 => attack.recovery = 1_000_001,
            4 => attack.damage.primary.sides = Some(8),
            5 => attack.damage.components.clear(),
            6 => attack
                .damage
                .components
                .push(attack.damage.components[0].clone()),
            7 => {
                attack.damage.components[0].amount = DamageAmountView::Rolled {
                    count: 65,
                    sides: 6,
                    bonus: 0,
                }
            }
            8 => attack.damage.components[0].amount = DamageAmountView::Fixed { value: 1_000_001 },
            9 => {
                attack.damage.primary.descriptor = Some(InspectionDescriptor::Fire);
                attack.damage.components[0].descriptor = Some(InspectionDescriptor::Fire);
                attack.damage.components.push(DamageComponentView {
                    category: DamageType::Energy,
                    descriptor: Some(InspectionDescriptor::Fire),
                    amount: DamageAmountView::Fixed { value: 1 },
                });
            }
            _ => unreachable!(),
        }
        assert!(
            changed.validate().is_err(),
            "accepted malformed natural attack {case}"
        );
    }
    let mut encoded = serde_json::to_value(source).unwrap();
    encoded["species"]["melee"]["damage"]["components"][0]["amount"]["unknown"] =
        serde_json::json!(1);
    assert!(serde_json::from_value::<CreatureInspectionView>(encoded).is_err());
}
