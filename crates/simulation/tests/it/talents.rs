use super::creature_fixture::ruleset;
use std::collections::BTreeSet;
use tor_simulation::attributes::{Attribute, Skill, SkillRanks};
use tor_simulation::content_ids::{ClassId, FlagId, SubtypeId, TalentId, TypeId};
use tor_simulation::progression::{HdLedger, HdSource, HitDie};
use tor_simulation::rules::{Requirement, Ruleset, SkillSelector, TalentDef, TalentFacts};
fn id(value: &str) -> TalentId {
    TalentId::new(value).unwrap()
}
fn make_ledger(racial: usize, mage: usize, warrior: usize) -> HdLedger {
    HdLedger::new(
        std::iter::repeat_n(HdSource::Racial, racial)
            .chain(std::iter::repeat_n(
                HdSource::Class(ClassId::new("mage").unwrap()),
                mage,
            ))
            .chain(std::iter::repeat_n(
                HdSource::Class(ClassId::new("warrior").unwrap()),
                warrior,
            ))
            .map(|source| HitDie::new(source, 42))
            .collect(),
    )
    .unwrap()
}
struct Facts<'a> {
    ledger: &'a HdLedger,
    kind: TypeId,
    skills: SkillRanks,
    mindless: bool,
}
impl TalentFacts for Facts<'_> {
    fn total_hd(&self) -> u16 {
        self.ledger.total_hd()
    }
    fn racial_hd(&self) -> u16 {
        self.ledger.racial_hd()
    }
    fn class_level(&self, id: &ClassId) -> u16 {
        self.ledger.class_level(id)
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
        self.skills
            .get(match skill {
                SkillSelector::Named { skill } => *skill,
                SkillSelector::NaturalAttack | SkillSelector::SelectedWeapon => {
                    Skill::HeavyWeaponry
                }
            })
            .into()
    }
    fn attribute(&self, _: Attribute) -> u16 {
        1
    }
    fn has_flag(&self, flag: &FlagId) -> bool {
        match flag.as_str() {
            "mindless" => self.mindless,
            "magical" => self.ledger.class_level(&ClassId::new("mage").unwrap()) > 0,
            _ => false,
        }
    }
}
fn facts(ledger: &HdLedger) -> Facts<'_> {
    Facts {
        ledger,
        kind: TypeId::new("humanoid").unwrap(),
        skills: SkillRanks::default()
            .with_rank(Skill::Discipline, 1)
            .unwrap()
            .with_rank(Skill::Spellcasting, 1)
            .unwrap()
            .with_rank(Skill::Intimidation, 1)
            .unwrap()
            .with_rank(Skill::HeavyWeaponry, 1)
            .unwrap(),
        mindless: false,
    }
}
#[test]
fn class_prerequisites_are_local_not_total_advancement() {
    let ledger = make_ledger(15, 0, 1);
    let facts = facts(&ledger);
    let selected = BTreeSet::from([
        id("brace_for_impact"),
        id("guarded_step"),
        id("open_the_guard"),
    ]);
    assert_eq!(
        ruleset().active_talents(&selected, &facts),
        BTreeSet::from([id("brace_for_impact")])
    );
}
fn local_talent(name: &str, requirement: Requirement, prerequisites: Vec<TalentId>) -> TalentDef {
    TalentDef {
        id: id(name),
        name: name.into(),
        requirement,
        prerequisites,
        modifiers: vec![],
        form_choices: vec![],
    }
}
#[test]
fn transitive_fixture_chain_needs_active_predecessors_and_total_hd() {
    // Only graph eligibility is under test: no permanent Health chain is shipped.
    let names = [
        "fixture_chain_first",
        "fixture_chain_second",
        "fixture_chain_third",
        "fixture_chain_fourth",
    ];
    let mut document = ruleset().document().clone();
    for (index, minimum) in [1, 4, 8, 12].into_iter().enumerate() {
        document.talents.push(local_talent(
            names[index],
            Requirement::TotalHd { minimum },
            if index == 0 {
                vec![]
            } else {
                vec![id(names[index - 1])]
            },
        ));
    }
    let rules = Ruleset::validate(document).unwrap();
    let selected = names.map(id).into();
    for (hd, expected) in [(1, 1), (4, 2), (8, 3), (12, 4)] {
        let ledger = make_ledger(hd, 0, 0);
        assert_eq!(
            rules.active_talents(&selected, &facts(&ledger)).len(),
            expected
        );
    }
    let ledger = make_ledger(16, 0, 0);
    assert!(rules
        .active_talents(&BTreeSet::from([id(names[3])]), &facts(&ledger))
        .is_empty());
}
#[test]
fn racial_fixture_eligibility_depends_on_type_and_training() {
    // This generic predicate fixture does not grant a racial route into a class root.
    let requirement = Requirement::All {
        requirements: vec![
            Requirement::SkillRank {
                skill: SkillSelector::Named {
                    skill: Skill::HeavyWeaponry,
                },
                minimum: 1,
            },
            Requirement::Any {
                requirements: vec![
                    Requirement::ClassLevel {
                        class: ClassId::new("warrior").unwrap(),
                        minimum: 1,
                    },
                    Requirement::All {
                        requirements: vec![
                            Requirement::RacialHd { minimum: 2 },
                            Requirement::TypeIn {
                                types: vec![
                                    TypeId::new("humanoid").unwrap(),
                                    TypeId::new("animal").unwrap(),
                                ],
                            },
                        ],
                    },
                ],
            },
        ],
    };
    let mut document = ruleset().document().clone();
    document.talents.push(local_talent(
        "fixture_racial_technique",
        requirement,
        vec![],
    ));
    let rules = Ruleset::validate(document).unwrap();
    let ledger = make_ledger(2, 0, 0);
    let mut facts = facts(&ledger);
    let def = rules.talent(&id("fixture_racial_technique")).unwrap();
    assert!(def.requirement.satisfied(&facts, &BTreeSet::new()));
    facts.kind = TypeId::new("animal").unwrap();
    assert!(def.requirement.satisfied(&facts, &BTreeSet::new()));
    facts.kind = TypeId::new("undead").unwrap();
    assert!(!def.requirement.satisfied(&facts, &BTreeSet::new()));
    facts.kind = TypeId::new("animal").unwrap();
    facts.skills = SkillRanks::default();
    assert!(!def.requirement.satisfied(&facts, &BTreeSet::new()));
    let warrior = make_ledger(0, 0, 1);
    facts.ledger = &warrior;
    facts.kind = TypeId::new("undead").unwrap();
    facts.skills = SkillRanks::default()
        .with_rank(Skill::HeavyWeaponry, 1)
        .unwrap();
    assert!(def.requirement.satisfied(&facts, &BTreeSet::new()));
}
#[test]
fn permanent_flag_fixture_becomes_dormant_and_reactivates_without_replacing_choices() {
    let mut document = ruleset().document().clone();
    document.talents.push(local_talent(
        "fixture_thinking_root",
        Requirement::Flag {
            flag: FlagId::new("mindless").unwrap(),
            present: false,
        },
        vec![],
    ));
    document.talents.push(local_talent(
        "fixture_thinking_child",
        Requirement::TotalHd { minimum: 4 },
        vec![id("fixture_thinking_root")],
    ));
    let rules = Ruleset::validate(document).unwrap();
    let ledger = make_ledger(0, 4, 0);
    let mut facts = facts(&ledger);
    let selected = BTreeSet::from([id("fixture_thinking_root"), id("fixture_thinking_child")]);
    assert_eq!(rules.active_talents(&selected, &facts), selected);
    facts.mindless = true;
    assert!(rules.active_talents(&selected, &facts).is_empty());
    facts.mindless = false;
    assert_eq!(rules.active_talents(&selected, &facts), selected);
}
