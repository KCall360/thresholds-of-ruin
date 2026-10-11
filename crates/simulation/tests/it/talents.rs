use super::creature_fixture::ruleset;
use std::collections::BTreeSet;
use tor_simulation::attributes::{Attribute, Skill, SkillRanks};
use tor_simulation::content_ids::{ClassId, FlagId, TalentId, TypeId};
use tor_simulation::progression::{HdLedger, HdSource, HitDie};
use tor_simulation::rules::{SkillSelector, TalentFacts};
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
    let ledger = make_ledger(15, 1, 0);
    let facts = facts(&ledger);
    let selected = BTreeSet::from([id("magic_bolt"), id("potent_bolt"), id("empowered_bolt")]);
    assert_eq!(
        ruleset().active_talents(&selected, &facts),
        BTreeSet::from([id("magic_bolt")])
    );
}
#[test]
fn passive_upgrade_chains_need_active_predecessors_and_total_hd() {
    let selected = BTreeSet::from([
        id("hardiness"),
        id("toughness"),
        id("unyielding"),
        id("indomitable"),
    ]);
    for (hd, expected) in [(1, 1), (4, 2), (8, 3), (12, 4)] {
        let ledger = make_ledger(hd, 0, 0);
        assert_eq!(
            ruleset().active_talents(&selected, &facts(&ledger)).len(),
            expected
        );
    }
    let ledger = make_ledger(16, 0, 0);
    assert!(ruleset()
        .active_talents(&BTreeSet::from([id("indomitable")]), &facts(&ledger))
        .is_empty());
}
#[test]
fn racial_technique_eligibility_depends_on_type_and_training() {
    let ledger = make_ledger(2, 0, 0);
    let mut facts = facts(&ledger);
    let rules = ruleset();
    let def = rules.talent(&id("power_strike")).unwrap();
    assert!(def.requirement.satisfied(&facts));
    facts.kind = TypeId::new("animal").unwrap();
    assert!(def.requirement.satisfied(&facts));
    facts.kind = TypeId::new("undead").unwrap();
    assert!(!def.requirement.satisfied(&facts));
    facts.kind = TypeId::new("animal").unwrap();
    facts.skills = SkillRanks::default();
    assert!(!def.requirement.satisfied(&facts));
    let warrior = make_ledger(0, 0, 1);
    facts.ledger = &warrior;
    facts.kind = TypeId::new("undead").unwrap();
    facts.skills = SkillRanks::default()
        .with_rank(Skill::HeavyWeaponry, 1)
        .unwrap();
    assert!(def.requirement.satisfied(&facts));
}
#[test]
fn fear_becomes_dormant_and_reactivates_without_replacing_the_choice() {
    let ledger = make_ledger(0, 4, 0);
    let mut facts = facts(&ledger);
    let selected = BTreeSet::from([id("fear"), id("fear_mastery")]);
    assert_eq!(ruleset().active_talents(&selected, &facts), selected);
    facts.mindless = true;
    assert!(ruleset().active_talents(&selected, &facts).is_empty());
    facts.mindless = false;
    assert_eq!(ruleset().active_talents(&selected, &facts), selected);
}
#[test]
fn validated_catalog_is_unique_and_topologically_ordered() {
    let rules = ruleset();
    let mut seen = BTreeSet::new();
    for id in rules.talent_order() {
        let def = rules.talent(id).unwrap();
        assert!(def.prerequisites.iter().all(|id| seen.contains(id)));
        assert!(seen.insert(id.clone()));
    }
    assert_eq!(seen.len(), 27);
}
