use tor_simulation::dice::{roll_check, DiceError, DicePool, Edge};

#[test]
fn ordinary_checks_preserve_the_published_combat_stream() {
    for (seed, expected) in [
        (0, [16, 1, 20, 5, 8, 11, 14, 1, 20, 11, 2, 7]),
        (42, [14, 12, 19, 5, 11, 3, 6, 9, 6, 15, 8, 7]),
        (u64::MAX, [17, 10, 2, 3, 7, 16, 6, 17, 1, 13, 10, 8]),
    ] {
        let mut state = seed;
        for value in expected {
            let check = roll_check(&mut state, Edge::from_counts(0, 0));
            assert_eq!(check.kept, value);
            assert_eq!(check.second, None);
        }
    }
}

#[test]
fn cancellation_uses_no_extra_randomness() {
    let mut plain = 42;
    let mut cancelled = 42;
    assert_eq!(
        roll_check(&mut plain, Edge::from_counts(0, 0)),
        roll_check(&mut cancelled, Edge::from_counts(u32::MAX, u32::MAX)),
    );
    assert_eq!(plain, cancelled);
}

#[test]
fn three_advantages_cover_check_then_two_damage_dice() {
    let mut state = 42;
    let mut budget = Edge::from_counts(5, 2);
    let check = roll_check(&mut state, budget.take(1));
    assert_eq!((check.first, check.second, check.kept), (14, Some(12), 14));
    let pool = DicePool::new(2, 6, 2).unwrap();
    let damage = pool.roll(&mut state, budget.take(pool.count()));
    assert_eq!(damage.rolled, [1, 1, 5, 1]);
    assert_eq!(damage.kept, [5, 1]);
    assert_eq!(damage.total, 8);
    assert!(budget.is_neutral());
}

#[test]
fn disadvantage_keeps_the_lowest_results_from_each_pool() {
    let mut state = 42;
    let mut budget = Edge::from_counts(1, 4);
    let check = roll_check(&mut state, budget.take(1));
    assert_eq!((check.first, check.second, check.kept), (14, Some(12), 12));
    let pool = DicePool::new(2, 6, 2).unwrap();
    let damage = pool.roll(&mut state, budget.take(pool.count()));
    assert_eq!(damage.rolled, [1, 1, 5, 1]);
    assert_eq!(damage.kept, [1, 1]);
    assert_eq!(damage.total, 4);
}

#[test]
fn enormous_edge_counts_do_not_expand_roll_work() {
    for edge in [
        Edge::from_counts(u32::MAX, 0),
        Edge::from_counts(0, u32::MAX),
    ] {
        let mut state = 42;
        let pool = DicePool::new(64, 1000, 0).unwrap();
        let damage = pool.roll(&mut state, edge);
        assert_eq!(damage.rolled.len(), 128);
        assert_eq!(damage.kept.len(), 64);
        assert!(damage.total <= 64_000);
    }
}

#[test]
fn pool_definitions_reject_invalid_and_unbounded_work() {
    for args in [(0, 6, 0), (65, 6, 0), (1, 0, 0), (1, 1, 0), (1, 1001, 0)] {
        assert_eq!(
            DicePool::new(args.0, args.1, args.2),
            Err(DiceError::InvalidPool)
        );
    }
    assert_eq!(
        DicePool::new(64, 1000, 1_000_000),
        Err(DiceError::InvalidPool)
    );
    assert_eq!(DicePool::new(1, 6, i32::MIN), Err(DiceError::InvalidPool));
}

#[test]
fn negative_flat_damage_is_clamped_after_dice_selection() {
    let mut state = 42;
    let damage = DicePool::new(2, 6, -100)
        .unwrap()
        .roll(&mut state, Edge::from_counts(1, 0));
    assert_eq!(damage.total, 0);
    assert_eq!(damage.kept.len(), 2);
}

#[test]
fn saved_random_state_reproduces_continuation() {
    let pool = DicePool::new(3, 8, 1).unwrap();
    let mut state = 42;
    roll_check(&mut state, Edge::from_counts(1, 0));
    let mut restored = state;
    for edge in [
        Edge::from_counts(2, 0),
        Edge::from_counts(0, 2),
        Edge::from_counts(0, 0),
    ] {
        assert_eq!(pool.roll(&mut state, edge), pool.roll(&mut restored, edge));
        assert_eq!(state, restored);
    }
}
