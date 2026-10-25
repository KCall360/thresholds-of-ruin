use tor_simulation::conditions::{ConditionError, ConditionState, ConditionUpdate};
use tor_simulation::content_ids::ConditionId;
use tor_simulation::rules::{ConditionDef, Eviction, Ruleset, Stacking, DEFAULT_RULESET_TOML};
use tor_simulation::ActorId;

fn definition(stacking: Stacking) -> ConditionDef {
    ConditionDef {
        hallucinating: false,
        id: ConditionId::new("marked").unwrap(),
        family: tor_simulation::content_ids::ConditionFamilyId::new("marked").unwrap(),
        lifecycle: tor_simulation::rules::ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: "Marked".into(),
        modifiers: vec![],
        stacking,
        source_scoped: true,
        descriptors: vec![],
        capacity: Eviction::Oldest,
    }
}
#[test]
fn more_than_128_causers_evict_oldest_without_panicking() {
    let def = definition(Stacking::Longest);
    let mut state = ConditionState::default();
    for source in 1..=256 {
        let update = state.apply(&def, ActorId(source), 300, false).unwrap();
        assert_eq!(
            update,
            if source <= 128 {
                ConditionUpdate::Applied
            } else {
                ConditionUpdate::Evicted
            }
        );
    }
    assert_eq!(state.instances().count(), 128);
    assert_eq!(state.remaining(&def.id, ActorId(128)), None);
    assert_eq!(state.remaining(&def.id, ActorId(129)), Some(300));
}
#[test]
fn refresh_and_longest_are_distinct_data_choices() {
    for (stacking, expected) in [(Stacking::Refresh, 100), (Stacking::Longest, 200)] {
        let def = definition(stacking);
        let mut state = ConditionState::default();
        state.apply(&def, ActorId(1), 300, false).unwrap();
        state.advance_active(100);
        state.apply(&def, ActorId(1), 100, false).unwrap();
        assert_eq!(state.remaining(&def.id, ActorId(1)), Some(expected));
    }
}
#[test]
fn sources_expire_independently_and_stacks_are_bounded() {
    let def = definition(Stacking::Stack { maximum: 2 });
    let mut state = ConditionState::default();
    for _ in 0..4 {
        state.apply(&def, ActorId(1), 300, false).unwrap();
    }
    state.apply(&def, ActorId(2), 400, false).unwrap();
    assert_eq!(state.instances().next().unwrap().stacks, 2);
    state.advance_active(300);
    assert_eq!(state.remaining(&def.id, ActorId(1)), None);
    assert_eq!(state.remaining(&def.id, ActorId(2)), Some(100));
    state.advance_active(u64::MAX);
    assert!(state.is_empty());
}
#[test]
fn immunity_only_clears_matching_condition() {
    let first = definition(Stacking::Longest);
    let mut second = first.clone();
    second.id = ConditionId::new("other").unwrap();
    let mut state = ConditionState::default();
    state.apply(&first, ActorId(1), 300, false).unwrap();
    state.apply(&second, ActorId(1), 300, false).unwrap();
    assert_eq!(
        state.apply(&first, ActorId(2), 300, true),
        Ok(ConditionUpdate::Immune)
    );
    assert_eq!(state.remaining(&first.id, ActorId(1)), None);
    assert_eq!(state.remaining(&second.id, ActorId(1)), Some(300));
}
#[test]
fn explicit_rejection_and_invalid_input_leave_state_unchanged() {
    let mut def = definition(Stacking::Longest);
    def.capacity = Eviction::Reject;
    let mut state = ConditionState::default();
    for source in 1..=128 {
        state.apply(&def, ActorId(source), 300, false).unwrap();
    }
    let before = state.clone();
    for (source, duration, error) in [
        (129, 300, ConditionError::Capacity),
        (0, 300, ConditionError::InvalidSource),
        (1, 0, ConditionError::InvalidDuration),
        (1, u64::MAX, ConditionError::InvalidDuration),
    ] {
        assert_eq!(
            state.apply(&def, ActorId(source), duration, false),
            Err(error)
        );
        assert_eq!(state, before);
    }
}
#[test]
fn named_records_restore_without_refresh() {
    let def = definition(Stacking::Longest);
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    document.conditions.push(def.clone());
    let rules = Ruleset::validate(document).unwrap();
    let mut state = ConditionState::default();
    state.apply(&def, ActorId(1), 300, false).unwrap();
    state.advance_active(99);
    let records: Vec<_> = state.instances().cloned().collect();
    assert_eq!(
        ConditionState::from_recorded(records.clone(), state.next_generation(), &rules).unwrap(),
        state
    );
}
#[test]
fn unscoped_condition_from_a_second_causer_refreshes_one_instance() {
    let mut def = definition(Stacking::Longest);
    def.source_scoped = false;
    let mut state = ConditionState::default();
    state.apply(&def, ActorId(1), 100, false).unwrap();
    state.apply(&def, ActorId(2), 300, false).unwrap();
    assert_eq!(state.instances().count(), 1);
    assert_eq!(state.remaining(&def.id, ActorId(2)), Some(300));
}

#[test]
fn recorded_condition_invariants_are_rejected_independently() {
    let def = definition(Stacking::Stack { maximum: 2 });
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    document.conditions.push(def.clone());
    let rules = Ruleset::validate(document).unwrap();
    let valid = tor_simulation::conditions::ConditionInstance {
        phase: tor_simulation::conditions::ConditionPhase::Active,
        condition: def.id.clone(),
        source: ActorId(1),
        remaining: Some(201),
        stacks: 2,
        order: 7,
        generation: 0,
        reserved_by: None,
        qualification: None,
    };
    let mut duplicate_key = valid.clone();
    duplicate_key.order = 8;
    duplicate_key.generation = 1;
    let mut duplicate_order = valid.clone();
    duplicate_order.source = ActorId(2);
    duplicate_order.generation = 1;
    let mut duplicate_generation = valid.clone();
    duplicate_generation.source = ActorId(2);
    duplicate_generation.order = 8;
    let mut zero_stacks = valid.clone();
    zero_stacks.stacks = 0;
    let mut excess_stacks = valid.clone();
    excess_stacks.stacks = 3;
    let mut unknown = valid.clone();
    unknown.condition = ConditionId::new("unknown_condition").unwrap();
    for (label, records, expected) in [
        (
            "duplicate key with distinct age",
            vec![valid.clone(), duplicate_key],
            ConditionError::Duplicate,
        ),
        (
            "duplicate age with distinct key",
            vec![valid.clone(), duplicate_order],
            ConditionError::InvalidOrder,
        ),
        (
            "duplicate generation with distinct key and age",
            vec![valid.clone(), duplicate_generation],
            ConditionError::InvalidGeneration,
        ),
        (
            "zero stacks",
            vec![zero_stacks],
            ConditionError::InvalidStacks,
        ),
        (
            "excess stacks",
            vec![excess_stacks],
            ConditionError::InvalidStacks,
        ),
        (
            "unknown condition",
            vec![unknown],
            ConditionError::UnknownCondition,
        ),
    ] {
        assert_eq!(
            ConditionState::from_recorded(records, 2, &rules),
            Err(expected),
            "{label}"
        );
    }
    assert_eq!(
        ConditionState::from_recorded(vec![valid.clone()], 1, &rules)
            .unwrap()
            .instances()
            .cloned()
            .collect::<Vec<_>>(),
        vec![valid]
    );
}

#[test]
fn restored_maximum_order_compacts_without_changing_age_source_or_duration() {
    let def = definition(Stacking::Longest);
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    document.conditions.push(def.clone());
    let rules = Ruleset::validate(document).unwrap();
    // Source/key ordering deliberately disagrees with age ordering.
    let records = [(ActorId(9), 5, 0, 201), (ActorId(2), u64::MAX, 1, 302)]
        .into_iter()
        .map(|(source, order, generation, remaining)| {
            tor_simulation::conditions::ConditionInstance {
                phase: tor_simulation::conditions::ConditionPhase::Active,
                condition: def.id.clone(),
                source,
                remaining: Some(remaining),
                stacks: 1,
                order,
                generation,
                reserved_by: None,
                qualification: None,
            }
        })
        .collect();
    let mut state = ConditionState::from_recorded(records, 2, &rules).unwrap();
    assert_eq!(
        state.apply(&def, ActorId(1), 403, false),
        Ok(ConditionUpdate::Applied)
    );
    let compacted: Vec<_> = state
        .instances()
        .map(|entry| (entry.source, entry.order, entry.generation, entry.remaining))
        .collect();
    assert_eq!(
        compacted,
        [
            (ActorId(1), 2, 2, Some(403)),
            (ActorId(2), 1, 1, Some(302)),
            (ActorId(9), 0, 0, Some(201))
        ]
    );
    let encoded = serde_json::to_vec(&state.instances().cloned().collect::<Vec<_>>()).unwrap();
    let restored = ConditionState::from_recorded(
        serde_json::from_slice(&encoded).unwrap(),
        state.next_generation(),
        &rules,
    )
    .unwrap();
    assert_eq!(restored, state);
    state.advance_active(100);
    for (source, remaining) in [(ActorId(9), 101), (ActorId(2), 202), (ActorId(1), 303)] {
        let oldest = state.evict_oldest().unwrap();
        assert_eq!(
            (
                oldest.condition,
                oldest.source,
                oldest.remaining,
                oldest.stacks
            ),
            (def.id.clone(), source, Some(remaining), 1)
        );
    }
    assert!(state.is_empty());
}

#[test]
fn recorded_generation_counter_must_exceed_every_live_generation() {
    let rules = Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap();
    let def = rules
        .condition(&ConditionId::new("readied").unwrap())
        .unwrap();
    let mut state = ConditionState::default();
    state.apply(def, ActorId(1), 300, false).unwrap();
    let mut entry = state.instances().next().unwrap().clone();
    entry.generation = 7;
    for counter in [0, 6, 7] {
        assert_eq!(
            ConditionState::from_recorded(vec![entry.clone()], counter, &rules),
            Err(ConditionError::InvalidGeneration),
            "counter {counter} must not reuse a live generation"
        );
    }
    for counter in [8, u64::MAX] {
        let restored = ConditionState::from_recorded(vec![entry.clone()], counter, &rules).unwrap();
        assert_eq!(restored.next_generation(), counter);
        assert_eq!(restored.instances().next().unwrap(), &entry);
    }
    entry.generation = u64::MAX;
    assert_eq!(
        ConditionState::from_recorded(vec![entry], u64::MAX, &rules),
        Err(ConditionError::InvalidGeneration)
    );
    for counter in [0, 8, u64::MAX] {
        let restored = ConditionState::from_recorded(vec![], counter, &rules).unwrap();
        assert!(restored.is_empty());
        assert_eq!(restored.next_generation(), counter);
    }
}

#[test]
fn generation_exhaustion_rejects_before_eviction_or_age_compaction() {
    let def = definition(Stacking::Longest);
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    document.conditions.push(def.clone());
    let rules = Ruleset::validate(document).unwrap();
    let mut state = ConditionState::default();
    for source in 1..=tor_simulation::limits::CONDITIONS as u64 {
        state.apply(&def, ActorId(source), 300, false).unwrap();
    }
    let mut entries = state.instances().cloned().collect::<Vec<_>>();
    entries.last_mut().unwrap().order = u64::MAX;
    let mut state = ConditionState::from_recorded(entries, u64::MAX, &rules).unwrap();
    let before = state.clone();
    assert_eq!(
        state.apply(&def, ActorId(1000), 300, false),
        Err(ConditionError::InvalidGeneration)
    );
    assert_eq!(state, before, "failure must not evict, compact or advance");
    // Exhaustion prevents only new instances, not renewal of an existing one.
    assert_eq!(
        state.apply(&def, ActorId(1), 400, false),
        Ok(ConditionUpdate::Refreshed)
    );
    assert_eq!(state.next_generation(), u64::MAX);
    assert_eq!(state.remaining(&def.id, ActorId(1)), Some(400));
}

#[test]
fn final_generation_allocation_never_wraps_or_resets_after_clear() {
    let rules = Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap();
    let def = rules
        .condition(&ConditionId::new("readied").unwrap())
        .unwrap();
    let mut state = ConditionState::from_recorded(vec![], u64::MAX - 1, &rules).unwrap();
    state.apply(def, ActorId(1), 300, false).unwrap();
    assert_eq!(state.instances().next().unwrap().generation, u64::MAX - 1);
    assert_eq!(state.next_generation(), u64::MAX);
    state.clear();
    assert!(state.is_empty());
    assert_eq!(state.next_generation(), u64::MAX);
    let before = state.clone();
    assert_eq!(
        state.apply(def, ActorId(1), 300, false),
        Err(ConditionError::InvalidGeneration)
    );
    assert_eq!(state, before);
}

#[test]
fn renewals_and_protection_keep_generation_until_a_new_instance_is_applied() {
    for stacking in [
        Stacking::Keep,
        Stacking::Refresh,
        Stacking::Extend,
        Stacking::Longest,
        Stacking::Stack { maximum: 2 },
    ] {
        let def = definition(stacking);
        let mut state = ConditionState::default();
        state.apply(&def, ActorId(1), 300, false).unwrap();
        state.apply(&def, ActorId(1), 400, false).unwrap();
        assert_eq!(state.instances().next().unwrap().generation, 0);
        assert_eq!(state.next_generation(), 1);
    }
    let mut def = definition(Stacking::Longest);
    def.timing = tor_simulation::rules::ConditionTiming::ActorActive {
        on_expiry: tor_simulation::rules::ConditionExpiry::Protect {
            duration: tor_simulation::rules::ConditionTicks::new(40).unwrap(),
        },
    };
    let mut state = ConditionState::default();
    state.apply(&def, ActorId(1), 300, false).unwrap();
    state.advance_active(300);
    let protected = state.instances().next().unwrap();
    assert_eq!(
        protected.phase,
        tor_simulation::conditions::ConditionPhase::Protection
    );
    assert_eq!(protected.generation, 0);
    assert_eq!(state.next_generation(), 1);
    assert_eq!(
        state.apply(&def, ActorId(1), 300, false),
        Ok(ConditionUpdate::Immune)
    );
    state.advance_active(40);
    state.apply(&def, ActorId(1), 300, false).unwrap();
    assert_eq!(state.instances().next().unwrap().generation, 1);
    assert_eq!(state.next_generation(), 2);
}
