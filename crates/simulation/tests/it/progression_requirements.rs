//! Progression requirements are permanent, validated composition facts.
use super::creature_fixture as fixture;
use std::collections::BTreeMap;
use tor_simulation::attributes::ManaBinding;
use tor_simulation::content_ids::{FlagId, SubtypeId, TalentId};
use tor_simulation::creatures::{Content, CreatureBuild, ScenarioCatalog};
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::rules::{Modifier, ModifierOp, Requirement, Ruleset, Scope, TalentDef};

fn id(value: &str) -> TalentId {
    TalentId::new(value).unwrap()
}

fn requirement(value: serde_json::Value) -> Requirement {
    serde_json::from_value(value).expect("the progression requirement must be supported")
}

fn node(name: &str, requirement: Requirement) -> TalentDef {
    TalentDef {
        form_choices: vec![],
        expertise_grants: vec![],
        id: id(name),
        name: name.into(),
        prerequisites: vec![],
        requirement,
        modifiers: vec![],
    }
}

#[test]
fn progression_requirements_or_parents_follow_active_permanent_ownership() {
    let mut document = fixture::ruleset().document().clone();
    document.talents.extend([
        node(
            "root_a",
            Requirement::TotalHd {
                minimum: tor_simulation::HdCount(3),
            },
        ),
        node(
            "root_b",
            Requirement::TotalHd {
                minimum: tor_simulation::HdCount(1),
            },
        ),
        node(
            "child",
            requirement(serde_json::json!({
                "kind": "any", "requirements": [
                    {"kind": "owned_talent", "talent": "root_a"},
                    {"kind": "owned_talent", "talent": "root_b"}
                ]
            })),
        ),
    ]);
    let species = fixture::species();
    let species_id = species.id.clone();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut build = CreatureBuild::new(
        content,
        species_id,
        HdLedger::seeded(vec![HdSource::Racial; 3], 42).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    assert!(!build.available_talents().unwrap().contains(&id("child")));
    build
        .select_talent(tor_simulation::HdIndex(1), id("root_a"))
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(0), id("child"))
        .unwrap();
    assert!(build
        .derive()
        .unwrap()
        .active_talents
        .contains(&id("child")));
    build.remove_latest().unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.dormant_talents.contains(&id("root_a")));
    assert!(derived.dormant_talents.contains(&id("child")));
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    build
        .select_talent(tor_simulation::HdIndex(2), id("root_b"))
        .unwrap();
    build.remove_latest().unwrap();
    assert!(build
        .derive()
        .unwrap()
        .dormant_talents
        .contains(&id("child")));

    let mut alternate = build.clone();
    alternate.remove_latest().unwrap();
    alternate.add_hit_die(HdSource::Racial, 42).unwrap();
    alternate
        .select_talent(tor_simulation::HdIndex(1), id("root_b"))
        .unwrap();
    assert!(alternate
        .derive()
        .unwrap()
        .active_talents
        .contains(&id("child")));
    let recorded = tor_simulation::creatures::record::BuildRecord::capture(&alternate);
    let restored = recorded.restore(alternate.content().clone()).unwrap();
    assert_eq!(restored, alternate);
    assert!(restored
        .derive()
        .unwrap()
        .active_talents
        .contains(&id("child")));
}

#[test]
fn progression_requirements_catalog_subtype_and_species_flag_are_distinct() {
    let mut document = fixture::ruleset().document().clone();
    document.talents.push(node(
        "species_gate",
        requirement(serde_json::json!({
            "kind": "all", "requirements": [
                {"kind": "subtype_in", "subtypes": ["goblinoid"]},
                {"kind": "species_flag", "flag": "magical", "present": true}
            ]
        })),
    ));
    let rules = Ruleset::validate(document).unwrap();
    for species_flag in [false, true] {
        let mut species = fixture::species();
        species
            .subtypes
            .insert(SubtypeId::new("goblinoid").unwrap());
        if species_flag {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Flag {
                    flag: FlagId::new("magical").unwrap(),
                },
            });
        }
        let species_id = species.id.clone();
        let content = Content::new(
            rules.clone(),
            ScenarioCatalog {
                species: BTreeMap::from([(species_id.clone(), species)]),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
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
        assert_eq!(
            build
                .available_talents()
                .unwrap()
                .contains(&id("species_gate")),
            species_flag
        );
    }
}

#[test]
fn progression_requirements_validate_references_and_cycles_in_any_branches() {
    for value in [
        serde_json::json!({"kind": "owned_talent", "talent": "missing"}),
        serde_json::json!({"kind": "subtype_in", "subtypes": ["missing"]}),
        serde_json::json!({"kind": "subtype_in", "subtypes": []}),
        serde_json::json!({"kind": "subtype_in", "subtypes": ["goblinoid", "goblinoid"]}),
        serde_json::json!({"kind": "species_flag", "flag": "missing", "present": true}),
    ] {
        let mut document = fixture::ruleset().document().clone();
        document.talents.push(node("invalid", requirement(value)));
        assert!(Ruleset::validate(document).is_err());
    }
    let mut document = fixture::ruleset().document().clone();
    document.talents.extend([
        node(
            "cycle_a",
            requirement(serde_json::json!({"kind": "owned_talent", "talent": "cycle_b"})),
        ),
        node(
            "cycle_b",
            requirement(serde_json::json!({"kind": "any", "requirements": [
                {"kind": "total_hd", "minimum": 1},
                {"kind": "owned_talent", "talent": "cycle_a"}
            ]})),
        ),
    ]);
    assert!(Ruleset::validate(document).is_err());
}
