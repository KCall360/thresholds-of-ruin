use std::collections::BTreeSet;
use tor_simulation::attributes::{Attributes, Skill, SkillRanks};
use tor_simulation::progression::{Class, CreatureType, HdLedger, HdSource, HitDie};
use tor_simulation::talents::{active_talents, Talent, TalentFacts};

fn make_ledger(racial: usize, mage: usize, warrior: usize) -> HdLedger {
    HdLedger::new(
        std::iter::repeat_n(HdSource::Racial, racial)
            .chain(std::iter::repeat_n(HdSource::Class(Class::Mage), mage))
            .chain(std::iter::repeat_n(
                HdSource::Class(Class::Warrior),
                warrior,
            ))
            .map(|source| HitDie::new(source, 42))
            .collect(),
    )
    .unwrap()
}

fn test_facts(ledger: &HdLedger) -> TalentFacts<'_> {
    TalentFacts {
        ledger,
        kind: CreatureType::Humanoid,
        attributes: Attributes::new([1; 6]).unwrap(),
        skills: SkillRanks::default()
            .with_rank(Skill::Spellcasting, 1)
            .unwrap()
            .with_rank(Skill::Intimidation, 1)
            .unwrap()
            .with_rank(Skill::HeavyWeaponry, 1)
            .unwrap(),
        melee_skill: Skill::HeavyWeaponry,
        magical: ledger.class_level(Class::Mage) > 0,
        mindless: false,
    }
}

#[test]
fn class_prerequisites_are_local_not_total_advancement() {
    let ledger = make_ledger(15, 1, 0);
    let facts = test_facts(&ledger);
    let selected = BTreeSet::from([Talent::MagicBolt, Talent::PotentBolt, Talent::EmpoweredBolt]);
    assert_eq!(
        active_talents(&facts, &selected),
        BTreeSet::from([Talent::MagicBolt])
    );
    assert!(!Talent::EmpoweredBolt.eligible(&facts, &BTreeSet::from([Talent::PotentBolt])));
}

#[test]
fn passive_upgrade_chains_need_active_predecessors_and_total_hd() {
    let selected = BTreeSet::from([
        Talent::Hardiness,
        Talent::Toughness,
        Talent::Unyielding,
        Talent::Indomitable,
    ]);
    for (hd, expected) in [(1, 1), (4, 2), (8, 3), (12, 4)] {
        let ledger = make_ledger(hd, 0, 0);
        assert_eq!(
            active_talents(&test_facts(&ledger), &selected).len(),
            expected
        );
    }
    let ledger = make_ledger(16, 0, 0);
    assert!(
        active_talents(&test_facts(&ledger), &BTreeSet::from([Talent::Indomitable])).is_empty()
    );
}

#[test]
fn racial_technique_eligibility_depends_on_type_and_training() {
    let ledger = make_ledger(2, 0, 0);
    let mut facts = test_facts(&ledger);
    assert!(Talent::PowerStrike.eligible(&facts, &BTreeSet::new()));
    facts.kind = CreatureType::Animal;
    assert!(Talent::PowerStrike.eligible(&facts, &BTreeSet::new()));
    facts.kind = CreatureType::Undead;
    assert!(!Talent::PowerStrike.eligible(&facts, &BTreeSet::new()));
    facts.kind = CreatureType::Animal;
    facts.skills = SkillRanks::default();
    assert!(!Talent::PowerStrike.eligible(&facts, &BTreeSet::new()));
    let warrior = make_ledger(0, 0, 1);
    let mut facts = test_facts(&warrior);
    facts.kind = CreatureType::Undead;
    assert!(Talent::PowerStrike.eligible(&facts, &BTreeSet::new()));
}

#[test]
fn fear_becomes_dormant_and_reactivates_without_replacing_the_choice() {
    let ledger = make_ledger(0, 4, 0);
    let mut facts = test_facts(&ledger);
    let selected = BTreeSet::from([Talent::Fear, Talent::FearMastery]);
    assert_eq!(active_talents(&facts, &selected), selected);
    facts.mindless = true;
    assert!(active_talents(&facts, &selected).is_empty());
    facts.mindless = false;
    assert_eq!(active_talents(&facts, &selected), selected);
    assert_eq!(selected.len(), 2);
}

#[test]
fn reference_catalog_is_unique_and_topologically_ordered() {
    let catalog: BTreeSet<_> = Talent::ALL.into_iter().collect();
    assert_eq!(catalog.len(), 27);
    assert_eq!(catalog.len(), Talent::ALL.len());
    let mut earlier = BTreeSet::new();
    for talent in Talent::ALL {
        if let Some(prerequisite) = talent.predecessor() {
            assert!(
                earlier.contains(&prerequisite),
                "{talent:?} precedes its prerequisite"
            );
        }
        assert!(!talent.grants().is_empty());
        assert!(earlier.insert(talent));
    }
}
