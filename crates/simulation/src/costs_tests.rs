use super::*;
use crate::test_support::test_ruleset;
use serde_json::json;

fn scalar(resource: Resource, start: u32, resolution: u32) -> ResourceCost {
    ResourceCost {
        resource,
        start,
        resolution,
    }
}

fn single(resource: Resource, start: u32, resolution: u32) -> CostBundle {
    CostBundle::single(scalar(resource, start, resolution)).unwrap()
}

fn maxima(values: [u32; 3]) -> ResourceMaxima {
    ResourceMaxima {
        stamina: values[0],
        focus: values[1],
        mana: values[2],
    }
}

fn ledger(values: [u32; 3]) -> CostLedger {
    CostLedger::new(Resources::new(maxima(values), &test_ruleset()))
}

fn balances(ledger: &CostLedger) -> [u32; 3] {
    Resource::ALL.map(|resource| ledger.resources().balance(resource))
}

fn available(ledger: &CostLedger) -> [u32; 3] {
    Resource::ALL.map(|resource| ledger.available(resource))
}

fn mixed() -> CostBundle {
    single(Resource::Mana, 3, 4)
        .checked_add(single(Resource::Stamina, 1, 2))
        .unwrap()
        .checked_add(single(Resource::Focus, 2, 3))
        .unwrap()
}

#[test]
fn authored_scalar_cost_keeps_its_existing_shape() {
    let cost = scalar(Resource::Focus, 2, 3);
    let value = json!({"resource": "focus", "start": 2, "resolution": 3});
    assert_eq!(serde_json::to_value(cost).unwrap(), value);
    assert_eq!(serde_json::from_value::<ResourceCost>(value).unwrap(), cost);
}

#[test]
fn focus_contributions_sum_both_payment_phases() {
    let first = single(Resource::Focus, 2, 3);
    let second = single(Resource::Focus, 5, 7);
    let combined = first.checked_add(second).unwrap();
    assert_eq!(
        combined.entries().collect::<Vec<_>>(),
        [scalar(Resource::Focus, 7, 10)]
    );
    assert_eq!(second.checked_add(first), Ok(combined));
    assert_eq!(
        first.entries().collect::<Vec<_>>(),
        [scalar(Resource::Focus, 2, 3)]
    );
    assert_eq!(
        second.entries().collect::<Vec<_>>(),
        [scalar(Resource::Focus, 5, 7)]
    );
}

#[test]
fn mixed_resources_are_canonical_and_keep_independent_phases() {
    let cost = mixed();
    assert_eq!(
        cost.entries().collect::<Vec<_>>(),
        [
            scalar(Resource::Stamina, 1, 2),
            scalar(Resource::Focus, 2, 3),
            scalar(Resource::Mana, 3, 4),
        ]
    );
    let encoded = r#"{"stamina":{"start":1,"resolution":2},"focus":{"start":2,"resolution":3},"mana":{"start":3,"resolution":4}}"#;
    assert_eq!(serde_json::to_string(&cost).unwrap(), encoded);
    assert_eq!(serde_json::from_str::<CostBundle>(encoded).unwrap(), cost);
    let reversed = r#"{"mana":{"resolution":4,"start":3},"focus":{"resolution":3,"start":2},"stamina":{"resolution":2,"start":1}}"#;
    assert_eq!(serde_json::from_str::<CostBundle>(reversed).unwrap(), cost);
    assert_eq!(
        cost.checked_add(single(Resource::Focus, 0, 1))
            .unwrap()
            .entries()
            .collect::<Vec<_>>()[1],
        scalar(Resource::Focus, 2, 4)
    );
}

#[test]
fn explicit_zero_is_present_and_absent_pools_serialize_as_null() {
    let zero = single(Resource::Focus, 0, 0);
    let encoded = r#"{"stamina":null,"focus":{"start":0,"resolution":0},"mana":null}"#;
    assert_eq!(serde_json::to_string(&zero).unwrap(), encoded);
    assert_eq!(serde_json::from_str::<CostBundle>(encoded).unwrap(), zero);
    assert_eq!(
        zero.entries().collect::<Vec<_>>(),
        [scalar(Resource::Focus, 0, 0)]
    );
    assert_eq!(zero.checked_add(zero), Ok(zero));
    let combined = zero.checked_add(single(Resource::Stamina, 0, 0)).unwrap();
    assert_eq!(
        combined.entries().collect::<Vec<_>>(),
        [
            scalar(Resource::Stamina, 0, 0),
            scalar(Resource::Focus, 0, 0)
        ]
    );
    assert_ne!(combined, zero);
}

#[test]
fn constructors_and_addition_reject_numeric_and_arithmetic_overflow() {
    let bound = crate::limits::NUMERIC as u32;
    for (start, resolution) in [
        (bound + 1, 0),
        (0, bound + 1),
        (bound, 1),
        (u32::MAX, 1),
        (1, u32::MAX),
    ] {
        assert_eq!(
            CostBundle::single(scalar(Resource::Focus, start, resolution)),
            Err(CostError::Invalid)
        );
    }
    let full = single(Resource::Focus, bound - 1, 1);
    assert_eq!(full.checked_add(single(Resource::Focus, 0, 0)), Ok(full));
    for added in [single(Resource::Focus, 1, 0), single(Resource::Focus, 0, 1)] {
        assert_eq!(full.checked_add(added), Err(CostError::Invalid));
        assert_eq!(added.checked_add(full), Err(CostError::Invalid));
    }
    assert_eq!(
        full.entries().collect::<Vec<_>>(),
        [scalar(Resource::Focus, bound - 1, 1)]
    );
    let independent = full.checked_add(single(Resource::Mana, bound, 0)).unwrap();
    assert_eq!(independent.entries().count(), 2);
}

#[test]
fn bundle_decoding_requires_every_named_nullable_pool_and_a_present_entry() {
    let valid = json!({"stamina": null, "focus": {"start": 0, "resolution": 0}, "mana": null});
    for field in ["stamina", "focus", "mana"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<CostBundle>(missing).is_err(),
            "missing {field}"
        );
    }
    for value in [
        json!({}),
        json!({"stamina": null, "focus": null, "mana": null}),
        json!({"stamina": null, "focus": {"start": 0, "resolution": 0}, "mana": null, "health": null}),
        json!([null, {"start": 0, "resolution": 0}, null]),
        json!({"resource": "focus", "start": 0, "resolution": 0}),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<CostBundle>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn bundle_decoding_rejects_duplicate_fields_and_malformed_phase_pairs() {
    for raw in [
        r#"{"stamina":null,"focus":null,"focus":{"start":0,"resolution":0},"mana":null}"#,
        r#"{"stamina":null,"focus":{"start":0,"start":0,"resolution":0},"mana":null}"#,
        r#"{"stamina":null,"focus":{"start":0,"resolution":0,"resolution":0},"mana":null}"#,
    ] {
        assert!(serde_json::from_str::<CostBundle>(raw).is_err(), "{raw}");
    }
    let bound = crate::limits::NUMERIC as u32;
    for phases in [
        json!({}),
        json!({"start": 0}),
        json!({"resolution": 0}),
        json!({"start": 0, "resolution": 0, "resource": "focus"}),
        json!([0, 0]),
        json!({"start": -1, "resolution": 0}),
        json!({"start": 0, "resolution": 0.5}),
        json!({"start": null, "resolution": 0}),
        json!({"start": "0", "resolution": 0}),
        json!({"start": bound, "resolution": 1}),
        json!({"start": u32::MAX, "resolution": 1}),
        json!({"start": 0, "resolution": u64::from(u32::MAX) + 1}),
    ] {
        let value = json!({"stamina": null, "focus": phases, "mana": null});
        assert!(
            serde_json::from_value::<CostBundle>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn readonly_validation_and_start_preflight_all_pools_before_any_mutation() {
    let cost = mixed();
    for scarce in Resource::ALL {
        let mut capacities = maxima([10; 3]);
        let entry = cost
            .entries()
            .find(|entry| entry.resource == scarce)
            .unwrap();
        *capacities.get_mut(scarce) = entry.start + entry.resolution - 1;
        let mut state = CostLedger::new(Resources::new(capacities, &test_ruleset()));
        let before = state.clone();
        assert_eq!(
            state.validate_start(IntentionId(1), cost),
            Err(CostError::Insufficient)
        );
        assert_eq!(state, before);
        assert_eq!(
            state.start(IntentionId(1), cost),
            Err(CostError::Insufficient)
        );
        assert_eq!(state, before, "unfunded {scarce:?}");
    }
    let mut state = ledger([3, 5, 7]);
    let before = state.clone();
    assert_eq!(
        state.validate_start(IntentionId(1), cost),
        Ok(StartCost::Started)
    );
    assert_eq!(state, before);
    assert_eq!(state.start(IntentionId(1), cost), Ok(StartCost::Started));
    assert_eq!(balances(&state), [2, 3, 4]);
    assert_eq!(available(&state), [0; 3]);
    assert_eq!(state.reservations().len(), 1);
    assert_eq!(state.reservation(IntentionId(1)), Some(cost));
}

#[test]
fn mixed_resume_is_idempotent_and_completion_pays_each_resolution_once() {
    let cost = mixed();
    let mut state = ledger([10; 3]);
    state.start(IntentionId(1), cost).unwrap();
    assert_eq!(balances(&state), [9, 8, 7]);
    assert_eq!(available(&state), [7, 5, 3]);
    let started = state.clone();
    assert_eq!(
        state.validate_start(IntentionId(1), cost),
        Ok(StartCost::Resumed)
    );
    assert_eq!(state.start(IntentionId(1), cost), Ok(StartCost::Resumed));
    assert_eq!(state, started);
    assert_eq!(state.finish(IntentionId(1)), Ok(cost));
    assert_eq!(balances(&state), [7, 5, 3]);
    assert_eq!(available(&state), [7, 5, 3]);
    assert!(state.reservations().is_empty());
    let finished = state.clone();
    assert_eq!(state.finish(IntentionId(1)), Err(CostError::Unknown));
    assert_eq!(state, finished);
}

#[test]
fn conflicting_phase_or_presence_and_invalid_ids_leave_every_pool_unchanged() {
    let cost = single(Resource::Focus, 1, 1);
    let mut state = ledger([5; 3]);
    state.start(IntentionId(1), cost).unwrap();
    let before = state.clone();
    for conflicting in [
        single(Resource::Focus, 2, 0),
        single(Resource::Focus, 0, 2),
        single(Resource::Mana, 1, 1),
        cost.checked_add(single(Resource::Stamina, 0, 0)).unwrap(),
    ] {
        assert_eq!(
            state.validate_start(IntentionId(1), conflicting),
            Err(CostError::Conflict)
        );
        assert_eq!(
            state.start(IntentionId(1), conflicting),
            Err(CostError::Conflict)
        );
        assert_eq!(state, before);
    }
    for id in [IntentionId(0), IntentionId(u64::MAX)] {
        assert_eq!(state.validate_start(id, cost), Err(CostError::Invalid));
        assert_eq!(state.start(id, cost), Err(CostError::Invalid));
        assert_eq!(state, before);
    }
}

#[test]
fn previous_mixed_holds_prevent_spending_another_intentions_resolution() {
    let mut state = ledger([10; 3]);
    state.start(IntentionId(1), mixed()).unwrap();
    let before = state.clone();
    let costly = single(Resource::Stamina, 1, 1)
        .checked_add(single(Resource::Mana, 4, 0))
        .unwrap();
    assert_eq!(
        state.start(IntentionId(2), costly),
        Err(CostError::Insufficient)
    );
    assert_eq!(state, before);
    state.finish(IntentionId(1)).unwrap();
    assert_eq!(balances(&state), [7, 5, 3]);
}

#[test]
fn finish_preflight_rejects_a_late_unfunded_pool_without_partial_payment() {
    let mut state = ledger([10; 3]);
    state.start(IntentionId(1), mixed()).unwrap();
    // Public mutations preserve funded holds. Deliberate private corruption
    // exercises the defensive finish preflight before the first pool is spent.
    assert!(state.resources.spend(Resource::Mana, 4));
    let before = state.clone();
    assert_eq!(state.finish(IntentionId(1)), Err(CostError::Insufficient));
    assert_eq!(state, before);
    assert_eq!(balances(&state), [9, 8, 3]);
    assert_eq!(state.reservation(IntentionId(1)), Some(mixed()));
}

#[test]
fn cancel_returns_the_whole_bundle_and_releases_only_unpaid_costs() {
    let cost = mixed();
    let mut state = ledger([10; 3]);
    state.start(IntentionId(1), cost).unwrap();
    assert_eq!(state.cancel(IntentionId(1)), Some(cost));
    assert_eq!(balances(&state), [9, 8, 7]);
    assert_eq!(available(&state), [9, 8, 7]);
    let canceled = state.clone();
    assert_eq!(state.cancel(IntentionId(1)), None);
    assert_eq!(state.finish(IntentionId(1)), Err(CostError::Unknown));
    assert_eq!(state, canceled);
}

#[test]
fn zero_cost_is_a_real_resumable_finishable_and_cancelable_hold() {
    let zero = single(Resource::Focus, 0, 0);
    let mut state = ledger([0; 3]);
    assert_eq!(state.start(IntentionId(1), zero), Ok(StartCost::Started));
    assert_eq!(state.reservation(IntentionId(1)), Some(zero));
    assert_eq!(state.reservations().len(), 1);
    let before = state.clone();
    assert_eq!(state.start(IntentionId(1), zero), Ok(StartCost::Resumed));
    assert_eq!(state, before);
    assert_eq!(state.finish(IntentionId(1)), Ok(zero));
    assert_eq!(state.finish(IntentionId(1)), Err(CostError::Unknown));
    state.start(IntentionId(2), zero).unwrap();
    assert_eq!(state.cancel(IntentionId(2)), Some(zero));
    assert_eq!(balances(&state), [0; 3]);
    assert!(state.reservations().is_empty());
}

#[test]
fn maxima_cancels_whole_bundle_and_recomputes_later_pools_without_overcanceling() {
    let mut state = ledger([10; 3]);
    let old = single(Resource::Focus, 0, 2);
    let shared = single(Resource::Stamina, 1, 3)
        .checked_add(single(Resource::Focus, 1, 3))
        .unwrap()
        .checked_add(single(Resource::Mana, 1, 4))
        .unwrap();
    let newer = single(Resource::Focus, 0, 1);
    state.start(IntentionId(1), old).unwrap();
    state.start(IntentionId(2), shared).unwrap();
    state.start(IntentionId(3), newer).unwrap();
    assert_eq!(state.set_maxima(maxima([0, 3, 0])), [IntentionId(2)]);
    assert_eq!(state.reservation(IntentionId(2)), None);
    assert_eq!(state.reservation(IntentionId(1)), Some(old));
    assert_eq!(state.reservation(IntentionId(3)), Some(newer));
    assert_eq!(balances(&state), [0, 3, 0]);
    assert_eq!(available(&state), [0; 3]);
    assert_eq!(state.finish(IntentionId(1)), Ok(old));
    assert_eq!(state.finish(IntentionId(3)), Ok(newer));
    assert_eq!(balances(&state), [0; 3]);
    assert_eq!(state.set_maxima(maxima([10; 3])), []);
    assert_eq!(balances(&state), [0; 3]);
}

#[test]
fn maxima_cancellation_preserves_resource_order_then_newest_id_and_never_repeats() {
    let mut state = ledger([10; 3]);
    let first = single(Resource::Stamina, 0, 2)
        .checked_add(single(Resource::Mana, 0, 2))
        .unwrap();
    let second = single(Resource::Focus, 0, 2);
    let third = single(Resource::Stamina, 0, 2)
        .checked_add(single(Resource::Focus, 0, 2))
        .unwrap();
    let fourth = single(Resource::Mana, 0, 1);
    for (id, cost) in [(1, first), (2, second), (3, third), (4, fourth)] {
        state.start(IntentionId(id), cost).unwrap();
    }
    assert_eq!(
        state.set_maxima(maxima([2, 0, 0])),
        [
            IntentionId(3),
            IntentionId(2),
            IntentionId(4),
            IntentionId(1)
        ]
    );
    assert!(state.reservations().is_empty());
    assert_eq!(available(&state), [2, 0, 0]);
}

#[test]
fn explicit_zero_hold_stays_funded_when_capacity_falls_to_zero() {
    let mut state = ledger([1, 0, 0]);
    state
        .start(IntentionId(1), single(Resource::Stamina, 0, 1))
        .unwrap();
    state
        .start(IntentionId(2), single(Resource::Stamina, 0, 0))
        .unwrap();
    assert_eq!(state.set_maxima(maxima([0; 3])), [IntentionId(1)]);
    assert_eq!(state.reservations().len(), 1);
    assert_eq!(
        state.reservation(IntentionId(2)),
        Some(single(Resource::Stamina, 0, 0))
    );
}

#[test]
fn recorded_bundle_roundtrip_and_resume_never_recharge_paid_phases() {
    let cost = mixed().checked_add(single(Resource::Focus, 1, 1)).unwrap();
    let zero = single(Resource::Mana, 0, 0);
    let mut original = ledger([20; 3]);
    original.start(IntentionId(9), cost).unwrap();
    original.start(IntentionId(10), zero).unwrap();
    original.advance_active(99);
    let recorded: Vec<_> = original
        .reservations()
        .iter()
        .map(|(&id, &cost)| (id, cost))
        .collect();
    let encoded = serde_json::to_string(&recorded).unwrap();
    let decoded = serde_json::from_str(&encoded).unwrap();
    let mut restored = CostLedger::from_recorded(original.resources().clone(), decoded).unwrap();
    assert_eq!(restored, original);
    for (id, cost) in [(IntentionId(9), cost), (IntentionId(10), zero)] {
        assert_eq!(restored.start(id, cost), Ok(StartCost::Resumed));
        assert_eq!(restored, original);
    }
    original.advance_active(1);
    restored.advance_active(1);
    assert_eq!(restored, original);
    assert_eq!(
        restored.finish(IntentionId(9)),
        original.finish(IntentionId(9))
    );
    assert_eq!(
        restored.finish(IntentionId(10)),
        original.finish(IntentionId(10))
    );
    assert_eq!(restored, original);
}

#[test]
fn recorded_holds_reject_duplicate_invalid_and_unfunded_owners_in_every_pool() {
    let cost = mixed();
    let resources = ledger([10; 3]).resources().clone();
    assert_eq!(
        CostLedger::from_recorded(
            resources.clone(),
            vec![(IntentionId(1), cost), (IntentionId(1), cost)]
        ),
        Err(CostError::Conflict)
    );
    for id in [0, u64::MAX] {
        assert_eq!(
            CostLedger::from_recorded(resources.clone(), vec![(IntentionId(id), cost)]),
            Err(CostError::Invalid)
        );
    }
    for scarce in Resource::ALL {
        let mut capacities = maxima([10; 3]);
        let entry = cost
            .entries()
            .find(|entry| entry.resource == scarce)
            .unwrap();
        *capacities.get_mut(scarce) = entry.resolution * 2 - 1;
        let resources = Resources::new(capacities, &test_ruleset());
        assert_eq!(
            CostLedger::from_recorded(
                resources,
                vec![(IntentionId(1), cost), (IntentionId(2), cost)]
            ),
            Err(CostError::Insufficient),
            "unfunded {scarce:?}"
        );
    }
}

#[test]
fn recorded_validation_defends_against_invalid_private_bundle_state() {
    let resources = ledger([10; 3]).resources().clone();
    for cost in [
        CostBundle(ByResource::from_fn(|_| None)),
        CostBundle(ByResource {
            stamina: Some(CostPhases {
                start: u32::MAX,
                resolution: 1,
            }),
            focus: None,
            mana: None,
        }),
    ] {
        assert_eq!(
            CostLedger::from_recorded(resources.clone(), vec![(IntentionId(1), cost)]),
            Err(CostError::Invalid)
        );
        let mut state = CostLedger::new(resources.clone());
        let before = state.clone();
        assert_eq!(state.start(IntentionId(1), cost), Err(CostError::Invalid));
        assert_eq!(state, before);
    }
}

#[test]
fn zero_holds_share_runtime_and_recorded_limits_and_resume_at_capacity() {
    let limit = crate::limits::COST_HOLDS;
    let zero = single(Resource::Focus, 0, 0);
    let mut state = ledger([0; 3]);
    for id in 1..=limit {
        state.start(IntentionId(id as u64), zero).unwrap();
    }
    let before = state.clone();
    assert_eq!(state.start(IntentionId(1), zero), Ok(StartCost::Resumed));
    assert_eq!(
        state.validate_start(IntentionId(limit as u64 + 1), zero),
        Err(CostError::Limit)
    );
    assert_eq!(
        state.start(IntentionId(limit as u64 + 1), zero),
        Err(CostError::Limit)
    );
    assert_eq!(state, before);
    let recorded = (1..=limit)
        .map(|id| (IntentionId(id as u64), zero))
        .collect();
    assert_eq!(
        CostLedger::from_recorded(state.resources().clone(), recorded).unwrap(),
        state
    );
    let too_many = (1..=limit + 1)
        .map(|id| (IntentionId(id as u64), zero))
        .collect();
    assert_eq!(
        CostLedger::from_recorded(state.resources().clone(), too_many),
        Err(CostError::Limit)
    );
    state.cancel(IntentionId(1)).unwrap();
    assert_eq!(
        state.start(IntentionId(limit as u64 + 1), zero),
        Ok(StartCost::Started)
    );
}

#[test]
fn recorded_aggregate_is_widened_and_funding_is_separate_from_individual_numeric_bounds() {
    let limit = crate::limits::COST_HOLDS;
    let bound = crate::limits::NUMERIC as u32;
    let cost = single(Resource::Focus, 0, bound);
    let recorded: Vec<_> = (1..=limit)
        .map(|id| (IntentionId(id as u64), cost))
        .collect();
    assert_eq!(
        CostLedger::from_recorded(ledger([bound; 3]).resources().clone(), recorded.clone()),
        Err(CostError::Insufficient)
    );
    let total = bound.checked_mul(limit as u32).unwrap();
    let restored =
        CostLedger::from_recorded(ledger([0, total, 0]).resources().clone(), recorded).unwrap();
    assert_eq!(restored.reserved(Resource::Focus), u64::from(total));
    assert_eq!(restored.available(Resource::Focus), 0);
}

#[test]
fn zero_resolution_entry_does_not_cancel_an_unrelated_funded_hold() {
    let mut state = ledger([4, 4, 0]);
    state
        .start(IntentionId(1), single(Resource::Stamina, 0, 2))
        .unwrap();
    let newer = single(Resource::Stamina, 1, 0)
        .checked_add(single(Resource::Focus, 0, 2))
        .unwrap();
    state.start(IntentionId(2), newer).unwrap();
    assert_eq!(state.set_maxima(maxima([1, 4, 0])), [IntentionId(1)]);
    assert_eq!(state.reservation(IntentionId(2)), Some(newer));
    assert_eq!(state.available(Resource::Focus), 2);
    state.finish(IntentionId(2)).unwrap();
    assert_eq!(state.resources().balance(Resource::Focus), 2);
}

#[test]
fn ai_budget_checks_each_frozen_pool_and_its_reserve_threshold() {
    let mut state = ledger([10, 10, 10]);
    let cost = single(Resource::Stamina, 1, 1)
        .checked_add(single(Resource::Focus, 0, 4))
        .unwrap();
    assert!(crate::ai::resource_budget_satisfies(&state, cost, 50));
    state
        .start(IntentionId(1), single(Resource::Focus, 0, 6))
        .unwrap();
    assert!(crate::ai::resource_budget_satisfies(&state, cost, 0));
    assert!(!crate::ai::resource_budget_satisfies(&state, cost, 50));
    state
        .start(IntentionId(2), single(Resource::Focus, 0, 1))
        .unwrap();
    assert!(!crate::ai::resource_budget_satisfies(&state, cost, 0));
}
