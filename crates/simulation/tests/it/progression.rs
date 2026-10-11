use super::creature_fixture::ruleset;
use tor_simulation::content_ids::{ClassId, TypeId};
use tor_simulation::progression::{HdError, HdLedger, HdSource, HealthContribution, HitDie};

fn health(die: u16, health: u32) -> HealthContribution {
    HealthContribution { die, health }
}

#[test]
fn racial_and_class_health_dice_use_the_ruleset_catalog() {
    let rules = ruleset();
    assert_eq!(rules.document().creature_types.len(), 15);
    for (id, sides) in [
        ("fey", 6),
        ("construct", 10),
        ("magical_beast", 10),
        ("ooze", 10),
        ("dragon", 12),
        ("undead", 12),
        ("animal", 8),
    ] {
        assert_eq!(
            rules
                .creature_type(&TypeId::new(id).unwrap())
                .unwrap()
                .health_die,
            sides
        );
    }
    assert_eq!(
        rules
            .class(&ClassId::new("warrior").unwrap())
            .unwrap()
            .health_die,
        10
    );
    assert_eq!(
        rules
            .class(&ClassId::new("mage").unwrap())
            .unwrap()
            .health_die,
        6
    );
}

#[test]
fn only_the_first_overall_hit_die_is_maximized() {
    let racial_first = HdLedger::new(vec![
        HitDie::new(HdSource::Racial, 42),
        HitDie::new(HdSource::Class(ClassId::new("warrior").unwrap()), 42),
        HitDie::new(HdSource::Class(ClassId::new("mage").unwrap()), 42),
    ])
    .unwrap();
    assert_eq!(
        racial_first
            .health_provenance(&TypeId::new("animal").unwrap(), &ruleset())
            .unwrap(),
        [health(8, 8), health(10, 4), health(6, 2)]
    );
    let class_first = HdLedger::new(vec![
        HitDie::new(HdSource::Class(ClassId::new("mage").unwrap()), 42),
        HitDie::new(HdSource::Racial, 42),
    ])
    .unwrap();
    assert_eq!(
        class_first
            .health_provenance(&TypeId::new("animal").unwrap(), &ruleset())
            .unwrap(),
        [health(6, 6), health(8, 6)]
    );
}

#[test]
fn type_conversion_reinterprets_racial_health_without_changing_class_health() {
    let ledger = HdLedger::new(vec![
        HitDie::new(HdSource::Racial, 0),
        HitDie::new(HdSource::Racial, 42),
        HitDie::new(HdSource::Class(ClassId::new("warrior").unwrap()), 42),
    ])
    .unwrap();
    let original = ledger
        .health_provenance(&TypeId::new("animal").unwrap(), &ruleset())
        .unwrap();
    let undead = ledger
        .health_provenance(&TypeId::new("undead").unwrap(), &ruleset())
        .unwrap();
    assert_eq!(original, [health(8, 8), health(8, 6), health(10, 4)]);
    assert_eq!(undead, [health(12, 12), health(12, 2), health(10, 4)]);
    assert_eq!(
        ledger
            .health_provenance(&TypeId::new("animal").unwrap(), &ruleset())
            .unwrap(),
        original
    );
    assert_eq!(original.iter().map(|value| value.health).sum::<u32>(), 18);
    assert_eq!(undead.iter().map(|value| value.health).sum::<u32>(), 18);
}

#[test]
fn advancement_budgets_are_distinct_from_class_local_levels() {
    let sources = [
        HdSource::Racial,
        HdSource::Class(ClassId::new("mage").unwrap()),
        HdSource::Racial,
        HdSource::Class(ClassId::new("warrior").unwrap()),
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
    assert_eq!(ledger.class_level(&ClassId::new("mage").unwrap()), 1);
    assert_eq!(ledger.class_level(&ClassId::new("warrior").unwrap()), 1);
    assert_eq!(ledger.training_points(&ruleset()).unwrap(), 6);
    assert_eq!(ledger.attribute_opportunities(&ruleset()), 1);
    assert_eq!(ledger.talent_slots(), 4);
}

#[test]
fn latest_removal_preserves_retained_health_randomness_and_exposes_zero_hd() {
    let mut ledger = HdLedger::new(vec![
        HitDie::new(HdSource::Racial, 0),
        HitDie::new(HdSource::Class(ClassId::new("warrior").unwrap()), 42),
    ])
    .unwrap();
    assert_eq!(
        ledger.remove_latest(),
        Some(HitDie::new(
            HdSource::Class(ClassId::new("warrior").unwrap()),
            42
        ))
    );
    assert_eq!(
        ledger
            .health_provenance(&TypeId::new("animal").unwrap(), &ruleset())
            .unwrap(),
        [health(8, 8)]
    );
    assert_eq!(
        ledger.remove_latest(),
        Some(HitDie::new(HdSource::Racial, 0))
    );
    assert_eq!(ledger.total_hd(), 0);
    assert_eq!(
        ledger
            .health_provenance(&TypeId::new("animal").unwrap(), &ruleset())
            .unwrap(),
        []
    );
    assert_eq!(ledger.remove_latest(), None);
}

#[test]
fn ledger_rejects_unbounded_authored_progression() {
    assert_eq!(
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 0); 257]),
        Err(HdError::TooManyHitDice)
    );
    let ledger = HdLedger::new(vec![
        HitDie::new(
            HdSource::Class(ClassId::new("warrior").unwrap()),
            42
        );
        256
    ])
    .unwrap();
    assert_eq!(ledger.total_hd(), 256);
    assert_eq!(ledger.training_points(&ruleset()).unwrap(), 512);
    assert_eq!(ledger.attribute_opportunities(&ruleset()), 64);
    assert_eq!(
        ledger
            .health_provenance(&TypeId::new("undead").unwrap(), &ruleset())
            .unwrap()
            .iter()
            .map(|value| value.health)
            .sum::<u32>(),
        1030
    );
}

#[test]
fn seeded_advancement_assigns_distinct_stable_health_streams_independent_of_class_choice() {
    let racial = HdLedger::seeded(vec![HdSource::Racial; 16], 42).unwrap();
    let repeated = HdLedger::seeded(vec![HdSource::Racial; 16], 42).unwrap();
    let warriors = HdLedger::seeded(
        vec![HdSource::Class(ClassId::new("warrior").unwrap()); 16],
        42,
    )
    .unwrap();
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
