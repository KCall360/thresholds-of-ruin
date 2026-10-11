use tor_simulation::attributes::{Attributes, ManaBinding};
use tor_simulation::resources::{Resource, ResourceMaxima, Resources};

#[test]
fn recovery_deadlines_follow_partial_phases_and_remove_full_pools() {
    let maxima = ResourceMaxima {
        stamina: 2,
        focus: 2,
        mana: 2,
    };
    let mut resources = Resources::new(maxima);
    assert_eq!(resources.next_recovery_in(), None);
    for resource in Resource::ALL {
        assert!(resources.spend(resource, 1));
    }
    assert_eq!(resources.next_recovery_in(), Some(100));
    resources.advance_active(99);
    assert_eq!(resources.next_recovery_in(), Some(1));
    resources.advance_active(1);
    assert_eq!(resources.next_recovery_in(), Some(200));
    resources.advance_active(200);
    assert_eq!(resources.next_recovery_in(), Some(700));
    let restored = Resources::from_recorded(maxima, [2, 2, 1], [0, 0, 300]).unwrap();
    assert_eq!(restored.next_recovery_in(), resources.next_recovery_in());
    resources.advance_active(700);
    assert_eq!(resources.next_recovery_in(), None);
    resources.spend(Resource::Stamina, 1);
    assert_eq!(resources.next_recovery_in(), Some(100));
    resources.set_maxima(ResourceMaxima {
        stamina: 1,
        focus: 2,
        mana: 2,
    });
    assert_eq!(resources.next_recovery_in(), None);
}

#[test]
fn restoring_resource_balances_preserves_fractional_recovery() {
    let maxima = ResourceMaxima {
        stamina: 3,
        focus: 3,
        mana: 3,
    };
    let mut original = Resources::new(maxima);
    for resource in Resource::ALL {
        original.spend(resource, 2);
    }
    original.advance_active(99);
    let mut restored = Resources::from_recorded(maxima, [1; 3], [99; 3]).unwrap();
    assert_eq!(restored, original);
    for ticks in [1, 199, 1, 700] {
        original.advance_active(ticks);
        restored.advance_active(ticks);
        assert_eq!(restored, original);
        for resource in Resource::ALL {
            assert_eq!(
                restored.recovery_elapsed(resource),
                original.recovery_elapsed(resource)
            );
        }
    }
}

#[test]
fn impossible_balances_recovery_phases_and_stored_full_pool_progress_reject() {
    let maxima = ResourceMaxima {
        stamina: 3,
        focus: 3,
        mana: 0,
    };
    assert!(Resources::from_recorded(maxima, [4, 0, 0], [0; 3]).is_err());
    assert!(Resources::from_recorded(maxima, [0, 0, 1], [0; 3]).is_err());
    for phase in [[100, 0, 0], [0, 300, 0], [0, 0, 1000], [0, 0, 1]] {
        assert!(Resources::from_recorded(maxima, [0; 3], phase).is_err());
    }
    assert!(Resources::from_recorded(maxima, [3, 0, 0], [1, 0, 0]).is_err());
}

#[test]
fn maxima_use_strength_willpower_and_the_selected_magical_binding() {
    let attributes = Attributes::new([3, 0, 1, 2, 4, 5]).unwrap();
    assert_eq!(
        ResourceMaxima::derived(attributes, ManaBinding::Awareness, true),
        ResourceMaxima {
            stamina: 5,
            focus: 4,
            mana: 6
        }
    );
    assert_eq!(
        ResourceMaxima::derived(attributes, ManaBinding::Presence, true).mana,
        7
    );
    assert_eq!(
        ResourceMaxima::derived(attributes, ManaBinding::Presence, false).mana,
        0
    );
}

#[test]
fn three_pools_recover_on_distinct_active_time_schedules() {
    let mut resources = Resources::new(ResourceMaxima {
        stamina: 10,
        focus: 10,
        mana: 10,
    });
    for resource in Resource::ALL {
        assert!(resources.spend(resource, 10));
    }
    resources.advance_active(99);
    for resource in Resource::ALL {
        assert_eq!(resources.balance(resource), 0);
    }
    resources.advance_active(1);
    assert_eq!(resources.balance(Resource::Stamina), 1);
    resources.advance_active(200);
    assert_eq!(resources.balance(Resource::Stamina), 3);
    assert_eq!(resources.balance(Resource::Focus), 1);
    assert_eq!(resources.balance(Resource::Mana), 0);
    resources.advance_active(700);
    assert_eq!(resources.balance(Resource::Stamina), 10);
    assert_eq!(resources.balance(Resource::Focus), 3);
    assert_eq!(resources.balance(Resource::Mana), 1);
}

#[test]
fn capacity_saturation_discards_both_whole_and_fractional_recovery() {
    let mut resources = Resources::new(ResourceMaxima {
        stamina: 2,
        focus: 2,
        mana: 2,
    });
    assert!(resources.spend(Resource::Stamina, 1));
    resources.advance_active(199);
    assert_eq!(resources.balance(Resource::Stamina), 2);
    assert!(resources.spend(Resource::Stamina, 1));
    resources.advance_active(1);
    assert_eq!(resources.balance(Resource::Stamina), 1);
    resources.advance_active(99);
    assert_eq!(resources.balance(Resource::Stamina), 2);
    resources.advance_active(u64::MAX);
    assert!(resources.spend(Resource::Stamina, 1));
    resources.advance_active(99);
    assert_eq!(resources.balance(Resource::Stamina), 1);
}

#[test]
fn recovery_partitioning_preserves_fractional_time() {
    let maxima = ResourceMaxima {
        stamina: 100,
        focus: 100,
        mana: 100,
    };
    let mut single = Resources::new(maxima);
    for resource in Resource::ALL {
        assert!(single.spend(resource, 100));
    }
    let mut partitioned = single.clone();
    single.advance_active(1999);
    for ticks in [1, 98, 301, 700, 899] {
        partitioned.advance_active(ticks);
    }
    assert_eq!(single, partitioned);
    single.advance_active(1);
    partitioned.advance_active(1);
    assert_eq!(single, partitioned);
    assert_eq!(single.balance(Resource::Mana), 2);
}

#[test]
fn changing_maxima_clamps_balances_without_refilling_them() {
    let mut resources = Resources::new(ResourceMaxima {
        stamina: 4,
        focus: 4,
        mana: 4,
    });
    assert!(resources.spend(Resource::Mana, 2));
    resources.set_maxima(ResourceMaxima {
        stamina: 2,
        focus: 8,
        mana: 0,
    });
    assert_eq!(resources.balance(Resource::Stamina), 2);
    assert_eq!(resources.balance(Resource::Focus), 4);
    assert_eq!(resources.balance(Resource::Mana), 0);
    resources.set_maxima(ResourceMaxima {
        stamina: 8,
        focus: 8,
        mana: 8,
    });
    assert_eq!(resources.balance(Resource::Stamina), 2);
    assert_eq!(resources.balance(Resource::Focus), 4);
    assert_eq!(resources.balance(Resource::Mana), 0);
}

#[test]
fn insufficient_spending_and_extreme_time_are_safe_and_atomic() {
    let mut resources = Resources::new(ResourceMaxima {
        stamina: u32::MAX,
        focus: 1,
        mana: 0,
    });
    assert!(resources.spend(Resource::Stamina, u32::MAX));
    let before = resources.clone();
    assert!(!resources.spend(Resource::Focus, 2));
    assert!(!resources.spend(Resource::Mana, 1));
    assert_eq!(resources, before);
    assert!(resources.spend(Resource::Mana, 0));
    resources.advance_active(u64::MAX);
    assert_eq!(resources.balance(Resource::Stamina), u32::MAX);
    assert_eq!(resources.balance(Resource::Mana), 0);
}
