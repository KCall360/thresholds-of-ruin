use tor_simulation::costs::{CostError, CostLedger, ResourceCost, StartCost};
use tor_simulation::resources::{Resource, ResourceMaxima, Resources};
use tor_simulation::IntentionId;

#[test]
fn restoring_holds_does_not_repay_started_costs() {
    let mut original = ledger(4);
    original.start(IntentionId(9), cost()).unwrap();
    original.advance_active(99);
    let mut restored =
        CostLedger::from_recorded(original.resources().clone(), vec![(IntentionId(9), cost())])
            .unwrap();
    assert_eq!(restored, original);
    assert_eq!(
        restored.start(IntentionId(9), cost()),
        Ok(StartCost::Resumed)
    );
    assert_eq!(restored, original);
    original.advance_active(1);
    restored.advance_active(1);
    assert_eq!(
        restored.finish(IntentionId(9)),
        original.finish(IntentionId(9))
    );
    assert_eq!(restored, original);
}

#[test]
fn invalid_duplicate_and_unfunded_restored_holds_reject() {
    let resources = ledger(1).resources().clone();
    assert_eq!(
        CostLedger::from_recorded(
            resources.clone(),
            vec![(IntentionId(1), cost()), (IntentionId(1), cost())]
        ),
        Err(CostError::Conflict)
    );
    assert_eq!(
        CostLedger::from_recorded(
            resources.clone(),
            vec![(IntentionId(1), cost()), (IntentionId(2), cost())]
        ),
        Err(CostError::Insufficient)
    );
    for id in [0, u64::MAX] {
        assert_eq!(
            CostLedger::from_recorded(resources.clone(), vec![(IntentionId(id), cost())]),
            Err(CostError::Invalid)
        );
    }
    let invalid = ResourceCost {
        resolution: u32::MAX,
        ..cost()
    };
    assert_eq!(
        CostLedger::from_recorded(resources.clone(), vec![(IntentionId(1), invalid)]),
        Err(CostError::Invalid)
    );
    assert_eq!(
        CostLedger::from_recorded(
            resources,
            (1..=65).map(|id| (IntentionId(id), cost())).collect()
        ),
        Err(CostError::Limit)
    );
}

fn ledger(stamina: u32) -> CostLedger {
    CostLedger::new(Resources::new(ResourceMaxima {
        stamina,
        focus: 3,
        mana: 3,
    }))
}
fn cost() -> ResourceCost {
    ResourceCost {
        resource: Resource::Stamina,
        start: 1,
        resolution: 1,
    }
}

#[test]
fn readonly_start_validation_matches_commit_without_charging_or_holding() {
    let mut ledger = ledger(4);
    let before = ledger.clone();
    assert_eq!(
        ledger.validate_start(IntentionId(1), cost()),
        Ok(StartCost::Started)
    );
    assert_eq!(ledger, before);
    ledger.start(IntentionId(1), cost()).unwrap();
    let before = ledger.clone();
    assert_eq!(
        ledger.validate_start(IntentionId(1), cost()),
        Ok(StartCost::Resumed)
    );
    let conflicting = ResourceCost { start: 2, ..cost() };
    assert_eq!(
        ledger.validate_start(IntentionId(1), conflicting),
        Err(CostError::Conflict)
    );
    assert_eq!(
        ledger.validate_start(IntentionId(0), cost()),
        Err(CostError::Invalid)
    );
    assert_eq!(
        ledger.validate_start(IntentionId(u64::MAX), cost()),
        Err(CostError::Invalid)
    );
    assert_eq!(
        ledger.validate_start(
            IntentionId(2),
            ResourceCost {
                start: u32::MAX,
                ..cost()
            }
        ),
        Err(CostError::Invalid)
    );
    assert_eq!(ledger, before);
    ledger.start(IntentionId(2), cost()).unwrap();
    let before = ledger.clone();
    assert_eq!(
        ledger.validate_start(IntentionId(3), cost()),
        Err(CostError::Insufficient)
    );
    assert_eq!(ledger, before);
    for (id, candidate) in [(1, cost()), (3, cost()), (1, conflicting)] {
        let expected = ledger.validate_start(IntentionId(id), candidate);
        let mut committed = ledger.clone();
        assert_eq!(committed.start(IntentionId(id), candidate), expected);
    }
}

#[test]
fn preparation_requires_the_whole_cost_before_any_payment() {
    let mut ledger = ledger(1);
    let before = ledger.clone();
    assert_eq!(
        ledger.start(IntentionId(1), cost()),
        Err(CostError::Insufficient)
    );
    assert_eq!(ledger, before);
}

#[test]
fn resume_does_not_repay_and_resolution_charges_only_the_reserved_portion() {
    let mut ledger = ledger(3);
    assert_eq!(ledger.start(IntentionId(1), cost()), Ok(StartCost::Started));
    assert_eq!(ledger.resources().balance(Resource::Stamina), 2);
    assert_eq!(ledger.available(Resource::Stamina), 1);
    let prepared = ledger.clone();
    assert_eq!(ledger.start(IntentionId(1), cost()), Ok(StartCost::Resumed));
    assert_eq!(ledger, prepared);
    assert_eq!(ledger.finish(IntentionId(1)), Ok(cost()));
    assert_eq!(ledger.resources().balance(Resource::Stamina), 1);
    assert_eq!(ledger.available(Resource::Stamina), 1);
    let finished = ledger.clone();
    assert_eq!(ledger.finish(IntentionId(1)), Err(CostError::Unknown));
    assert_eq!(ledger, finished);
}

#[test]
fn cancellation_releases_unpaid_reservation_and_preserves_paid_cost() {
    let mut ledger = ledger(3);
    ledger.start(IntentionId(1), cost()).unwrap();
    assert_eq!(ledger.cancel(IntentionId(1)), Some(cost()));
    assert_eq!(ledger.resources().balance(Resource::Stamina), 2);
    assert_eq!(ledger.available(Resource::Stamina), 2);
    assert_eq!(ledger.cancel(IntentionId(1)), None);
}

#[test]
fn reservation_prevents_double_spending_and_cost_conflicts_are_atomic() {
    let mut ledger = ledger(3);
    ledger.start(IntentionId(1), cost()).unwrap();
    let before = ledger.clone();
    assert_eq!(
        ledger.start(IntentionId(2), cost()),
        Err(CostError::Insufficient)
    );
    assert_eq!(
        ledger.start(
            IntentionId(1),
            ResourceCost {
                resolution: 0,
                ..cost()
            }
        ),
        Err(CostError::Conflict)
    );
    assert_eq!(ledger, before);
}

#[test]
fn capacity_loss_cancels_newest_unfunded_reservation_without_refund() {
    let mut ledger = ledger(5);
    ledger.start(IntentionId(1), cost()).unwrap();
    ledger.start(IntentionId(2), cost()).unwrap();
    ledger
        .start(
            IntentionId(3),
            ResourceCost {
                resource: Resource::Focus,
                ..cost()
            },
        )
        .unwrap();
    assert_eq!(
        ledger.set_maxima(ResourceMaxima {
            stamina: 1,
            focus: 3,
            mana: 3
        }),
        vec![IntentionId(2)]
    );
    assert_eq!(ledger.resources().balance(Resource::Stamina), 1);
    assert_eq!(ledger.available(Resource::Stamina), 0);
    ledger.finish(IntentionId(1)).unwrap();
    assert_eq!(ledger.resources().balance(Resource::Stamina), 0);
    assert!(ledger.reservation(IntentionId(3)).is_some());
    assert_eq!(
        ledger.set_maxima(ResourceMaxima {
            stamina: 5,
            focus: 3,
            mana: 3
        }),
        vec![]
    );
    assert_eq!(ledger.resources().balance(Resource::Stamina), 0);
}

#[test]
fn malformed_costs_ids_and_reservation_counts_are_bounded() {
    let mut ledger = ledger(1_000_000);
    assert_eq!(
        ledger.start(IntentionId(0), cost()),
        Err(CostError::Invalid)
    );
    assert_eq!(
        ledger.start(
            IntentionId(1),
            ResourceCost {
                start: u32::MAX,
                resolution: 1,
                ..cost()
            }
        ),
        Err(CostError::Invalid)
    );
    for id in 1..=64 {
        ledger.start(IntentionId(id), cost()).unwrap();
    }
    let before = ledger.clone();
    assert_eq!(
        ledger.validate_start(IntentionId(1), cost()),
        Ok(StartCost::Resumed)
    );
    assert_eq!(
        ledger.validate_start(IntentionId(65), cost()),
        Err(CostError::Limit)
    );
    assert_eq!(ledger, before);
    assert_eq!(ledger.start(IntentionId(65), cost()), Err(CostError::Limit));
    let available = ledger.available(Resource::Stamina);
    ledger.advance_active(100);
    assert_eq!(ledger.available(Resource::Stamina), available + 1);
}
