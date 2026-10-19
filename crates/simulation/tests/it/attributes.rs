use tor_simulation::attributes::{
    Attribute, Attributes, BuildValueError, ByAttribute, ManaBinding, Skill, SkillCheck, SkillRanks,
};
use tor_simulation::dice::Edge;

#[test]
fn zero_attributes_and_untrained_skills_are_valid() {
    let attributes = Attributes::new(ByAttribute::from_fn(|_| 0)).unwrap();
    let ranks = SkillRanks::default();
    for attribute in Attribute::ALL {
        assert_eq!(attributes.get(attribute), 0);
    }
    for skill in Skill::ALL {
        assert_eq!(ranks.get(skill), 0);
    }
}

#[test]
fn attribute_representation_and_training_have_explicit_bounds() {
    assert_eq!(
        Attributes::new(ByAttribute {
            strength: tor_simulation::limits::DERIVED_ATTRIBUTE + 1,
            speed: 0,
            intellect: 0,
            willpower: 0,
            awareness: 0,
            presence: 0
        }),
        Err(BuildValueError::Attribute)
    );
    assert_eq!(
        SkillRanks::default().with_rank(Skill::Athletics, 6),
        Err(BuildValueError::Rank)
    );
    assert_eq!(
        Attributes::new(ByAttribute::from_fn(|_| 5))
            .unwrap()
            .get(Attribute::Strength),
        5
    );
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
    let base = Attributes::new(ByAttribute {
        strength: 5,
        speed: 0,
        intellect: 3,
        willpower: 2,
        awareness: 1,
        presence: 4,
    })
    .unwrap();
    let adjusted = base
        .adjusted(ByAttribute {
            strength: 2,
            speed: -1,
            intellect: -3,
            willpower: 0,
            awareness: 0,
            presence: 0,
        })
        .unwrap();
    assert_eq!(base.get(Attribute::Strength), 5);
    assert_eq!(adjusted.get(Attribute::Strength), 7);
    assert_eq!(adjusted.get(Attribute::Speed), 0);
    assert_eq!(adjusted.get(Attribute::Intellect), 0);
    assert_eq!(
        base.adjusted(ByAttribute::from_fn(|_| i16::MAX)),
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
            &[Skill::Discipline, Skill::Intimidation, Skill::Breath][..],
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
    assert_eq!(seen.len(), 19);
    for (binding, attribute) in [
        (ManaBinding::Intellect, Attribute::Intellect),
        (ManaBinding::Willpower, Attribute::Willpower),
        (ManaBinding::Awareness, Attribute::Awareness),
        (ManaBinding::Presence, Attribute::Presence),
    ] {
        assert_eq!(Skill::Spellcasting.attribute(binding), attribute);
    }
    assert_eq!(Skill::ALL.len(), 20);
}

#[test]
fn breath_has_one_canonical_name_and_all_skill_slots_roundtrip_independently() {
    let breath: Skill = serde_json::from_str("\"breath\"").unwrap();
    assert_eq!(breath.as_str(), "breath");
    assert!(Skill::ALL.contains(&breath));
    let mut names = std::collections::BTreeSet::new();
    for skill in Skill::ALL {
        assert!(names.insert(skill.as_str()));
        let encoded = serde_json::to_string(&skill).unwrap();
        assert_eq!(encoded, format!("\"{}\"", skill.as_str()));
        assert_eq!(serde_json::from_str::<Skill>(&encoded).unwrap(), skill);
        let ranks = SkillRanks::default().with_rank(skill, 5).unwrap();
        for other in Skill::ALL {
            assert_eq!(ranks.get(other), if other == skill { 5 } else { 0 });
        }
        assert_eq!(ranks.with_rank(skill, 6), Err(BuildValueError::Rank));
    }
    for unknown in ["Breath", "breath_weapon", "breath ", "missing"] {
        assert!(serde_json::from_value::<Skill>(serde_json::json!(unknown)).is_err());
    }
}

#[test]
fn breath_check_adds_willpower_and_owned_rank_independent_of_mana_binding() {
    let attributes = Attributes::new(ByAttribute {
        strength: 1,
        speed: 2,
        intellect: 0,
        willpower: 3,
        awareness: 4,
        presence: 5,
    })
    .unwrap();
    let ranks = SkillRanks::default().with_rank(Skill::Breath, 2).unwrap();
    for binding in [
        ManaBinding::Intellect,
        ManaBinding::Willpower,
        ManaBinding::Awareness,
        ManaBinding::Presence,
    ] {
        let check = SkillCheck {
            skill: Skill::Breath,
            binding,
            modifier: 0,
            threshold: 10,
        };
        assert_eq!(Skill::Breath.attribute(binding), Attribute::Willpower);
        assert_eq!(check.bonus(attributes, ranks), 5);
        let mut state = 42;
        let outcome = check.resolve(&mut state, attributes, ranks, Edge::default(), &mut ());
        assert_eq!(outcome.roll.kept, 5);
        assert_eq!(outcome.total, 10);
        assert!(outcome.success);
    }
}

#[test]
fn skill_check_uses_binding_ranks_modifiers_and_inclusive_threshold() {
    let attributes = Attributes::new(ByAttribute {
        strength: 0,
        speed: 0,
        intellect: 1,
        willpower: 2,
        awareness: 3,
        presence: 4,
    })
    .unwrap();
    let ranks = SkillRanks::default()
        .with_rank(Skill::Spellcasting, 2)
        .unwrap();
    let check = SkillCheck {
        skill: Skill::Spellcasting,
        binding: ManaBinding::Presence,
        modifier: -3,
        threshold: 8,
    };
    let mut state = 42;
    assert_eq!(check.bonus(attributes, ranks), 3);
    let outcome = check.resolve(&mut state, attributes, ranks, Edge::default(), &mut ());
    assert_eq!(outcome.roll.kept, 5);
    assert_eq!(outcome.total, 8);
    assert!(outcome.success);
    let mut state = 42;
    let disadvantaged = check.resolve(
        &mut state,
        attributes,
        ranks,
        Edge::from_counts(0, 1),
        &mut (),
    );
    assert_eq!(disadvantaged.total, 7);
    assert!(!disadvantaged.success);
}

#[test]
fn natural_three_and_eighteen_have_no_automatic_result() {
    let attributes = Attributes::default();
    let ranks = SkillRanks::default();
    let check = SkillCheck {
        skill: Skill::Athletics,
        binding: ManaBinding::Intellect,
        modifier: i32::MAX,
        threshold: i32::MAX,
    };
    let mut state = 223;
    let three = check.resolve(&mut state, attributes, ranks, Edge::default(), &mut ());
    assert_eq!(three.roll.kept, 3);
    assert_eq!(three.roll.rolled(), [1, 1, 1]);
    assert!(three.success);
    let mut state = 921;
    let eighteen = SkillCheck {
        modifier: i32::MIN,
        ..check
    }
    .resolve(&mut state, attributes, ranks, Edge::default(), &mut ());
    assert_eq!(eighteen.roll.kept, 18);
    assert_eq!(eighteen.roll.rolled(), [6, 6, 6]);
    assert!(!eighteen.success);
}
