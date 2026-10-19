use super::creature_fixture::{self as fixture, build_with};
use std::collections::{BTreeMap, BTreeSet};
use tor_simulation::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use tor_simulation::content_ids::*;
use tor_simulation::creatures::{
    BuildError, Content, CreatureBuild, ScenarioCatalog, SourceId, Species, Template,
};
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::rules::{
    Modifier, ModifierOp, Requirement, Ruleset, Scope, Selector, Stat, TalentDef,
};

fn talent(id: &str) -> TalentId {
    TalentId::new(id).unwrap()
}
fn ability(id: &str) -> AbilityId {
    AbilityId::new(id).unwrap()
}
fn class(id: &str) -> HdSource {
    HdSource::Class(ClassId::new(id).unwrap())
}
fn template(id: &str) -> TemplateId {
    TemplateId::new(id).unwrap()
}
fn species() -> Species {
    let mut value = fixture::species();
    value.default_attributes = Attributes::new(ByAttribute::from_fn(|_| 1)).unwrap();
    value.modifiers.clear();
    value.subtypes.insert(SubtypeId::new("goblinoid").unwrap());
    value
}
fn zombie() -> Template {
    let mut value = Template::new(template("zombified"), 0);
    value.kind = Some(TypeId::new("undead").unwrap());
    value.adjustments.strength = 2;
    value.adjustments.speed = -1;
    value.overrides.intellect = Some(0);
    value.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Flag {
            flag: FlagId::new("mindless").unwrap(),
        },
    });
    value
}
fn build(sources: &[HdSource]) -> CreatureBuild {
    build_with(species(), vec![zombie()], sources)
}
fn add(stat: Stat, value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add { stat, value },
    }
}
fn fear() -> Selector {
    Selector::Descriptor {
        descriptor: DescriptorId::new("fear").unwrap(),
    }
}

fn fixture_health() -> TalentDef {
    fixture::talent("fixture_health", vec![add(Stat::Health, 2)])
}
fn fixture_weapon_bonus(
    id: &str,
    stat: tor_simulation::rules::AbilityStat,
    value: i64,
) -> TalentDef {
    fixture::talent(
        id,
        vec![add(
            Stat::AbilityTag {
                tag: AbilityTagId::new("weapon_attack").unwrap(),
                stat,
            },
            value,
        )],
    )
}
fn select_form(
    value: &mut CreatureBuild,
    owner: usize,
    root: &str,
    form: &str,
) -> Result<(), BuildError> {
    value.purchase(
        tor_simulation::HdIndex::from_usize(owner).unwrap(),
        PointPoolId::new("advancement").unwrap(),
        tor_simulation::creatures::PurchaseSelection::TalentForm {
            talent: talent(root),
            form: FormId::new(form).unwrap(),
        },
    )
}

#[test]
fn techniques_use_the_whole_current_build_pool_and_older_unspent_slots() {
    let mut value = build(&[HdSource::Racial, HdSource::Racial, class("mage")]);
    assert!(!value
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("spark_lance_fire")));
    value = fixture::with_starting_skills(value, [Skill::Spellcasting, Skill::Intimidation]);
    select_form(&mut value, 0, "evocation_initiate", "spark_lance_fire").unwrap();
    select_form(&mut value, 1, "binding_initiate", "falter").unwrap();
    assert!(value
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("spark_lance_fire")));
    assert_eq!(
        select_form(&mut value, 2, "evocation_initiate", "spark_lance_fire"),
        Err(BuildError::DuplicateTalent)
    );
    assert_eq!(value.unspent_points(), 1);
}
#[test]
fn type_invalidated_choices_remain_owned_and_reactivate() {
    let mut value = build(&[HdSource::Racial, HdSource::Racial]);
    value = fixture::with_starting_skills(value, [Skill::HeavyWeaponry, Skill::Athletics]);
    let mut technique = fixture::talent(
        "fixture_living_technique",
        vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ability("rattling_blow"),
            },
        }],
    );
    technique.requirement = Requirement::All {
        requirements: vec![
            Requirement::RacialHd {
                minimum: tor_simulation::HdCount(2),
            },
            Requirement::TypeIn {
                types: vec![
                    TypeId::new("humanoid").unwrap(),
                    TypeId::new("animal").unwrap(),
                ],
            },
            Requirement::SkillRank {
                skill: tor_simulation::rules::SkillSelector::NaturalAttack,
                minimum: 1,
            },
        ],
    };
    value = fixture::with_talents(value, [technique, fixture_health()]);
    value
        .select_talent(
            tor_simulation::HdIndex(0),
            talent("fixture_living_technique"),
        )
        .unwrap();
    let original = value.derive().unwrap();
    value.set_templates(vec![template("zombified")]).unwrap();
    let undead = value.derive().unwrap();
    assert_eq!(undead.kind, TypeId::new("undead").unwrap());
    assert!(undead.flags.contains(&FlagId::new("mindless").unwrap()));
    assert_eq!(undead.attributes.get(Attribute::Strength), 3);
    assert_eq!(undead.attributes.get(Attribute::Speed), 0);
    assert_eq!(undead.attributes.get(Attribute::Intellect), 0);
    assert_eq!(undead.anatomy, original.anatomy);
    assert!(undead
        .dormant_talents
        .contains(&talent("fixture_living_technique")));
    assert!(!undead.abilities.contains(&ability("rattling_blow")));
    assert_eq!(
        value.select_talent(tor_simulation::HdIndex(0), talent("fixture_health")),
        Err(BuildError::NoAdvancementPoint)
    );
    value.set_templates(vec![]).unwrap();
    assert_eq!(value.derive().unwrap(), original);
}
#[test]
fn independent_immunity_sources_survive_template_removal() {
    let mut definition = species();
    definition.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity { selector: fear() },
    });
    let id = definition.id.clone();
    let mut value = build_with(definition, vec![zombie()], &[HdSource::Racial]);
    value.set_templates(vec![template("zombified")]).unwrap();
    assert!(value.derive().unwrap().immunity_sources(&fear()).len() >= 2);
    value.set_templates(vec![]).unwrap();
    assert_eq!(
        value.derive().unwrap().immunity_sources(&fear()),
        vec![SourceId::Species(id)]
    );
}
#[test]
fn template_conflicts_reject_atomically_and_priorities_resolve_explicitly() {
    let mut same = Template::new(template("same"), 0);
    same.kind = Some(TypeId::new("construct").unwrap());
    let mut later = same.clone();
    later.id = template("later");
    later.priority = 1;
    let mut value = build_with(species(), vec![zombie(), same, later], &[HdSource::Racial]);
    let before = value.clone();
    assert_eq!(
        value.set_templates(vec![template("zombified"), template("same")]),
        Err(BuildError::ConflictingTemplates)
    );
    assert_eq!(value, before);
    value
        .set_templates(vec![template("later"), template("zombified")])
        .unwrap();
    let derived = value.derive().unwrap();
    assert_eq!(derived.kind, TypeId::new("construct").unwrap());
    value
        .set_templates(vec![template("zombified"), template("later")])
        .unwrap();
    assert_eq!(value.derive().unwrap(), derived);
}
#[test]
fn same_priority_numeric_adjustments_are_aggregated_before_flooring() {
    let mut definition = species();
    definition.default_attributes = Attributes::default();
    let mut increase = Template::new(template("increase"), 0);
    increase.adjustments.speed = 1;
    let mut decrease = Template::new(template("decrease"), 0);
    decrease.adjustments.speed = -1;
    let mut value = build_with(definition, vec![increase, decrease], &[HdSource::Racial]);
    value
        .set_templates(vec![template("increase"), template("decrease")])
        .unwrap();
    assert_eq!(value.derive().unwrap().attributes.get(Attribute::Speed), 0);
}
#[test]
fn advancement_ownership_and_caps_reverse_with_the_latest_hd() {
    let mut value = build(&vec![class("warrior"); 4]);
    value = fixture::with_talents(value, [fixture_health()]);
    assert_eq!(
        value.increase_attribute(tor_simulation::HdIndex(0), Attribute::Strength),
        Err(BuildError::NoAttributeOpportunity)
    );
    value
        .increase_attribute(tor_simulation::HdIndex(3), Attribute::Strength)
        .unwrap();
    value
        .select_talent(tor_simulation::HdIndex(3), talent("fixture_health"))
        .unwrap();
    assert_eq!(
        value.derive().unwrap().attributes.get(Attribute::Strength),
        2
    );
    assert_eq!(
        value
            .remove_latest()
            .unwrap()
            .choices
            .talents()
            .cloned()
            .collect::<Vec<_>>(),
        vec![talent("fixture_health")]
    );
    let derived = value.derive().unwrap();
    assert_eq!(derived.attributes.get(Attribute::Strength), 1);
    assert_eq!(derived.skills.get(Skill::HeavyWeaponry), 0);
    assert!(!derived.active_talents.contains(&talent("fixture_health")));
    while value.remove_latest().is_some() {}
    assert_eq!(value.derive().unwrap().maximum_health, 0);
}
#[test]
fn training_overspending_and_rank_overflow_leave_choices_unchanged() {
    let mut value = build(&vec![class("warrior"); 6]);
    value
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    let before = value.clone();
    assert_eq!(
        value.train(tor_simulation::HdIndex(0), Skill::Athletics),
        Err(BuildError::NoAdvancementPoint)
    );
    assert_eq!(value, before);
    for owner in 1..5 {
        value
            .train(
                tor_simulation::HdIndex::from_usize(owner).unwrap(),
                Skill::HeavyWeaponry,
            )
            .unwrap();
    }
    let before = value.clone();
    assert_eq!(
        value.train(tor_simulation::HdIndex(5), Skill::HeavyWeaponry),
        Err(BuildError::RankCap)
    );
    assert_eq!(value, before);
}

#[test]
fn catalog_subtypes_have_explicit_identity_or_deferred_mechanics() {
    let rules = fixture::ruleset();
    assert_eq!(rules.document().subtypes.len(), 20);
    assert!(
        rules
            .subtype(&SubtypeId::new("swarm").unwrap())
            .unwrap()
            .mechanics_deferred
    );
    assert!(
        !rules
            .subtype(&SubtypeId::new("fire").unwrap())
            .unwrap()
            .mechanics_deferred
    );
    let mut definition = species();
    definition.subtypes.insert(SubtypeId::new("fire").unwrap());
    let value = build_with(definition, vec![], &[HdSource::Racial]);
    let selector = Selector::Descriptor {
        descriptor: DescriptorId::new("fire").unwrap(),
    };
    assert_eq!(
        value.derive().unwrap().immunity_sources(&selector),
        vec![SourceId::Subtype(SubtypeId::new("fire").unwrap())]
    );
}
#[test]
fn passive_modifiers_change_only_declared_stats_and_expose_sources() {
    let mut value = build(&vec![class("warrior"); 4]);
    value = fixture::with_talents(
        value,
        [
            fixture_health(),
            fixture::talent(
                "fixture_physical_defense",
                vec![add(
                    Stat::Defense {
                        defense: tor_simulation::rules::DefenseKind::Physical,
                    },
                    1,
                )],
            ),
            fixture_weapon_bonus(
                "fixture_weapon_flat",
                tor_simulation::rules::AbilityStat::FlatBonus,
                1,
            ),
            fixture_weapon_bonus(
                "fixture_weapon_die",
                tor_simulation::rules::AbilityStat::Dice,
                1,
            ),
        ],
    );
    let before = value.derive().unwrap();
    for (slot, id) in [
        "fixture_health",
        "fixture_physical_defense",
        "fixture_weapon_flat",
        "fixture_weapon_die",
    ]
    .into_iter()
    .enumerate()
    {
        value
            .select_talent(
                tor_simulation::HdIndex::from_usize(slot).unwrap(),
                talent(id),
            )
            .unwrap();
    }
    let after = value.derive().unwrap();
    assert_eq!(after.maximum_health, before.maximum_health + 2);
    assert_eq!(after.defenses.physical, before.defenses.physical + 1);
    assert_eq!(after.defenses.cognitive, before.defenses.cognitive);
    assert_eq!(after.defenses.spiritual, before.defenses.spiritual);
    assert_eq!(after.ability_values(&ability("basic_melee")).dice, 1);
    assert_eq!(after.ability_values(&ability("basic_melee")).flat, 1);
    assert_eq!(
        after.ability_values(&ability("spark_lance_fire")),
        before.ability_values(&ability("spark_lance_fire"))
    );
    assert_eq!(
        after.sources[&SourceId::Talent(talent("fixture_health"))],
        vec![add(Stat::Health, 2)]
    );
}
#[test]
fn invalid_definitions_and_derived_limits_are_rejected_without_mutation() {
    let mut huge = Template::new(template("huge"), 0);
    huge.modifiers = vec![add(Stat::HealthRecoveryPeriod, 1_000_000)];
    let mut value = build_with(species(), vec![huge, zombie()], &[HdSource::Racial]);
    let before = value.clone();
    assert_eq!(
        value.set_templates(vec![template("huge")]),
        Err(BuildError::DerivedLimit)
    );
    assert_eq!(value, before);
    assert_eq!(
        value.set_templates(vec![template("zombified"); 33]),
        Err(BuildError::InvalidDefinition)
    );
    let mut bad = Template::new(template("bad"), 0);
    bad.adjustments.speed = i16::MAX;
    let definition = species();
    let catalog = ScenarioCatalog {
        species: BTreeMap::from([(definition.id.clone(), definition)]),
        templates: BTreeMap::from([(bad.id.clone(), bad)]),
    };
    assert_eq!(
        Content::new(fixture::ruleset(), catalog),
        Err(BuildError::InvalidDefinition)
    );
}
#[test]
fn recorded_choices_restore_dormancy_and_validate_ownership() {
    let mut value = build(&vec![class("mage"); 4]);
    value = fixture::with_starting_skills(value, [Skill::Intimidation, Skill::Lore]);
    let mut root = fixture::talent("fixture_thinking_root", vec![]);
    root.requirement = Requirement::Flag {
        flag: FlagId::new("mindless").unwrap(),
        present: false,
    };
    let mut child = fixture::talent("fixture_thinking_child", vec![]);
    child.prerequisites = vec![root.id.clone()];
    value = fixture::with_talents(value, [root, child]);
    value
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_thinking_root"))
        .unwrap();
    value
        .select_talent(tor_simulation::HdIndex(1), talent("fixture_thinking_child"))
        .unwrap();
    value.set_templates(vec![template("zombified")]).unwrap();
    // Persistence retains original birth learning across later polymorph and
    // purchases; fresh from_recorded construction intentionally starts a birth.
    let restored = tor_simulation::creatures::record::BuildRecord::capture(&value)
        .restore(value.content().clone())
        .unwrap();
    assert_eq!(restored, value);
    assert_eq!(
        restored.derive().unwrap().dormant_talents,
        BTreeSet::from([
            talent("fixture_thinking_root"),
            talent("fixture_thinking_child")
        ])
    );
    let mut choices = value.choices().to_vec();
    choices[0].attribute = Some(Attribute::Strength);
    assert_eq!(
        CreatureBuild::from_recorded(
            value.content().clone(),
            value.species_id().clone(),
            value.initial_attributes(),
            value.ledger().clone(),
            value.binding(),
            tor_simulation::creatures::BuildAdvancement {
                choices,
                ..value.advancement().clone()
            },
            vec![]
        ),
        Err(BuildError::NoAttributeOpportunity)
    );
}
#[test]
fn conflicting_overrides_and_subtype_operations_reject_atomically() {
    let mut a = Template::new(template("a"), 0);
    a.overrides.intellect = Some(0);
    let mut b = Template::new(template("b"), 0);
    b.overrides.intellect = Some(2);
    let mut c = Template::new(template("c"), 0);
    c.add_subtypes.insert(SubtypeId::new("fire").unwrap());
    let mut d = Template::new(template("d"), 0);
    d.remove_subtypes.insert(SubtypeId::new("fire").unwrap());
    let mut value = build_with(species(), vec![a, b, c, d], &[HdSource::Racial]);
    let before = value.clone();
    for ids in [
        vec![template("a"), template("b")],
        vec![template("c"), template("d")],
    ] {
        assert_eq!(
            value.set_templates(ids),
            Err(BuildError::ConflictingTemplates)
        );
        assert_eq!(value, before);
    }
    assert_eq!(
        value.set_templates(vec![template("a"); 2]),
        Err(BuildError::InvalidDefinition)
    );
}
#[test]
fn aggregate_protection_limits_are_validated_during_build_derivation() {
    let selector = Selector::Category {
        category: tor_simulation::combat::DamageType::Energy,
    };
    let mut definition = species();
    definition.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Reduction {
            selector: selector.clone(),
            amount: 1_000_000,
        },
    });
    let mut excessive = Template::new(template("excessive"), 0);
    excessive.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Reduction {
            selector,
            amount: 1,
        },
    });
    let mut value = build_with(definition, vec![excessive], &[HdSource::Racial]);
    let before = value.clone();
    assert_eq!(
        value.set_templates(vec![template("excessive")]),
        Err(BuildError::DerivedLimit)
    );
    assert_eq!(value, before);
}
#[test]
fn individual_attribute_choices_do_not_redefine_the_species() {
    let mut strong = build(&vec![class("warrior"); 4]);
    let mut fast = strong.clone();
    let baseline = ByAttribute::from_fn(|_| 1);
    strong
        .set_initial_attributes(
            Attributes::new(ByAttribute {
                strength: 4,
                speed: 0,
                ..baseline
            })
            .unwrap(),
        )
        .unwrap();
    fast.set_initial_attributes(
        Attributes::new(ByAttribute {
            strength: 0,
            speed: 4,
            ..baseline
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(strong.species(), fast.species());
    let a = strong.derive().unwrap();
    let b = fast.derive().unwrap();
    assert_eq!(a.defenses.physical, b.defenses.physical);
    assert_eq!(a.maximum_health, b.maximum_health + 4);
    assert_eq!(a.resources.stamina, b.resources.stamina + 4);
}
#[test]
fn changing_starting_attributes_cannot_overflow_owned_advancement_choices() {
    let mut value = build(&vec![class("warrior"); 4]);
    value
        .increase_attribute(tor_simulation::HdIndex(3), Attribute::Strength)
        .unwrap();
    let before = value.clone();
    assert_eq!(
        value.set_initial_attributes(
            Attributes::new(ByAttribute {
                strength: 5,
                ..Default::default()
            })
            .unwrap()
        ),
        Err(BuildError::AttributeCap)
    );
    assert_eq!(value, before);
    let invalid = Attributes::new(ByAttribute::from_fn(|_| 5))
        .unwrap()
        .adjusted(ByAttribute {
            strength: 1,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        value.set_initial_attributes(invalid),
        Err(BuildError::AttributeCap)
    );
    assert_eq!(value, before);
}
#[test]
fn added_hit_die_preserves_retained_seeds_and_owned_choices() {
    let mut value = build(&[class("warrior"), class("mage")]);
    value
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    let before = value.clone();
    value.add_hit_die(HdSource::Racial, 123).unwrap();
    assert_eq!(&value.ledger().entries()[..2], before.ledger().entries());
    assert_eq!(&value.choices()[..2], before.choices());
    let seeded =
        HdLedger::seeded(vec![class("warrior"), class("mage"), HdSource::Racial], 123).unwrap();
    let actual = serde_json::to_value(tor_simulation::creatures::record::BuildRecord::capture(
        &value,
    ))
    .unwrap();
    // Retained rows have explicit seed 42; only the new ordinal uses root 123.
    // Pin acquisition facts without asking the build compiler to supply them.
    let mut expected = actual.clone();
    expected["hd"] = serde_json::json!([
        {
            "source": {"kind": "class", "id": "warrior"},
            "health_seed": 42,
            "health_origin": {"templates": [], "kind": "humanoid", "die": 10},
            "health_improvement": 0,
        },
        {
            "source": {"kind": "class", "id": "mage"},
            "health_seed": 42,
            "health_origin": {"templates": [], "kind": "humanoid", "die": 6},
            "health_improvement": 0,
        },
        {
            "source": {"kind": "racial"},
            "health_seed": seeded.entries()[2].health_seed(),
            "health_origin": {"templates": [], "kind": "humanoid", "die": 8},
            "health_improvement": 0,
        },
    ]);
    expected["health_root_seed"] = serde_json::json!(null);
    expected["drained_hd"] = serde_json::json!([]);
    for field in ["hd", "health_root_seed", "drained_hd"] {
        assert_eq!(actual[field], expected[field], "exact owned ledger {field}");
    }
    let expected: tor_simulation::creatures::record::BuildRecord =
        serde_json::from_value(expected).unwrap();
    let expected = expected.restore(value.content().clone()).unwrap();
    assert_eq!(value.ledger(), expected.ledger());
    value
        .train(tor_simulation::HdIndex(2), Skill::Lore)
        .unwrap();
    let trained = value.clone();
    assert_eq!(
        value.train(tor_simulation::HdIndex(2), Skill::Discipline),
        Err(BuildError::NoAdvancementPoint)
    );
    assert_eq!(value, trained);
    let removed = value.remove_latest().unwrap();
    assert_eq!(
        removed.choices.skills().collect::<Vec<_>>(),
        vec![Skill::Lore]
    );
    // Removal restores the prior build while preserving the exact dormant row.
    let mut expected = serde_json::to_value(
        tor_simulation::creatures::record::BuildRecord::capture(&before),
    )
    .unwrap();
    expected["drained_hd"] = serde_json::json!([{
        "ordinal": 2,
        "hit_die": {
            "source": {"kind": "racial"},
            "health_seed": seeded.entries()[2].health_seed(),
            "health_origin": {"templates": [], "kind": "humanoid", "die": 8},
            "health_improvement": 0,
        },
    }]);
    let expected: tor_simulation::creatures::record::BuildRecord =
        serde_json::from_value(expected).unwrap();
    let expected = expected.restore(before.content().clone()).unwrap();
    assert_eq!(value, expected);
    value.add_hit_die(HdSource::Racial, 123).unwrap();
    assert_eq!(value.ledger().entries()[2], removed.hit_die);
}
#[test]
fn adding_hit_dice_at_the_bound_rejects_without_mutation() {
    let mut value = build(&vec![class("warrior"); 255]);
    value.add_hit_die(class("mage"), 123).unwrap();
    let before = value.clone();
    assert_eq!(value.ledger().total_hd().get(), 256);
    assert!(value.add_hit_die(HdSource::Racial, 123).is_err());
    assert_eq!(value, before);
}
#[test]
fn new_talent_data_flows_through_the_same_fold_and_attribution() {
    let mut document = fixture::ruleset().document().clone();
    let id = talent("fixture_vitality");
    document.talents.push(TalentDef {
        form_choices: vec![],
        expertise_grants: vec![],
        id: id.clone(),
        name: "Fixture Vitality".into(),
        prerequisites: vec![],
        requirement: Requirement::All {
            requirements: vec![],
        },
        modifiers: vec![add(Stat::Health, 7)],
    });
    let definition = species();
    let species_id = definition.id.clone();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), definition)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut value = CreatureBuild::new(
        content,
        species_id,
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let before = value.derive().unwrap();
    value
        .select_talent(tor_simulation::HdIndex(0), id.clone())
        .unwrap();
    let after = value.derive().unwrap();
    assert_eq!(after.maximum_health, before.maximum_health + 7);
    assert_eq!(
        after.sources[&SourceId::Talent(id)],
        vec![add(Stat::Health, 7)]
    );
}
#[test]
fn unknown_references_fail_without_changing_the_build() {
    let mut value = build(&[HdSource::Racial]);
    let before = value.clone();
    assert_eq!(
        value.select_talent(tor_simulation::HdIndex(0), talent("missing")),
        Err(BuildError::UnknownTalent)
    );
    assert_eq!(value, before);
    assert_eq!(
        value.set_templates(vec![template("missing")]),
        Err(BuildError::UnknownTemplate)
    );
    assert_eq!(value, before);
    assert_eq!(
        value.add_hit_die(class("missing"), 42),
        Err(BuildError::UnknownClass)
    );
    assert_eq!(value, before);
}

#[test]
fn fresh_hit_dice_report_unknown_classes_before_binding_health_origins() {
    let value = build(&[HdSource::Racial]);
    for ledger in [
        HdLedger::seeded(vec![class("missing")], 42).unwrap(),
        HdLedger::new(vec![tor_simulation::progression::HitDie::new(
            class("missing"),
            42,
        )])
        .unwrap(),
    ] {
        assert_eq!(
            CreatureBuild::new(
                value.content().clone(),
                value.species_id().clone(),
                ledger.clone(),
                value.binding(),
                value.advancement().starting_skills,
            ),
            Err(BuildError::UnknownClass)
        );
        assert_eq!(
            CreatureBuild::from_recorded(
                value.content().clone(),
                value.species_id().clone(),
                value.initial_attributes(),
                ledger,
                value.binding(),
                value.advancement().clone(),
                vec![],
            ),
            Err(BuildError::UnknownClass)
        );
    }
}

#[test]
fn recorded_unknown_classes_remain_invalid_bound_provenance() {
    use tor_simulation::creatures::record::BuildRecord;

    for drained in [false, true] {
        let mut value = build(&[class("mage"), class("mage")]);
        if drained {
            value.remove_latest().unwrap();
        }
        let record = BuildRecord::capture(&value);
        assert_eq!(record.restore(value.content().clone()).unwrap(), value);
        let mut record = serde_json::to_value(record).unwrap();
        let row = if drained {
            &mut record["drained_hd"][0]["hit_die"]
        } else {
            &mut record["hd"][0]
        };
        row["source"] = serde_json::json!({"kind": "class", "id": "missing"});
        let record: BuildRecord = serde_json::from_value(record).unwrap();
        assert_eq!(
            record.restore(value.content().clone()),
            Err(BuildError::InvalidDefinition)
        );
    }
}

#[test]
fn unknown_classes_preserve_append_error_precedence_and_dormant_rows() {
    let fixture = build(&[HdSource::Racial]);
    let mut value = CreatureBuild::new(
        fixture.content().clone(),
        fixture.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial; 256], 42).unwrap(),
        fixture.binding(),
        fixture.advancement().starting_skills,
    )
    .unwrap();
    let full = value.clone();
    for root in [42, 43] {
        assert_eq!(
            value.add_hit_die(class("missing"), root),
            Err(BuildError::TooManyHitDice)
        );
        assert_eq!(value, full);
    }
    value.remove_latest().unwrap();
    let drained = value.clone();
    assert_eq!(
        value.add_hit_die(class("missing"), 43),
        Err(BuildError::InvalidHealthRoot)
    );
    assert_eq!(value, drained);
    assert_eq!(
        value.add_hit_die(class("missing"), 42),
        Err(BuildError::UnknownClass)
    );
    assert_eq!(value, drained);
    value.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(value, full);

    let mut advancement = fixture.advancement().clone();
    advancement.restoration = tor_simulation::creatures::RestorationPolicy::Authored(vec![
        tor_simulation::creatures::PlannedAdvancement {
            source: HdSource::Racial,
            choices: fixture.choices()[0].clone(),
        },
    ]);
    let mut value = CreatureBuild::from_recorded(
        fixture.content().clone(),
        fixture.species_id().clone(),
        fixture.initial_attributes(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        fixture.binding(),
        advancement,
        vec![],
    )
    .unwrap();
    value.remove_latest().unwrap();
    let before = value.clone();
    for root in [42, 43] {
        assert_eq!(
            value.add_hit_die(class("missing"), root),
            Err(BuildError::InvalidOwnership)
        );
        assert_eq!(value, before);
    }
    value.restore_hit_die(42).unwrap();
}

#[test]
fn validated_content_rejects_oversized_species_and_template_catalogs() {
    for templates in [false, true] {
        let mut catalog = ScenarioCatalog::default();
        for index in 0..=tor_simulation::limits::CATALOG_ENTRIES {
            let name = format!("entry_{index}");
            if templates {
                let value = Template::new(TemplateId::new(name).unwrap(), 0);
                catalog.templates.insert(value.id.clone(), value);
            } else {
                let mut value = species();
                value.id = tor_simulation::content_ids::SpeciesId::new(name).unwrap();
                catalog.species.insert(value.id.clone(), value);
            }
        }
        assert_eq!(
            Content::new(fixture::ruleset(), catalog).unwrap_err(),
            BuildError::InvalidDefinition
        );
    }
}

#[test]
fn builds_reuse_catalog_identity_allocations_after_loading_and_edits() {
    let definition = species();
    let id = definition.id.clone();
    let mut form = Template::new(template("shared_form"), 0);
    form.adjustments.strength = 1;
    let content = Content::new(
        fixture::ruleset(),
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), definition)]),
            templates: BTreeMap::from([(form.id.clone(), form)]),
        },
    )
    .unwrap();
    let mut value = CreatureBuild::new(
        content.clone(),
        tor_simulation::content_ids::SpeciesId::new(id.as_str()).unwrap(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    assert!(std::ptr::eq(
        value.species_id().as_str(),
        content
            .catalog()
            .species
            .get_key_value(&id)
            .unwrap()
            .0
            .as_str()
    ));
    value.set_templates(vec![template("shared_form")]).unwrap();
    assert!(std::ptr::eq(
        value.template_ids()[0].as_str(),
        content
            .catalog()
            .templates
            .get_key_value(&template("shared_form"))
            .unwrap()
            .0
            .as_str()
    ));
}

#[test]
fn validated_content_rejects_unknown_natural_damage_descriptors() {
    for (name, valid) in [("fire", true), ("missing_descriptor", false)] {
        let mut definition = species();
        definition.melee = tor_simulation::attacks::NaturalAttack::from_melee(
            tor_simulation::attacks::MeleeAttack::fixed(
                Skill::HeavyWeaponry,
                0,
                60,
                40,
                tor_simulation::combat::DamageType::Energy,
                Some(DescriptorId::new(name).unwrap()),
                3,
            )
            .unwrap(),
        )
        .unwrap();
        let catalog = ScenarioCatalog {
            species: BTreeMap::from([(definition.id.clone(), definition)]),
            templates: BTreeMap::new(),
        };
        assert_eq!(Content::new(fixture::ruleset(), catalog).is_ok(), valid);
    }
}

#[test]
fn cached_health_provenance_tracks_type_changes_and_retains_owned_seeds() {
    let mut build = build(&[HdSource::Racial, HdSource::Racial, class("mage")]);
    let original = build.derive().unwrap();
    assert_eq!(
        original
            .health_contributions
            .iter()
            .map(|value| (value.die, value.health))
            .collect::<Vec<_>>(),
        vec![(8, 8), (8, 6), (6, 2)]
    );
    let seeds: Vec<_> = build
        .ledger()
        .entries()
        .iter()
        .map(|die| die.health_seed())
        .collect();
    build.set_templates(vec![template("zombified")]).unwrap();
    let changed = build.derive().unwrap();
    assert_eq!(
        changed
            .health_contributions
            .iter()
            .map(|value| (value.die, value.health))
            .collect::<Vec<_>>(),
        vec![(12, 12), (12, 2), (6, 2)]
    );
    assert_eq!(
        build
            .ledger()
            .entries()
            .iter()
            .map(|die| die.health_seed())
            .collect::<Vec<_>>(),
        seeds
    );
    build.set_templates(vec![]).unwrap();
    assert_eq!(
        build.derive().unwrap().health_contributions,
        original.health_contributions
    );
}

#[test]
fn authored_weapon_tag_applies_existing_talents_to_new_abilities_and_keeps_provenance() {
    let mut document = fixture::ruleset().document().clone();
    let mut custom = document
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "basic_melee")
        .unwrap()
        .clone();
    custom.id = ability("fixture_sweep");
    custom.name = "Fixture sweep".into();
    custom.basic_attack = false;
    document.constants.default_abilities.push(custom.id.clone());
    document.abilities.push(custom.clone());
    custom.id = ability("untagged_sweep");
    custom.tags.clear();
    document.constants.default_abilities.push(custom.id.clone());
    document.abilities.push(custom);
    let mut previous = None;
    for (id, stat, minimum) in [
        (
            "fixture_weapon_flat",
            tor_simulation::rules::AbilityStat::FlatBonus,
            1,
        ),
        (
            "fixture_weapon_die_one",
            tor_simulation::rules::AbilityStat::Dice,
            4,
        ),
        (
            "fixture_weapon_die_two",
            tor_simulation::rules::AbilityStat::Dice,
            8,
        ),
        (
            "fixture_weapon_die_three",
            tor_simulation::rules::AbilityStat::Dice,
            12,
        ),
    ] {
        let mut probe = fixture_weapon_bonus(id, stat, 1);
        probe.prerequisites = previous.into_iter().collect();
        probe.requirement = Requirement::TotalHd {
            minimum: tor_simulation::HdCount(minimum),
        };
        previous = Some(probe.id.clone());
        document.talents.push(probe);
    }
    let rules = Ruleset::validate(document).unwrap();
    let species = species();
    let content = Content::new(
        rules,
        ScenarioCatalog {
            species: BTreeMap::from([(species.id.clone(), species.clone())]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut build = CreatureBuild::new(
        content,
        species.id,
        HdLedger::seeded(vec![HdSource::Racial; 12], 42).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    for (owner, talent) in [
        "fixture_weapon_flat",
        "fixture_weapon_die_one",
        "fixture_weapon_die_two",
        "fixture_weapon_die_three",
    ]
    .into_iter()
    .enumerate()
    {
        build
            .select_talent(
                tor_simulation::HdIndex::from_usize(owner).unwrap(),
                TalentId::new(talent).unwrap(),
            )
            .unwrap();
    }
    let derived = build.derive().unwrap();
    for id in ["basic_melee", "fixture_sweep"] {
        let values = derived.ability_values(&ability(id));
        assert_eq!((values.flat, values.dice), (1, 3));
    }
    let values = derived.ability_values(&ability("untagged_sweep"));
    assert_eq!((values.flat, values.dice), (0, 0));
    assert!(
        matches!(&derived.sources[&SourceId::Talent(talent("fixture_weapon_flat"))][0].op,
        ModifierOp::Add { stat: Stat::AbilityTag { tag, stat: tor_simulation::rules::AbilityStat::FlatBonus }, value: 1 }
        if tag.as_str() == "weapon_attack")
    );
}

#[test]
fn tag_and_specific_ability_override_conflicts_report_modifier_error_atomically() {
    let mut document = fixture::ruleset().document().clone();
    document.talents.push(fixture::talent(
        "fixture_ability_override",
        vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::Ability {
                    ability: ability("basic_melee"),
                    stat: tor_simulation::rules::AbilityStat::FlatBonus,
                },
                value: 6,
            },
        }],
    ));
    let mut species = species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::AbilityTag {
                tag: AbilityTagId::new("weapon_attack").unwrap(),
                stat: tor_simulation::rules::AbilityStat::FlatBonus,
            },
            value: 5,
        },
    });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(species.id.clone(), species.clone())]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut build = CreatureBuild::new(
        content,
        species.id,
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let before = build.clone();
    assert_eq!(
        build.select_talent(
            tor_simulation::HdIndex(0),
            talent("fixture_ability_override")
        ),
        Err(BuildError::ConflictingOverride)
    );
    assert_eq!(build, before);
}

#[test]
fn default_one_hd_cannot_buy_a_talent_and_a_skill_rank() {
    let mut value = build(&[class("warrior")]);
    value = fixture::with_talents(value, [fixture_health()]);
    value
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_health"))
        .unwrap();
    let before = value.clone();
    assert!(value
        .train(tor_simulation::HdIndex(0), Skill::Athletics)
        .is_err());
    assert_eq!(value, before);
}
