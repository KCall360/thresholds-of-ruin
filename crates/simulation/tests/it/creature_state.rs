use tor_simulation::attributes::{Attributes, ByAttribute, ManaBinding};
use tor_simulation::content_ids::{ClassId, ConditionId, DescriptorId, TemplateId};
use tor_simulation::costs::{CostError, ResourceCost, StartCost};
use tor_simulation::creatures::{Content, CreatureBuild, CreatureState, Template};
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::resources::Resource;
use tor_simulation::rules::{Modifier, ModifierOp, Scope, Selector, Stat};
use tor_simulation::{ActorId, IntentionId};

#[test]
fn cost_preparation_checks_are_readonly_and_reject_dead_actors() {
    let mut creature = CreatureState::new(build()).unwrap();
    let cost = tor_simulation::costs::CostBundle::single(ResourceCost {
        resource: Resource::Mana,
        start: 1,
        resolution: 1,
    })
    .unwrap();
    let before = creature.clone();
    assert_eq!(
        creature.validate_cost(IntentionId(1), cost),
        Ok(StartCost::Started)
    );
    assert_eq!(creature, before);
    creature.start_cost(IntentionId(1), cost).unwrap();
    let before = creature.clone();
    assert_eq!(
        creature.validate_cost(IntentionId(1), cost),
        Ok(StartCost::Resumed)
    );
    assert_eq!(creature, before);
    creature.kill();
    let before = creature.clone();
    assert_eq!(
        creature.validate_cost(IntentionId(1), cost),
        Err(CostError::Invalid)
    );
    assert_eq!(
        creature.start_cost(IntentionId(1), cost),
        Err(CostError::Invalid)
    );
    assert_eq!(creature, before);
}

#[test]
fn next_timer_change_combines_recovery_and_expiry_and_stops_at_death() {
    let mut creature = CreatureState::new(build()).unwrap();
    assert_eq!(creature.next_change_in(), None);
    creature
        .start_cost(
            IntentionId(1),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 0,
            })
            .unwrap(),
        )
        .unwrap();
    creature.finish_cost(IntentionId(1)).unwrap();
    creature
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(9),
            50,
        )
        .unwrap();
    assert_eq!(creature.next_change_in(), Some(50));
    creature.advance_active(49);
    assert_eq!(creature.next_change_in(), Some(1));
    creature.advance_active(1);
    assert_eq!(creature.next_change_in(), Some(50));
    creature.advance_active(50);
    assert_eq!(creature.next_change_in(), None);
    creature
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(9),
            50,
        )
        .unwrap();
    creature.kill();
    assert_eq!(creature.next_change_in(), None);
    assert!(!creature.needs_active_time());
}

pub(super) fn build() -> CreatureBuild {
    let mut species = super::creature_fixture::species();
    species.modifiers.clear();
    species.default_attributes = Attributes::new(ByAttribute {
        strength: 2,
        speed: 1,
        intellect: 2,
        willpower: 2,
        awareness: 1,
        presence: 1,
    })
    .unwrap();
    species.melee = tor_simulation::attacks::NaturalAttack::from_melee(
        super::creature_fixture::melee(0, 60, tor_simulation::combat::DamageType::Impact, 3),
    )
    .unwrap();
    let id = species.id.clone();
    let mut immune = Template::new(TemplateId::new("immune_form").unwrap(), 0);
    immune.modifiers = vec![
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Health,
                value: 2,
            },
        },
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::ResourceMax {
                    resource: Resource::Mana,
                },
                value: 2,
            },
        },
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: DescriptorId::new("fear").unwrap(),
                },
            },
        },
    ];
    CreatureBuild::new(
        super::creature_fixture::content_with(species, vec![immune]),
        id,
        HdLedger::seeded(
            vec![
                HdSource::Class(ClassId::new("warrior").unwrap()),
                HdSource::Class(ClassId::new("mage").unwrap()),
            ],
            42,
        )
        .unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap()
}

pub(super) fn with_template(build: &CreatureBuild, template: Template) -> CreatureBuild {
    let mut catalog = (*build.content().catalog()).clone();
    let id = template.id.clone();
    catalog.templates.insert(id.clone(), template);
    let content = Content::new(build.content().rules().clone(), catalog).unwrap();
    let mut ids = build.template_ids().to_vec();
    ids.push(id);
    CreatureBuild::from_recorded(
        content,
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        build.advancement().clone(),
        ids,
    )
    .unwrap()
}

#[test]
fn rebuilding_preserves_injury_and_balances_and_clears_newly_immune_fear() {
    let mut creature = CreatureState::new(build()).unwrap();
    creature.damage(5);
    creature
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(9),
            300,
        )
        .unwrap();
    let maximum = creature.health().maximum();
    creature
        .start_cost(
            IntentionId(1),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 1,
            })
            .unwrap(),
        )
        .unwrap();
    let balance = creature.costs().resources().balance(Resource::Stamina);
    let mut transformed = creature.build().clone();
    transformed
        .set_templates(vec![TemplateId::new("immune_form").unwrap()])
        .unwrap();
    let outcome = creature.rebuild(transformed).unwrap();
    assert!(outcome.conditions_cleared);
    assert!(!outcome.died);
    assert_eq!(creature.health().injury(), 5);
    assert_eq!(creature.health().maximum(), maximum + 2);
    assert_eq!(
        creature.costs().resources().balance(Resource::Stamina),
        balance
    );
    assert!(creature.conditions().is_empty());
    let mut restored = creature.build().clone();
    restored.set_templates(vec![]).unwrap();
    creature.rebuild(restored).unwrap();
    assert_eq!(creature.health().maximum(), maximum);
    assert_eq!(creature.health().current(), maximum - 5);
    assert!(creature.conditions().is_empty());
}

#[test]
fn reduced_resource_capacity_cancels_unfunded_holds_without_refunding_paid_costs() {
    let mut creature = CreatureState::new(build()).unwrap();
    let cost = tor_simulation::costs::CostBundle::single(ResourceCost {
        resource: Resource::Mana,
        start: 1,
        resolution: 3,
    })
    .unwrap();
    assert_eq!(
        creature.start_cost(IntentionId(7), cost),
        Ok(StartCost::Started)
    );
    let mut transformed = creature.build().clone();
    transformed
        .set_templates(vec![TemplateId::new("immune_form").unwrap()])
        .unwrap();
    let outcome = creature.rebuild(transformed).unwrap();
    assert_eq!(outcome.canceled, vec![IntentionId(7)]);
    assert_eq!(creature.costs().resources().balance(Resource::Mana), 2);
    assert!(creature.costs().reservations().is_empty());
    let mut restored = creature.build().clone();
    restored.set_templates(vec![]).unwrap();
    creature.rebuild(restored).unwrap();
    assert_eq!(creature.costs().resources().balance(Resource::Mana), 2);
}

#[test]
fn zero_hd_death_cancels_holds_and_never_revives_or_recovers() {
    let original = build();
    let mut creature = CreatureState::new(original.clone()).unwrap();
    creature
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(9),
            300,
        )
        .unwrap();
    creature
        .start_cost(
            IntentionId(7),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Focus,
                start: 1,
                resolution: 1,
            })
            .unwrap(),
        )
        .unwrap();
    let mut drained = original.clone();
    while drained.remove_latest().is_some() {}
    let outcome = creature.rebuild(drained).unwrap();
    assert!(outcome.died);
    assert_eq!(outcome.canceled, vec![IntentionId(7)]);
    assert!(creature.conditions().is_empty());
    creature.rebuild(original).unwrap();
    assert!(creature.health().dead());
    assert_eq!(creature.heal(100), 0);
    let frozen = creature.clone();
    creature.advance_active(10_000);
    assert_eq!(creature, frozen);
    assert!(creature
        .start_cost(
            IntentionId(8),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Focus,
                start: 1,
                resolution: 1
            })
            .unwrap()
        )
        .is_err());
}

#[test]
fn creature_checkpoint_preserves_paid_costs_recovery_fear_and_next_outcomes() {
    let mut original = CreatureState::new(build()).unwrap();
    original.damage(5);
    original
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(9),
            300,
        )
        .unwrap();
    let cost = tor_simulation::costs::CostBundle::single(ResourceCost {
        resource: Resource::Focus,
        start: 1,
        resolution: 1,
    })
    .unwrap();
    original.start_cost(IntentionId(7), cost).unwrap();
    original.advance_active(99);
    let data = serde_json::to_vec(&original).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&data).unwrap();
    assert!(value.get("derived").is_none());
    assert!(value.get("maximum_health").is_none());
    let record: tor_simulation::creatures::record::CreatureRecord =
        serde_json::from_slice(&data).unwrap();
    let mut restored = record
        .restore(original.build().content().clone(), Default::default())
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(
        restored.start_cost(IntentionId(7), cost),
        Ok(StartCost::Resumed)
    );
    for ticks in [201, 1, 299] {
        original.advance_active(ticks);
        restored.advance_active(ticks);
        assert_eq!(restored, original);
    }
    assert_eq!(
        restored.finish_cost(IntentionId(7)),
        original.finish_cost(IntentionId(7))
    );
    assert_eq!(restored.damage(u32::MAX), original.damage(u32::MAX));
    assert_eq!(restored, original);
}

#[test]
fn forged_cross_field_state_rejects_instead_of_clamping_or_repairing() {
    let value = serde_json::to_value(CreatureState::new(build()).unwrap()).unwrap();
    for (field, forged) in [
        ("injury", serde_json::json!(1_000_000)),
        (
            "pools",
            serde_json::json!({"stamina":{"balance":100,"recovery_elapsed":0},"focus":{"balance":4,"recovery_elapsed":0},"mana":{"balance":4,"recovery_elapsed":0}}),
        ),
        (
            "holds",
            serde_json::json!([{"owner":7,"resource":"focus","start":1,"resolution":100}]),
        ),
        (
            "conditions",
            serde_json::json!([{"condition":"fear","source":9,"remaining":300,"stacks":1,"order":0,"generation":0,"phase":"active","reserved_by":null,"qualification":null},{"condition":"fear","source":9,"remaining":200,"stacks":1,"order":1,"generation":1,"phase":"active","reserved_by":null,"qualification":null}]),
        ),
    ] {
        let mut forged_value = value.clone();
        forged_value[field] = forged;
        if field == "conditions" {
            forged_value["next_condition_generation"] = serde_json::json!(2);
        }
        assert!(restore_value(forged_value).is_none(), "{field}");
    }
    let mut forged = value.clone();
    forged["dead"] = serde_json::json!(true);
    forged["next_condition_generation"] = serde_json::json!(1);
    forged["conditions"] = serde_json::json!([{"condition":"fear","source":9,"remaining":300,"stacks":1,"order":0,"generation":0,"phase":"active","reserved_by":null,"qualification":null}]);
    assert!(restore_value(forged).is_none());
    let mut forged = value;
    forged.as_object_mut().unwrap().remove("holds");
    assert!(restore_value(forged).is_none());
}

fn restore_value(value: serde_json::Value) -> Option<CreatureState> {
    serde_json::from_value::<tor_simulation::creatures::record::CreatureRecord>(value)
        .ok()?
        .restore(build().content().clone(), Default::default())
        .ok()
}

fn condition_build(definitions: Vec<tor_simulation::rules::ConditionDef>) -> CreatureBuild {
    let original = build();
    let mut document = original.ruleset().document().clone();
    document.conditions.extend(definitions);
    let content = Content::new(
        tor_simulation::rules::Ruleset::validate(document).unwrap(),
        (*original.content().catalog()).clone(),
    )
    .unwrap();
    CreatureBuild::from_recorded(
        content,
        original.species_id().clone(),
        original.initial_attributes(),
        original.ledger().clone(),
        original.binding(),
        original.advancement().clone(),
        vec![],
    )
    .unwrap()
}
fn timed(id: &str, op: ModifierOp) -> tor_simulation::rules::ConditionDef {
    tor_simulation::rules::ConditionDef {
        detecting_monsters: false,
        hallucinating: false,
        id: ConditionId::new(id).unwrap(),
        family: tor_simulation::content_ids::ConditionFamilyId::new(id).unwrap(),
        lifecycle: tor_simulation::rules::ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: id.into(),
        modifiers: vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op,
        }],
        stacking: tor_simulation::rules::Stacking::Longest,
        source_scoped: true,
        descriptors: vec![],
        capacity: tor_simulation::rules::Eviction::Oldest,
    }
}
#[test]
fn expiring_a_mask_evicts_invalid_oldest_sources_and_restores_valid_baseline() {
    let cap = timed(
        "temporary_cap",
        ModifierOp::Set {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 10,
        },
    );
    let giant = timed(
        "temporary_giant",
        ModifierOp::Add {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 1000,
        },
    );
    let mut creature =
        CreatureState::new(condition_build(vec![cap.clone(), giant.clone()])).unwrap();
    let baseline = creature.derived().clone();
    creature.apply_condition(&cap.id, ActorId(9), 10).unwrap();
    creature
        .apply_condition(&giant.id, ActorId(9), 100)
        .unwrap();
    assert_eq!(
        creature
            .derived()
            .attributes
            .get(tor_simulation::attributes::Attribute::Strength),
        10
    );
    let transition = creature.advance_active(10);
    assert_eq!(transition.removed_conditions.len(), 2);
    assert!(creature.conditions().is_empty());
    assert_eq!(creature.derived(), &baseline);
}

#[test]
fn modifier_reconciliation_preserves_older_active_and_protection_entries() {
    let lock: tor_simulation::rules::ConditionDef = serde_json::from_value(
        super::status_conditions::definition("fixture_lock", Some(400)),
    )
    .unwrap();
    let cap = timed(
        "temporary_cap",
        ModifierOp::Set {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 10,
        },
    );
    let giant = timed(
        "temporary_giant",
        ModifierOp::Add {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 1000,
        },
    );
    for elapsed in [0, 200] {
        let mut creature = CreatureState::new(condition_build(vec![
            lock.clone(),
            cap.clone(),
            giant.clone(),
        ]))
        .unwrap();
        creature.apply_condition(&lock.id, ActorId(9), 200).unwrap();
        creature.advance_active(elapsed);
        let mut control = creature.clone();
        let retained = creature.conditions().instances().next().unwrap().clone();
        creature.apply_condition(&cap.id, ActorId(9), 10).unwrap();
        creature
            .apply_condition(&giant.id, ActorId(9), 100)
            .unwrap();
        creature.advance_active(10);
        control.advance_active(10);
        let entries: Vec<_> = creature.conditions().instances().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].condition, retained.condition);
        assert_eq!(entries[0].phase, retained.phase);
        assert_eq!(
            entries[0].remaining,
            retained.remaining.map(|remaining| remaining - 10)
        );
        assert_eq!(creature.derived(), control.derived());
        assert_eq!(entries[0], control.conditions().instances().next().unwrap());
    }
}

#[test]
fn death_and_administrative_removal_do_not_grant_post_effect_protection() {
    let lock: tor_simulation::rules::ConditionDef = serde_json::from_value(
        super::status_conditions::definition("fixture_lock", Some(400)),
    )
    .unwrap();
    for elapsed in [0, 200] {
        let mut creature = CreatureState::new(condition_build(vec![lock.clone()])).unwrap();
        creature.apply_condition(&lock.id, ActorId(9), 200).unwrap();
        creature.advance_active(elapsed);
        let mut dead = creature.clone();
        dead.damage(u32::MAX);
        assert!(dead.conditions().is_empty());
        creature.rebuild(condition_build(vec![])).unwrap();
        assert!(creature.conditions().is_empty());
    }
}
#[test]
fn administrative_definition_replacement_cannot_spin_on_a_protected_entry() {
    const CHILD: &str = "TOR_TEST_PROTECTED_CONDITION_REBUILD";
    if std::env::var_os(CHILD).is_none() {
        // Isolate the pre-fix nonterminating fold so first-red cannot strand
        // the test runner or leave a busy thread behind.
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "creature_state::administrative_definition_replacement_cannot_spin_on_a_protected_entry"])
            .env(CHILD, "1").spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("administrative condition replacement did not terminate");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let lock: tor_simulation::rules::ConditionDef = serde_json::from_value(
        super::status_conditions::definition("fixture_lock", Some(400)),
    )
    .unwrap();
    let mut replacement = lock.clone();
    replacement.restrictions.clear();
    replacement.preparation = tor_simulation::rules::ConditionPreparation::Preserve;
    replacement.timing = tor_simulation::rules::ConditionTiming::expiring();
    replacement.modifiers = vec![Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 1000,
        },
    }];
    let mut creature = CreatureState::new(condition_build(vec![lock.clone()])).unwrap();
    creature.apply_condition(&lock.id, ActorId(9), 200).unwrap();
    let outcome = creature
        .rebuild(condition_build(vec![replacement]))
        .unwrap();
    assert!(outcome.conditions_cleared);
    assert!(creature.conditions().is_empty());
}

#[test]
fn administrative_semantic_changes_clear_retained_phases_without_gameplay_protection() {
    let lock: tor_simulation::rules::ConditionDef = serde_json::from_value(
        super::status_conditions::definition("fixture_lock", Some(400)),
    )
    .unwrap();
    for elapsed in [0, 200] {
        let mut creature = CreatureState::new(condition_build(vec![lock.clone()])).unwrap();
        creature.apply_condition(&lock.id, ActorId(9), 200).unwrap();
        creature.advance_active(elapsed);
        let retained: Vec<_> = creature.conditions().instances().cloned().collect();
        creature
            .rebuild(condition_build(vec![lock.clone()]))
            .unwrap();
        assert_eq!(
            creature
                .conditions()
                .instances()
                .cloned()
                .collect::<Vec<_>>(),
            retained
        );
        let mut renamed = lock.clone();
        renamed.name = "Presentation-only rename".into();
        creature.rebuild(condition_build(vec![renamed])).unwrap();
        assert_eq!(
            creature
                .conditions()
                .instances()
                .cloned()
                .collect::<Vec<_>>(),
            retained
        );
        let mut changed = lock.clone();
        changed.restrictions = vec![tor_simulation::rules::ConditionRestriction::OrdinaryMovement];
        let outcome = creature.rebuild(condition_build(vec![changed])).unwrap();
        assert!(outcome.conditions_cleared);
        assert!(
            creature.conditions().is_empty(),
            "same protection metadata is not the same authored semantics"
        );
    }
}

#[test]
fn administrative_replacement_cannot_reinterpret_a_reserved_condition_tombstone() {
    let original = condition_build(vec![]);
    let mut creature = CreatureState::new(original.clone()).unwrap();
    creature
        .apply_condition(&ConditionId::new("readied").unwrap(), ActorId(9), 200)
        .unwrap();
    let mut recorded = serde_json::to_value(
        tor_simulation::creatures::record::CreatureRecord::capture(&creature),
    )
    .unwrap();
    recorded["conditions"][0]["reserved_by"] = serde_json::json!(9);
    let mut creature =
        serde_json::from_value::<tor_simulation::creatures::record::CreatureRecord>(recorded)
            .unwrap()
            .restore(original.content().clone(), vec![])
            .unwrap();
    let before = creature.clone();
    let mut document = original.ruleset().document().clone();
    document
        .conditions
        .iter_mut()
        .find(|condition| condition.id.as_str() == "readied")
        .unwrap()
        .stacking = tor_simulation::rules::Stacking::Keep;
    let content = Content::new(
        tor_simulation::rules::Ruleset::validate(document).unwrap(),
        (*original.content().catalog()).clone(),
    )
    .unwrap();
    let replacement = CreatureBuild::from_recorded(
        content,
        original.species_id().clone(),
        original.initial_attributes(),
        original.ledger().clone(),
        original.binding(),
        original.advancement().clone(),
        vec![],
    )
    .unwrap();
    assert_eq!(
        creature.rebuild(replacement),
        Err(tor_simulation::creatures::CreatureStateError::Inconsistent)
    );
    assert_eq!(
        creature, before,
        "failed rebuild keeps the reservation and original definition"
    );
}

#[test]
fn protected_and_reserved_books_keep_claims_when_ordinary_modifier_folds_reconcile() {
    let lock: tor_simulation::rules::ConditionDef = serde_json::from_value(
        super::status_conditions::definition("fixture_lock", Some(400)),
    )
    .unwrap();
    let cap = timed(
        "temporary_cap",
        ModifierOp::Set {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 10,
        },
    );
    let giant = timed(
        "temporary_giant",
        ModifierOp::Add {
            stat: Stat::Attribute {
                attribute: tor_simulation::attributes::Attribute::Strength,
            },
            value: 1000,
        },
    );
    for mixed in [false, true] {
        for elapsed in [0, 200] {
            let mut creature = CreatureState::new(condition_build(vec![
                lock.clone(),
                cap.clone(),
                giant.clone(),
            ]))
            .unwrap();
            creature.apply_condition(&lock.id, ActorId(9), 200).unwrap();
            creature.advance_active(elapsed);
            creature
                .apply_condition(&ConditionId::new("readied").unwrap(), ActorId(9), 1000)
                .unwrap();
            creature
                .start_cost(
                    IntentionId(7),
                    tor_simulation::costs::CostBundle::single(ResourceCost {
                        resource: Resource::Focus,
                        start: 1,
                        resolution: 1,
                    })
                    .unwrap(),
                )
                .unwrap();
            let mut recorded = serde_json::to_value(
                tor_simulation::creatures::record::CreatureRecord::capture(&creature),
            )
            .unwrap();
            recorded["conditions"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["condition"] == "readied")
                .unwrap()["reserved_by"] = serde_json::json!(9);
            let mut creature = serde_json::from_value::<
                tor_simulation::creatures::record::CreatureRecord,
            >(recorded)
            .unwrap()
            .restore(creature.build().content().clone(), vec![])
            .unwrap();
            let holds = creature.costs().reservations().clone();
            if mixed {
                creature.apply_condition(&cap.id, ActorId(9), 10).unwrap();
                creature
                    .apply_condition(&giant.id, ActorId(9), 100)
                    .unwrap();
            }
            creature.advance_active(10);
            creature.rebuild(creature.build().clone()).unwrap();
            let entries: Vec<_> = creature.conditions().instances().collect();
            assert_eq!(entries.len(), 2);
            let reserved = entries
                .iter()
                .find(|entry| entry.condition.as_str() == "readied")
                .unwrap();
            assert_eq!(reserved.reserved_by, Some(ActorId(9)));
            assert_eq!(
                reserved.remaining,
                Some(990),
                "an unexpired reservation still contributes no modifiers"
            );
            assert_eq!(creature.costs().reservations(), &holds);
            assert!(creature.finish_cost(IntentionId(7)).is_ok());
        }
    }
}

#[test]
fn expiring_capacity_cancels_holds_and_chunks_recovery_at_the_boundary() {
    let capacity = timed(
        "capacity",
        ModifierOp::Add {
            stat: Stat::ResourceMax {
                resource: Resource::Mana,
            },
            value: 4,
        },
    );
    let mut creature = CreatureState::new(condition_build(vec![capacity.clone()])).unwrap();
    creature
        .apply_condition(&capacity.id, ActorId(9), 10)
        .unwrap();
    assert_eq!(creature.restore_resource(Resource::Mana, 4), 4);
    creature
        .start_cost(
            IntentionId(7),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Mana,
                start: 0,
                resolution: 6,
            })
            .unwrap(),
        )
        .unwrap();
    let transition = creature.advance_active(10);
    assert_eq!(transition.canceled, vec![IntentionId(7)]);
    assert_eq!(creature.costs().resources().maximum(Resource::Mana), 4);
    assert_eq!(creature.costs().resources().balance(Resource::Mana), 4);
}
#[test]
fn temporary_health_expiry_can_kill_without_revival_or_stale_modifiers() {
    let health = timed(
        "health",
        ModifierOp::Add {
            stat: Stat::Health,
            value: 10,
        },
    );
    let mut creature = CreatureState::new(condition_build(vec![health.clone()])).unwrap();
    let baseline = creature.health().maximum();
    creature
        .apply_condition(&health.id, ActorId(9), 10)
        .unwrap();
    creature.damage(u32::try_from(baseline + 5).unwrap());
    assert!(!creature.health().dead());
    assert!(creature.advance_active(10).died);
    assert!(creature.health().dead());
    assert_eq!(creature.health().maximum(), baseline);
    assert!(creature.conditions().is_empty());
    assert_eq!(creature.derived().maximum_health, baseline);
}

#[test]
fn source_scoped_condition_caps_check_edge_but_preserves_shared_effect_edge() {
    let mut marked = timed(
        "marked",
        ModifierOp::Add {
            stat: Stat::Edge,
            value: -2,
        },
    );
    marked.modifiers[0].scope = Scope::VersusSource;
    let mut creature = CreatureState::new(condition_build(vec![marked.clone()])).unwrap();
    creature
        .apply_condition(&marked.id, ActorId(9), 20)
        .unwrap();
    assert_eq!(creature.derived().edge_against(ActorId(9)).balance(), -1);
    assert_eq!(
        creature
            .derived()
            .effect_edge_against(creature.build().ruleset(), ActorId(9))
            .unwrap(),
        -2
    );
    assert_eq!(creature.derived().edge_against(ActorId(10)).balance(), 0);
    creature.advance_active(20);
    assert_eq!(creature.derived().edge_against(ActorId(9)).balance(), 0);
}
