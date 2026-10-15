//! The playable starter catalog uses ordinary permanent gates and owned forms.
use super::creature_fixture;
use std::collections::{BTreeMap, BTreeSet};
use tor_simulation::{
    attributes::{Attribute, ManaBinding, Skill, SkillRanks},
    content_ids::{AbilityId, ClassId, FlagId, FormId, PointPoolId, SubtypeId, TalentId, TypeId},
    creatures::{
        record::CreatureRecord, Content, CreatureBuild, CreatureState, PurchaseSelection,
        ScenarioCatalog,
    },
    progression::{HdLedger, HdSource},
    rules::{Requirement, SkillSelector, TalentFacts},
};

const TALENTS: [&str; 12] = [
    "brace_for_impact",
    "measured_cut",
    "rattling_blow",
    "guarded_step",
    "read_the_rhythm",
    "dislocating_strike",
    "open_the_guard",
    "evocation_initiate",
    "binding_initiate",
    "warding_initiate",
    "steady_recovery",
    "hardy_recovery",
];
const NATURAL_TALENTS: [&str; 2] = ["second_strike", "third_strike"];
const FORMS: [(&str, &[&str]); 3] = [
    (
        "evocation_initiate",
        &["spark_lance_fire", "spark_lance_cold"],
    ),
    ("binding_initiate", &["falter"]),
    (
        "warding_initiate",
        &["shell_impact", "shell_keen", "shell_energy"],
    ),
];
fn talent(value: &str) -> TalentId {
    TalentId::new(value).unwrap()
}
fn ability(value: &str) -> AbilityId {
    AbilityId::new(value).unwrap()
}
fn class(value: &str) -> HdSource {
    HdSource::Class(ClassId::new(value).unwrap())
}

struct Facts {
    total: u16,
    warrior: u16,
    mage: u16,
    skills: SkillRanks,
    kind: TypeId,
}
impl TalentFacts for Facts {
    fn total_hd(&self) -> tor_simulation::HdCount {
        tor_simulation::HdCount(u64::from(self.total))
    }
    fn racial_hd(&self) -> tor_simulation::HdCount {
        tor_simulation::HdCount(u64::from(self.total - self.warrior - self.mage))
    }
    fn class_level(&self, id: &ClassId) -> tor_simulation::HdCount {
        tor_simulation::HdCount(u64::from(match id.as_str() {
            "warrior" => self.warrior,
            "mage" => self.mage,
            _ => 0,
        }))
    }
    fn creature_type(&self) -> &TypeId {
        &self.kind
    }
    fn has_subtype(&self, _: &SubtypeId) -> bool {
        false
    }
    fn species_has_flag(&self, _: &FlagId) -> bool {
        false
    }
    fn skill_rank(&self, skill: &SkillSelector) -> u16 {
        match skill {
            SkillSelector::Named { skill } => self.skills.get(*skill).into(),
            SkillSelector::NaturalAttack | SkillSelector::SelectedWeapon => 0,
        }
    }
    fn attribute(&self, _: Attribute) -> u16 {
        1
    }
    fn has_flag(&self, _: &FlagId) -> bool {
        false
    }
}
fn facts(total: u16, warrior: u16, mage: u16, ranks: &[(Skill, u8)]) -> Facts {
    let mut skills = SkillRanks::default();
    for &(skill, rank) in ranks {
        skills = skills.with_rank(skill, rank).unwrap();
    }
    Facts {
        total,
        warrior,
        mage,
        skills,
        kind: TypeId::new("humanoid").unwrap(),
    }
}
fn active(id: &str, parents: &[&str], facts: &Facts) -> bool {
    let selected = parents.iter().copied().chain([id]).map(talent).collect();
    creature_fixture::ruleset()
        .active_talents(&selected, facts)
        .contains(&talent(id))
}
fn dependencies<'a>(requirement: &'a Requirement, result: &mut Vec<&'a TalentId>) {
    match requirement {
        Requirement::All { requirements } | Requirement::Any { requirements } => {
            for child in requirements {
                dependencies(child, result);
            }
        }
        Requirement::OwnedTalent { talent } => result.push(talent),
        _ => {}
    }
}

#[test]
fn catalog_contains_twelve_starter_and_two_natural_talents_six_forms_fifteen_abilities() {
    let rules = creature_fixture::ruleset();
    let actual: BTreeSet<_> = rules
        .document()
        .talents
        .iter()
        .map(|row| row.id.as_str())
        .collect();
    assert_eq!(
        actual,
        TALENTS
            .into_iter()
            .chain(NATURAL_TALENTS)
            .collect::<BTreeSet<_>>(),
        "catalog must contain twelve starter and two explicit natural unlock talents"
    );
    assert_eq!(rules.document().forms.len(), 6);
    assert_eq!(rules.document().abilities.len(), 15);
    let mut seen = BTreeSet::new();
    for id in rules.talent_order() {
        let definition = rules.talent(id).unwrap();
        let mut required: Vec<_> = definition.prerequisites.iter().collect();
        dependencies(&definition.requirement, &mut required);
        assert!(
            required.into_iter().all(|parent| seen.contains(parent)),
            "{id:?}"
        );
        assert!(seen.insert(id.clone()));
    }
    assert_eq!(seen.len(), TALENTS.len() + NATURAL_TALENTS.len());
}

#[test]
fn warrior_roots_require_their_own_permanent_rank_and_never_racial_substitutes() {
    for (root, rank) in [
        ("brace_for_impact", Skill::Discipline),
        ("measured_cut", Skill::HeavyWeaponry),
        ("rattling_blow", Skill::LightWeaponry),
    ] {
        assert!(
            active(root, &[], &facts(1, 1, 0, &[(rank, 1)])),
            "starter catalog independent Warrior root {root} must unlock"
        );
        assert!(!active(root, &[], &facts(1, 1, 0, &[])), "{root}");
        assert!(!active(root, &[], &facts(20, 0, 0, &[(rank, 5)])), "{root}");
        assert!(
            !active(root, &[], &facts(20, 0, 20, &[(rank, 5)])),
            "{root}"
        );
    }
    for root in ["measured_cut", "rattling_blow"] {
        for skill in [Skill::HeavyWeaponry, Skill::LightWeaponry] {
            assert!(active(root, &[], &facts(1, 1, 0, &[(skill, 1)])));
        }
        assert!(!active(
            root,
            &[],
            &facts(1, 1, 0, &[(Skill::Discipline, 5)])
        ));
    }
}

#[test]
fn warrior_second_row_accepts_each_active_or_parent_at_total_six_class_three() {
    let ranks = [(Skill::Discipline, 1), (Skill::HeavyWeaponry, 1)];
    for (root, parents) in [
        ("guarded_step", ["brace_for_impact", "measured_cut"]),
        ("read_the_rhythm", ["measured_cut", "rattling_blow"]),
        ("dislocating_strike", ["brace_for_impact", "rattling_blow"]),
    ] {
        for parent in parents {
            assert!(
                active(root, &[parent], &facts(6, 3, 0, &ranks)),
                "starter catalog second-row {root} must unlock via {parent}"
            );
            assert!(!active(root, &[parent], &facts(5, 3, 0, &ranks)));
            assert!(!active(root, &[parent], &facts(16, 2, 0, &ranks)));
            assert!(!active(root, &[parent], &facts(6, 3, 0, &[])));
        }
        assert!(!active(root, &[], &facts(6, 3, 0, &ranks)));
    }
}

#[test]
fn open_the_guard_keeps_total_eleven_class_six_and_either_active_route() {
    let ranks = [(Skill::Discipline, 1), (Skill::HeavyWeaponry, 1)];
    for parents in [
        ["brace_for_impact", "guarded_step"],
        ["rattling_blow", "dislocating_strike"],
    ] {
        assert!(
            active("open_the_guard", &parents, &facts(11, 6, 0, &ranks)),
            "starter catalog Open the Guard must unlock at HD11 Warrior6"
        );
        assert!(!active(
            "open_the_guard",
            &parents,
            &facts(10, 6, 0, &ranks)
        ));
        assert!(!active(
            "open_the_guard",
            &parents,
            &facts(16, 5, 0, &ranks)
        ));
        assert!(!active(
            "open_the_guard",
            &parents[1..],
            &facts(11, 6, 0, &ranks)
        ));
    }
}

#[test]
fn recovery_roots_keep_survival_and_the_eleven_hd_upgrade_gate() {
    assert!(
        active(
            "steady_recovery",
            &[],
            &facts(1, 0, 0, &[(Skill::Survival, 1)])
        ),
        "starter catalog Steady Recovery must unlock at HD1 Survival1"
    );
    assert!(!active("steady_recovery", &[], &facts(11, 0, 0, &[])));
    assert!(active(
        "hardy_recovery",
        &["steady_recovery"],
        &facts(11, 0, 0, &[(Skill::Survival, 2)])
    ));
    for (hd, rank) in [(10, 2), (11, 1)] {
        assert!(!active(
            "hardy_recovery",
            &["steady_recovery"],
            &facts(hd, 0, 0, &[(Skill::Survival, rank)])
        ));
    }
    assert!(!active(
        "hardy_recovery",
        &[],
        &facts(11, 0, 0, &[(Skill::Survival, 2)])
    ));
}

fn mage_build(sources: Vec<HdSource>) -> CreatureBuild {
    let species = creature_fixture::unmodified_species("starter_form_subject");
    let id = species.id.clone();
    let content = Content::new(
        creature_fixture::ruleset(),
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    CreatureBuild::new(
        content,
        id,
        HdLedger::seeded(sources, 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Spellcasting, Skill::HeavyWeaponry],
    )
    .unwrap()
}
fn select_form(build: &mut CreatureBuild, root: &str, form: &str) {
    build
        .purchase(
            tor_simulation::HdIndex(0),
            PointPoolId::new("advancement").unwrap(),
            PurchaseSelection::TalentForm {
                talent: talent(root),
                form: FormId::new(form).unwrap(),
            },
        )
        .unwrap();
}

#[test]
fn each_mage_root_independently_buys_exactly_its_selected_immutable_form() {
    let rules = creature_fixture::ruleset();
    for (root, forms) in FORMS {
        assert!(
            rules.talent(&talent(root)).is_some(),
            "starter catalog independent Mage root {root} must exist"
        );
        let definition = rules.talent(&talent(root)).unwrap();
        assert_eq!(
            definition
                .form_choices
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            forms
        );
        assert!(
            definition.modifiers.is_empty(),
            "the acquired form owns the ability grant"
        );
        assert!(active(
            root,
            &[],
            &facts(1, 0, 1, &[(Skill::Spellcasting, 1)])
        ));
        assert!(!active(
            root,
            &[],
            &facts(16, 0, 0, &[(Skill::Spellcasting, 5)])
        ));
        assert!(!active(root, &[], &facts(1, 0, 1, &[])));
        for &selected in forms {
            let mut build = mage_build(vec![class("mage")]);
            let before = build.clone();
            assert!(build
                .select_talent(tor_simulation::HdIndex(0), talent(root))
                .is_err());
            assert_eq!(build, before);
            for &foreign in FORMS
                .iter()
                .filter(|(other, _)| *other != root)
                .flat_map(|(_, forms)| *forms)
            {
                assert!(build
                    .purchase(
                        tor_simulation::HdIndex(0),
                        PointPoolId::new("advancement").unwrap(),
                        PurchaseSelection::TalentForm {
                            talent: talent(root),
                            form: FormId::new(foreign).unwrap()
                        }
                    )
                    .is_err());
                assert_eq!(build, before);
            }
            select_form(&mut build, root, selected);
            assert_eq!(build.unspent_points(), 0);
            for &known in FORMS.iter().flat_map(|(_, forms)| *forms) {
                assert_eq!(
                    build.derive().unwrap().abilities.contains(&ability(known)),
                    known == selected
                );
            }
        }
    }
}

#[test]
fn acquired_falter_survives_class_dormancy_return_and_named_record_restore() {
    assert!(
        creature_fixture::ruleset()
            .talent(&talent("binding_initiate"))
            .is_some(),
        "starter catalog Falter acquisition root must exist before ownership roundtrip"
    );
    let mut build = mage_build(vec![HdSource::Racial, class("mage")]);
    select_form(&mut build, "binding_initiate", "falter");
    let choice = build.choices()[0].clone();
    build.remove_latest().unwrap();
    assert_eq!(build.choices()[0], choice);
    assert!(build
        .derive()
        .unwrap()
        .dormant_talents
        .contains(&talent("binding_initiate")));
    assert!(!build
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("falter")));
    let creature = CreatureState::new(build).unwrap();
    let bytes = serde_json::to_vec(&CreatureRecord::capture(&creature)).unwrap();
    let record: CreatureRecord = serde_json::from_slice(&bytes).unwrap();
    let restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
    let mut returned = restored.build().clone();
    returned.add_hit_die(class("mage"), 42).unwrap();
    assert_eq!(returned.choices()[0], choice);
    assert!(returned
        .derive()
        .unwrap()
        .abilities
        .contains(&ability("falter")));
}
