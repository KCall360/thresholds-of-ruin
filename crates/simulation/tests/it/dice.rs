use tor_simulation::dice::{roll_check, DiceError, DicePool, Edge};

#[test]
fn three_d6_checks_have_bounded_natural_totals() {
    for seed in 0..256 {
        let mut state = seed;
        let result = roll_check(&mut state, Edge::default());
        assert!(
            (3..=18).contains(&result.kept),
            "invalid 3d6 total at seed {seed}"
        );
    }
}

#[test]
fn three_d6_check_allocates_up_to_three_edge_dice() {
    let mut state = 42;
    let mut budget = Edge::from_counts(7, 2);
    let result = roll_check(&mut state, budget.take(3));
    assert_eq!(result.kept, 9);
    assert_eq!(budget.balance(), 2);
}

#[test]
fn check_capacity_clamps_extreme_edge_without_extra_random_draws() {
    for positive in [false, true] {
        let counts = |value| {
            if positive {
                Edge::from_counts(value, 0)
            } else {
                Edge::from_counts(0, value)
            }
        };
        let mut extreme = 42;
        let mut bounded = 42;
        assert_eq!(
            roll_check(&mut extreme, counts(u32::MAX)),
            roll_check(&mut bounded, counts(3))
        );
        assert_eq!(extreme, bounded);
    }
}

#[test]
fn ordinary_three_d6_checks_have_pinned_deterministic_vectors() {
    for (seed, expected) in [
        (0, [5, 8, 15, 10, 14, 9, 10, 10, 13, 15, 9, 8]),
        (42, [5, 7, 7, 14, 8, 13, 6, 8, 9, 8, 13, 9]),
        (u64::MAX, [9, 4, 6, 9, 9, 8, 14, 11, 5, 13, 14, 12]),
    ] {
        let mut state = seed;
        for value in expected {
            let check = roll_check(&mut state, Edge::from_counts(0, 0));
            assert_eq!(check.kept, value);
            assert_eq!(check.rolled().len(), 3);
            assert_eq!(check.retained().iter().sum::<u16>(), u16::from(check.kept));
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
fn five_advantages_cover_three_check_and_two_damage_dice() {
    let mut state = 42;
    let mut budget = Edge::from_counts(7, 2);
    let check = roll_check(&mut state, budget.take(3));
    assert_eq!(check.rolled(), [2, 2, 1, 1, 5, 1]);
    assert_eq!(*check.retained(), [5, 2, 2]);
    assert_eq!(check.kept, 9);
    let pool = DicePool::new(2, 6, 2).unwrap();
    let damage = pool.roll(&mut state, budget.take(pool.count()));
    assert_eq!(damage.rolled, [2, 3, 2, 3]);
    assert_eq!(damage.kept, [3, 3]);
    assert_eq!(damage.total, 8);
    assert!(budget.is_neutral());
}

#[test]
fn disadvantage_keeps_the_lowest_results_from_each_pool() {
    let mut state = 42;
    let mut budget = Edge::from_counts(1, 6);
    let check = roll_check(&mut state, budget.take(3));
    assert_eq!(check.rolled(), [2, 2, 1, 1, 5, 1]);
    assert_eq!(*check.retained(), [1, 1, 1]);
    assert_eq!(check.kept, 3);
    let pool = DicePool::new(2, 6, 2).unwrap();
    let damage = pool.roll(&mut state, budget.take(pool.count()));
    assert_eq!(damage.rolled, [2, 3, 2, 3]);
    assert_eq!(damage.kept, [2, 2]);
    assert_eq!(damage.total, 6);
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
fn identical_explicit_random_state_reproduces_continuation() {
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

#[test]
fn dice_constructor_and_record_share_named_structural_limits() {
    use tor_simulation::limits;
    let maximum = DicePool::new(limits::DICE, limits::DICE_SIDES, 0).unwrap();
    let encoded = serde_json::to_value(maximum).unwrap();
    assert_eq!(
        serde_json::from_value::<DicePool>(encoded.clone()).unwrap(),
        maximum
    );
    for (field, value) in [
        ("count", i64::from(limits::DICE) + 1),
        ("sides", i64::from(limits::DICE_SIDES) + 1),
        ("bonus", limits::NUMERIC + 1),
        ("bonus", -limits::NUMERIC - 1),
    ] {
        let mut invalid = encoded.clone();
        invalid[field] = value.into();
        assert!(serde_json::from_value::<DicePool>(invalid).is_err());
    }
}
