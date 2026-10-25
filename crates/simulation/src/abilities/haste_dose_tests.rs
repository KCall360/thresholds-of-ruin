//! Real admitted potion work plus native completion/clock controls. Hosted
//! wand delivery, source-free Service and process acceptance have separate owners.
use super::*;
use crate::content_ids::{ConditionFamilyId, ConditionId};
use crate::rules::*;
use crate::{Action, ActorId, Game, ItemId, Work};
const OWNER: ActorId = ActorId(1);
const PEER: ActorId = ActorId(2);
fn haste() -> ConditionId {
    ConditionId::new("fixture_haste").unwrap()
}
fn rest() -> ConditionId {
    ConditionId::new("fixture_haste_rest").unwrap()
}
fn slow() -> ConditionId {
    ConditionId::new("fixture_haste_slow").unwrap()
}
fn definition(id: ConditionId) -> ConditionDef {
    ConditionDef {
        family: ConditionFamilyId::new(id.as_str()).unwrap(),
        name: id.as_str().into(),
        id,
        lifecycle: ConditionLifecycle::Timed,
        modifiers: vec![],
        stacking: Stacking::Keep,
        source_scoped: false,
        descriptors: vec![],
        capacity: Eviction::Reject,
        timing: ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: ConditionPreparation::Preserve,
    }
}
fn dose() -> Effect {
    Effect::HasteDose {
        condition: haste(),
        rest: rest(),
    }
}
fn fixture(effects: Vec<Effect>) -> (Game, ItemId) {
    let mut game = super::ray_tests::fixture_with(|document, _, _| {
        let mut h = definition(haste());
        h.timing = ConditionTiming::ActorActiveOrUntilRemoved;
        // Exercise all applications of a shared condition, not only this dose's causer.
        h.source_scoped = true;
        h.modifiers = vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::ActionDuration {
                value: crate::action_duration::ActionDuration::Haste,
            },
        }];
        let mut r = definition(rest());
        r.restrictions = vec![ConditionRestriction::VoluntaryActions];
        r.preparation = ConditionPreparation::Pause;
        let mut s = definition(slow());
        s.modifiers = vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::ActionDuration {
                value: crate::action_duration::ActionDuration::Slow,
            },
        }];
        document.conditions.extend([h, r, s]);
    });
    game.actors.get_mut(&PEER).unwrap().ready_at = 10_000;
    let mut spec = crate::ItemSpec::ordinary("unmarked dose".into());
    spec.identity = "fixture.haste_dose".into();
    spec.concealed = true;
    spec.class = crate::ItemClass::Potion;
    spec.stackable = true;
    spec.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::OnUse,
        effects,
    });
    let at = game.actors[&OWNER].location;
    let item = game.place_item_stack(70, at, Some(OWNER), 3, spec).unwrap();
    assert_eq!(game.actors[&OWNER].turn_ticks.get(), 100);
    assert!(game
        .content
        .rules
        .condition(&haste())
        .unwrap()
        .timing
        .permits_finite());
    assert!(game
        .content
        .rules
        .condition(&haste())
        .unwrap()
        .timing
        .permits_persistent());
    (game, item)
}
fn start(game: &mut Game, item: ItemId) -> (crate::IntentionId, u64) {
    let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
    assert!(matches!(
        &plan.on_success().last().unwrap(),
        PlannedEffect::HasteDose {
            turn_ticks: 100,
            ..
        }
    ));
    let due = game.tick() + plan.preparation();
    game.actors.get_mut(&PEER).unwrap().ready_at = game.tick();
    let id = game
        .admit_intention(OWNER, Action::Drink { item }, crate::IntentionOrigin::Human)
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert_eq!(game.preparation(OWNER).unwrap().plan, plan);
    assert_eq!(game.items[&item].quantity, 3);
    (id, due)
}
fn finish(game: &mut Game, due: u64) {
    // Existing native paid-item completion seam: peer keeps the boundary open;
    // resolve_preparations performs ordinary actor-local timer settlement.
    game.with_recovery_transition(true, |candidate| {
        candidate.tick = due;
        candidate.resolve_preparations()
    })
    .unwrap();
}
fn published(game: &Game) -> HasteDoseOutcome {
    game.combat_events()
        .iter()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => {
                outcome.effects.iter().find_map(|effect| match effect {
                    ResolvedEffect::HasteDose { outcome, .. } => Some(*outcome),
                    _ => None,
                })
            }
            _ => None,
        })
        .expect("canonical dose receipt")
}
#[test]
fn paid_haste_dose_uses_one_base_turn_draw_and_finite_actor_clock() {
    for (seed, turns) in [(6, 4), (0, 7)] {
        let (mut game, item) = fixture(vec![dose()]);
        game.rules_rng = seed;
        let (_, due) = start(&mut game, item);
        assert_eq!(due, 100);
        let mut expected_rng = seed;
        assert_eq!(
            crate::dice::DicePool::new(1, 4, 3)
                .unwrap()
                .roll(&mut expected_rng, crate::dice::Edge::default())
                .total,
            turns
        );
        assert_eq!(game.rules_rng, seed);
        finish(&mut game, due);
        assert_eq!(game.items[&item].quantity, 2);
        assert!(game.actors[&OWNER].knowledge.contains("fixture.haste_dose"));
        assert_eq!(game.rules_rng, expected_rng);
        assert_eq!(
            published(&game),
            HasteDoseOutcome::Hasted {
                duration: u64::from(turns) * 100
            }
        );
        assert_eq!(
            game.creature(OWNER)
                .unwrap()
                .conditions()
                .remaining(&haste(), OWNER),
            Some(u64::from(turns) * 100)
        );
        assert_eq!(
            game.creature(OWNER).unwrap().derived().action_duration,
            crate::action_duration::ActionDuration::Haste
        );
        game.with_recovery_transition(true, |candidate| {
            candidate.tick = due + u64::from(turns) * 100 - 1;
            candidate.settle_actor_time(OWNER)
        })
        .unwrap();
        assert!(game
            .creature(OWNER)
            .unwrap()
            .conditions()
            .has_active(&haste()));
        game.with_recovery_transition(true, |candidate| {
            candidate.tick += 1;
            candidate.settle_actor_time(OWNER)
        })
        .unwrap();
        assert!(!game
            .creature(OWNER)
            .unwrap()
            .conditions()
            .has_active(&haste()));
    }
}
#[test]
fn paid_haste_overdose_clears_all_named_sources_even_slow_masked_and_omits_zero_rest() {
    for (seed, turns) in [(6, 0), (0, 7)] {
        let (mut game, item) = fixture(vec![dose()]);
        game.apply_effects(
            OWNER,
            &[Effect::ApplyPersistentCondition {
                condition: haste(),
                cancels: None,
            }],
        )
        .unwrap();
        game.apply_condition(OWNER, &haste(), PEER, 2000).unwrap();
        game.apply_condition(OWNER, &slow(), OWNER, 2000).unwrap();
        assert_eq!(
            game.creature(OWNER).unwrap().derived().action_duration,
            crate::action_duration::ActionDuration::Normal
        );
        game.rules_rng = seed;
        let (_, due) = start(&mut game, item);
        let mut expected_rng = seed;
        assert_eq!(
            crate::dice::DicePool::new(1, 8, -1)
                .unwrap()
                .roll(&mut expected_rng, crate::dice::Edge::default())
                .total,
            turns
        );
        finish(&mut game, due);
        assert_eq!(game.items[&item].quantity, 2);
        assert_eq!(game.rules_rng, expected_rng);
        assert_eq!(
            published(&game),
            HasteDoseOutcome::Overdose {
                rest: u64::from(turns) * 100
            }
        );
        assert!(!game
            .creature(OWNER)
            .unwrap()
            .conditions()
            .has_active(&haste()));
        assert!(game
            .creature(OWNER)
            .unwrap()
            .conditions()
            .has_active(&slow()));
        assert_eq!(
            game.creature(OWNER)
                .unwrap()
                .conditions()
                .remaining(&rest(), OWNER),
            if turns == 0 {
                None
            } else {
                Some(u64::from(turns) * 100)
            }
        );
        assert_eq!(game.condition_allows_voluntary_input(OWNER), turns == 0);
        if turns > 0 {
            game.with_recovery_transition(true, |candidate| {
                candidate.tick = due + u64::from(turns) * 100;
                candidate.settle_actor_time(OWNER)
            })
            .unwrap();
            assert!(game.condition_allows_voluntary_input(OWNER));
            assert!(!game
                .creature(OWNER)
                .unwrap()
                .conditions()
                .has_active(&rest()));
        }
    }
}
#[test]
fn haste_dose_branch_uses_completion_state_and_ordered_self_effects() {
    let (mut game, item) = fixture(vec![dose()]);
    game.apply_condition(OWNER, &haste(), OWNER, 1).unwrap();
    game.rules_rng = 6;
    let (_, due) = start(&mut game, item);
    assert_eq!(due, 50);
    finish(&mut game, due);
    assert_eq!(published(&game), HasteDoseOutcome::Hasted { duration: 400 });
    for (prefix, expected) in [
        (
            Effect::ApplyPersistentCondition {
                condition: haste(),
                cancels: None,
            },
            HasteDoseOutcome::Overdose { rest: 0 },
        ),
        (
            Effect::RemoveCondition { condition: haste() },
            HasteDoseOutcome::Hasted { duration: 400 },
        ),
    ] {
        let (mut game, item) = fixture(vec![prefix, dose()]);
        if matches!(expected, HasteDoseOutcome::Hasted { .. }) {
            game.apply_effects(
                OWNER,
                &[Effect::ApplyPersistentCondition {
                    condition: haste(),
                    cancels: None,
                }],
            )
            .unwrap();
        }
        game.rules_rng = 6;
        let (_, due) = start(&mut game, item);
        finish(&mut game, due);
        assert_eq!(published(&game), expected);
        assert_eq!(game.items[&item].quantity, 2);
    }
}

#[test]
fn haste_mixed_lifetime_keep_preserves_deadline_and_remove_reapply_gets_fresh_generation() {
    let (mut game, _) = fixture(vec![dose()]);
    game.apply_condition(OWNER, &haste(), OWNER, 400).unwrap();
    let generation = game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .instances()
        .find(|row| row.condition == haste())
        .unwrap()
        .generation;
    game.with_recovery_transition(true, |candidate| {
        candidate.tick = 100;
        candidate.settle_actor_time(OWNER)
    })
    .unwrap();
    game.apply_effects(
        OWNER,
        &[Effect::ApplyPersistentCondition {
            condition: haste(),
            cancels: None,
        }],
    )
    .unwrap();
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .remaining(&haste(), OWNER),
        Some(300)
    );
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .instances()
            .find(|row| row.condition == haste())
            .unwrap()
            .generation,
        generation
    );
    game.apply_effects(OWNER, &[Effect::RemoveCondition { condition: haste() }])
        .unwrap();
    game.apply_condition(OWNER, &haste(), OWNER, 400).unwrap();
    let creature = game.creature(OWNER).unwrap();
    let entry = creature
        .conditions()
        .instances()
        .find(|row| row.condition == haste())
        .unwrap();
    assert!(entry.generation > generation);
    assert_eq!(entry.remaining, Some(400));
    drop(creature);
    game.apply_effects(
        OWNER,
        &[
            Effect::RemoveCondition { condition: haste() },
            Effect::ApplyPersistentCondition {
                condition: haste(),
                cancels: None,
            },
        ],
    )
    .unwrap();
    assert!(game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&haste()));
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .remaining(&haste(), OWNER),
        None
    );
}

#[test]
fn haste_explicit_mixed_timing_preserves_strict_old_lifetimes_and_current_record_shape() {
    let (game, _) = fixture(vec![dose()]);
    let mixed = game.content.rules.condition(&haste()).unwrap();
    for duration in [None, Some(400)] {
        let mut state = crate::conditions::ConditionState::default();
        match duration {
            None => {
                state.apply_persistent(mixed, OWNER, false).unwrap();
            }
            Some(ticks) => {
                state.apply(mixed, OWNER, ticks, false).unwrap();
            }
        }
        let entries = state.instances().cloned().collect::<Vec<_>>();
        assert_eq!(
            crate::conditions::ConditionState::from_recorded(
                entries.clone(),
                state.next_generation(),
                &game.content.rules
            )
            .unwrap(),
            state
        );
        let mut malformed = entries.clone();
        malformed[0].remaining = Some(0);
        assert!(crate::conditions::ConditionState::from_recorded(
            malformed,
            state.next_generation(),
            &game.content.rules
        )
        .is_err());
        let mut document = game.content.rules.document().clone();
        document
            .conditions
            .iter_mut()
            .find(|row| row.id == haste())
            .unwrap()
            .timing = if duration.is_some() {
            ConditionTiming::UntilRemoved
        } else {
            ConditionTiming::expiring()
        };
        let old = Ruleset::validate(document).unwrap();
        assert!(crate::conditions::ConditionState::from_recorded(
            entries,
            state.next_generation(),
            &old
        )
        .is_err());
    }
    for (text, expected) in [
        (r#"{"kind":"until_removed"}"#, ConditionTiming::UntilRemoved),
        (
            r#"{"kind":"actor_active","on_expiry":{"kind":"remove"}}"#,
            ConditionTiming::expiring(),
        ),
        (
            r#"{"kind":"actor_active_or_until_removed"}"#,
            ConditionTiming::ActorActiveOrUntilRemoved,
        ),
    ] {
        assert_eq!(
            serde_json::from_str::<ConditionTiming>(text).unwrap(),
            expected
        );
    }
    for text in [
        r#""actor_active_or_until_removed""#,
        r#"{"actor_active_or_until_removed":null}"#,
        r#"{"kind":"actor_active_or_until_removed","on_expiry":{"kind":"remove"}}"#,
        r#"{"kind":"actor_active_or_until_removed","extra":null}"#,
        "null",
    ] {
        assert!(
            serde_json::from_str::<ConditionTiming>(text).is_err(),
            "{text}"
        );
    }
}

#[test]
fn haste_paid_pending_checkpoint_replays_one_branch_and_exact_rng() {
    let (mut game, item) = fixture(vec![dose()]);
    game.rules_rng = 0;
    let (_, due) = start(&mut game, item);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let encoded = serde_json::to_vec(&(snapshot, &shared)).unwrap();
    let (snapshot, shared): (crate::checkpoint::Snapshot, crate::checkpoint::SharedState) =
        serde_json::from_slice(&encoded).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(game, restored);
    finish(&mut game, due);
    finish(&mut restored, due);
    assert_eq!(game, restored);
    assert_eq!(
        published(&restored),
        HasteDoseOutcome::Hasted { duration: 700 }
    );
    assert_eq!(restored.items[&item].quantity, 2);
}

#[test]
fn haste_late_rest_allocation_failure_rolls_back_paid_completion_rng_and_all_sources() {
    let (mut game, item) = fixture(vec![dose()]);
    game.apply_effects(
        OWNER,
        &[Effect::ApplyPersistentCondition {
            condition: haste(),
            cancels: None,
        }],
    )
    .unwrap();
    game.rules_rng = 0;
    let (_, due) = start(&mut game, item);
    // Valid exhaustion is an existing recorded condition-allocator boundary;
    // inject it after genuine admission, not as an invalid action prerequisite.
    let mut record = serde_json::to_value(crate::creatures::record::CreatureRecord::capture(
        &game.creature(OWNER).unwrap(),
    ))
    .unwrap();
    record["next_condition_generation"] = serde_json::json!(u64::MAX);
    let exhausted = serde_json::from_value::<crate::creatures::record::CreatureRecord>(record)
        .unwrap()
        .restore(game.content.clone(), vec![])
        .unwrap();
    let identity = game.actors[&OWNER].combat.as_ref().unwrap().identity();
    game.actors.get_mut(&OWNER).unwrap().combat = Some(
        crate::actor_creatures::CombatState::from_creature(identity, exhausted),
    );
    let before = game.clone();
    assert!(game
        .with_effect_publication(|candidate| {
            candidate.tick = due;
            candidate.resolve_preparations()
        })
        .is_err());
    assert_eq!(game, before);
    assert_eq!(game.items[&item].quantity, 3);
    assert!(game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&haste()));
}

#[test]
fn haste_finite_actor_clock_pauses_while_loaded_frozen_without_global_catchup() {
    let (mut game, _) = fixture(vec![dose()]);
    game.apply_condition(OWNER, &haste(), OWNER, 400).unwrap();
    let mut records = crate::MemoryRecords::default();
    let (_, report) = game
        .transition_regions(
            &crate::RegionTransition {
                active: [tor_world::RegionId(2)].into(),
                loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
            },
            &mut records,
        )
        .unwrap();
    assert!(report.frozen.contains(&tor_world::RegionId(1)));
    let clock = game.actor_clock(OWNER);
    game.advance_physics(100).unwrap();
    assert_eq!(game.actor_clock(OWNER), clock);
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .remaining(&haste(), OWNER),
        Some(400)
    );
    game.transition_regions(
        &crate::RegionTransition {
            active: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
            loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
        },
        &mut records,
    )
    .unwrap();
    let until = game.tick() + 400;
    game.with_recovery_transition(true, |candidate| {
        candidate.tick = until;
        candidate.settle_actor_time(OWNER)
    })
    .unwrap();
    assert!(!game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&haste()));
}

#[test]
fn haste_source_contract_is_checkless_self_only_with_explicit_condition_shapes() {
    let (game, _) = fixture(vec![dose()]);
    let mut valid = game.content.rules.document().clone();
    let ability = valid
        .abilities
        .iter_mut()
        .find(|row| row.id.as_str() == "fixture_ray")
        .unwrap();
    let original_check = match &ability.strikes {
        StrikeSource::Authored { strikes } => strikes[0].check.clone(),
        _ => unreachable!(),
    };
    ability.invocation = InvocationKind::ActorTarget;
    ability.reach = Reach::SelfOnly;
    ability.interaction = Interaction::NonHostile;
    ability.ai.hostile = false;
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    strikes[0].check = None;
    strikes[0].on_success = vec![dose()];
    strikes[0].on_failure.clear();
    strikes[0].riders.clear();
    Ruleset::validate(valid.clone()).unwrap();
    for case in 0..8 {
        let mut document = valid.clone();
        let ability = document
            .abilities
            .iter_mut()
            .find(|row| row.id.as_str() == "fixture_ray")
            .unwrap();
        match case {
            0 => ability.reach = Reach::Sight { range: 6 },
            1 => ability.interaction = Interaction::HostileToTarget,
            2 => {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                strikes[0].check = original_check.clone();
            }
            3 => {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                strikes[0].on_success.clear();
                strikes[0].riders.push(RiderDef {
                    cancellable: false,
                    trigger: RiderTrigger::Success,
                    check: None,
                    condition: None,
                    effects: vec![dose()],
                    descriptors: vec![],
                });
            }
            4 => {
                ability.strikes = StrikeSource::NaturalMelee {
                    additional_riders: vec![RiderDef {
                        cancellable: false,
                        trigger: RiderTrigger::Success,
                        check: None,
                        condition: None,
                        effects: vec![dose()],
                        descriptors: vec![],
                    }],
                }
            }
            5 => {
                document
                    .conditions
                    .iter_mut()
                    .find(|row| row.id == haste())
                    .unwrap()
                    .timing = ConditionTiming::UntilRemoved
            }
            6 => {
                document
                    .conditions
                    .iter_mut()
                    .find(|row| row.id == haste())
                    .unwrap()
                    .stacking = Stacking::Refresh
            }
            7 => document
                .conditions
                .iter_mut()
                .find(|row| row.id == rest())
                .unwrap()
                .restrictions
                .clear(),
            _ => unreachable!(),
        }
        assert!(Ruleset::validate(document).is_err(), "case {case}");
    }
    // Ordered self effects are not a second planner or a singleton restriction.
    let ability = valid
        .abilities
        .iter_mut()
        .find(|row| row.id.as_str() == "fixture_ray")
        .unwrap();
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    strikes[0]
        .on_success
        .insert(0, Effect::RemoveCondition { condition: haste() });
    Ruleset::validate(valid).unwrap();
    let natural = &game
        .content
        .catalog()
        .species
        .values()
        .next()
        .unwrap()
        .melee;
    let create = |strikes| {
        crate::attacks::NaturalAttack::new(
            natural.skill(),
            natural.bonus(),
            natural.wind_up(),
            natural.recovery(),
            strikes,
        )
    };
    create(natural.strikes().to_vec()).unwrap();
    let mut strikes = natural.strikes().to_vec();
    strikes[0].on_success.push(dose());
    assert!(create(strikes).is_err());
}

#[test]
fn haste_captured_duration_operands_and_condition_references_are_strict() {
    let (game, item) = fixture(vec![dose()]);
    let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
    assert!(plan.valid(&game.content.rules));
    let encoded = serde_json::to_value(plan.on_success().last().unwrap()).unwrap();
    let positive: PlannedEffect = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(&positive, plan.on_success().last().unwrap());
    for ticks in [0, 142_858, u64::MAX] {
        let mut invalid = encoded.clone();
        invalid["turn_ticks"] = serde_json::json!(ticks);
        assert!(serde_json::from_value::<PlannedEffect>(invalid).is_err());
    }
    let mut limit = encoded.clone();
    limit["turn_ticks"] = serde_json::json!(142_857);
    serde_json::from_value::<PlannedEffect>(limit).unwrap();
    for field in ["id", "rest", "turn_ticks"] {
        let mut missing = encoded.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<PlannedEffect>(missing).is_err());
    }
    let mut equal = encoded.clone();
    equal["rest"] = equal["id"].clone();
    assert!(serde_json::from_value::<PlannedEffect>(equal).is_err());
    let mut changed = plan.clone();
    let Plan::Drink { effects, .. } = &mut changed else {
        unreachable!()
    };
    let PlannedEffect::HasteDose { id, rest, .. } = &mut effects[0] else {
        unreachable!()
    };
    std::mem::swap(id, rest);
    assert!(!changed.valid(&game.content.rules));
}
