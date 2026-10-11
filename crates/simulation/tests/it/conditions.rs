use tor_simulation::conditions::{ConditionError, ConditionState, ConditionUpdate};
use tor_simulation::content_ids::ConditionId;
use tor_simulation::rules::{ConditionDef, Eviction, Ruleset, Stacking, DEFAULT_RULESET_TOML};
use tor_simulation::ActorId;

fn definition(stacking: Stacking) -> ConditionDef {
    ConditionDef {
        id: ConditionId::new("marked").unwrap(),
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
fn named_records_restore_without_refresh_and_reject_duplicates() {
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
        ConditionState::from_recorded(records.clone(), &rules).unwrap(),
        state
    );
    assert!(
        ConditionState::from_recorded(vec![records[0].clone(), records[0].clone()], &rules)
            .is_err()
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
