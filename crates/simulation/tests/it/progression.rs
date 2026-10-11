use tor_simulation::progression::{Class, CreatureType, HdError, HdLedger, HdSource, HitDie};

#[test]
fn racial_and_class_health_dice_use_the_tor_catalog() {
    for kind in CreatureType::ALL {
        let expected = match kind {
            CreatureType::Fey => 6,
            CreatureType::Construct | CreatureType::MagicalBeast | CreatureType::Ooze => 10,
            CreatureType::Dragon | CreatureType::Undead => 12,
            _ => 8,
        };
        assert_eq!(kind.health_die(), expected);
    }
    assert_eq!(CreatureType::ALL.len(), 15);
    assert_eq!(Class::Warrior.health_die(), 10);
    assert_eq!(Class::Mage.health_die(), 6);
}

#[test]
fn only_the_first_overall_hit_die_is_maximized() {
    let racial_first = HdLedger::new(vec![
        HitDie::new(HdSource::Racial, 42),
        HitDie::new(HdSource::Class(Class::Warrior), 42),
        HitDie::new(HdSource::Class(Class::Mage), 42),
    ])
    .unwrap();
    assert_eq!(
        racial_first.health_contributions(CreatureType::Animal),
        [8, 4, 2]
    );
    let class_first = HdLedger::new(vec![
        HitDie::new(HdSource::Class(Class::Mage), 42),
        HitDie::new(HdSource::Racial, 42),
    ])
    .unwrap();
    assert_eq!(
        class_first.health_contributions(CreatureType::Animal),
        [6, 6]
    );
}

#[test]
fn type_conversion_reinterprets_racial_health_without_changing_class_health() {
    let ledger = HdLedger::new(vec![
        HitDie::new(HdSource::Racial, 0),
        HitDie::new(HdSource::Racial, 42),
        HitDie::new(HdSource::Class(Class::Warrior), 42),
    ])
    .unwrap();
    let original = ledger.health_contributions(CreatureType::Animal);
    let undead = ledger.health_contributions(CreatureType::Undead);
    assert_eq!(original, [8, 6, 4]);
    assert_eq!(undead, [12, 2, 4]);
    assert_eq!(ledger.health_contributions(CreatureType::Animal), original);
    assert_eq!(ledger.health(CreatureType::Animal), 18);
    assert_eq!(ledger.health(CreatureType::Undead), 18);
}

#[test]
fn advancement_budgets_are_distinct_from_class_local_levels() {
    let sources = [
        HdSource::Racial,
        HdSource::Class(Class::Mage),
        HdSource::Racial,
        HdSource::Class(Class::Warrior),
    ];
    let ledger = HdLedger::new(
        sources
            .into_iter()
            .map(|source| HitDie::new(source, 42))
            .collect(),
    )
    .unwrap();
    assert_eq!(ledger.total_hd(), 4);
    assert_eq!(ledger.racial_hd(), 2);
    assert_eq!(ledger.class_level(Class::Mage), 1);
    assert_eq!(ledger.class_level(Class::Warrior), 1);
    assert_eq!(ledger.training_points(), 6);
    assert_eq!(ledger.attribute_opportunities(), 1);
    assert_eq!(ledger.talent_slots(), 4);
}

#[test]
fn latest_removal_preserves_retained_health_randomness_and_exposes_zero_hd() {
    let mut ledger = HdLedger::new(vec![
        HitDie::new(HdSource::Racial, 0),
        HitDie::new(HdSource::Class(Class::Warrior), 42),
    ])
    .unwrap();
    assert_eq!(
        ledger.remove_latest(),
        Some(HitDie::new(HdSource::Class(Class::Warrior), 42))
    );
    assert_eq!(ledger.health_contributions(CreatureType::Animal), [8]);
    assert_eq!(
        ledger.remove_latest(),
        Some(HitDie::new(HdSource::Racial, 0))
    );
    assert_eq!(ledger.total_hd(), 0);
    assert_eq!(ledger.health(CreatureType::Animal), 0);
    assert_eq!(ledger.remove_latest(), None);
}

#[test]
fn ledger_rejects_unbounded_authored_progression() {
    assert_eq!(
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 0); 257]),
        Err(HdError::TooManyHitDice)
    );
    let ledger =
        HdLedger::new(vec![HitDie::new(HdSource::Class(Class::Warrior), 42); 256]).unwrap();
    assert_eq!(ledger.total_hd(), 256);
    assert_eq!(ledger.training_points(), 512);
    assert_eq!(ledger.attribute_opportunities(), 64);
    assert_eq!(ledger.health(CreatureType::Undead), 1030);
}

#[test]
fn seeded_advancement_assigns_distinct_stable_health_streams_independent_of_class_choice() {
    let racial = HdLedger::seeded(vec![HdSource::Racial; 16], 42).unwrap();
    let repeated = HdLedger::seeded(vec![HdSource::Racial; 16], 42).unwrap();
    let warriors = HdLedger::seeded(vec![HdSource::Class(Class::Warrior); 16], 42).unwrap();
    assert_eq!(racial, repeated);
    let seeds: std::collections::BTreeSet<_> = racial
        .entries()
        .iter()
        .map(|entry| entry.health_seed())
        .collect();
    assert_eq!(seeds.len(), 16);
    assert_eq!(
        racial
            .entries()
            .iter()
            .map(|entry| entry.health_seed())
            .collect::<Vec<_>>(),
        warriors
            .entries()
            .iter()
            .map(|entry| entry.health_seed())
            .collect::<Vec<_>>()
    );
    assert_ne!(
        racial,
        HdLedger::seeded(vec![HdSource::Racial; 16], 43).unwrap()
    );
    assert_eq!(
        HdLedger::seeded(vec![HdSource::Racial; 257], 42),
        Err(HdError::TooManyHitDice)
    );
}
