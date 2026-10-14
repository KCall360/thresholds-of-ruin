use super::creature_fixture::*;
use tor_simulation::damage::DamageAmount;
use tor_simulation::{
    attributes::{Attributes, Skill},
    combat::DamageType,
};

#[test]
fn unmodified_subject_keeps_baseline_rolled_melee_without_fixture_bonuses() {
    let subject = unmodified_species("baseline_subject");
    assert_eq!(subject.id.as_str(), "baseline_subject");
    assert_eq!(subject.kind.as_str(), "humanoid");
    assert_eq!(subject.default_attributes, Attributes::default());
    assert!(subject.subtypes.is_empty());
    assert!(subject.modifiers.is_empty());
    assert_eq!(subject.melee.skill(), Skill::HeavyWeaponry);
    assert_eq!(subject.melee.bonus(), 0);
    assert_eq!(
        (subject.melee.wind_up(), subject.melee.recovery()),
        (60, 40)
    );
    let damage = subject.melee.first_primary_damage().unwrap();
    let [component] = damage.components() else {
        panic!("baseline natural melee must have one damage component");
    };
    assert_eq!(component.key().category, DamageType::Impact);
    assert_eq!(component.key().descriptor, None);
    assert_eq!(damage.primary(), Some(component.key()));
    let DamageAmount::Rolled(pool) = component.amount() else {
        panic!("baseline natural melee must roll its damage");
    };
    assert_eq!((pool.count(), pool.sides(), pool.bonus()), (1, 4, 0));
    content_with(subject, vec![]);
}

#[test]
fn local_talent_probes_preserve_the_explicit_future_growth_source() {
    let mut original = build(species());
    let policy = tor_simulation::earned_growth::FutureGrowth::Selected {
        source: tor_simulation::earned_growth::GrowthSource::Racial {},
    };
    original.set_future_growth(policy.clone()).unwrap();
    let changed = with_talents(original, [talent("fixture_growth_probe", vec![])]);
    assert_eq!(changed.future_growth(), &policy);
}

#[test]
fn fixture_starting_skills_preserve_the_explicit_future_growth_source() {
    let mut original = build(species());
    let policy = tor_simulation::earned_growth::FutureGrowth::Selected {
        source: tor_simulation::earned_growth::GrowthSource::Racial {},
    };
    original.set_future_growth(policy.clone()).unwrap();
    let changed = with_starting_skills(original, [Skill::Athletics, Skill::Survival]);
    assert_eq!(changed.future_growth(), &policy);
}
