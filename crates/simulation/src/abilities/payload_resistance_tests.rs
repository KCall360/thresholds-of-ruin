//! Delivery, immunity and unilateral resistance are independent receipts.
use super::*;
use crate::attributes::{Attribute, Attributes, ByAttribute, Skill};
use crate::content_ids::{ConditionId, DescriptorId};
use crate::dice::{roll_check, DicePool, Edge};
use crate::rules::{
    AmountExpr, DescriptorResistance, Effect, Modifier, ModifierOp, NamedDescriptor, Scope,
    Selector,
};
use crate::{ActorId, Game, ItemDiscovery, ItemSpec, Work};

fn descriptor() -> DescriptorId {
    DescriptorId::new("fixture_exposure").unwrap()
}
fn fixture(dc: i32, immune: bool) -> Game {
    super::ray_tests::fixture_with(|document, _, species| {
        document.descriptors.push(NamedDescriptor {
            id: descriptor(),
            name: "Fixture exposure".into(),
            resistance: Some(DescriptorResistance {
                skill: Skill::Athletics,
                difficulty: dc,
            }),
        });
        species.default_attributes = Attributes::new(ByAttribute {
            strength: 5,
            ..Default::default()
        })
        .unwrap();
        if immune {
            species.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Immunity {
                    selector: Selector::Descriptor {
                        descriptor: descriptor(),
                    },
                },
            });
        }
    })
}
fn loss() -> PlannedEffect {
    PlannedEffect::LoseAttribute {
        attribute: Attribute::Strength,
        amount: PlannedAmount::Rolled {
            dice: DicePool::new(1, 3, 0).unwrap(),
        },
    }
}
fn exposure<'a>(
    effects: &'a [PlannedEffect],
    descriptors: &'a [DescriptorId],
) -> ResolutionPlan<'a> {
    ResolutionPlan::Exposure {
        actor: ActorId(1),
        target: ActorId(2),
        effects,
        descriptors,
    }
}
fn roundtrip(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let (saved, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(saved, shared)).unwrap()).unwrap();
    Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap()
}

#[test]
fn secondary_athletics_resistance_is_once_after_delivery_and_before_payload_draws() {
    for dc in [0, 100] {
        let game = fixture(dc, false);
        let snapshot = Snapshot::capture(&game.creature(ActorId(2)).unwrap());
        let before = snapshot.clone();
        let effects = [loss()];
        let descriptors = [descriptor()];
        let mut expected_rng = 42;
        let rolled = roll_check(&mut expected_rng, Edge::default());
        let derived = snapshot.creature.derived();
        let resisted = i64::from(rolled.kept)
            + i64::from(derived.attributes.get(Attribute::Strength))
            + i64::from(derived.skills.get(Skill::Athletics))
            >= i64::from(dc);
        let amount = (!resisted).then(|| {
            DicePool::new(1, 3, 0)
                .unwrap()
                .roll(&mut expected_rng, Edge::default())
                .total
        });
        let mut rng = 42;
        let result = resolve(
            exposure(&effects, &descriptors),
            &snapshot,
            &mut rng,
            &mut (),
        )
        .unwrap();
        assert_eq!(
            rng, expected_rng,
            "exactly one resistance, no fabricated Physical delivery check"
        );
        assert_eq!(result.check, None);
        assert_eq!(
            result.resistance,
            Some(ResolvedCheck {
                actor: crate::rules::CheckActor::Target,
                success: resisted,
                opposed: false
            })
        );
        assert_eq!(
            result.effects,
            amount
                .map(|amount| vec![ResolvedEffect::LoseAttribute {
                    attribute: Attribute::Strength,
                    amount
                }])
                .unwrap_or_default()
        );
        assert_eq!(snapshot, before);
        assert_eq!(
            serde_json::from_value::<Outcome>(serde_json::to_value(&result).unwrap()).unwrap(),
            result
        );
    }
}

#[test]
fn immune_dead_empty_or_missed_delivery_never_rolls_resistance() {
    for immune in [false, true] {
        for dead in [false, true] {
            for delivered in [false, true] {
                let game = fixture(100, immune);
                let mut creature = game.creature(ActorId(2)).unwrap().into_owned();
                if dead {
                    creature.kill();
                }
                let snapshot = Snapshot::capture(&creature);
                let effects = [loss()];
                let descriptors = [descriptor()];
                let check = PlannedCheck {
                    actor: crate::rules::CheckActor::User,
                    target_participates: true,
                    bonus: if delivered { 100 } else { -100 },
                    threshold: CheckThreshold::Fixed { value: 12 },
                    edge: 0,
                };
                let plan = ResolutionPlan::Environment {
                    target: ActorId(2),
                    check: &check,
                    effects: &effects,
                    descriptors: &descriptors,
                };
                let mut expected = 42;
                if !immune && !dead {
                    roll_check(&mut expected, Edge::default());
                    if delivered {
                        roll_check(&mut expected, Edge::default());
                        DicePool::new(1, 3, 0)
                            .unwrap()
                            .roll(&mut expected, Edge::default());
                    }
                }
                let mut rng = 42;
                let outcome = resolve(plan, &snapshot, &mut rng, &mut ()).unwrap();
                assert_eq!(rng, expected);
                assert_eq!(outcome.resistance.is_some(), delivered && !immune && !dead);
                assert_eq!(
                    outcome.check.map(|c| c.success),
                    (!immune && !dead).then_some(delivered)
                );
            }
        }
    }
    let game = fixture(100, false);
    let mut rng = 42;
    let outcome = resolve(
        exposure(&[], &[descriptor()]),
        &Snapshot::capture(&game.creature(ActorId(2)).unwrap()),
        &mut rng,
        &mut (),
    )
    .unwrap();
    assert_eq!((outcome.resistance, rng), (None, 42));
}

#[test]
fn resistance_receipt_rejects_independent_malformed_variants_after_valid_baseline() {
    let game = fixture(0, false);
    let outcome = resolve(
        exposure(&[loss()], &[descriptor()]),
        &Snapshot::capture(&game.creature(ActorId(2)).unwrap()),
        &mut 42,
        &mut (),
    )
    .unwrap();
    let valid = serde_json::to_value(&outcome).unwrap();
    assert_eq!(
        serde_json::from_value::<Outcome>(valid.clone()).unwrap(),
        outcome
    );
    for mutation in 0..6 {
        let mut value = valid.clone();
        match mutation {
            0 => {
                value.as_object_mut().unwrap().remove("resistance");
            }
            1 => value["resistance"]["actor"] = serde_json::json!("user"),
            2 => value["resistance"]["opposed"] = serde_json::json!(true),
            3 => {
                value["effects"] = serde_json::to_value(vec![ResolvedEffect::LoseAttribute {
                    attribute: Attribute::Strength,
                    amount: 1,
                }])
                .unwrap()
            }
            4 => value["resistance"] = serde_json::json!(["target", true, false]),
            _ => value["participation"]["payload"] = serde_json::json!({"kind":"source"}),
        }
        assert!(
            serde_json::from_value::<Outcome>(value).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn exact_outgoing_token_receipt_is_consumed_only_on_actual_exposure() {
    let token = ConditionId::new("ray_token").unwrap();
    for immune in [false, true] {
        for dc in [0, 100] {
            let mut game = fixture(dc, immune);
            game.apply_condition(ActorId(2), &token, ActorId(1), 100)
                .unwrap();
            let mut restored = roundtrip(&game);
            let effects = [Effect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 1 },
            }];
            for current in [&mut game, &mut restored] {
                let before = current
                    .creature(ActorId(2))
                    .unwrap()
                    .derived()
                    .attributes
                    .get(Attribute::Strength);
                let mut expected = current.rules_rng;
                if !immune {
                    roll_check(&mut expected, Edge::from_counts(1, 0));
                }
                let applied = current
                    .with_recovery_transition(true, |g| {
                        g.apply_exposure_in_transaction(ActorId(2), &effects, &[descriptor()])
                    })
                    .unwrap();
                assert_eq!(applied, !immune && dc == 100);
                assert_eq!(current.rules_rng, expected);
                assert_eq!(
                    current
                        .creature(ActorId(2))
                        .unwrap()
                        .conditions()
                        .has_active(&token),
                    immune
                );
                assert_eq!(
                    current
                        .creature(ActorId(2))
                        .unwrap()
                        .derived()
                        .attributes
                        .get(Attribute::Strength),
                    before - u16::from(applied)
                );
            }
            assert_eq!(game, restored);
        }
    }
}

#[test]
fn primary_damage_survives_resisted_or_immune_poison_rider() {
    for immune in [false, true] {
        for dc in [0, 100] {
            let (mut game, _) = super::strike_tests::sequence_game_custom(
                &[0],
                |ability| {
                    let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes
                    else {
                        unreachable!()
                    };
                    strikes[0].riders.push(crate::rules::RiderDef {
                        cancellable: false,
                        trigger: crate::rules::RiderTrigger::Success,
                        condition: None,
                        check: None,
                        descriptors: vec![descriptor()],
                        effects: vec![Effect::LoseAttribute {
                            attribute: Attribute::Strength,
                            amount: AmountExpr::Fixed { amount: 1 },
                        }],
                    });
                },
                |document, species| {
                    document.descriptors.push(NamedDescriptor {
                        id: descriptor(),
                        name: "Exposure".into(),
                        resistance: Some(DescriptorResistance {
                            skill: Skill::Athletics,
                            difficulty: dc,
                        }),
                    });
                    species.default_attributes = Attributes::new(ByAttribute {
                        strength: 5,
                        ..Default::default()
                    })
                    .unwrap();
                    if immune {
                        species.modifiers.push(Modifier {
                            cancellable: false,
                            scope: Scope::Always,
                            op: ModifierOp::Immunity {
                                selector: Selector::Descriptor {
                                    descriptor: descriptor(),
                                },
                            },
                        });
                    }
                },
            );
            let health = game.health(ActorId(2)).unwrap().0;
            let strength = game
                .creature(ActorId(2))
                .unwrap()
                .derived()
                .attributes
                .get(Attribute::Strength);
            let mut expected_rng = game.rules_rng;
            if !immune {
                roll_check(&mut expected_rng, Edge::default());
            }
            game.with_recovery_transition(true, |g| {
                g.tick = 10;
                g.resolve_preparations()
            })
            .unwrap();
            assert_eq!(game.health(ActorId(2)).unwrap().0, health - 3);
            assert_eq!(
                game.creature(ActorId(2))
                    .unwrap()
                    .derived()
                    .attributes
                    .get(Attribute::Strength),
                strength - u16::from(!immune && dc == 100)
            );
            assert_eq!(
                game.rules_rng, expected_rng,
                "unconditional delivered rider has no second Physical attack"
            );
        }
    }
}

#[test]
fn poisoned_self_item_freezes_descriptor_spends_exact_unit_and_replays_resistance_after_restore() {
    for dc in [0, 100] {
        let mut game = fixture(dc, false);
        let mut spec = ItemSpec::ordinary("fixture poison".into());
        spec.identity = "fixture.poisoned_unit".into();
        spec.concealed = true;
        spec.class = crate::ItemClass::Potion;
        spec.stackable = true;
        spec.consumable = Some(crate::ConsumableSpec {
            discovery: ItemDiscovery::OnUse,
            descriptors: vec![descriptor()],
            effects: vec![Effect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 1 },
            }],
        });
        let location = game.actors[&ActorId(1)].location;
        let item = game
            .place_item_stack(20, location, Some(ActorId(1)), 2, spec)
            .unwrap();
        let plan = game.plan_work(ActorId(1), Work::Drink { item }).unwrap();
        assert!(plan.valid(game.content.rules()));
        let mut forged = plan.clone();
        let Plan::Drink { descriptors, .. } = &mut forged else {
            unreachable!()
        };
        descriptors.clear();
        assert!(
            forged.valid(game.content.rules()),
            "well-shaped but wrong source capture"
        );
        assert!(game.item_allocation(&plan).is_ok());
        assert!(game.item_allocation(&forged).is_err());
        let due = plan.preparation();
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
        let issued = game
            .admit_intention(
                ActorId(1),
                crate::Action::Drink { item },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        let done = game.execute_next_intention().unwrap();
        assert_eq!(done.intention.id, issued);
        done.outcome.unwrap();
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, plan);
        let mut restored = roundtrip(&game);
        for current in [&mut game, &mut restored] {
            let strength = current
                .creature(ActorId(1))
                .unwrap()
                .derived()
                .attributes
                .get(Attribute::Strength);
            let mut expected_rng = current.rules_rng;
            roll_check(&mut expected_rng, Edge::default());
            current
                .with_recovery_transition(true, |g| {
                    g.tick = due;
                    g.resolve_preparations()
                })
                .unwrap();
            assert_eq!(current.items[&item].quantity, 1);
            assert!(current.items[&item].holds.is_empty());
            assert!(current.preparation(ActorId(1)).is_none());
            assert!(current.actors[&ActorId(1)]
                .knowledge
                .contains("fixture.poisoned_unit"));
            assert_eq!(
                current
                    .creature(ActorId(1))
                    .unwrap()
                    .derived()
                    .attributes
                    .get(Attribute::Strength),
                strength - u16::from(dc == 100)
            );
            assert_eq!(current.rules_rng, expected_rng);
        }
        assert_eq!(game, restored);
    }
}

#[test]
fn resistance_sources_and_rng_roll_back_with_hard_payload_failure() {
    let mut game = fixture(100, false);
    let token = ConditionId::new("ray_token").unwrap();
    game.apply_condition(ActorId(2), &token, ActorId(1), 100)
        .unwrap();
    let before = game.clone();
    let error = game.with_recovery_transition(true, |g| {
        let effects = [
            loss(),
            PlannedEffect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: PlannedAmount::Fixed {
                    amount: crate::limits::NUMERIC as u32 + 1,
                },
            },
        ];
        let snapshot = Snapshot::capture(&g.creature(ActorId(2)).unwrap());
        let mut rng = g.rules_rng;
        let outcome = super::resolve::resolve_receipt(
            exposure(&effects, &[descriptor()]),
            &snapshot,
            &mut rng,
            &mut (),
            true,
        )?;
        g.rules_rng = rng;
        g.consume_resistance_tokens(&outcome.resistance_tokens)?;
        g.apply_outcome(outcome.outcome)
    });
    assert!(error.is_err());
    assert_eq!(
        game, before,
        "hard payload failure cannot spend the outgoing token or resistance draw"
    );
}

#[test]
fn descriptor_policy_validation_has_positive_baseline_and_independent_conflicts() {
    let game = fixture(12, false);
    let rules = game.content.rules();
    assert!(rules.validate_payload_descriptors(&[descriptor()]).is_ok());
    assert!(rules.validate_payload_descriptors(&[]).is_ok());
    assert!(rules
        .validate_payload_descriptors(&[descriptor(), descriptor()])
        .is_err());
    assert!(rules
        .validate_payload_descriptors(&[DescriptorId::new("unknown").unwrap()])
        .is_err());
    let mut document = rules.document().clone();
    let other = DescriptorId::new("second_exposure").unwrap();
    document.descriptors.push(NamedDescriptor {
        id: other.clone(),
        name: "Second exposure".into(),
        resistance: Some(DescriptorResistance {
            skill: Skill::Athletics,
            difficulty: 12,
        }),
    });
    let two = crate::rules::Ruleset::validate(document).unwrap();
    assert!(two
        .validate_payload_descriptors(std::slice::from_ref(&other))
        .is_ok());
    assert!(
        two.validate_payload_descriptors(&[descriptor(), other])
            .is_err(),
        "even equal policies cannot create ambiguous repeated rolls"
    );
    let valid = serde_json::json!({"skill":"athletics", "difficulty":12});
    serde_json::from_value::<DescriptorResistance>(valid.clone()).unwrap();
    for field in ["skill", "difficulty"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<DescriptorResistance>(missing).is_err());
    }
    for difficulty in [-1, crate::limits::NUMERIC + 1] {
        let mut invalid = valid.clone();
        invalid["difficulty"] = serde_json::json!(difficulty);
        assert!(serde_json::from_value::<DescriptorResistance>(invalid).is_err());
    }
    assert!(
        serde_json::from_value::<DescriptorResistance>(serde_json::json!(["athletics", 12]))
            .is_err()
    );
}

#[test]
fn actual_ray_primary_claim_cannot_be_reused_by_secondary_resistance() {
    for (immune, dodges, dc) in [
        (false, false, 0),
        (false, false, 100),
        (false, true, 100),
        (true, false, 100),
    ] {
        let mut game = super::ray_tests::fixture_with(|document, ray, species| {
            document.descriptors.push(NamedDescriptor {
                id: descriptor(),
                name: "Exposure".into(),
                resistance: Some(DescriptorResistance {
                    skill: Skill::Athletics,
                    difficulty: dc,
                }),
            });
            ray.invocation = InvocationKind::DirectionalContactRay;
            let crate::rules::StrikeSource::Authored { strikes } = &mut ray.strikes else {
                unreachable!()
            };
            strikes[0].descriptors = vec![descriptor()];
            strikes[0].on_failure = vec![Effect::Damage {
                damage: crate::rules::DamageExpr::Components {
                    components: vec![crate::rules::DamageComponentExpr {
                        category: crate::combat::DamageType::Impact,
                        descriptor: None,
                        amount: AmountExpr::Fixed { amount: 1 },
                    }],
                },
            }];
            if immune {
                species.modifiers.push(Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Immunity {
                        selector: Selector::Descriptor {
                            descriptor: descriptor(),
                        },
                    },
                });
            }
        });
        let token = ConditionId::new("ray_token").unwrap();
        game.apply_condition(ActorId(2), &token, ActorId(1), 100)
            .unwrap();
        let plan = game
            .ability_plan_with_supplies(
                ActorId(1),
                &crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
                AbilityInvocation::directional_ray(StepDirection::East),
                Default::default(),
            )
            .unwrap();
        let ray = plan.as_ray().unwrap();
        let mut budget = super::resolve::StrikeBudget::ray(ray);
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        let mut expected = game.rules_rng;
        if !immune {
            roll_check(&mut expected, Edge::from_counts(1, 0));
            if !dodges {
                roll_check(&mut expected, Edge::default());
            }
        }
        let outcome = game
            .resolve_ray_encounter(
                ray,
                ActorId(2),
                0,
                if dodges { 0 } else { 1000 },
                true,
                None,
                crate::event_history::SourceAbilityDisclosure::Public,
                &mut budget,
                &mut activity,
            )
            .unwrap();
        assert_eq!(
            game.rules_rng, expected,
            "primary token must not lend an edge to resistance too"
        );
        assert_eq!(
            outcome.check.map(|check| check.success),
            (!immune).then_some(dodges)
        );
        assert_eq!(
            outcome.resistance.map(|check| check.success),
            (!immune && !dodges).then_some(dc == 0)
        );
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .conditions()
                .has_active(&token),
            immune
        );
        assert_eq!(outcome.effects.is_empty(), immune || dodges || dc == 0);
    }
}

#[test]
fn resisted_source_payload_is_rejected_without_breaking_source_utilities() {
    let game = fixture(12, false);
    let rules = game.content.rules();
    let target = [Effect::LoseAttribute {
        attribute: Attribute::Strength,
        amount: AmountExpr::Fixed { amount: 1 },
    }];
    let source = [Effect::ObservePublicPlan];
    assert!(rules
        .validate_payload_recipients(&[descriptor()], target.iter().map(Effect::recipient))
        .is_ok());
    assert!(rules
        .validate_payload_recipients(&[], source.iter().map(Effect::recipient))
        .is_ok());
    assert!(rules
        .validate_payload_recipients(&[descriptor()], source.iter().map(Effect::recipient))
        .is_err());

    let mut spec = crate::ReadableSpec {
        control: None,
        descriptors: vec![],
        effects: source.to_vec(),
        identify: None,
        discovery: ItemDiscovery::Never,
    };
    spec.validate_rules(rules).unwrap();
    spec.descriptors = vec![descriptor()];
    assert!(spec.validate_rules(rules).is_err());
    let mut plan = Plan::Read {
        control: None,
        actor: ActorId(1),
        item: crate::ItemId(1),
        preparation: 100,
        descriptors: vec![],
        effects: vec![PlannedEffect::ObservePublicPlan],
        identify: None,
        discovery: ItemDiscovery::Never,
    };
    assert!(plan.valid(rules));
    if let Plan::Read { descriptors, .. } = &mut plan {
        *descriptors = vec![descriptor()];
    }
    assert!(!plan.valid(rules));
    let snapshot = Snapshot::capture(&game.creature(ActorId(2)).unwrap());
    let mut rng = 42;
    assert!(matches!(
        resolve(plan.resolution().unwrap(), &snapshot, &mut rng, &mut ()),
        Err(EffectApplicationError::InvalidDefinition)
    ));
    assert_eq!(
        rng, 42,
        "invalid recipient is rejected before a resistance or amount draw"
    );

    // Check the authored primary boundary, not only a bare policy helper.
    let mut document = rules.document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    ability.interaction = crate::rules::Interaction::NonHostile;
    ability.strikes = crate::rules::StrikeSource::Authored {
        strikes: vec![crate::rules::StrikeDef {
            cancellable: false,
            offset: 0,
            unlock: None,
            check: None,
            on_success: source.to_vec(),
            on_failure: vec![],
            descriptors: vec![],
            riders: vec![],
        }],
    };
    let positive = crate::rules::Ruleset::validate(document.clone()).unwrap();
    assert!(positive
        .validate_payload_descriptors(&[descriptor()])
        .is_ok());
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    strikes[0].descriptors = vec![descriptor()];
    assert!(crate::rules::Ruleset::validate(document).is_err());
}

#[test]
fn authored_and_additional_natural_riders_validate_their_own_resisted_recipient() {
    use crate::rules::{RiderDef, RiderTrigger, Ruleset, RulesetError, StrikeDef, StrikeSource};

    let game = fixture(12, false);
    for natural in [false, true] {
        let mut baseline = game.content.rules().document().clone();
        let ability = baseline
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        let rider = RiderDef {
            cancellable: false,
            trigger: RiderTrigger::Success,
            condition: None,
            check: None,
            effects: vec![Effect::ObservePublicPlan],
            descriptors: vec![],
        };
        ability.strikes = if natural {
            StrikeSource::NaturalMelee {
                additional_riders: vec![rider],
            }
        } else {
            StrikeSource::Authored {
                strikes: vec![StrikeDef {
                    cancellable: false,
                    offset: 0,
                    unlock: None,
                    check: None,
                    on_success: vec![],
                    on_failure: vec![],
                    descriptors: vec![],
                    riders: vec![rider],
                }],
            }
        };
        Ruleset::validate(baseline.clone()).unwrap();

        // Independent clones preserve the valid descriptorless Source utility;
        // only a resistant descriptor makes that recipient incoherent.
        for source in [false, true] {
            let mut document = baseline.clone();
            let ability = document
                .abilities
                .iter_mut()
                .find(|ability| ability.basic_attack)
                .unwrap();
            let rider = match &mut ability.strikes {
                StrikeSource::NaturalMelee { additional_riders } => &mut additional_riders[0],
                StrikeSource::Authored { strikes } => &mut strikes[0].riders[0],
                _ => unreachable!(),
            };
            rider.descriptors = vec![descriptor()];
            if !source {
                rider.effects = vec![Effect::LoseAttribute {
                    attribute: Attribute::Strength,
                    amount: AmountExpr::Fixed { amount: 1 },
                }];
            }
            let result = Ruleset::validate(document);
            if source {
                assert!(matches!(result, Err(RulesetError::Bound { field })
                    if field == "resistance_target_payload"));
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
fn frozen_primary_branches_and_riders_reject_resisted_source_independently() {
    let game = fixture(12, false);
    let rules = game.content.rules();
    let baseline = FrozenStrike {
        offset: 0,
        origin: StrikeOrigin::Ability { ordinal: 0 },
        check: Some(PlannedCheck {
            actor: crate::rules::CheckActor::User,
            target_participates: true,
            bonus: 0,
            threshold: CheckThreshold::Fixed { value: 12 },
            edge: 0,
        }),
        on_success: vec![PlannedEffect::ObservePublicPlan],
        on_failure: vec![PlannedEffect::ObservePublicPlan],
        descriptors: vec![],
        riders: vec![FrozenRider {
            trigger: crate::rules::RiderTrigger::Success,
            condition: None,
            check: None,
            effects: vec![PlannedEffect::ObservePublicPlan],
            descriptors: vec![],
        }],
        edge: 0,
        shared_check: Some(CheckPhase::Primary {
            side: CheckSide::Initiating,
        }),
    };
    assert!(baseline.valid(rules));
    for site in 0..3 {
        let mut positive = baseline.clone();
        if site < 2 {
            positive.descriptors = vec![descriptor()];
            positive.on_success = vec![loss()];
            positive.on_failure = vec![loss()];
        } else {
            positive.riders[0].descriptors = vec![descriptor()];
            positive.riders[0].effects = vec![loss()];
        }
        assert!(positive.valid(rules), "positive site {site}");
        let mut invalid = positive;
        let effects = match site {
            0 => &mut invalid.on_success,
            1 => &mut invalid.on_failure,
            _ => &mut invalid.riders[0].effects,
        };
        *effects = vec![PlannedEffect::ObservePublicPlan];
        assert!(!invalid.valid(rules), "resisted Source site {site}");
    }
}
