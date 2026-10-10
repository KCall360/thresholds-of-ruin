use std::collections::BTreeSet;
use tor_simulation::attributes::{Attribute, Attributes, ManaBinding, Skill};
use tor_simulation::creatures::{
    BuildError, CreatureBuild, GrantSource, Species, Subtype, Template,
};
use tor_simulation::dice::DicePool;
use tor_simulation::grants::{Ability, Descriptor, Grant, Selector};
use tor_simulation::progression::{Class, CreatureType, HdLedger, HdSource, HitDie};
use tor_simulation::talents::Talent;
use tor_simulation::AnatomySpec;

fn species() -> Species {
    Species {
        id: "hobgoblin".into(),
        kind: CreatureType::Humanoid,
        subtypes: BTreeSet::from([Subtype::Goblinoid]),
        default_attributes: Attributes::new([1; 6]).unwrap(),
        anatomy: AnatomySpec::humanoid(),
        melee: tor_simulation::attacks::MeleeAttack::new(Skill::HeavyWeaponry, 0, 60, 40, {
            let component = tor_simulation::damage::DamageComponent::rolled(
                tor_simulation::combat::DamageType::Impact,
                None,
                DicePool::new(1, 6, 0).unwrap(),
            );
            let primary = component.key();
            tor_simulation::damage::DamageSpec::new(vec![component], Some(primary)).unwrap()
        })
        .unwrap(),
        grants: vec![],
    }
}

fn build(sources: &[HdSource]) -> CreatureBuild {
    CreatureBuild::new(
        species(),
        HdLedger::new(
            sources
                .iter()
                .map(|&source| HitDie::new(source, 42))
                .collect(),
        )
        .unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap()
}

#[test]
fn techniques_use_the_whole_current_build_pool_and_older_unspent_slots() {
    let mut build = build(&[
        HdSource::Racial,
        HdSource::Racial,
        HdSource::Class(Class::Mage),
    ]);
    assert!(!build
        .derive()
        .unwrap()
        .abilities
        .contains(&Ability::MagicBolt));
    build.train(0, Skill::Spellcasting).unwrap();
    build.train(1, Skill::Intimidation).unwrap();
    build.select_talent(0, Talent::MagicBolt).unwrap();
    build.select_talent(1, Talent::Fear).unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.abilities.contains(&Ability::MagicBolt));
    assert!(derived.abilities.contains(&Ability::Fear));
    assert_eq!(build.unspent_talent_slots(), 1);
    assert_eq!(
        build.select_talent(2, Talent::Fear),
        Err(BuildError::DuplicateTalent)
    );
    assert_eq!(build.unspent_talent_slots(), 1);
}

#[test]
fn type_invalidated_choices_remain_owned_and_reactivate() {
    let mut build = build(&[HdSource::Racial, HdSource::Racial]);
    build.train(0, Skill::HeavyWeaponry).unwrap();
    build.select_talent(0, Talent::PowerStrike).unwrap();
    let original = build.derive().unwrap();
    build.set_templates(vec![Template::zombified(0)]).unwrap();
    let undead = build.derive().unwrap();
    assert_eq!(undead.kind, CreatureType::Undead);
    assert!(undead.mindless);
    assert_eq!(undead.attributes.get(Attribute::Strength), 3);
    assert_eq!(undead.attributes.get(Attribute::Speed), 0);
    assert_eq!(undead.attributes.get(Attribute::Intellect), 0);
    assert_eq!(undead.anatomy, original.anatomy);
    assert!(undead.dormant_talents.contains(&Talent::PowerStrike));
    assert!(!undead.abilities.contains(&Ability::PowerStrike));
    assert_eq!(
        build.select_talent(0, Talent::Hardiness),
        Err(BuildError::OccupiedSlot)
    );
    build.set_templates(vec![]).unwrap();
    assert_eq!(build.derive().unwrap(), original);
}

#[test]
fn independent_immunity_sources_survive_template_removal() {
    let fear = Selector::Descriptor(Descriptor::Fear);
    let mut definition = species();
    definition.grants.push(Grant::Immunity(fear));
    let mut build = CreatureBuild::new(
        definition,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    build.set_templates(vec![Template::zombified(0)]).unwrap();
    assert!(build.derive().unwrap().immunity_sources(fear).len() >= 2);
    build.set_templates(vec![]).unwrap();
    assert_eq!(
        build.derive().unwrap().immunity_sources(fear),
        vec![GrantSource::Species("hobgoblin".into())]
    );
}

#[test]
fn template_conflicts_reject_atomically_and_priorities_resolve_explicitly() {
    let mut build = build(&[HdSource::Racial]);
    let original = build.derive().unwrap();
    let undead = Template::zombified(0);
    let mut construct = Template::new("construct", 0);
    construct.kind = Some(CreatureType::Construct);
    assert_eq!(
        build.set_templates(vec![undead.clone(), construct.clone()]),
        Err(BuildError::ConflictingTemplates)
    );
    assert_eq!(build.derive().unwrap(), original);
    construct.priority = 1;
    build
        .set_templates(vec![construct.clone(), undead.clone()])
        .unwrap();
    let derived = build.derive().unwrap();
    assert_eq!(derived.kind, CreatureType::Construct);
    build.set_templates(vec![undead, construct]).unwrap();
    assert_eq!(build.derive().unwrap(), derived);
}

#[test]
fn same_priority_numeric_adjustments_are_aggregated_before_flooring() {
    let mut definition = species();
    definition.default_attributes = Attributes::default();
    let mut build = CreatureBuild::new(
        definition,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 0)]).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    let mut increase = Template::new("z_increase", 0);
    increase.adjustments[Attribute::Speed as usize] = 1;
    let mut decrease = Template::new("a_decrease", 0);
    decrease.adjustments[Attribute::Speed as usize] = -1;
    build.set_templates(vec![increase, decrease]).unwrap();
    assert_eq!(build.derive().unwrap().attributes.get(Attribute::Speed), 0);
}

#[test]
fn advancement_ownership_and_caps_reverse_with_the_latest_hd() {
    let mut build = build(&[HdSource::Class(Class::Warrior); 4]);
    assert_eq!(
        build.increase_attribute(0, Attribute::Strength),
        Err(BuildError::NoAttributeOpportunity)
    );
    build.increase_attribute(3, Attribute::Strength).unwrap();
    build.train(3, Skill::HeavyWeaponry).unwrap();
    build.select_talent(3, Talent::Hardiness).unwrap();
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        2
    );
    let removed = build.remove_latest().unwrap();
    assert_eq!(removed.choices.talent, Some(Talent::Hardiness));
    let derived = build.derive().unwrap();
    assert_eq!(derived.attributes.get(Attribute::Strength), 1);
    assert_eq!(derived.skills.get(Skill::HeavyWeaponry), 0);
    assert!(!derived.active_talents.contains(&Talent::Hardiness));
    while build.remove_latest().is_some() {}
    assert_eq!(build.derive().unwrap().maximum_health, 0);
}

#[test]
fn training_overspending_and_rank_overflow_leave_choices_unchanged() {
    let mut build = build(&[HdSource::Class(Class::Warrior); 4]);
    build.train(0, Skill::HeavyWeaponry).unwrap();
    build.train(0, Skill::HeavyWeaponry).unwrap();
    assert_eq!(
        build.train(0, Skill::Athletics),
        Err(BuildError::NoTrainingPoint)
    );
    build.train(1, Skill::HeavyWeaponry).unwrap();
    build.train(1, Skill::HeavyWeaponry).unwrap();
    build.train(2, Skill::HeavyWeaponry).unwrap();
    let before = build.derive().unwrap();
    assert_eq!(
        build.train(2, Skill::HeavyWeaponry),
        Err(BuildError::RankCap)
    );
    assert_eq!(build.derive().unwrap(), before);
}

#[test]
fn catalog_subtypes_have_explicit_identity_or_deferred_mechanics() {
    assert_eq!(Subtype::ALL.into_iter().collect::<BTreeSet<_>>().len(), 20);
    let mut definition = species();
    definition.subtypes.insert(Subtype::Fire);
    let build = CreatureBuild::new(
        definition,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 0)]).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    assert_eq!(
        build
            .derive()
            .unwrap()
            .immunity_sources(Selector::Descriptor(Descriptor::Fire)),
        vec![GrantSource::Subtype(Subtype::Fire)]
    );
    assert!(Subtype::Swarm.mechanics_deferred());
    assert!(!Subtype::Fire.mechanics_deferred());
}

#[test]
fn passive_grants_change_only_their_declared_stats_and_pools() {
    let mut build = build(&[HdSource::Class(Class::Warrior); 4]);
    let before = build.derive().unwrap();
    build.select_talent(0, Talent::Hardiness).unwrap();
    build.select_talent(1, Talent::Guard).unwrap();
    build.select_talent(2, Talent::HeavyBlows).unwrap();
    build.select_talent(3, Talent::MightyBlows).unwrap();
    let after = build.derive().unwrap();
    assert_eq!(after.maximum_health, before.maximum_health + 2);
    assert_eq!(after.defenses.physical, before.defenses.physical + 1);
    assert_eq!(after.defenses.cognitive, before.defenses.cognitive);
    assert_eq!(after.defenses.spiritual, before.defenses.spiritual);
    assert_eq!(
        after.melee_damage(0).unwrap().components()[0].amount(),
        tor_simulation::damage::DamageAmount::Rolled(DicePool::new(2, 6, 1).unwrap())
    );
    assert_eq!(after.bolt, before.bolt);
    let mut state = 42;
    assert_eq!(
        after
            .melee_damage(0)
            .unwrap()
            .resolve(
                &mut state,
                tor_simulation::dice::Edge::default(),
                &tor_simulation::damage::Protection::default()
            )
            .total,
        5
    );
}

#[test]
fn invalid_definitions_and_derived_limits_are_rejected_without_mutation() {
    let mut build = build(&[HdSource::Racial]);
    let before = build.clone();
    let mut invalid = Template::new("bad", 0);
    invalid.grants = vec![Grant::Health(1_000_000)];
    assert_eq!(
        build.set_templates(vec![invalid]),
        Err(BuildError::DerivedLimit)
    );
    assert_eq!(build, before);
    let mut invalid = Template::new("bad", 0);
    invalid.adjustments = [i16::MAX; 6];
    assert_eq!(
        build.set_templates(vec![invalid]),
        Err(BuildError::InvalidDefinition)
    );
    assert_eq!(build, before);
    assert_eq!(
        build.set_templates(vec![Template::zombified(0); 33]),
        Err(BuildError::InvalidDefinition)
    );
    let attack = species().melee;
    assert!(tor_simulation::attacks::MeleeAttack::new(
        Skill::Spellcasting,
        0,
        60,
        40,
        attack.damage().clone()
    )
    .is_err());
}

#[test]
fn recorded_choices_restore_dormancy_and_validate_ownership() {
    let mut build = build(&[HdSource::Class(Class::Mage); 4]);
    build.train(0, Skill::Intimidation).unwrap();
    build.select_talent(0, Talent::Fear).unwrap();
    build.select_talent(1, Talent::FearMastery).unwrap();
    build.set_templates(vec![Template::zombified(0)]).unwrap();
    let restored = CreatureBuild::from_recorded(
        build.species().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        build.choices().to_vec(),
        build.templates().to_vec(),
    )
    .unwrap();
    assert_eq!(restored, build);
    assert_eq!(
        restored.derive().unwrap().dormant_talents,
        BTreeSet::from([Talent::Fear, Talent::FearMastery])
    );
    let mut choices = build.choices().to_vec();
    choices[0].attribute = Some(Attribute::Strength);
    assert_eq!(
        CreatureBuild::from_recorded(
            build.species().clone(),
            build.initial_attributes(),
            build.ledger().clone(),
            build.binding(),
            choices,
            vec![]
        ),
        Err(BuildError::NoAttributeOpportunity)
    );
}

#[test]
fn conflicting_attribute_overrides_and_subtype_operations_reject_atomically() {
    let mut build = build(&[HdSource::Racial]);
    let before = build.clone();
    let mut first = Template::new("first", 0);
    let mut second = Template::new("second", 0);
    first.overrides.insert(Attribute::Intellect, 0);
    second.overrides.insert(Attribute::Intellect, 2);
    assert_eq!(
        build.set_templates(vec![first, second]),
        Err(BuildError::ConflictingTemplates)
    );
    assert_eq!(build, before);
    let mut first = Template::new("first", 0);
    let mut second = Template::new("second", 0);
    first.add_subtypes.insert(Subtype::Fire);
    second.remove_subtypes.insert(Subtype::Fire);
    assert_eq!(
        build.set_templates(vec![first, second]),
        Err(BuildError::ConflictingTemplates)
    );
    assert_eq!(build, before);
    assert_eq!(
        build.set_templates(vec![Template::zombified(0); 2]),
        Err(BuildError::InvalidDefinition)
    );
}

#[test]
fn aggregate_protection_limits_are_validated_during_build_derivation() {
    let mut definition = species();
    let selector = Selector::Category(tor_simulation::combat::DamageType::Energy);
    definition
        .grants
        .push(Grant::Reduction(selector, 1_000_000));
    let mut build = CreatureBuild::new(
        definition,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
        ManaBinding::Intellect,
    )
    .unwrap();
    let before = build.clone();
    let mut excessive = Template::new("excessive", 0);
    excessive.grants.push(Grant::Reduction(selector, 1));
    assert_eq!(
        build.set_templates(vec![excessive]),
        Err(BuildError::DerivedLimit)
    );
    assert_eq!(build, before);
}

#[test]
fn individual_attribute_choices_do_not_redefine_the_species() {
    let mut strong = build(&[HdSource::Class(Class::Warrior); 4]);
    let mut fast = strong.clone();
    strong
        .set_initial_attributes(Attributes::new([4, 0, 1, 1, 1, 1]).unwrap())
        .unwrap();
    fast.set_initial_attributes(Attributes::new([0, 4, 1, 1, 1, 1]).unwrap())
        .unwrap();
    assert_eq!(strong.species(), fast.species());
    let strong_stats = strong.derive().unwrap();
    let fast_stats = fast.derive().unwrap();
    assert_eq!(strong_stats.defenses.physical, fast_stats.defenses.physical);
    assert_eq!(strong_stats.maximum_health, fast_stats.maximum_health + 4);
    assert_eq!(
        strong_stats.resources.stamina,
        fast_stats.resources.stamina + 4
    );
    assert_eq!(strong_stats.attributes.get(Attribute::Strength), 4);
    assert_eq!(fast_stats.attributes.get(Attribute::Speed), 4);
    let restored = CreatureBuild::from_recorded(
        strong.species().clone(),
        strong.initial_attributes(),
        strong.ledger().clone(),
        strong.binding(),
        strong.choices().to_vec(),
        strong.templates().to_vec(),
    )
    .unwrap();
    assert_eq!(restored, strong);
}

#[test]
fn changing_starting_attributes_cannot_overflow_owned_advancement_choices() {
    let mut build = build(&[HdSource::Class(Class::Warrior); 4]);
    build.increase_attribute(3, Attribute::Strength).unwrap();
    let before = build.clone();
    assert_eq!(
        build.set_initial_attributes(Attributes::new([5, 0, 0, 0, 0, 0]).unwrap()),
        Err(BuildError::AttributeCap)
    );
    assert_eq!(build, before);
    let invalid = Attributes::new([5; 6])
        .unwrap()
        .adjusted([1, 0, 0, 0, 0, 0])
        .unwrap();
    assert_eq!(
        build.set_initial_attributes(invalid),
        Err(BuildError::AttributeCap)
    );
    assert_eq!(build, before);
}

// These use that module's existing build() and species() helpers.
#[test]
fn added_hit_die_preserves_retained_seeds_and_owned_choices() {
    let mut creature = build(&[
        HdSource::Class(Class::Warrior),
        HdSource::Class(Class::Mage),
    ]);
    creature.train(0, Skill::HeavyWeaponry).unwrap();
    let before = creature.clone();
    let root_seed = 123;
    creature.add_hit_die(HdSource::Racial, root_seed).unwrap();
    assert_eq!(&creature.ledger().entries()[..2], before.ledger().entries());
    assert_eq!(&creature.choices()[..2], before.choices());
    let expected = HdLedger::seeded(
        vec![
            HdSource::Class(Class::Warrior),
            HdSource::Class(Class::Mage),
            HdSource::Racial,
        ],
        root_seed,
    )
    .unwrap();
    assert_eq!(creature.ledger().entries()[2], expected.entries()[2]);
    assert_eq!(creature.choices()[2], Default::default());
    creature.train(2, Skill::Lore).unwrap();
    let trained = creature.clone();
    assert_eq!(
        creature.train(2, Skill::Discipline),
        Err(BuildError::NoTrainingPoint)
    );
    assert_eq!(creature, trained);
    let removed = creature.remove_latest().unwrap();
    assert_eq!(removed.choices.training, vec![Skill::Lore]);
    assert_eq!(creature, before);
    creature.add_hit_die(HdSource::Racial, root_seed).unwrap();
    assert_eq!(creature.ledger().entries()[2], removed.hit_die);
    assert_eq!(creature.choices()[2], Default::default());
}

#[test]
fn adding_hit_dice_at_the_bound_rejects_without_mutation() {
    let mut creature = build(&[HdSource::Class(Class::Warrior); 255]);
    creature
        .add_hit_die(HdSource::Class(Class::Mage), 123)
        .unwrap();
    let before = creature.clone();
    assert_eq!(creature.ledger().total_hd(), 256);
    assert!(creature.add_hit_die(HdSource::Racial, 123).is_err());
    assert_eq!(creature, before);
}
