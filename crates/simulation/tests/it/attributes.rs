use tor_simulation::attributes::{
    Attribute, Attributes, BuildValueError, ManaBinding, Skill, SkillCheck, SkillRanks,
};
use tor_simulation::dice::Edge;

#[test]
fn zero_attributes_and_untrained_skills_are_valid() {
    let attributes = Attributes::new([0; 6]).unwrap();
    let ranks = SkillRanks::default();
    for attribute in Attribute::ALL {
        assert_eq!(attributes.get(attribute), 0);
    }
    for skill in Skill::ALL {
        assert_eq!(ranks.get(skill), 0);
    }
    assert_eq!(attributes.defenses().physical, 10);
    assert_eq!(attributes.defenses().cognitive, 10);
    assert_eq!(attributes.defenses().spiritual, 10);
}

#[test]
fn permanent_attributes_and_training_have_explicit_caps() {
    assert_eq!(
        Attributes::new([6, 0, 0, 0, 0, 0]),
        Err(BuildValueError::Attribute)
    );
    assert_eq!(
        SkillRanks::default().with_rank(Skill::Athletics, 6),
        Err(BuildValueError::Rank)
    );
    assert_eq!(Attributes::new([5; 6]).unwrap().get(Attribute::Strength), 5);
    assert_eq!(
        SkillRanks::default()
            .with_rank(Skill::Athletics, 5)
            .unwrap()
            .get(Skill::Athletics),
        5
    );
}

#[test]
fn template_adjustments_exceed_advancement_cap_without_changing_base() {
    let base = Attributes::new([5, 0, 3, 2, 1, 4]).unwrap();
    let adjusted = base.adjusted([2, -1, -3, 0, 0, 0]).unwrap();
    assert_eq!(base.get(Attribute::Strength), 5);
    assert_eq!(adjusted.get(Attribute::Strength), 7);
    assert_eq!(adjusted.get(Attribute::Speed), 0);
    assert_eq!(adjusted.get(Attribute::Intellect), 0);
    assert_eq!(adjusted.defenses().physical, 17);
    assert_eq!(adjusted.defenses().cognitive, 12);
    assert_eq!(adjusted.defenses().spiritual, 15);
    assert_eq!(
        base.adjusted([i16::MAX; 6]),
        Err(BuildValueError::Attribute)
    );
}

#[test]
fn all_reference_skills_and_mana_bindings_use_declared_attributes() {
    let mapping = [
        (
            Attribute::Strength,
            &[Skill::Athletics, Skill::HeavyWeaponry][..],
        ),
        (
            Attribute::Speed,
            &[
                Skill::Agility,
                Skill::LightWeaponry,
                Skill::Stealth,
                Skill::Thievery,
            ][..],
        ),
        (
            Attribute::Intellect,
            &[
                Skill::Crafting,
                Skill::Deduction,
                Skill::Lore,
                Skill::Medicine,
            ][..],
        ),
        (
            Attribute::Willpower,
            &[Skill::Discipline, Skill::Intimidation][..],
        ),
        (
            Attribute::Awareness,
            &[Skill::Insight, Skill::Perception, Skill::Survival][..],
        ),
        (
            Attribute::Presence,
            &[Skill::Deception, Skill::Leadership, Skill::Persuasion][..],
        ),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (attribute, skills) in mapping {
        for &skill in skills {
            assert_eq!(skill.attribute(ManaBinding::Intellect), attribute);
            assert!(seen.insert(skill));
        }
    }
    assert_eq!(seen.len(), 18);
    for (binding, attribute) in [
        (ManaBinding::Intellect, Attribute::Intellect),
        (ManaBinding::Willpower, Attribute::Willpower),
        (ManaBinding::Awareness, Attribute::Awareness),
        (ManaBinding::Presence, Attribute::Presence),
    ] {
        assert_eq!(Skill::Spellcasting.attribute(binding), attribute);
    }
    assert_eq!(Skill::ALL.len(), 19);
}

#[test]
fn skill_check_uses_binding_ranks_modifiers_and_inclusive_threshold() {
    let attributes = Attributes::new([0, 0, 1, 2, 3, 4]).unwrap();
    let ranks = SkillRanks::default()
        .with_rank(Skill::Spellcasting, 2)
        .unwrap();
    let check = SkillCheck {
        skill: Skill::Spellcasting,
        binding: ManaBinding::Presence,
        modifier: -3,
        threshold: 17,
    };
    let mut state = 42;
    let outcome = check.resolve(&mut state, attributes, ranks, Edge::default());
    assert_eq!(outcome.roll.kept, 14);
    assert_eq!(outcome.total, 17);
    assert!(outcome.success);
    let mut state = 42;
    let disadvantaged = check.resolve(&mut state, attributes, ranks, Edge::from_counts(0, 1));
    assert_eq!(disadvantaged.total, 15);
    assert!(!disadvantaged.success);
}

#[test]
fn natural_one_and_twenty_have_no_automatic_result() {
    let attributes = Attributes::default();
    let ranks = SkillRanks::default();
    let check = SkillCheck {
        skill: Skill::Athletics,
        binding: ManaBinding::Intellect,
        modifier: i32::MAX,
        threshold: i32::MAX,
    };
    let mut state = 0;
    let _ = check.resolve(&mut state, attributes, ranks, Edge::default());
    let one = check.resolve(&mut state, attributes, ranks, Edge::default());
    assert_eq!(one.roll.kept, 1);
    assert!(one.success);
    let twenty = SkillCheck {
        modifier: i32::MIN,
        ..check
    }
    .resolve(&mut state, attributes, ranks, Edge::default());
    assert_eq!(twenty.roll.kept, 20);
    assert!(!twenty.success);
}

#[test]
fn speed_scales_only_designated_durations_with_ceil_and_minimum() {
    let base = Attributes::default();
    let fast = Attributes::new([0, 3, 0, 0, 0, 0]).unwrap();
    assert_eq!(base.physical_duration(100), 100);
    assert_eq!(fast.physical_duration(100), 63);
    assert_eq!(fast.physical_duration(1), 1);
    assert_eq!(fast.physical_duration(0), 1);
    assert_eq!(base.physical_duration(u64::MAX), u64::MAX);
    assert_eq!(fast.physical_duration(u64::MAX), 11_529_215_046_068_469_760);
}
