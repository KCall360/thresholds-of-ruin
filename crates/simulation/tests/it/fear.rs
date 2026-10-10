use tor_simulation::fear::{FearError, FearState, FearUpdate};
use tor_simulation::ActorId;

#[test]
fn expiry_deadline_tracks_refresh_simultaneous_sources_and_immunity() {
    let mut fear = FearState::default();
    assert_eq!(fear.next_expiry_in(), None);
    fear.apply(ActorId(1), 100, false).unwrap();
    fear.apply(ActorId(2), 200, false).unwrap();
    fear.advance_active(99);
    assert_eq!(fear.next_expiry_in(), Some(1));
    fear.apply(ActorId(1), 101, false).unwrap();
    assert_eq!(fear.next_expiry_in(), Some(101));
    let restored = FearState::from_recorded(vec![(ActorId(1), 101), (ActorId(2), 101)]).unwrap();
    assert_eq!(restored.next_expiry_in(), fear.next_expiry_in());
    fear.advance_active(101);
    assert_eq!(fear.next_expiry_in(), None);
    fear.apply(ActorId(3), 100, false).unwrap();
    fear.reconcile_immunity(true);
    assert_eq!(fear.next_expiry_in(), None);
}

#[test]
fn restoring_fear_preserves_remaining_durations_and_rejects_duplicates() {
    let mut original = FearState::default();
    original.apply(ActorId(1), 300, false).unwrap();
    original.apply(ActorId(2), 400, false).unwrap();
    original.advance_active(99);
    let mut restored =
        FearState::from_recorded(vec![(ActorId(1), 201), (ActorId(2), 301)]).unwrap();
    assert_eq!(restored, original);
    original.advance_active(201);
    restored.advance_active(201);
    assert_eq!(restored, original);
    assert!(FearState::from_recorded(vec![(ActorId(1), 201), (ActorId(1), 301)]).is_err());
    assert!(FearState::from_recorded(vec![(ActorId(0), 201)]).is_err());
    assert!(FearState::from_recorded(vec![(ActorId(1), 0)]).is_err());
    assert!(FearState::from_recorded((1..=129).map(|id| (ActorId(id), 1)).collect()).is_err());
}

#[test]
fn fear_refreshes_without_stacking_and_only_penalizes_its_causer() {
    let mut fear = FearState::default();
    assert_eq!(fear.apply(ActorId(1), 300, false), Ok(FearUpdate::Applied));
    fear.advance_active(100);
    assert_eq!(fear.remaining(ActorId(1)), Some(200));
    assert_eq!(
        fear.apply(ActorId(1), 300, false),
        Ok(FearUpdate::Refreshed)
    );
    assert_eq!(fear.remaining(ActorId(1)), Some(300));
    assert_eq!(fear.disadvantages_against(ActorId(1)), 1);
    assert_eq!(fear.disadvantages_against(ActorId(2)), 0);
    fear.apply(ActorId(1), 100, false).unwrap();
    assert_eq!(fear.remaining(ActorId(1)), Some(300));
}

#[test]
fn different_causers_expire_independently_in_active_time() {
    let mut fear = FearState::default();
    fear.apply(ActorId(1), 300, false).unwrap();
    fear.apply(ActorId(2), 400, false).unwrap();
    fear.advance_active(300);
    assert_eq!(fear.remaining(ActorId(1)), None);
    assert_eq!(fear.remaining(ActorId(2)), Some(100));
    fear.advance_active(u64::MAX);
    assert!(fear.sources().is_empty());
}

#[test]
fn immunity_clears_relations_without_restoring_them_when_removed() {
    let mut fear = FearState::default();
    fear.apply(ActorId(1), 300, false).unwrap();
    fear.reconcile_immunity(true);
    assert!(fear.sources().is_empty());
    assert_eq!(fear.apply(ActorId(1), 300, true), Ok(FearUpdate::Immune));
    fear.reconcile_immunity(false);
    assert!(fear.sources().is_empty());
}

#[test]
fn malformed_sources_durations_and_excess_relations_reject_atomically() {
    let mut fear = FearState::default();
    assert_eq!(
        fear.apply(ActorId(0), 300, false),
        Err(FearError::InvalidSource)
    );
    assert_eq!(
        fear.apply(ActorId(1), 0, false),
        Err(FearError::InvalidDuration)
    );
    assert_eq!(
        fear.apply(ActorId(1), u64::MAX, false),
        Err(FearError::InvalidDuration)
    );
    for id in 1..=128 {
        fear.apply(ActorId(id), 300, false).unwrap();
    }
    let before = fear.clone();
    assert_eq!(
        fear.apply(ActorId(129), 300, false),
        Err(FearError::TooManySources)
    );
    assert_eq!(fear, before);
    assert_eq!(
        fear.apply(ActorId(1), 400, false),
        Ok(FearUpdate::Refreshed)
    );
}
