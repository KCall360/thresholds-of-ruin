use tor_simulation::{
    attributes::{Attributes, ManaBinding, Skill, SkillCheck, SkillRanks},
    combat::DamageType,
    damage::{AttackCheck, DamageComponent, DamageSpec, Protection},
    dice::{DicePool, Edge},
    resolution_diagnostics::{
        ResolutionObserver, ResolutionRecord, ResolutionStep, ResolutionTrace, MAX_RESOLUTION_STEPS,
    },
};

fn attack() -> AttackCheck {
    AttackCheck {
        check: SkillCheck {
            skill: Skill::HeavyWeaponry,
            binding: ManaBinding::Intellect,
            modifier: 0,
            threshold: 0,
        },
        attributes: Attributes::default(),
        skills: SkillRanks::default(),
    }
}

#[test]
fn owned_trace_retains_actual_dice_after_outcome_drops_and_replays_exactly() {
    let damage = DamageSpec::new(
        vec![DamageComponent::rolled(
            DamageType::Impact,
            None,
            DicePool::new(2, 6, 3).unwrap(),
        )],
        None,
    )
    .unwrap();
    let mut rng = 42;
    let mut trace = ResolutionTrace::default();
    let outcome = attack().resolve_with_diagnostics(
        &mut rng,
        Edge::from_counts(3, 0),
        &damage,
        &Protection::default(),
        &mut trace,
    );
    let expected = outcome.damage.unwrap().components.remove(0);
    let component = trace
        .steps()
        .iter()
        .find_map(|step| match step {
            ResolutionRecord::Component(value) => Some(value),
            _ => None,
        })
        .unwrap();
    assert_eq!(component.component, expected);
    assert_eq!(component.expression, Some(DicePool::new(2, 6, 3).unwrap()));
    assert_eq!(component.edge, 2);
    assert_eq!(component.component.pool.as_ref().unwrap().rolled.len(), 4);
    assert!(!trace.truncated());
    let mut replay_rng = 42;
    let mut replay = ResolutionTrace::default();
    attack().resolve_with_diagnostics(
        &mut replay_rng,
        Edge::from_counts(3, 0),
        &damage,
        &Protection::default(),
        &mut replay,
    );
    assert_eq!(trace, replay);
    assert_eq!(rng, replay_rng);
}

#[test]
fn trace_bound_marks_loss_and_never_changes_resolution() {
    let mut trace = ResolutionTrace::default();
    for _ in 0..MAX_RESOLUTION_STEPS {
        trace.record(ResolutionStep::AttackStarted { net_edge: 0 });
    }
    assert_eq!(trace.steps().len(), MAX_RESOLUTION_STEPS);
    assert!(!trace.truncated());
    let damage = DamageSpec::new(
        vec![DamageComponent::rolled(
            DamageType::Impact,
            None,
            DicePool::new(1, 6, 0).unwrap(),
        )],
        None,
    )
    .unwrap();
    let mut observed_rng = 9;
    let observed = attack().resolve_with_diagnostics(
        &mut observed_rng,
        Edge::default(),
        &damage,
        &Protection::default(),
        &mut trace,
    );
    let mut ordinary_rng = 9;
    let ordinary = attack().resolve(
        &mut ordinary_rng,
        Edge::default(),
        &damage,
        &Protection::default(),
    );
    assert_eq!(observed, ordinary);
    assert_eq!(observed_rng, ordinary_rng);
    assert_eq!(trace.steps().len(), MAX_RESOLUTION_STEPS);
    assert!(trace.truncated());
}

#[test]
fn maximum_component_bundle_fits_trace_bound_with_all_draws() {
    let damage = DamageSpec::new(
        (2..34)
            .map(|sides| {
                DamageComponent::rolled(
                    DamageType::Impact,
                    None,
                    DicePool::new(1, sides, 0).unwrap(),
                )
            })
            .collect(),
        None,
    )
    .unwrap();
    let mut trace = ResolutionTrace::default();
    let mut rng = 100;
    let outcome = attack().resolve_with_diagnostics(
        &mut rng,
        Edge::from_counts(65, 0),
        &damage,
        &Protection::default(),
        &mut trace,
    );
    let actual = outcome.damage.unwrap();
    let retained: Vec<_> = trace
        .steps()
        .iter()
        .filter_map(|step| match step {
            ResolutionRecord::Component(value) => Some(&value.component),
            _ => None,
        })
        .collect();
    assert_eq!(retained.len(), 32);
    assert_eq!(retained, actual.components.iter().collect::<Vec<_>>());
    assert!(retained
        .iter()
        .all(|component| component.pool.as_ref().unwrap().rolled.len() == 2));
    assert!(
        matches!(trace.steps().last(), Some(ResolutionRecord::DamageFinished { total, unused_edge: 32, .. }) if *total == actual.total)
    );
    assert!(!trace.truncated());
}
