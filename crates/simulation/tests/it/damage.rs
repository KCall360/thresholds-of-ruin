use tor_simulation::attributes::{Attributes, ManaBinding, Skill, SkillCheck, SkillRanks};
use tor_simulation::combat::DamageType;
use tor_simulation::damage::{AttackCheck, DamageComponent, DamageError, DamageSpec, Protection};
use tor_simulation::dice::{roll_check, DicePool, Edge};
use tor_simulation::grants::{Descriptor, Grant, Selector};

fn check(threshold: i32) -> AttackCheck {
    AttackCheck {
        check: SkillCheck {
            skill: Skill::HeavyWeaponry,
            binding: ManaBinding::Intellect,
            modifier: 0,
            threshold,
        },
        attributes: Attributes::default(),
        skills: SkillRanks::default(),
    }
}

#[test]
fn category_bonus_preserves_primary_pool_dice_edge_and_other_components() {
    let primary = DamageComponent::rolled(
        DamageType::Impact,
        Some(Descriptor::Fire),
        DicePool::new(2, 6, -1).unwrap(),
    );
    let key = primary.key();
    let base = DamageSpec::new(
        vec![
            primary,
            DamageComponent::rolled(DamageType::Energy, None, DicePool::new(1, 8, 0).unwrap()),
            DamageComponent::rolled(
                DamageType::Impact,
                Some(Descriptor::Cold),
                DicePool::new(1, 4, 0).unwrap(),
            ),
        ],
        Some(key),
    )
    .unwrap();
    let boosted = base.with_category_bonus(DamageType::Impact, 3).unwrap();
    assert_eq!(boosted.primary(), base.primary());
    assert_eq!(boosted.components().len(), base.components().len());
    for edge in [
        Edge::default(),
        Edge::from_counts(2, 0),
        Edge::from_counts(0, 2),
    ] {
        let mut before_rng = 42;
        let before = check(0).resolve(&mut before_rng, edge, &base, &Protection::default());
        let mut after_rng = 42;
        let after = check(0).resolve(&mut after_rng, edge, &boosted, &Protection::default());
        assert_eq!(before.check, after.check);
        assert_eq!(before_rng, after_rng);
        let before = before.damage.unwrap();
        let after = after.damage.unwrap();
        assert_eq!(after.total, before.total + 3);
        assert_eq!(after.components[0].raw, before.components[0].raw + 3);
        assert_eq!(after.components[1..], before.components[1..]);
        assert_eq!(
            after.components[0].pool.as_ref().unwrap().rolled,
            before.components[0].pool.as_ref().unwrap().rolled
        );
    }
}

#[test]
fn missing_category_bonus_does_not_change_existing_roll_order_or_bypass_protection() {
    let primary = DamageComponent::rolled(DamageType::Keen, None, DicePool::new(1, 6, 0).unwrap());
    let key = primary.key();
    let base = DamageSpec::new(
        vec![
            primary,
            DamageComponent::rolled(DamageType::Energy, None, DicePool::new(1, 8, 0).unwrap()),
        ],
        Some(key),
    )
    .unwrap();
    let boosted = base.with_category_bonus(DamageType::Impact, 3).unwrap();
    assert_eq!(boosted.primary(), base.primary());
    let protection =
        Protection::from_grants([Grant::Immunity(Selector::Category(DamageType::Impact))]).unwrap();
    let mut before_rng = 42;
    let before = base.resolve(&mut before_rng, Edge::from_counts(1, 0), &protection);
    let mut after_rng = 42;
    let after = boosted.resolve(&mut after_rng, Edge::from_counts(1, 0), &protection);
    assert_eq!(before_rng, after_rng);
    assert_eq!(after.raw, before.raw + 3);
    assert_eq!(after.total, before.total);
    assert_eq!(after.components[0], before.components[0]);
}

#[test]
fn category_bonus_checks_aggregate_limits_without_mutating_the_original() {
    let base = DamageSpec::new(
        vec![DamageComponent::fixed(DamageType::Impact, None, 1_000_000)],
        None,
    )
    .unwrap();
    let before = base.clone();
    assert_eq!(
        base.with_category_bonus(DamageType::Impact, 3),
        Err(DamageError::InvalidSpec)
    );
    assert_eq!(
        base.with_category_bonus(DamageType::Impact, u32::MAX),
        Err(DamageError::InvalidSpec)
    );
    assert_eq!(
        base.with_category_bonus(DamageType::Impact, 0),
        Ok(before.clone())
    );
    assert_eq!(base, before);
    let full = DamageSpec::new(
        (2..=33)
            .map(|sides| {
                DamageComponent::rolled(
                    DamageType::Energy,
                    None,
                    DicePool::new(1, sides, 0).unwrap(),
                )
            })
            .collect(),
        None,
    )
    .unwrap();
    assert_eq!(
        full.with_category_bonus(DamageType::Impact, 3),
        Err(DamageError::InvalidSpec)
    );
}

#[test]
fn splitting_fixed_damage_cannot_apply_reduction_more_than_once() {
    let spec = DamageSpec::new(
        vec![
            DamageComponent::fixed(DamageType::Impact, None, 5),
            DamageComponent::fixed(DamageType::Impact, None, 5),
        ],
        None,
    )
    .unwrap();
    let protection =
        Protection::from_grants([Grant::Reduction(Selector::Category(DamageType::Impact), 4)])
            .unwrap();
    let mut state = 42;
    let result = spec.resolve(&mut state, Edge::from_counts(u32::MAX, 0), &protection);
    assert_eq!(result.raw, 10);
    assert_eq!(result.total, 6);
    assert_eq!(result.components.len(), 1);
    assert_eq!(state, 42);
}

#[test]
fn immunity_precedes_descriptor_and_category_reduction() {
    let spec = DamageSpec::new(
        vec![
            DamageComponent::fixed(DamageType::Energy, Some(Descriptor::Fire), 7),
            DamageComponent::fixed(DamageType::Energy, Some(Descriptor::Cold), 5),
            DamageComponent::fixed(DamageType::Impact, None, 3),
        ],
        None,
    )
    .unwrap();
    let protection = Protection::from_grants([
        Grant::Immunity(Selector::Descriptor(Descriptor::Fire)),
        Grant::Reduction(Selector::Descriptor(Descriptor::Cold), 1),
        Grant::Reduction(Selector::Category(DamageType::Energy), 2),
    ])
    .unwrap();
    let result = spec.resolve(&mut 42, Edge::default(), &protection);
    assert_eq!(result.raw, 15);
    assert_eq!(result.after_immunity, 8);
    assert_eq!(result.after_descriptors, 7);
    assert_eq!(result.by_category[&DamageType::Energy], 2);
    assert_eq!(result.by_category[&DamageType::Impact], 3);
    assert_eq!(result.total, 5);
}

#[test]
fn exact_selector_grants_combine_before_specific_and_broad_layers() {
    let spec = DamageSpec::new(
        vec![DamageComponent::fixed(
            DamageType::Energy,
            Some(Descriptor::Fire),
            6,
        )],
        None,
    )
    .unwrap();
    let protection = Protection::from_grants([
        Grant::Reduction(Selector::Descriptor(Descriptor::Fire), 1),
        Grant::Reduction(Selector::Descriptor(Descriptor::Fire), 1),
        Grant::Reduction(Selector::Category(DamageType::Energy), 3),
    ])
    .unwrap();
    let result = spec.resolve(&mut 42, Edge::default(), &protection);
    assert_eq!(result.after_descriptors, 4);
    assert_eq!(result.total, 1);
}

#[test]
fn canonical_pools_are_independent_of_content_order_and_splitting() {
    let fire = DamageComponent::rolled(
        DamageType::Energy,
        Some(Descriptor::Fire),
        DicePool::new(1, 6, 1).unwrap(),
    );
    let cold = DamageComponent::rolled(
        DamageType::Energy,
        Some(Descriptor::Cold),
        DicePool::new(1, 8, 0).unwrap(),
    );
    let primary = Some(fire.key());
    let split = DamageSpec::new(vec![cold.clone(), fire.clone(), fire.clone()], primary).unwrap();
    let merged = DamageSpec::new(
        vec![
            DamageComponent::rolled(
                DamageType::Energy,
                Some(Descriptor::Fire),
                DicePool::new(2, 6, 2).unwrap(),
            ),
            cold,
        ],
        primary,
    )
    .unwrap();
    assert_eq!(split, merged);
    let mut first = 42;
    let mut second = 42;
    assert_eq!(
        split.resolve(&mut first, Edge::from_counts(2, 0), &Protection::default()),
        merged.resolve(&mut second, Edge::from_counts(2, 0), &Protection::default())
    );
    assert_eq!(first, second);
}

#[test]
fn double_advantage_covers_check_and_damage_and_misses_skip_all_damage_draws() {
    let component =
        DamageComponent::rolled(DamageType::Impact, None, DicePool::new(1, 6, 2).unwrap());
    let spec = DamageSpec::new(vec![component.clone()], Some(component.key())).unwrap();
    let mut state = 42;
    let hit = check(14).resolve(
        &mut state,
        Edge::from_counts(2, 0),
        &spec,
        &Protection::default(),
    );
    assert_eq!(hit.check.roll.second, Some(12));
    assert!(hit.check.success);
    let damage = hit.damage.unwrap();
    assert_eq!(damage.components[0].pool.as_ref().unwrap().rolled, [1, 1]);
    assert_eq!(damage.total, 3);
    let mut state = 42;
    let miss = check(15).resolve(
        &mut state,
        Edge::from_counts(2, 0),
        &spec,
        &Protection::default(),
    );
    assert!(!miss.check.success);
    assert!(miss.damage.is_none());
    assert_eq!(roll_check(&mut state, Edge::default()).kept, 19);
}

#[test]
fn unsupported_overlapping_selector_groups_and_unbounded_damage_reject() {
    assert_eq!(
        DamageSpec::new(
            vec![
                DamageComponent::fixed(DamageType::Energy, Some(Descriptor::Fire), 2),
                DamageComponent::fixed(DamageType::Impact, Some(Descriptor::Fire), 2)
            ],
            None
        ),
        Err(DamageError::NonNestedDescriptors)
    );
    assert_eq!(
        DamageSpec::new(
            vec![DamageComponent::fixed(DamageType::Impact, None, u32::MAX)],
            None
        ),
        Err(DamageError::InvalidSpec)
    );
    assert_eq!(DamageSpec::new(vec![], None), Err(DamageError::InvalidSpec));
    assert_eq!(
        Protection::from_grants([Grant::Reduction(
            Selector::Category(DamageType::Impact),
            u32::MAX
        )]),
        Err(DamageError::InvalidProtection)
    );
}

#[derive(Default)]
struct DiagnosticRecorder {
    checks: Vec<tor_simulation::resolution_diagnostics::CheckDiagnostic>,
    components: Vec<(u32, i64, usize, bool, bool, u32)>,
    reductions: Vec<(Selector, u32, u32, u32)>,
    finished: Vec<(u32, i64)>,
    misses: Vec<i64>,
}
impl tor_simulation::resolution_diagnostics::ResolutionObserver for DiagnosticRecorder {
    fn record(&mut self, step: tor_simulation::resolution_diagnostics::ResolutionStep<'_>) {
        use tor_simulation::resolution_diagnostics::ResolutionStep as S;
        match step {
            S::Check(value) => self.checks.push(value),
            S::Component(value) => self.components.push((
                value.component.raw,
                value.edge,
                value
                    .component
                    .pool
                    .as_ref()
                    .map_or(0, |pool| pool.rolled.len()),
                value.category_immune,
                value.descriptor_immune,
                value.after_immunity,
            )),
            S::Reduction {
                selector,
                before,
                capacity,
                after,
                ..
            } => self.reductions.push((selector, before, capacity, after)),
            S::DamageFinished {
                total, unused_edge, ..
            } => self.finished.push((total, unused_edge)),
            S::AttackMissed { unused_edge } => self.misses.push(unused_edge),
            S::AttackStarted { .. }
            | S::DamageStarted { .. }
            | S::FearStarted { .. }
            | S::FearImmunity { .. }
            | S::FearFinished { .. } => {}
        }
    }
}

#[test]
fn diagnostics_capture_actual_check_draws_protection_order_and_unused_edge() {
    let damage = DamageSpec::new(
        vec![
            DamageComponent::fixed(DamageType::Energy, Some(Descriptor::Fire), 7),
            DamageComponent::rolled(
                DamageType::Impact,
                Some(Descriptor::Cold),
                DicePool::new(2, 6, 20).unwrap(),
            ),
        ],
        None,
    )
    .unwrap();
    let protection = Protection::from_grants([
        Grant::Immunity(Selector::Descriptor(Descriptor::Fire)),
        Grant::Reduction(Selector::Descriptor(Descriptor::Cold), 3),
        Grant::Reduction(Selector::Category(DamageType::Impact), 4),
    ])
    .unwrap();
    let mut attack = check(0);
    attack.check.modifier = 2;
    attack.attributes = Attributes::new([3, 0, 0, 0, 0, 0]).unwrap();
    attack.skills = SkillRanks::default()
        .with_rank(Skill::HeavyWeaponry, 1)
        .unwrap();
    let mut ordinary_rng = 42;
    let ordinary = attack.resolve(
        &mut ordinary_rng,
        Edge::from_counts(5, 1),
        &damage,
        &protection,
    );
    let mut recorded_rng = 42;
    let mut recorder = DiagnosticRecorder::default();
    let recorded = attack.resolve_with_diagnostics(
        &mut recorded_rng,
        Edge::from_counts(5, 1),
        &damage,
        &protection,
        &mut recorder,
    );
    assert_eq!(recorded, ordinary);
    assert_eq!(recorded_rng, ordinary_rng);
    let detail = recorder.checks[0];
    assert_eq!(
        detail.attribute,
        tor_simulation::attributes::Attribute::Strength
    );
    assert_eq!(
        (
            detail.attribute_value,
            detail.rank,
            detail.check.modifier,
            detail.check.threshold
        ),
        (3, 1, 2, 0)
    );
    assert_eq!(detail.outcome, recorded.check);
    assert_eq!((detail.rng_before, detail.edge), (42, 1));
    assert_ne!(detail.rng_after, detail.rng_before);
    assert_eq!(recorder.components[0], (7, 0, 0, false, true, 0));
    assert_eq!((recorder.components[1].1, recorder.components[1].2), (2, 4));
    let raw_cold = recorded.damage.as_ref().unwrap().components[1].raw;
    let descriptor = recorder
        .reductions
        .iter()
        .position(|value| value.0 == Selector::Descriptor(Descriptor::Cold))
        .unwrap();
    let category = recorder
        .reductions
        .iter()
        .position(|value| value.0 == Selector::Category(DamageType::Impact))
        .unwrap();
    assert!(descriptor < category);
    assert_eq!(
        recorder.reductions[descriptor],
        (
            Selector::Descriptor(Descriptor::Cold),
            raw_cold,
            3,
            raw_cold - 3
        )
    );
    assert_eq!(
        recorder.reductions[category],
        (
            Selector::Category(DamageType::Impact),
            raw_cold - 3,
            4,
            raw_cold - 7
        )
    );
    assert_eq!(recorder.finished, [(raw_cold - 7, 1)]);
    assert!(recorder.misses.is_empty());
}

#[test]
fn missed_diagnostics_do_not_roll_damage_or_apply_protection() {
    let damage = DamageSpec::new(
        vec![DamageComponent::rolled(
            DamageType::Impact,
            None,
            DicePool::new(1, 6, 0).unwrap(),
        )],
        None,
    )
    .unwrap();
    let mut rng = 9;
    let mut reference = rng;
    let expected = roll_check(&mut reference, Edge::from_counts(0, 1));
    let mut recorder = DiagnosticRecorder::default();
    let outcome = check(100).resolve_with_diagnostics(
        &mut rng,
        Edge::from_counts(0, 4),
        &damage,
        &Protection::default(),
        &mut recorder,
    );
    assert_eq!(outcome.check.roll, expected);
    assert_eq!(rng, reference);
    assert!(outcome.damage.is_none());
    assert!(recorder.components.is_empty());
    assert!(recorder.reductions.is_empty());
    assert!(recorder.finished.is_empty());
    assert_eq!(recorder.misses, [-3]);
}
