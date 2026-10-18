//! Directional invocation contracts. No production attack skill is selected here.
use super::*;
use crate::attributes::{Attribute, ManaBinding, Skill};
use crate::content_ids::{AbilityId, ConditionId};
use crate::rules::*;
use crate::{ActorId, Game, GameError};
use std::collections::BTreeSet;
use std::num::NonZeroU64;
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture() -> Game {
    fixture_with(|_, _, _| {})
}
fn fixture_with(
    customize: impl FnOnce(&mut RulesetDocument, &mut AbilityDef, &mut crate::creatures::Species),
) -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    for (id, stat, value, scope) in [
        (
            "ray_strong",
            Stat::Attribute {
                attribute: Attribute::Strength,
            },
            100,
            Scope::Always,
        ),
        (
            "ray_agile",
            Stat::Attribute {
                attribute: Attribute::Speed,
            },
            100,
            Scope::Always,
        ),
        ("ray_advantage", Stat::CheckEdge, 5, Scope::Always),
        ("ray_fear", Stat::CheckEdge, -2, Scope::VersusSource),
        ("ray_focus", Stat::CheckEdge, 2, Scope::VersusSource),
        (
            "ray_incoming",
            Stat::IncomingEdge { defense: None },
            -2,
            Scope::VersusSource,
        ),
    ] {
        let mut condition = document
            .conditions
            .iter()
            .find(|c| c.id.as_str() == "distracted")
            .unwrap()
            .clone();
        condition.id = ConditionId::new(id).unwrap();
        condition.family = crate::content_ids::ConditionFamilyId::new(id).unwrap();
        condition.name = id.into();
        condition.source_scoped = true;
        condition.lifecycle = ConditionLifecycle::Timed;
        condition.modifiers = vec![Modifier {
            scope,
            op: ModifierOp::Add { stat, value },
        }];
        document.conditions.push(condition);
    }
    for (id, lifecycle, stat, value) in [
        (
            "ray_token",
            ConditionLifecycle::NextCheck,
            Stat::CheckEdge,
            1,
        ),
        (
            "ray_incoming_token",
            ConditionLifecycle::NextCheckAgainst,
            Stat::IncomingEdge { defense: None },
            -1,
        ),
    ] {
        let mut condition = document
            .conditions
            .iter()
            .find(|c| c.id.as_str() == "distracted")
            .unwrap()
            .clone();
        condition.id = ConditionId::new(id).unwrap();
        condition.family = crate::content_ids::ConditionFamilyId::new(id).unwrap();
        condition.name = id.into();
        condition.lifecycle = lifecycle;
        condition.source_scoped = false;
        condition.modifiers = vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add { stat, value },
        }];
        document.conditions.push(condition);
    }
    let mut ward = document
        .conditions
        .iter()
        .find(|c| c.id.as_str() == "distracted")
        .unwrap()
        .clone();
    ward.id = ConditionId::new("ray_ward").unwrap();
    ward.family = crate::content_ids::ConditionFamilyId::new("ray_ward").unwrap();
    ward.name = "Ray ward".into();
    ward.lifecycle = ConditionLifecycle::Timed;
    ward.modifiers = vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Reduction {
            selector: Selector::Category {
                category: crate::combat::DamageType::Energy,
            },
            amount: 7,
        },
    }];
    document.conditions.push(ward);
    let mut ray = document
        .abilities
        .iter()
        .find(|a| a.basic_attack)
        .unwrap()
        .clone();
    ray.id = AbilityId::new("fixture_ray").unwrap();
    ray.basic_attack = false;
    ray.invocation = InvocationKind::DirectionalRay;
    ray.reach = Reach::Sight { range: 6 };
    ray.cost = None;
    ray.cooldown = None;
    ray.timing = Timing {
        source: TimingSource::Fixed {
            prepare: 10,
            recover: 20,
        },
        scaled_by: None,
    };
    ray.strikes = StrikeSource::Authored {
        strikes: vec![StrikeDef {
            offset: 0,
            unlock: None,
            check: Some(CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named {
                    skill: Skill::Agility,
                },
                against: Against::Opposed {
                    skill: SkillSelector::Named {
                        skill: Skill::Athletics,
                    },
                },
            }),
            on_success: vec![],
            on_failure: vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: crate::combat::DamageType::Energy,
                        descriptor: None,
                        amount: AmountExpr::Rolled {
                            dice: crate::rules::DiceExpr {
                                count: 6,
                                sides: 6,
                                bonus: 0,
                            },
                        },
                    }],
                },
            }],
            descriptors: vec![],
            riders: vec![],
        }],
    };
    let mut species = crate::test_creatures::species();
    species.modifiers.extend([
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::Health,
                value: 100,
            },
        },
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ray.id.clone(),
            },
        },
    ]);
    customize(&mut document, &mut ray, &mut species);
    document.abilities.push(ray);
    let id = species.id.clone();
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content.clone(),
        id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, content);
    for x in [1, 3] {
        let actor = game
            .spawn_actor(at(x), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: format!("ray subject {x}"),
                faction: format!("side{x}"),
            },
            build.clone(),
        )
        .unwrap();
        game.actors.get_mut(&actor).unwrap().ready_at = 1000;
    }
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 0;
    game
}
fn plan(game: &Game) -> Plan {
    game.ability_plan_with_supplies(
        ActorId(1),
        &AbilityId::new("fixture_ray").unwrap(),
        AbilityInvocation::DirectionalRay {
            direction: StepDirection::East,
        },
        Default::default(),
    )
    .unwrap()
}
fn buff(game: &mut Game, actor: ActorId, id: &str, source: ActorId) {
    game.apply_condition(actor, &ConditionId::new(id).unwrap(), source, 500)
        .unwrap();
}
fn outcomes(game: &Game) -> Vec<&Outcome> {
    game.combat_events()
        .iter()
        .filter_map(|e| match e {
            crate::combat::CombatEvent::Resolved { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

#[test]
fn directional_plan_is_strict_targetless_and_freezes_source_only() {
    let mut game = fixture();
    let captured = plan(&game);
    let ray = captured.as_ray().unwrap();
    let value = serde_json::to_value(ray).unwrap();
    assert!(value.get("target").is_none());
    assert_eq!(
        serde_json::from_value::<RayPlan>(value.clone()).unwrap(),
        *ray
    );
    for field in ["target", "path", "interceptor"] {
        let mut forged = value.clone();
        forged[field] = serde_json::json!(null);
        assert!(serde_json::from_value::<RayPlan>(forged).is_err());
    }
    for field in [
        "origin",
        "direction",
        "source_bonus",
        "incoming",
        "cost",
        "cooldown",
    ] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<RayPlan>(missing).is_err(),
            "{field}"
        );
    }
    buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
    assert_ne!(plan(&game).as_ray().unwrap().source_bonus, ray.source_bonus);
    assert_eq!(serde_json::to_value(ray).unwrap(), value);
}

#[test]
fn current_interceptor_checks_live_agility_against_the_frozen_launch() {
    let game = fixture();
    let frozen = plan(&game);
    let mut baseline = game.clone();
    let mut changed_source = game.clone();
    buff(&mut baseline, ActorId(2), "ray_agile", ActorId(2));
    buff(&mut changed_source, ActorId(2), "ray_agile", ActorId(2));
    buff(&mut changed_source, ActorId(1), "ray_strong", ActorId(1));
    baseline
        .resolve_ray(frozen.as_ray().unwrap(), None)
        .unwrap();
    changed_source
        .resolve_ray(frozen.as_ray().unwrap(), None)
        .unwrap();
    assert_eq!(outcomes(&baseline), outcomes(&changed_source));
    assert_eq!(baseline.rules_rng, changed_source.rules_rng);
    assert!(outcomes(&baseline).iter().all(|o| o.check.unwrap().success));
    let mut intercepted = game.clone();
    let old = intercepted.actors[&ActorId(2)].clone();
    intercepted.actors.get_mut(&ActorId(2)).unwrap().location = Location {
        position: Position { x: 3, y: 2, z: 0 },
        ..at(3)
    };
    let third = intercepted
        .spawn_actor(at(2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let build = old.combat.as_ref().unwrap().creature().build().clone();
    intercepted
        .configure_creature(
            third,
            crate::CreatureIdentity {
                name: "interceptor".into(),
                faction: "other".into(),
            },
            build,
        )
        .unwrap();
    buff(&mut intercepted, third, "ray_agile", third);
    intercepted
        .resolve_ray(frozen.as_ray().unwrap(), None)
        .unwrap();
    assert!(outcomes(&intercepted).iter().any(|o| o.target == third));
    assert!(outcomes(&intercepted)
        .iter()
        .all(|o| o.target != ActorId(2)));
}

#[test]
fn encounters_are_contiguous_passes_and_source_can_be_hit_after_exit() {
    let mut e = super::ray::RayEncounters::default();
    let source = ActorId(1);
    let other = ActorId(2);
    assert!(e.enter([source].into(), source, true).is_empty());
    assert_eq!(e.enter([other].into(), source, false), vec![(other, 0)]);
    e.record(other).unwrap();
    assert!(e.enter([other].into(), source, false).is_empty());
    assert!(e.enter(BTreeSet::new(), source, false).is_empty());
    assert_eq!(e.enter([other].into(), source, false), vec![(other, 1)]);
    e.record(other).unwrap();
    assert_eq!(e.enter([source].into(), source, true), vec![(source, 0)]);
    let mut fresh = super::ray::RayEncounters::default();
    assert_eq!(fresh.enter([other].into(), source, false), vec![(other, 0)]);
}

#[test]
fn repeat_disadvantage_cancels_advantage_before_the_existing_six_dice_cap() {
    let game = fixture_with(|_, _, species| {
        species.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::CheckEdge,
                value: 5,
            },
        });
    });
    let ray = plan(&game);
    let ray = ray.as_ray().unwrap();
    let recipient = game.creature(ActorId(2)).unwrap();
    let base = recipient
        .derived()
        .ray_recipient_edge(
            &game.content.rules,
            ActorId(2),
            ActorId(1),
            &ray.ability,
            &ray.incoming,
            true,
            &[],
        )
        .unwrap();
    assert_eq!(base, 5, "ordinary sources retain their uncapped magnitude");
    for previous in [0, 1, 2, 3, 4, 5, 6, 8, 15] {
        let signed = base - i64::from(previous);
        let edge = crate::dice::Edge::from_counts(signed.max(0) as u32, (-signed).max(0) as u32);
        let mut rng = 42;
        let roll = crate::dice::roll_check(&mut rng, edge);
        assert_eq!(
            roll.rolled().len(),
            3 + signed.unsigned_abs().min(3) as usize
        );
        assert!(roll.rolled().len() <= 6);
    }
}

#[test]
fn targetless_launch_ignores_both_signs_of_opponent_qualified_source_edges() {
    for condition in ["ray_fear", "ray_focus"] {
        let mut game = fixture();
        let before = plan(&game);
        buff(&mut game, ActorId(1), condition, ActorId(2));
        let after = plan(&game);
        assert_eq!(
            before.as_ray().unwrap().source_edge,
            after.as_ray().unwrap().source_edge
        );
        let mut a = 11;
        let mut b = 11;
        assert_eq!(
            super::resolve::resolve_ray_launch(before.as_ray().unwrap(), &mut a, &mut ()),
            super::resolve::resolve_ray_launch(after.as_ray().unwrap(), &mut b, &mut ())
        );
        assert_eq!(a, b);
    }
}

#[test]
fn incoming_source_predicates_are_frozen_but_bound_to_the_real_recipient() {
    for value in [-2_i64, 2] {
        let mut game = fixture_with(|document, _, _| {
            document
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "ray_incoming")
                .unwrap()
                .modifiers[0]
                .op = ModifierOp::Add {
                stat: Stat::IncomingEdge { defense: None },
                value,
            };
        });
        buff(&mut game, ActorId(1), "ray_incoming", ActorId(2));
        let frozen = plan(&game);
        let ray = frozen.as_ray().unwrap();
        let recipient = game.creature(ActorId(2)).unwrap();
        let selected = recipient
            .derived()
            .ray_recipient_edge(
                &game.content.rules,
                ActorId(2),
                ActorId(1),
                &ray.ability,
                &ray.incoming,
                true,
                &[],
            )
            .unwrap();
        let other = recipient
            .derived()
            .ray_recipient_edge(
                &game.content.rules,
                ActorId(3),
                ActorId(1),
                &ray.ability,
                &ray.incoming,
                true,
                &[],
            )
            .unwrap();
        assert_eq!(
            selected,
            other + value.signum(),
            "one condition family contributes one signed edge"
        );
    }
}

#[test]
fn bounded_no_progress_reversals_consume_only_one_source_roll() {
    let mut game = fixture();
    let captured = plan(&game);
    let ray = captured.as_ray().unwrap();
    game.world.set_wall(at(0), true).unwrap();
    game.world.set_wall(at(2), true).unwrap();
    let mut expected = game.rules_rng;
    super::resolve::resolve_ray_launch(ray, &mut expected, &mut ());
    game.resolve_ray(ray, None).unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(outcomes(&game).is_empty());
}

#[test]
fn ray_preparation_pause_restore_resume_preserves_frozen_operands_and_replay() {
    let mut game = fixture();
    let captured = plan(&game);
    game.start_work(captured.clone(), None).unwrap();
    game.tick = 4;
    game.pause_preparation(ActorId(1)).unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 6);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for current in [&mut game, &mut restored] {
        current.start_work(captured.clone(), None).unwrap();
        current.tick = 10;
        current.resolve_preparations().unwrap();
        assert!(current.preparation(ActorId(1)).is_none());
        assert_eq!(current.actors[&ActorId(1)].ready_at, 30);
    }
    assert_eq!(game, restored);
}

#[test]
fn ray_release_failure_rolls_back_rng_effects_and_local_encounter_state() {
    let mut game = fixture();
    let captured = plan(&game);
    let before = game.clone();
    let result = game.with_recovery_transition(true, |candidate| {
        candidate.resolve_ray(captured.as_ray().unwrap(), None)?;
        Err::<(), _>(GameError::IdentityExhausted)
    });
    assert!(result.is_err());
    assert_eq!(game, before);
    let mut expected = before.clone();
    game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
    expected
        .resolve_ray(captured.as_ray().unwrap(), None)
        .unwrap();
    assert_eq!(game, expected);
}

#[test]
fn paid_ray_scheduler_failure_preserves_holds_supplies_tokens_cooldown_queue_and_replay() {
    use crate::resources::Resource;
    let mut game = fixture_with(|document, ray, _| {
        for resource in &mut document.resources {
            resource.capacity.base = 100;
        }
        ray.cost = Some(Cost {
            resource: Resource::Stamina,
            start: 1,
            resolution: 2,
        });
        ray.cooldown = NonZeroU64::new(100);
        ray.supply_costs = vec![SupplyCost {
            supply: crate::content_ids::SupplyId::new("bandage").unwrap(),
            quantity: 1,
            access: SupplyAccess::CarriedOnly,
            spend_at: SupplySpendAt::FirstResolution,
        }];
    });
    let ability = AbilityId::new("fixture_ray").unwrap();
    // Keep an ordinary decision before the release. Otherwise action completion
    // advances past the ray while the fixture's other actor waits until tick 1000.
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
    buff(&mut game, ActorId(1), "ray_token", ActorId(1));
    let mut supply = crate::ItemSpec::ordinary("ray bandage".into());
    supply.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
    supply.stackable = true;
    let item = game
        .place_item_stack(10, at(1), Some(ActorId(1)), 2, supply)
        .unwrap();
    let intention = game
        .admit_intention(
            ActorId(1),
            crate::Action::UseAbility {
                id: ability.clone(),
                invocation: AbilityInvocation::DirectionalRay {
                    direction: StepDirection::East,
                },
                supplies: Default::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    execution.outcome.unwrap();
    assert_eq!(game.tick, 0);
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    assert!(outcomes(&game).is_empty());
    let pending = game.preparation(ActorId(1)).unwrap();
    assert_eq!(pending.intention, Some(intention));
    assert!(pending.active);
    assert_eq!(pending.remaining, 10);
    assert_eq!(pending.hold, Some(intention));
    assert!(matches!(
        pending.item_payment,
        crate::supplies::ItemPayment::Held { .. }
    ));
    assert_eq!(game.items[&item].quantity, 2);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .cooldowns()
            .remaining(&ability),
        100
    );
    assert!(game.token_reservations_valid(ActorId(1), false));
    // A different actor's queued action must neither interleave nor disappear.
    let queued = game
        .admit_intention(
            ActorId(2),
            crate::Action::Wait,
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    buff(&mut game, ActorId(2), "ray_token", ActorId(2));
    let before = game.clone();
    let mut expected_creature = game.creature(ActorId(1)).unwrap().into_owned();
    expected_creature.advance_active(10);
    expected_creature.finish_cost(intention).unwrap();
    let expected_stamina = expected_creature
        .costs()
        .resources()
        .balance(Resource::Stamina);
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let checkpoint = serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    let result = game.with_recovery_transition(true, |candidate| {
        candidate.tick = 10;
        candidate.resolve_preparations()?;
        assert!(candidate.preparation(ActorId(1)).is_none());
        assert_eq!(candidate.items[&item].quantity, 1);
        assert!(candidate
            .creature(ActorId(1))
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        assert_eq!(outcomes(candidate).len(), 1);
        assert!(!outcomes(candidate)[0].check.unwrap().success);
        Err::<(), _>(GameError::IdentityExhausted)
    });
    assert_eq!(result, Err(GameError::IdentityExhausted));
    assert_eq!(
        game, before,
        "the complete paid release and queue roll back together"
    );
    for current in [&mut game, &mut restored] {
        current
            .with_recovery_transition(true, |candidate| {
                candidate.tick = 10;
                candidate.resolve_preparations()
            })
            .unwrap();
        assert!(current.preparation(ActorId(1)).is_none());
        assert_eq!(current.actors[&ActorId(1)].ready_at, 30);
        assert_eq!(current.pending_intention(ActorId(2)).unwrap().id, queued);
        assert_eq!(current.items[&item].quantity, 1);
        let source = current.creature(ActorId(1)).unwrap();
        assert_eq!(
            source.costs().resources().balance(Resource::Stamina),
            expected_stamina
        );
        assert!(source.costs().reservations().is_empty());
        assert_eq!(source.cooldowns().remaining(&ability), 90);
        for actor in [ActorId(1), ActorId(2)] {
            assert!(current.token_reservations_valid(actor, false));
            assert!(current
                .creature(actor)
                .unwrap()
                .conditions()
                .instances()
                .all(|entry| entry.condition.as_str() != "ray_token"));
        }
    }
    assert_eq!(
        game, restored,
        "a restored held release consumes the same rolls and payments"
    );
}

#[test]
fn reflected_ray_uses_borrowed_operands_inside_one_scheduler_pass() {
    let mut game = fixture();
    for actor in [ActorId(1), ActorId(2)] {
        buff(&mut game, actor, "ray_agile", actor);
    }
    game.world.set_wall(at(0), true).unwrap();
    game.world.set_wall(at(4), true).unwrap();
    game.start_work(plan(&game), None).unwrap();
    let before = crate::diagnostics::work_counts();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.ability_plans, before.ability_plans,
        "no source recapture per encounter"
    );
    assert_eq!(
        after.game_clones, before.game_clones,
        "release uses the existing outer transaction"
    );
    assert_eq!(
        after.actor_table_copies, before.actor_table_copies,
        "no nested actor table transaction"
    );
    assert_eq!(
        after.preparation_candidates - before.preparation_candidates,
        2
    );
    assert_eq!(after.strike_resolutions - before.strike_resolutions, 3);
    assert_eq!(after.rider_resolutions, before.rider_resolutions);
    assert_eq!(
        outcomes(&game)
            .iter()
            .map(|outcome| outcome.target)
            .collect::<Vec<_>>(),
        vec![ActorId(2), ActorId(1), ActorId(2)]
    );
    assert_eq!(game.actors[&ActorId(1)].ready_at, 30);
}

#[test]
fn ray_scheduler_observer_is_equivalent_and_reuses_the_actual_launch_receipt() {
    let mut plain = fixture();
    for actor in [ActorId(1), ActorId(2)] {
        buff(&mut plain, actor, "ray_agile", actor);
    }
    plain.world.set_wall(at(0), true).unwrap();
    plain.world.set_wall(at(4), true).unwrap();
    plain.start_work(plan(&plain), None).unwrap();
    let mut observed = plain.clone();
    observed.set_combat_diagnostics(true);
    for current in [&mut plain, &mut observed] {
        current.tick = 10;
        current.resolve_preparations().unwrap();
    }
    let diagnostics = observed.combat_diagnostics().unwrap();
    assert!(diagnostics.active.is_none());
    let records = diagnostics.records();
    assert_eq!(
        records
            .iter()
            .map(|record| record.target)
            .collect::<Vec<_>>(),
        vec![ActorId(2), ActorId(1), ActorId(2)]
    );
    let launch = &records[0].trace.steps()[0];
    assert_eq!(launch.id, "ray_launch_check");
    for record in records {
        assert_eq!(record.actor, ActorId(1));
        assert_eq!(record.outcome.target, record.target);
        assert_eq!(&record.trace.steps()[0], launch);
        assert_eq!(
            record
                .trace
                .steps()
                .iter()
                .filter(|step| step.id == "ray_launch_check")
                .count(),
            1
        );
        assert_eq!(
            record
                .trace
                .steps()
                .iter()
                .filter(|step| step.id == "check")
                .count(),
            1
        );
        assert!(!record.trace.truncated());
    }
    observed.set_combat_diagnostics(false);
    assert_eq!(
        observed, plain,
        "retaining diagnostics changes neither RNG nor authority"
    );
}

#[test]
fn a_ray_with_no_encounter_does_not_fabricate_a_diagnostic_target() {
    let mut game = fixture();
    game.world.set_wall(at(0), true).unwrap();
    game.world.set_wall(at(2), true).unwrap();
    game.start_work(plan(&game), None).unwrap();
    game.set_combat_diagnostics(true);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let diagnostics = game.combat_diagnostics().unwrap();
    assert!(diagnostics.records().is_empty());
    assert!(diagnostics.active.is_none());
}

#[test]
fn scoped_ray_incoming_modifiers_accept_both_signs_and_reject_defense_specific_scopes() {
    for value in [-2, 2] {
        let game = fixture_with(|document, _, _| {
            let condition = document
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "ray_incoming")
                .unwrap();
            condition.modifiers[0].op = ModifierOp::Add {
                stat: Stat::IncomingEdge { defense: None },
                value,
            };
        });
        assert!(game
            .content
            .rules
            .condition(&ConditionId::new("ray_incoming").unwrap())
            .is_some());
        for defense in [
            DefenseKind::Physical,
            DefenseKind::Cognitive,
            DefenseKind::Spiritual,
        ] {
            let mut document = game.content.rules.document().clone();
            let condition = document
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "ray_incoming")
                .unwrap();
            condition.modifiers[0].op = ModifierOp::Add {
                stat: Stat::IncomingEdge {
                    defense: Some(defense),
                },
                value,
            };
            assert!(Ruleset::validate(document).is_err());
        }
    }
}

#[test]
fn six_clear_visits_include_source_return_and_reentry_without_multicell_duplicates() {
    for multicell in [false, true] {
        let mut game = fixture();
        if multicell {
            game.actors.get_mut(&ActorId(2)).unwrap().location = at(2);
            game.set_body(
                ActorId(2),
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [1, 0, 0]],
                    eye: [0, 0, 0],
                    mass: 100,
                },
            )
            .unwrap();
        }
        buff(&mut game, ActorId(1), "ray_agile", ActorId(1));
        buff(&mut game, ActorId(2), "ray_agile", ActorId(2));
        game.world.set_wall(at(0), true).unwrap();
        game.world.set_wall(at(4), true).unwrap();
        let frozen = plan(&game);
        let mut expected_rng = game.rules_rng;
        game.resolve_ray(frozen.as_ray().unwrap(), None).unwrap();
        let actual: Vec<_> = outcomes(&game).into_iter().map(|o| o.target).collect();
        assert_eq!(
            actual,
            vec![ActorId(2), ActorId(1), ActorId(2)],
            "multicell={multicell}"
        );
        assert!(outcomes(&game).iter().all(|o| o.check.unwrap().success));
        super::resolve::resolve_ray_launch(frozen.as_ray().unwrap(), &mut expected_rng, &mut ());
        crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::default());
        crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::default());
        crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::from_counts(0, 1));
        assert_eq!(
            game.rules_rng, expected_rng,
            "one source roll and three actual dodge checks"
        );
    }
}

#[test]
fn failed_dodge_takes_the_full_frozen_six_dice_then_ordinary_live_protection() {
    for protected in [false, true] {
        let mut game = fixture();
        buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
        let captured = plan(&game);
        let ray = captured.as_ray().unwrap();
        if protected {
            buff(&mut game, ActorId(2), "ray_ward", ActorId(2));
        }
        let mut expected = game.rules_rng;
        super::resolve::resolve_ray_launch(ray, &mut expected, &mut ());
        crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
        let rolled = crate::dice::DicePool::new(6, 6, 0)
            .unwrap()
            .roll(&mut expected, crate::dice::Edge::default())
            .total;
        game.resolve_ray(ray, None).unwrap();
        let actual = outcomes(&game);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].target, ActorId(2));
        assert!(!actual[0].check.unwrap().success);
        assert_eq!(
            game.health(ActorId(2)).unwrap().0,
            100 - u64::from(rolled.saturating_sub(if protected { 7 } else { 0 }))
        );
        assert_eq!(game.rules_rng, expected);
    }
}

#[test]
fn failed_dodge_against_fire_component_immunity_still_checks_and_intercepts() {
    let mut game = fixture_with(|_, ray, species| {
        let StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        let Effect::Damage {
            damage: DamageExpr::Components { components },
        } = &mut strikes[0].on_failure[0]
        else {
            unreachable!()
        };
        components[0].descriptor = Some(crate::content_ids::DescriptorId::new("fire").unwrap());
        species.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: crate::content_ids::DescriptorId::new("fire").unwrap(),
                },
            },
        });
    });
    buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
    game.world.set_wall(at(4), true).unwrap();
    let captured = plan(&game);
    game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
    let actual = outcomes(&game);
    assert_eq!(
        actual.len(),
        1,
        "failed dodge intercepts before a wall return"
    );
    assert_eq!(actual[0].target, ActorId(2));
    assert!(
        !actual[0].check.unwrap().success,
        "component immunity never skips Agility"
    );
    assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
}

#[test]
fn a_door_changed_after_preparation_changes_live_ray_geometry() {
    let mut game = fixture();
    buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
    game.world.place_door(at(2), 33, true, 1).unwrap();
    let captured = plan(&game);
    let mut closed = game.clone();
    closed.world.set_door(at(2), false);
    game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
    closed
        .resolve_ray(captured.as_ray().unwrap(), None)
        .unwrap();
    assert!(outcomes(&game).iter().any(|o| o.target == ActorId(2)));
    assert!(outcomes(&closed).iter().all(|o| o.target != ActorId(2)));
}

#[test]
fn source_tokens_are_held_at_preparation_and_live_recipient_tokens_are_spent_once() {
    let mut game = fixture();
    for actor in [ActorId(1), ActorId(2)] {
        buff(&mut game, actor, "ray_agile", actor);
    }
    buff(&mut game, ActorId(1), "ray_token", ActorId(1));
    buff(&mut game, ActorId(1), "ray_incoming_token", ActorId(1));
    game.world.set_wall(at(0), true).unwrap();
    game.world.set_wall(at(4), true).unwrap();
    let captured = plan(&game);
    assert_eq!(captured.tokens().len(), 2);
    game.start_work(captured.clone(), None).unwrap();
    assert!(game.token_reservations_valid(ActorId(1), false));
    buff(&mut game, ActorId(2), "ray_token", ActorId(2));
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
    let mut expected = game.rules_rng;
    super::resolve::resolve_ray_launch(captured.as_ray().unwrap(), &mut expected, &mut ());
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(0, 1));
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert_eq!(game.rules_rng, expected);
    for actor in [ActorId(1), ActorId(2)] {
        assert!(game
            .creature(actor)
            .unwrap()
            .conditions()
            .instances()
            .all(|entry| !matches!(entry.condition.as_str(), "ray_token" | "ray_incoming_token")));
        assert!(game.token_reservations_valid(actor, false));
    }
}

#[test]
fn public_ray_aim_contains_only_frozen_disclosed_direction() {
    let mut game = fixture();
    let captured = plan(&game);
    game.start_work(captured, None).unwrap();
    let observed = game
        .capture_public_plan_observation(ActorId(2), ActorId(1))
        .unwrap();
    assert!(matches!(
        observed.state,
        crate::public_plan::PublicPlanState::Preparing {
            aim: crate::public_plan::ObservedAim::Direction {
                direction: Position { x: 1, y: 0, z: 0 }
            },
            ..
        }
    ));
    let mut hidden = game.clone();
    let at = Location {
        region: RegionId(2),
        position: Position { x: 4, y: 1, z: 0 },
    };
    hidden.world.set_wall(at, true).unwrap();
    assert_eq!(
        hidden.capture_public_plan_observation(ActorId(2), ActorId(1)),
        Some(observed)
    );
}

fn empty_ray_world(game: &mut Game, regions: u64) {
    *game.world = tor_world::World::new(
        (1..=regions)
            .map(|id| tor_world::Region {
                id: RegionId(id),
                name: format!("ray-frame-{id}"),
                bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
}
fn ray_cell(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

#[test]
fn source_origin_and_body_frame_remain_frozen_after_source_relocation() {
    for orientation in 0..24 {
        let mut game = fixture();
        empty_ray_world(&mut game, 1);
        let origin = ray_cell(1, 4, 4, 4);
        let direction = tor_world::Direction::East.rotated(orientation);
        let (x, y, z) = direction.delta();
        game.actors.get_mut(&ActorId(1)).unwrap().location = origin;
        game.actors.get_mut(&ActorId(1)).unwrap().orientation = orientation;
        game.actors.get_mut(&ActorId(2)).unwrap().location =
            ray_cell(1, 4 + 2 * x, 4 + 2 * y, 4 + 2 * z);
        buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
        let captured = plan(&game);
        game.actors.get_mut(&ActorId(1)).unwrap().location = ray_cell(1, 1, 1, 1);
        game.actors.get_mut(&ActorId(1)).unwrap().orientation = 23 - orientation;
        game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
        assert_eq!(
            outcomes(&game)
                .iter()
                .map(|outcome| outcome.target)
                .collect::<Vec<_>>(),
            vec![ActorId(2)],
            "frozen orientation={orientation}"
        );
    }
}

#[test]
fn every_physical_portal_rotation_changes_the_remaining_ray_direction() {
    for rotation in 0..24 {
        let mut game = fixture();
        empty_ray_world(&mut game, 2);
        game.actors.get_mut(&ActorId(1)).unwrap().location = ray_cell(1, 7, 4, 4);
        let to = ray_cell(2, 4, 4, 4);
        game.world
            .connect_portal_area(
                tor_world::Passage {
                    from: ray_cell(1, 8, 4, 4),
                    direction: tor_world::Direction::East,
                    to,
                },
                rotation,
                1,
                1,
            )
            .unwrap();
        let (x, y, z) = tor_world::Direction::East.rotated(rotation).delta();
        game.actors.get_mut(&ActorId(2)).unwrap().location = ray_cell(2, 4 + x, 4 + y, 4 + z);
        buff(&mut game, ActorId(1), "ray_strong", ActorId(1));
        let captured = plan(&game);
        game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
        assert_eq!(
            outcomes(&game)
                .iter()
                .map(|outcome| outcome.target)
                .collect::<Vec<_>>(),
            vec![ActorId(2)],
            "crossing rotation={rotation}"
        );
    }
}

#[test]
fn unavailable_or_ambiguous_geometry_stops_without_reversing_or_checking() {
    for ambiguous in [false, true] {
        let mut game = fixture();
        empty_ray_world(&mut game, 3);
        let origin = ray_cell(1, 8, 0, 4);
        game.actors.get_mut(&ActorId(1)).unwrap().location = origin;
        game.actors.get_mut(&ActorId(2)).unwrap().location = ray_cell(1, 7, 0, 4);
        let direction = if ambiguous {
            for (direction, region) in [
                (tor_world::Direction::North, 2),
                (tor_world::Direction::East, 3),
            ] {
                game.world
                    .connect(
                        tor_world::Passage {
                            from: origin,
                            direction,
                            to: ray_cell(region, 4, 4, 4),
                        },
                        0,
                    )
                    .unwrap();
            }
            StepDirection::NorthEast
        } else {
            StepDirection::East
        };
        let captured = game
            .ability_plan_with_supplies(
                ActorId(1),
                &AbilityId::new("fixture_ray").unwrap(),
                AbilityInvocation::DirectionalRay { direction },
                Default::default(),
            )
            .unwrap();
        let mut expected = game.rules_rng;
        super::resolve::resolve_ray_launch(captured.as_ray().unwrap(), &mut expected, &mut ());
        game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
        assert!(outcomes(&game).is_empty());
        assert_eq!(game.rules_rng, expected, "ambiguous={ambiguous}");
    }
}

#[test]
fn opponent_qualified_tokens_of_both_signs_are_not_launch_operands_or_claims() {
    for value in [-1_i64, 1] {
        let mut game = fixture_with(|document, ray, _| {
            ray.tags = vec![crate::content_ids::AbilityTagId::new("weapon_attack").unwrap()];
            document
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "ray_token")
                .unwrap()
                .modifiers[0]
                .op = ModifierOp::Add {
                stat: Stat::CheckEdge,
                value,
            };
        });
        let before = plan(&game);
        let mut holder = game.actor_outcome(ActorId(1)).unwrap();
        holder
            .state
            .combat
            .as_mut()
            .unwrap()
            .with_recovery_context(None, |creature| {
                creature.apply_qualified_condition(
                    &ConditionId::new("ray_token").unwrap(),
                    ActorId(2),
                    500,
                    Some(crate::conditions::TokenQualification {
                        opponent: ActorId(2),
                        ability_tag: crate::content_ids::AbilityTagId::new("weapon_attack")
                            .unwrap(),
                    }),
                )
            })
            .unwrap();
        game.apply_outcome(holder).unwrap();
        // This token really does apply to an ordinary aimed check with matching
        // ability membership, but a targetless launch must neither use nor hold it.
        let creature = game.creature(ActorId(1)).unwrap();
        assert_eq!(
            creature
                .derived()
                .check_edge(
                    &game.content.rules,
                    ActorId(2),
                    &before.as_ray().unwrap().ability
                )
                .unwrap(),
            value
        );
        let after = plan(&game);
        assert_eq!(
            before.as_ray().unwrap().source_edge,
            after.as_ray().unwrap().source_edge
        );
        assert!(after.tokens().next().is_none());
        let mut a = 77;
        let mut b = 77;
        assert_eq!(
            super::resolve::resolve_ray_launch(before.as_ray().unwrap(), &mut a, &mut ()),
            super::resolve::resolve_ray_launch(after.as_ray().unwrap(), &mut b, &mut ())
        );
        assert_eq!(a, b);
    }
}
