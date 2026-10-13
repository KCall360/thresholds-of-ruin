use tor_simulation::{
    abilities::{
        resolve, AbilityPlan, CheckPhase, CheckSide, CheckThreshold, FrozenStrike, PlannedCheck,
        PlannedEffect, Snapshot, StrikeOrigin,
    },
    combat::DamageType,
    damage::{DamageComponent, DamageSpec},
    dice::DicePool,
    resolution_diagnostics::{
        ResolutionObserver, ResolutionRecord, ResolutionStep, ResolutionTrace, TraceValueRecord,
        MAX_RESOLUTION_STEPS,
    },
    rules::CheckActor,
    ActorId,
};

fn plan(damage: DamageSpec, edge: i64) -> AbilityPlan {
    AbilityPlan {
        actor: ActorId(1),
        target: ActorId(2),
        public_aim: tor_simulation::public_plan::PublicAim::Unavailable,
        ability: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
        preparation: 1,
        recovery: 0,
        cost: None,
        cooldown: None,
        supply_costs: vec![],
        supplies: Default::default(),
        reach: tor_simulation::rules::Reach::Melee,
        strikes: vec![FrozenStrike {
            offset: 0,
            origin: StrikeOrigin::Ability { ordinal: 0 },
            check: Some(PlannedCheck {
                actor: CheckActor::User,
                target_participates: true,
                bonus: 0,
                threshold: CheckThreshold::Fixed { value: 0 },
                edge,
            }),
            on_success: vec![PlannedEffect::Damage { damage }],
            on_failure: vec![],
            descriptors: vec![],
            riders: vec![],
            edge,
            shared_check: Some(CheckPhase::Primary {
                side: CheckSide::Initiating,
            }),
        }],
        tokens: vec![],
    }
}
fn snapshot() -> Snapshot {
    let mut species = super::creature_fixture::species();
    species.modifiers = vec![tor_simulation::rules::Modifier {
        scope: tor_simulation::rules::Scope::Always,
        op: tor_simulation::rules::ModifierOp::Add {
            stat: tor_simulation::rules::Stat::Health,
            value: 999_900,
        },
    }];
    Snapshot::capture(
        &tor_simulation::creatures::CreatureState::new(super::creature_fixture::build(species))
            .unwrap(),
    )
}

fn field<'a>(record: &'a ResolutionRecord, id: &str) -> &'a TraceValueRecord {
    &record
        .fields
        .iter()
        .find(|field| field.id == id)
        .unwrap()
        .value
}
#[test]
fn owned_trace_retains_actual_dice_after_outcome_drops_and_replays_exactly() {
    let plan = plan(
        DamageSpec::new(
            vec![DamageComponent::rolled(
                DamageType::Impact,
                None,
                DicePool::new(2, 6, 3).unwrap(),
            )],
            None,
        )
        .unwrap(),
        5,
    );
    let mut rng = 42;
    let mut trace = ResolutionTrace::default();
    let outcome = resolve(&plan, &snapshot(), &mut rng, &mut trace);
    let total = super::abilities::damage_total(&outcome);
    drop(outcome);
    let component = trace
        .steps()
        .iter()
        .find(|step| step.id == "damage.component")
        .unwrap();
    assert_eq!(field(component, "count"), &TraceValueRecord::Unsigned(2));
    assert_eq!(field(component, "sides"), &TraceValueRecord::Unsigned(6));
    assert_eq!(field(component, "bonus"), &TraceValueRecord::Signed(3));
    assert_eq!(field(component, "edge"), &TraceValueRecord::Signed(2));
    assert!(matches!(field(component,"rolled"), TraceValueRecord::Dice(values) if values.len()==4));
    let finished = trace
        .steps()
        .iter()
        .find(|step| step.id == "damage.finished")
        .unwrap();
    assert_eq!(
        field(finished, "total"),
        &TraceValueRecord::Unsigned(u64::from(total))
    );
    assert!(!trace.truncated());
    let mut replay_rng = 42;
    let mut replay = ResolutionTrace::default();
    resolve(&plan, &snapshot(), &mut replay_rng, &mut replay);
    assert_eq!(trace, replay);
    assert_eq!(rng, replay_rng);
}
#[test]
fn trace_bound_marks_loss_and_never_changes_resolution() {
    let mut trace = ResolutionTrace::default();
    for _ in 0..MAX_RESOLUTION_STEPS {
        trace.record(ResolutionStep {
            id: "test.step",
            fields: &[],
        });
    }
    assert_eq!(trace.steps().len(), MAX_RESOLUTION_STEPS);
    assert!(!trace.truncated());
    let plan = plan(
        DamageSpec::new(
            vec![DamageComponent::rolled(
                DamageType::Impact,
                None,
                DicePool::new(1, 6, 0).unwrap(),
            )],
            None,
        )
        .unwrap(),
        0,
    );
    let mut observed_rng = 9;
    let observed = resolve(&plan, &snapshot(), &mut observed_rng, &mut trace);
    let mut ordinary_rng = 9;
    let ordinary = resolve(&plan, &snapshot(), &mut ordinary_rng, &mut ());
    assert_eq!(observed, ordinary);
    assert_eq!(observed_rng, ordinary_rng);
    assert_eq!(trace.steps().len(), MAX_RESOLUTION_STEPS);
    assert!(trace.truncated());
}
#[test]
fn maximum_component_bundle_fits_trace_bound_with_all_draws() {
    let plan = plan(
        DamageSpec::new(
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
        .unwrap(),
        65,
    );
    let mut trace = ResolutionTrace::default();
    let mut rng = 100;
    let outcome = resolve(&plan, &snapshot(), &mut rng, &mut trace);
    let retained: Vec<_> = trace
        .steps()
        .iter()
        .filter(|step| step.id == "damage.component")
        .collect();
    assert_eq!(retained.len(), 32);
    assert!(retained.iter().all(
        |step| matches!(field(step,"rolled"),TraceValueRecord::Dice(values) if values.len()==2)
    ));
    let finished = trace
        .steps()
        .iter()
        .find(|step| step.id == "damage.finished")
        .unwrap();
    assert_eq!(
        field(finished, "unused_edge"),
        &TraceValueRecord::Signed(30)
    );
    assert_eq!(
        field(finished, "total"),
        &TraceValueRecord::Unsigned(u64::from(super::abilities::damage_total(&outcome)))
    );
    assert!(!trace.truncated());
}
