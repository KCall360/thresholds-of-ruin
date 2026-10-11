use tor_simulation::attributes::{Attributes, ManaBinding, Skill, SkillCheck, SkillRanks};
use tor_simulation::combat::DamageType;
use tor_simulation::content_ids::DescriptorId;
use tor_simulation::damage::{AttackCheck, DamageComponent, DamageError, DamageSpec, Protection};
use tor_simulation::dice::{roll_check, DicePool, Edge};
use tor_simulation::rules::{ModifierOp, Selector};

fn protection(operations: impl IntoIterator<Item = ModifierOp>) -> Result<Protection, DamageError> {
    let mut result = Protection::default();
    for operation in operations {
        result.apply(&operation)?;
    }
    Ok(result)
}

fn check(threshold: i32) -> AttackCheck {
    AttackCheck {
        check: SkillCheck {
            skill: Skill::HeavyWeaponry,
            binding: ManaBinding::Intellect,
            modifier: 0,
            threshold,
        },
        attributes: Attributes::default(),
        skills: SkillRanks::default(),
    }
}

#[test]
fn category_bonus_preserves_primary_pool_dice_edge_and_other_components() {
    let primary = DamageComponent::rolled(
        DamageType::Impact,
        Some(DescriptorId::new("fire").unwrap()),
        DicePool::new(2, 6, -1).unwrap(),
    );
    let key = primary.key();
    let base = DamageSpec::new(
        vec![
            primary,
            DamageComponent::rolled(DamageType::Energy, None, DicePool::new(1, 8, 0).unwrap()),
            DamageComponent::rolled(
                DamageType::Impact,
                Some(DescriptorId::new("cold").unwrap()),
                DicePool::new(1, 4, 0).unwrap(),
            ),
        ],
        Some(key.clone()),
    )
    .unwrap();
    let boosted = base.with_category_bonus(DamageType::Impact, 3).unwrap();
    assert_eq!(boosted.primary(), base.primary());
    assert_eq!(boosted.components().len(), base.components().len());
    for edge in [
        Edge::default(),
        Edge::from_counts(2, 0),
        Edge::from_counts(0, 2),
    ] {
        let mut before_rng = 42;
        let before = check(0).resolve(
            &mut before_rng,
            edge,
            &base,
            &Protection::default(),
            &mut (),
        );
        let mut after_rng = 42;
        let after = check(0).resolve(
            &mut after_rng,
            edge,
            &boosted,
            &Protection::default(),
            &mut (),
        );
        assert_eq!(before.check, after.check);
        assert_eq!(before_rng, after_rng);
        let before = before.damage.unwrap();
        let after = after.damage.unwrap();
        assert_eq!(after.total, before.total + 3);
        assert_eq!(after.components[0].raw, before.components[0].raw + 3);
        assert_eq!(after.components[1..], before.components[1..]);
        assert_eq!(
            after.components[0].pool.as_ref().unwrap().rolled,
            before.components[0].pool.as_ref().unwrap().rolled
        );
    }
}

#[test]
fn missing_category_bonus_does_not_change_existing_roll_order_or_bypass_protection() {
    let primary = DamageComponent::rolled(DamageType::Keen, None, DicePool::new(1, 6, 0).unwrap());
    let key = primary.key();
    let base = DamageSpec::new(
        vec![
            primary,
            DamageComponent::rolled(DamageType::Energy, None, DicePool::new(1, 8, 0).unwrap()),
        ],
        Some(key.clone()),
    )
    .unwrap();
    let boosted = base.with_category_bonus(DamageType::Impact, 3).unwrap();
    assert_eq!(boosted.primary(), base.primary());
    let protection = protection([ModifierOp::Immunity {
        selector: Selector::Category {
            category: DamageType::Impact,
        },
    }])
    .unwrap();
    let mut before_rng = 42;
    let before = base.resolve(
        &mut before_rng,
        Edge::from_counts(1, 0),
        &protection,
        &mut (),
    );
    let mut after_rng = 42;
    let after = boosted.resolve(
        &mut after_rng,
        Edge::from_counts(1, 0),
        &protection,
        &mut (),
    );
    assert_eq!(before_rng, after_rng);
    assert_eq!(after.raw, before.raw + 3);
    assert_eq!(after.total, before.total);
    assert_eq!(after.components[0], before.components[0]);
}

#[test]
fn category_bonus_checks_aggregate_limits_without_mutating_the_original() {
    let base = DamageSpec::new(
        vec![DamageComponent::fixed(DamageType::Impact, None, 1_000_000)],
        None,
    )
    .unwrap();
    let before = base.clone();
    assert_eq!(
        base.with_category_bonus(DamageType::Impact, 3),
        Err(DamageError::InvalidSpec)
    );
    assert_eq!(
        base.with_category_bonus(DamageType::Impact, u32::MAX),
        Err(DamageError::InvalidSpec)
    );
    assert_eq!(
        base.with_category_bonus(DamageType::Impact, 0),
        Ok(before.clone())
    );
    assert_eq!(base, before);
    let full = DamageSpec::new(
        (2..=33)
            .map(|sides| {
                DamageComponent::rolled(
                    DamageType::Energy,
                    None,
                    DicePool::new(1, sides, 0).unwrap(),
                )
            })
            .collect(),
        None,
    )
    .unwrap();
    assert_eq!(
        full.with_category_bonus(DamageType::Impact, 3),
        Err(DamageError::InvalidSpec)
    );
}

#[test]
fn splitting_fixed_damage_cannot_apply_reduction_more_than_once() {
    let spec = DamageSpec::new(
        vec![
            DamageComponent::fixed(DamageType::Impact, None, 5),
            DamageComponent::fixed(DamageType::Impact, None, 5),
        ],
        None,
    )
    .unwrap();
    let protection = protection([ModifierOp::Reduction {
        selector: Selector::Category {
            category: DamageType::Impact,
        },
        amount: 4,
    }])
    .unwrap();
    let mut state = 42;
    let result = spec.resolve(
        &mut state,
        Edge::from_counts(u32::MAX, 0),
        &protection,
        &mut (),
    );
    assert_eq!(result.raw, 10);
    assert_eq!(result.total, 6);
    assert_eq!(result.components.len(), 1);
    assert_eq!(state, 42);
}

#[test]
fn immunity_precedes_descriptor_and_category_reduction() {
    let spec = DamageSpec::new(
        vec![
            DamageComponent::fixed(
                DamageType::Energy,
                Some(DescriptorId::new("fire").unwrap()),
                7,
            ),
            DamageComponent::fixed(
                DamageType::Energy,
                Some(DescriptorId::new("cold").unwrap()),
                5,
            ),
            DamageComponent::fixed(DamageType::Impact, None, 3),
        ],
        None,
    )
    .unwrap();
    let protection = protection([
        ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
        },
        ModifierOp::Reduction {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("cold").unwrap(),
            },
            amount: 1,
        },
        ModifierOp::Reduction {
            selector: Selector::Category {
                category: DamageType::Energy,
            },
            amount: 2,
        },
    ])
    .unwrap();
    let result = spec.resolve(&mut 42, Edge::default(), &protection, &mut ());
    assert_eq!(result.raw, 15);
    assert_eq!(result.after_immunity, 8);
    assert_eq!(result.after_descriptors, 7);
    assert_eq!(result.by_category[&DamageType::Energy], 2);
    assert_eq!(result.by_category[&DamageType::Impact], 3);
    assert_eq!(result.total, 5);
}

#[test]
fn exact_selector_modifiers_combine_before_specific_and_broad_layers() {
    let spec = DamageSpec::new(
        vec![DamageComponent::fixed(
            DamageType::Energy,
            Some(DescriptorId::new("fire").unwrap()),
            6,
        )],
        None,
    )
    .unwrap();
    let protection = protection([
        ModifierOp::Reduction {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
            amount: 1,
        },
        ModifierOp::Reduction {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
            amount: 1,
        },
        ModifierOp::Reduction {
            selector: Selector::Category {
                category: DamageType::Energy,
            },
            amount: 3,
        },
    ])
    .unwrap();
    let result = spec.resolve(&mut 42, Edge::default(), &protection, &mut ());
    assert_eq!(result.after_descriptors, 4);
    assert_eq!(result.total, 1);
}

#[test]
fn canonical_pools_are_independent_of_content_order_and_splitting() {
    let fire = DamageComponent::rolled(
        DamageType::Energy,
        Some(DescriptorId::new("fire").unwrap()),
        DicePool::new(1, 6, 1).unwrap(),
    );
    let cold = DamageComponent::rolled(
        DamageType::Energy,
        Some(DescriptorId::new("cold").unwrap()),
        DicePool::new(1, 8, 0).unwrap(),
    );
    let primary = Some(fire.key());
    let split = DamageSpec::new(
        vec![cold.clone(), fire.clone(), fire.clone()],
        primary.clone(),
    )
    .unwrap();
    let merged = DamageSpec::new(
        vec![
            DamageComponent::rolled(
                DamageType::Energy,
                Some(DescriptorId::new("fire").unwrap()),
                DicePool::new(2, 6, 2).unwrap(),
            ),
            cold,
        ],
        primary,
    )
    .unwrap();
    assert_eq!(split, merged);
    let mut first = 42;
    let mut second = 42;
    assert_eq!(
        split.resolve(
            &mut first,
            Edge::from_counts(2, 0),
            &Protection::default(),
            &mut ()
        ),
        merged.resolve(
            &mut second,
            Edge::from_counts(2, 0),
            &Protection::default(),
            &mut ()
        )
    );
    assert_eq!(first, second);
}

#[test]
fn double_advantage_covers_check_and_damage_and_misses_skip_all_damage_draws() {
    let component =
        DamageComponent::rolled(DamageType::Impact, None, DicePool::new(1, 6, 2).unwrap());
    let spec = DamageSpec::new(vec![component.clone()], Some(component.key())).unwrap();
    let mut state = 42;
    let hit = check(14).resolve(
        &mut state,
        Edge::from_counts(2, 0),
        &spec,
        &Protection::default(),
        &mut (),
    );
    assert_eq!(hit.check.roll.second, Some(12));
    assert!(hit.check.success);
    let damage = hit.damage.unwrap();
    assert_eq!(damage.components[0].pool.as_ref().unwrap().rolled, [1, 1]);
    assert_eq!(damage.total, 3);
    let mut state = 42;
    let miss = check(15).resolve(
        &mut state,
        Edge::from_counts(2, 0),
        &spec,
        &Protection::default(),
        &mut (),
    );
    assert!(!miss.check.success);
    assert!(miss.damage.is_none());
    assert_eq!(roll_check(&mut state, Edge::default()).kept, 19);
}

#[test]
fn unsupported_overlapping_selector_groups_and_unbounded_damage_reject() {
    assert_eq!(
        DamageSpec::new(
            vec![
                DamageComponent::fixed(
                    DamageType::Energy,
                    Some(DescriptorId::new("fire").unwrap()),
                    2
                ),
                DamageComponent::fixed(
                    DamageType::Impact,
                    Some(DescriptorId::new("fire").unwrap()),
                    2
                )
            ],
            None
        ),
        Err(DamageError::NonNestedDescriptors)
    );
    assert_eq!(
        DamageSpec::new(
            vec![DamageComponent::fixed(DamageType::Impact, None, u32::MAX)],
            None
        ),
        Err(DamageError::InvalidSpec)
    );
    assert_eq!(DamageSpec::new(vec![], None), Err(DamageError::InvalidSpec));
    assert_eq!(
        protection([ModifierOp::Reduction {
            selector: Selector::Category {
                category: DamageType::Impact
            },
            amount: u32::MAX
        }]),
        Err(DamageError::InvalidProtection)
    );
}

fn trace_field<'a>(
    record: &'a tor_simulation::resolution_diagnostics::ResolutionRecord,
    id: &str,
) -> &'a tor_simulation::resolution_diagnostics::TraceValueRecord {
    &record
        .fields
        .iter()
        .find(|field| field.id == id)
        .unwrap()
        .value
}

#[test]
fn diagnostics_capture_actual_check_draws_protection_order_and_unused_edge() {
    let damage = DamageSpec::new(
        vec![
            DamageComponent::fixed(
                DamageType::Energy,
                Some(DescriptorId::new("fire").unwrap()),
                7,
            ),
            DamageComponent::rolled(
                DamageType::Impact,
                Some(DescriptorId::new("cold").unwrap()),
                DicePool::new(2, 6, 20).unwrap(),
            ),
        ],
        None,
    )
    .unwrap();
    let protection = protection([
        ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
        },
        ModifierOp::Reduction {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("cold").unwrap(),
            },
            amount: 3,
        },
        ModifierOp::Reduction {
            selector: Selector::Category {
                category: DamageType::Impact,
            },
            amount: 4,
        },
    ])
    .unwrap();
    let mut attack = check(0);
    attack.check.modifier = 2;
    attack.attributes = Attributes::new(tor_simulation::attributes::ByAttribute {
        strength: 3,
        ..Default::default()
    })
    .unwrap();
    attack.skills = SkillRanks::default()
        .with_rank(Skill::HeavyWeaponry, 1)
        .unwrap();
    let mut ordinary_rng = 42;
    let ordinary = attack.resolve(
        &mut ordinary_rng,
        Edge::from_counts(5, 1),
        &damage,
        &protection,
        &mut (),
    );
    let mut recorded_rng = 42;
    let mut recorder = tor_simulation::resolution_diagnostics::ResolutionTrace::default();
    let recorded = attack.resolve(
        &mut recorded_rng,
        Edge::from_counts(5, 1),
        &damage,
        &protection,
        &mut recorder,
    );
    assert_eq!(recorded, ordinary);
    assert_eq!(recorded_rng, ordinary_rng);
    use tor_simulation::resolution_diagnostics::TraceValueRecord as V;
    let detail = recorder
        .steps()
        .iter()
        .find(|step| step.id == "check.resolved")
        .unwrap();
    assert_eq!(
        trace_field(detail, "attribute"),
        &V::Text("strength".into())
    );
    assert_eq!(trace_field(detail, "attribute_value"), &V::Unsigned(3));
    assert_eq!(trace_field(detail, "rank"), &V::Unsigned(1));
    assert_eq!(trace_field(detail, "modifier"), &V::Signed(2));
    assert_eq!(trace_field(detail, "threshold"), &V::Signed(0));
    assert_eq!(trace_field(detail, "rng_before"), &V::Unsigned(42));
    assert_eq!(trace_field(detail, "edge"), &V::Signed(1));
    assert_eq!(
        trace_field(detail, "total"),
        &V::Signed(recorded.check.total)
    );
    let components: Vec<_> = recorder
        .steps()
        .iter()
        .filter(|step| step.id == "damage.component")
        .collect();
    assert_eq!(trace_field(components[0], "raw"), &V::Unsigned(7));
    assert_eq!(
        trace_field(components[0], "descriptor"),
        &V::Text("fire".into())
    );
    assert_eq!(
        trace_field(components[0], "descriptor_immune"),
        &V::Bool(true)
    );
    assert_eq!(
        trace_field(components[0], "after_immunity"),
        &V::Unsigned(0)
    );
    assert_eq!(trace_field(components[1], "edge"), &V::Signed(2));
    let raw_cold = recorded.damage.as_ref().unwrap().components[1].raw;
    let reductions: Vec<_> = recorder
        .steps()
        .iter()
        .filter(|step| step.id == "damage.reduction")
        .collect();
    let descriptor = reductions
        .iter()
        .position(|step| trace_field(step, "selector") == &V::Text("cold".into()))
        .unwrap();
    let category = reductions
        .iter()
        .position(|step| trace_field(step, "selector") == &V::Text("impact".into()))
        .unwrap();
    assert!(descriptor < category);
    assert_eq!(
        trace_field(reductions[descriptor], "before"),
        &V::Unsigned(u64::from(raw_cold))
    );
    assert_eq!(
        trace_field(reductions[descriptor], "capacity"),
        &V::Unsigned(3)
    );
    assert_eq!(
        trace_field(reductions[descriptor], "after"),
        &V::Unsigned(u64::from(raw_cold - 3))
    );
    assert_eq!(
        trace_field(reductions[category], "before"),
        &V::Unsigned(u64::from(raw_cold - 3))
    );
    assert_eq!(
        trace_field(reductions[category], "capacity"),
        &V::Unsigned(4)
    );
    assert_eq!(
        trace_field(reductions[category], "after"),
        &V::Unsigned(u64::from(raw_cold - 7))
    );
    let finished = recorder
        .steps()
        .iter()
        .find(|step| step.id == "damage.finished")
        .unwrap();
    assert_eq!(
        trace_field(finished, "total"),
        &V::Unsigned(u64::from(raw_cold - 7))
    );
    assert_eq!(trace_field(finished, "unused_edge"), &V::Signed(1));
    assert!(!recorder
        .steps()
        .iter()
        .any(|step| step.id == "attack.missed"));
}

#[test]
fn missed_diagnostics_do_not_roll_damage_or_apply_protection() {
    let damage = DamageSpec::new(
        vec![DamageComponent::rolled(
            DamageType::Impact,
            None,
            DicePool::new(1, 6, 0).unwrap(),
        )],
        None,
    )
    .unwrap();
    let mut rng = 9;
    let mut reference = rng;
    let expected = roll_check(&mut reference, Edge::from_counts(0, 1));
    let mut recorder = tor_simulation::resolution_diagnostics::ResolutionTrace::default();
    let outcome = check(100).resolve(
        &mut rng,
        Edge::from_counts(0, 4),
        &damage,
        &Protection::default(),
        &mut recorder,
    );
    assert_eq!(outcome.check.roll, expected);
    assert_eq!(rng, reference);
    assert!(outcome.damage.is_none());
    use tor_simulation::resolution_diagnostics::TraceValueRecord as V;
    assert!(!recorder
        .steps()
        .iter()
        .any(|step| step.id.starts_with("damage.")));
    let missed = recorder
        .steps()
        .iter()
        .find(|step| step.id == "attack.missed")
        .unwrap();
    assert_eq!(trace_field(missed, "unused_edge"), &V::Signed(-3));
}

#[test]
fn authored_descriptor_immunity_protects_fire_and_new_content_without_engine_variants() {
    for name in ["fire", "test.plasma", "scenario-ember"] {
        let descriptor = DescriptorId::new(name).unwrap();
        let damage = DamageSpec::new(
            vec![DamageComponent::fixed(
                DamageType::Energy,
                Some(descriptor.clone()),
                19,
            )],
            None,
        )
        .unwrap();
        let protection = protection([ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new(name).unwrap(),
            },
        }])
        .unwrap();
        let mut state = 42;
        let outcome = damage.resolve(&mut state, Edge::default(), &protection, &mut ());
        assert_eq!(
            (outcome.raw, outcome.after_immunity, outcome.total),
            (19, 0, 0)
        );
        assert_eq!(state, 42);
        assert!(protection.immune(DamageType::Energy, Some(&descriptor)));
        assert!(!protection.immune(
            DamageType::Energy,
            Some(&DescriptorId::new("other").unwrap())
        ));
    }
}

#[test]
fn rejected_reduction_leaves_the_previous_fold_unchanged() {
    let selector = Selector::Descriptor {
        descriptor: DescriptorId::new("fire").unwrap(),
    };
    let mut protection = protection([ModifierOp::Reduction {
        selector: selector.clone(),
        amount: 900_000,
    }])
    .unwrap();
    let before = protection.clone();
    assert_eq!(
        protection.apply(&ModifierOp::Reduction {
            selector: selector.clone(),
            amount: 100_001,
        }),
        Err(DamageError::InvalidProtection)
    );
    assert_eq!(protection, before);
    assert_eq!(protection.reduction(&selector), 900_000);
    let new_selector = Selector::Category {
        category: DamageType::Impact,
    };
    assert_eq!(
        protection.apply(&ModifierOp::Reduction {
            selector: new_selector,
            amount: u32::MAX,
        }),
        Err(DamageError::InvalidProtection)
    );
    assert_eq!(protection, before);
}

#[test]
fn unit_and_retaining_observers_preserve_damage_outcomes_and_random_states() {
    let first = DamageComponent::rolled(
        DamageType::Energy,
        Some(DescriptorId::new("fire").unwrap()),
        DicePool::new(2, 6, -1).unwrap(),
    );
    let primary = first.key();
    let damage = DamageSpec::new(
        vec![
            first,
            DamageComponent::rolled(DamageType::Impact, None, DicePool::new(1, 8, 2).unwrap()),
        ],
        Some(primary),
    )
    .unwrap();
    let protection = protection([
        ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
        },
        ModifierOp::Reduction {
            selector: Selector::Category {
                category: DamageType::Impact,
            },
            amount: 3,
        },
    ])
    .unwrap();
    for seed in 0..64 {
        for edge in [
            Edge::default(),
            Edge::from_counts(4, 0),
            Edge::from_counts(0, 4),
        ] {
            for threshold in [0, 15, 100] {
                let mut ordinary_rng = seed;
                let mut recorded_rng = seed;
                let mut trace = tor_simulation::resolution_diagnostics::ResolutionTrace::default();
                let ordinary = check(threshold).resolve(
                    &mut ordinary_rng,
                    edge,
                    &damage,
                    &protection,
                    &mut (),
                );
                let recorded = check(threshold).resolve(
                    &mut recorded_rng,
                    edge,
                    &damage,
                    &protection,
                    &mut trace,
                );
                assert_eq!(ordinary, recorded);
                assert_eq!(ordinary_rng, recorded_rng);
                assert!(!trace.steps().is_empty());
                assert!(!trace.truncated());
            }
        }
    }
}
