use super::creature_fixture::{self as fixture, ruleset};
use tor_simulation::content_ids::{ClassId, TypeId};
use tor_simulation::creatures::{record::BuildRecord, CreatureBuild};
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
    assert_eq!(ledger.total_hd().get(), 4);
    assert_eq!(ledger.racial_hd().get(), 2);
    assert_eq!(
        ledger.class_level(&ClassId::new("mage").unwrap()),
        tor_simulation::HdCount(1)
    );
    assert_eq!(
        ledger.class_level(&ClassId::new("warrior").unwrap()),
        tor_simulation::HdCount(1)
    );
    assert_eq!(ledger.advancement_points(&ruleset()).unwrap(), 4);
    assert_eq!(
        ledger.attribute_opportunities(&ruleset()),
        tor_simulation::HdCount(1)
    );
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
    assert_eq!(ledger.total_hd().get(), 0);
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
    assert_eq!(ledger.total_hd().get(), 256);
    assert_eq!(ledger.advancement_points(&ruleset()).unwrap(), 256);
    assert_eq!(
        ledger.attribute_opportunities(&ruleset()),
        tor_simulation::HdCount(64)
    );
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

#[test]
fn ordinal_appends_preserve_exact_mixed_source_health_and_drained_regrowth() {
    let rules = ruleset();
    let sources: Vec<_> = (0..tor_simulation::limits::HIT_DICE)
        .map(|ordinal| match ordinal % 3 {
            0 => HdSource::Racial,
            1 => HdSource::Class(ClassId::new("warrior").unwrap()),
            _ => HdSource::Class(ClassId::new("mage").unwrap()),
        })
        .collect();
    for root in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
        let sequential = HdLedger::seeded(sources.clone(), root).unwrap();
        let mut appended = HdLedger::seeded(vec![], root).unwrap();
        for source in &sources {
            appended.append_seeded(source.clone(), root).unwrap();
        }
        assert_eq!(appended, sequential);
        for kind in ["animal", "undead"] {
            let kind = TypeId::new(kind).unwrap();
            assert_eq!(
                appended.health_provenance(&kind, &rules).unwrap(),
                sequential.health_provenance(&kind, &rules).unwrap()
            );
        }
        // Revisit every owned ordinal in reverse without a drain-history ledger.
        for expected in sequential.entries().iter().rev() {
            assert_eq!(appended.remove_latest().as_ref(), Some(expected));
            appended.append_seeded(expected.source(), root).unwrap();
            assert_eq!(appended.entries().last(), Some(expected));
            appended.remove_latest().unwrap();
        }
        assert!(appended.entries().is_empty());
        assert_eq!(appended.health_root_seed(), Some(root));
    }
}

fn build_with_ledger(ledger: HdLedger) -> CreatureBuild {
    let original = fixture::build(fixture::species());
    CreatureBuild::new(
        original.content().clone(),
        original.species_id().clone(),
        ledger,
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap()
}

#[test]
fn seeded_build_records_preserve_the_health_root_for_later_growth() {
    let original = build_with_ledger(HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap());
    let value = serde_json::to_value(BuildRecord::capture(&original)).unwrap();
    assert_eq!(value["health_root_seed"], 42);
    let restored = serde_json::from_value::<BuildRecord>(value)
        .unwrap()
        .restore(original.content().clone())
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn drained_ordinals_keep_their_health_root_and_regrow_with_the_same_seed() {
    let original = HdLedger::seeded(vec![HdSource::Racial; 3], 42).unwrap();
    let mut ledger = original.clone();
    let removed = ledger.remove_latest().unwrap();
    let root = ledger.health_root_seed().unwrap();
    assert_eq!(root, 42);
    ledger
        .append_seeded(HdSource::Class(ClassId::new("mage").unwrap()), root)
        .unwrap();
    assert_eq!(ledger.health_root_seed(), Some(42));
    assert_eq!(ledger.entries()[2].health_seed(), removed.health_seed());
    assert_eq!(&ledger.entries()[..2], &original.entries()[..2]);
    while ledger.remove_latest().is_some() {}
    assert_eq!(ledger.health_root_seed(), Some(42));
    ledger.append_seeded(HdSource::Racial, root).unwrap();
    assert_eq!(ledger.entries()[0], original.entries()[0]);
}

#[test]
fn restored_health_roots_resume_the_original_ordinal_stream() {
    let mut original = build_with_ledger(HdLedger::seeded(vec![HdSource::Racial; 3], 42).unwrap());
    let removed = original.remove_latest().unwrap();
    let record = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
    let mut restored = serde_json::from_slice::<BuildRecord>(&record)
        .unwrap()
        .restore(original.content().clone())
        .unwrap();
    restored
        .add_hit_die(
            HdSource::Racial,
            restored.ledger().health_root_seed().unwrap(),
        )
        .unwrap();
    assert_eq!(restored.ledger().entries()[2], removed.hit_die);
    assert_eq!(restored.ledger().health_root_seed(), Some(42));
}

#[test]
fn manually_recorded_health_seeds_have_an_explicit_unknown_root() {
    let original = fixture::build(fixture::species());
    assert_eq!(original.ledger().health_root_seed(), None);
    let value = serde_json::to_value(BuildRecord::capture(&original)).unwrap();
    assert!(value.as_object().unwrap().contains_key("health_root_seed"));
    assert!(value["health_root_seed"].is_null());
    let restored = serde_json::from_value::<BuildRecord>(value)
        .unwrap()
        .restore(original.content().clone())
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.ledger().health_root_seed(), None);
}

#[test]
fn saved_builds_require_the_health_root_key_even_when_the_root_is_unknown() {
    for ledger in [
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
    ] {
        let build = build_with_ledger(ledger);
        let mut value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
        value.as_object_mut().unwrap().remove("health_root_seed");
        assert!(serde_json::from_value::<BuildRecord>(value).is_err());
    }
}

#[test]
fn saved_health_roots_reject_malformed_values_and_inconsistent_ordinal_seeds() {
    let original = build_with_ledger(HdLedger::seeded(vec![HdSource::Racial; 3], 42).unwrap());
    let value = serde_json::to_value(BuildRecord::capture(&original)).unwrap();
    for invalid in [
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("42"),
        serde_json::json!({}),
        serde_json::json!([]),
    ] {
        let mut malformed = value.clone();
        malformed["health_root_seed"] = invalid;
        assert!(serde_json::from_value::<BuildRecord>(malformed).is_err());
    }
    let mut wrong_root = value.clone();
    wrong_root["health_root_seed"] = serde_json::json!(43);
    assert!(serde_json::from_value::<BuildRecord>(wrong_root)
        .unwrap()
        .restore(original.content().clone())
        .is_err());
    for ordinal in 0..3 {
        let mut wrong_seed = value.clone();
        let seed = wrong_seed["hd"][ordinal]["health_seed"].as_u64().unwrap();
        wrong_seed["hd"][ordinal]["health_seed"] = serde_json::json!(seed ^ 1);
        assert!(serde_json::from_value::<BuildRecord>(wrong_seed)
            .unwrap()
            .restore(original.content().clone())
            .is_err());
    }
}

#[test]
fn appending_from_another_root_rejects_without_changing_seeded_provenance() {
    let mut ledger = HdLedger::seeded(vec![HdSource::Racial], 42).unwrap();
    let original = ledger.clone();
    assert_eq!(
        ledger.append_seeded(HdSource::Racial, 43),
        Err(HdError::InvalidHealthRoot)
    );
    assert_eq!(ledger, original);
    assert_eq!(ledger.health_root_seed(), Some(42));
    ledger.append_seeded(HdSource::Racial, 42).unwrap();
    assert_eq!(
        ledger,
        HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap()
    );
}

#[test]
fn recorded_only_builds_allow_explicit_manual_growth_without_claiming_a_health_root() {
    let mut original = fixture::build(fixture::species());
    let retained = original.ledger().entries()[0].clone();
    original.add_hit_die(HdSource::Racial, 43).unwrap();
    assert_eq!(original.ledger().entries()[0], retained);
    let other = HdLedger::seeded(vec![HdSource::Racial; 2], 43).unwrap();
    let actual = serde_json::to_value(BuildRecord::capture(&original)).unwrap();
    let mut expected = actual.clone();
    expected["hd"] = serde_json::json!([
        {
            "source": {"kind": "racial"},
            "health_seed": 42,
            "health_policy": "maximized",
            "owner": null,
            "health_origin": {"templates": [], "kind": "humanoid", "die": 8},
            "health_improvement": 0,
        },
        {
            "source": {"kind": "racial"},
            "health_seed": other.entries()[1].health_seed(),
            "health_policy": "seeded",
            "owner": null,
            "health_origin": {"templates": [], "kind": "humanoid", "die": 8},
            "health_improvement": 0,
        },
    ]);
    expected["health_root_seed"] = serde_json::json!(null);
    expected["drained_hd"] = serde_json::json!([]);
    for field in ["hd", "health_root_seed", "drained_hd"] {
        assert_eq!(actual[field], expected[field], "exact owned ledger {field}");
    }
    let expected: BuildRecord = serde_json::from_value(expected).unwrap();
    let expected = expected.restore(original.content().clone()).unwrap();
    assert_eq!(original.ledger(), expected.ledger());
    assert_eq!(original.ledger().health_root_seed(), None);
    let restored = BuildRecord::capture(&original)
        .restore(original.content().clone())
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn rejected_health_growth_does_not_change_the_retained_root() {
    let mut ledger = HdLedger::seeded(vec![HdSource::Racial; 256], 42).unwrap();
    let original = ledger.clone();
    assert_eq!(
        ledger.append_seeded(HdSource::Racial, 43),
        Err(HdError::TooManyHitDice)
    );
    assert_eq!(ledger, original);
    assert_eq!(ledger.health_root_seed(), Some(42));
}
