use std::num::NonZeroU64;
use tor_simulation::grants::Ability;
use tor_simulation::resources::Resource;
use tor_simulation::{Action, ActorId, CreatureIdentity, Game, IntentionOrigin};
use tor_world::{Location, Position, RegionId};

fn game() -> (Game, ActorId, ActorId) {
    game_with_seed(42)
}

fn game_with_seed(seed: u64) -> (Game, ActorId, ActorId) {
    let (sample, sample_actor) = super::abilities::game();
    let build = sample.creature(sample_actor).unwrap().build().clone();
    let mut game = Game::two_room_in_stone(seed);
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "subject".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        target,
        CreatureIdentity {
            name: "target".into(),
            faction: "neutral".into(),
        },
        game.creature(actor).unwrap().build().clone(),
    )
    .unwrap();
    game.refresh_navigation();
    (game, actor, target)
}

#[test]
fn own_stats_disclose_current_build_and_reserved_resources_only_to_the_owner() {
    let (mut game, actor, target) = game();
    let before = game.clone();
    let view = game.observe(actor).unwrap();
    let stats = view.combat.unwrap().own_stats.unwrap();
    let creature = game.creature(actor).unwrap();
    assert_eq!(stats.attributes, creature.derived().attributes);
    assert_eq!(stats.skills, creature.derived().skills);
    assert_eq!(stats.defenses, creature.derived().defenses);
    assert_eq!(stats.binding, creature.build().binding());
    assert_eq!(stats.active_talents, creature.derived().active_talents);
    assert_eq!(stats.dormant_talents, creature.derived().dormant_talents);
    assert_eq!(stats.abilities, creature.derived().abilities);
    assert_eq!(
        stats.hit_dice.len(),
        usize::from(creature.build().ledger().total_hd())
    );
    assert_eq!(game, before, "inspection must not advance or mutate state");

    let intention = game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::Fear,
                target,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(intention)
    );
    let stats = game
        .observe(actor)
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    let focus = stats
        .resources
        .iter()
        .find(|r| r.resource == Resource::Focus)
        .unwrap();
    assert_eq!(focus.reserved, 1);
    assert_eq!(focus.available + focus.reserved, focus.balance);
    let target_stats = game
        .observe(target)
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert!(target_stats.resources.iter().all(|r| r.reserved == 0));
    assert_ne!(stats.resources, target_stats.resources);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
    assert_eq!(
        restored.observe(actor).unwrap().combat.unwrap().own_stats,
        Some(stats)
    );
}

#[test]
fn own_stats_rederive_after_transformation_without_disclosing_another_build() {
    let (mut game, actor, target) = game();
    let mut original = game
        .observe(actor)
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    let target_before = game.observe(target).unwrap().combat.unwrap().own_stats;
    let original_build = game.creature(actor).unwrap().build().clone();
    let mut transformed = original_build.clone();
    transformed
        .set_templates(vec![tor_simulation::creatures::Template::zombified(0)])
        .unwrap();
    game.rebuild_creature(actor, transformed).unwrap();
    let changed = game
        .observe(actor)
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert_eq!(
        changed.kind,
        tor_simulation::progression::CreatureType::Undead
    );
    assert_ne!(changed.attributes, original.attributes);
    assert_eq!(changed.hit_dice, original.hit_dice);
    assert_eq!(changed.binding, original.binding);
    assert_eq!(
        game.observe(target).unwrap().combat.unwrap().own_stats,
        target_before
    );
    game.rebuild_creature(actor, original_build).unwrap();
    // Returning to a larger capacity must not refill a pool reduced by the
    // transformation. Build values revert; actual balances retain the loss.
    for resource in &mut original.resources {
        let changed = changed
            .resources
            .iter()
            .find(|r| r.resource == resource.resource)
            .unwrap();
        resource.balance = changed.balance.min(resource.maximum);
        resource.available = resource.balance;
    }
    assert_eq!(
        game.observe(actor).unwrap().combat.unwrap().own_stats,
        Some(original)
    );
}

#[test]
fn paid_abilities_queue_free_then_pay_split_cost_through_preparation_and_restore() {
    for (ability, resource) in [
        (Ability::PowerStrike, Resource::Stamina),
        (Ability::MagicBolt, Resource::Mana),
        (Ability::Fear, Resource::Focus),
    ] {
        let (mut game, actor, target) = game();
        let balance = game
            .creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(resource);
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility { ability, target },
                IntentionOrigin::Human,
            )
            .unwrap();
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .costs()
                .resources()
                .balance(resource),
            balance
        );
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(
            game.preparation(actor).unwrap().origin_intention(),
            Some(intention)
        );
        let costs = game.creature(actor).unwrap().costs();
        assert_eq!(costs.resources().balance(resource), balance - 1);
        assert_eq!(costs.available(resource), balance - 2);
        assert_eq!(costs.reservation(intention).unwrap().resolution, 1);
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
        assert_eq!(restored, game);
        for state in [&mut game, &mut restored] {
            state.act(target, Action::Wait).unwrap();
            let costs = state.creature(actor).unwrap().costs();
            assert_eq!(costs.resources().balance(resource), balance - 2);
            assert!(costs.reservations().is_empty());
            assert!(state.preparation(actor).is_none());
        }
        assert_eq!(restored, game);
    }
}

#[test]
fn paused_paid_work_resumes_under_a_new_receipt_without_repayment_then_cancels() {
    let (mut game, actor, target) = game();
    let original = game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.pause_preparation(actor).unwrap();
    let before = game.creature(actor).unwrap().costs().clone();
    let resumed = game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    assert_ne!(original, resumed);
    assert_eq!(game.creature(actor).unwrap().costs(), &before);
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.creature(actor).unwrap().costs(), &before);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(original)
    );
    game.cancel_intention(actor, resumed).unwrap();
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Mana),
        before.resources().balance(Resource::Mana)
    );
    assert!(game
        .creature(actor)
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
}

#[test]
fn paid_actions_reject_unowned_direct_execution_without_mutation() {
    let (mut game, actor, target) = game();
    let before = game.clone();
    assert_eq!(
        game.act(
            actor,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target
            }
        ),
        Err(tor_simulation::GameError::InvalidIntention)
    );
    assert_eq!(game, before);
    assert!(game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::BasicMelee,
                target
            },
            IntentionOrigin::Human
        )
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn queue_execution_rechecks_grants_and_paid_preparation_cancels_on_grant_loss() {
    for started in [false, true] {
        let (mut game, actor, target) = game();
        let balance = game
            .creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Stamina);
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    ability: Ability::PowerStrike,
                    target,
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        if started {
            game.execute_next_intention().unwrap().outcome.unwrap();
        }
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        let outcome = game.rebuild_creature(actor, build).unwrap();
        if started {
            assert_eq!(outcome.canceled, vec![intention]);
            assert!(game.preparation(actor).is_none());
        } else {
            assert!(game.execute_next_intention().unwrap().outcome.is_err());
        }
        let costs = game.creature(actor).unwrap().costs();
        assert_eq!(
            costs.resources().balance(Resource::Stamina),
            balance - u32::from(started)
        );
        assert!(costs.reservations().is_empty());
    }
}

#[test]
fn affordability_is_rechecked_at_start_and_queueing_remains_free() {
    let (mut game, actor, target) = game();
    for _ in 0..3 {
        let id = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    ability: Ability::MagicBolt,
                    target,
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.cancel_intention(actor, id).unwrap();
    }
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Mana),
        1
    );
    let id = game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    let before = game.creature(actor).unwrap().costs().clone();
    let executed = game.execute_next_intention().unwrap();
    assert_eq!(executed.intention.id, id);
    assert!(executed.outcome.is_err());
    assert_eq!(game.creature(actor).unwrap().costs(), &before);
    assert!(game.preparation(actor).is_none());
}

#[test]
fn power_strike_applies_the_shared_melee_bonus_and_bolt_matches_the_seeded_kernel() {
    let (game, actor, target) = game();
    let mut basic = game.clone();
    let mut power = game.clone();
    basic.act(actor, Action::Attack { target }).unwrap();
    power
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::PowerStrike,
                target,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    power.execute_next_intention().unwrap().outcome.unwrap();
    basic.act(target, Action::Wait).unwrap();
    power.act(target, Action::Wait).unwrap();
    assert_eq!(
        basic.health(target).unwrap().0,
        power.health(target).unwrap().0 + 3
    );

    let mut bolt = game.clone();
    let expected = tor_simulation::abilities::resolve_magic_bolt(
        game.creature(actor).unwrap(),
        game.effective_combat(target).unwrap().defense,
        &game.creature(target).unwrap().derived().protection,
        &mut 42,
        tor_simulation::dice::Edge::default(),
    )
    .unwrap();
    let damage = expected.damage.map_or(0, |damage| damage.total);
    bolt.admit_intention(
        actor,
        Action::UseAbility {
            ability: Ability::MagicBolt,
            target,
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    bolt.execute_next_intention().unwrap().outcome.unwrap();
    bolt.act(target, Action::Wait).unwrap();
    assert_eq!(
        bolt.health(target).unwrap().0,
        game.health(target).unwrap().0 - damage
    );
}

#[test]
fn replacing_paid_work_can_use_the_hold_released_by_that_replacement() {
    let (mut game, actor, target) = game();
    let action = Action::UseAbility {
        ability: Ability::MagicBolt,
        target,
    };
    let first = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.cancel_intention(actor, first).unwrap();
    let original = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.pause_preparation(actor).unwrap();
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .available(Resource::Mana),
        1
    );
    let other = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 3, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        other,
        CreatureIdentity {
            name: "other".into(),
            faction: "neutral".into(),
        },
        game.creature(actor).unwrap().build().clone(),
    )
    .unwrap();
    let replacement = game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target: other,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let costs = game.creature(actor).unwrap().costs();
    assert!(costs.reservation(original).is_none());
    assert!(costs.reservation(replacement).is_some());
    assert_eq!(costs.resources().balance(Resource::Mana), 1);
    assert_eq!(costs.available(Resource::Mana), 0);
}

#[test]
fn paid_preparation_records_reject_orphan_mismatched_and_unissued_holds() {
    let (mut game, actor, target) = game();
    game.admit_intention(
        actor,
        Action::UseAbility {
            ability: Ability::MagicBolt,
            target,
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let value = serde_json::to_value((snapshot, shared)).unwrap();
    let key = actor.0.to_string();
    for kind in 0..6 {
        let mut forged = value.clone();
        let state = &mut forged[1]["actors"][0][&key];
        match kind {
            0 => state["pending"] = serde_json::Value::Null,
            1 => state["combat"]["creature"]["holds"] = serde_json::json!([]),
            2 => {
                state["pending"]["charge"]["resource"] = serde_json::json!("focus");
                state["combat"]["creature"]["holds"][0][1] = serde_json::json!(1);
            }
            3 => {
                state["pending"]["origin_intention"] = serde_json::json!(100);
                state["combat"]["creature"]["holds"][0][0] = serde_json::json!(100);
            }
            4 => state["pending"]["duration"] = serde_json::json!(101),
            5 => state["pending"]["intention"] = serde_json::Value::Null,
            _ => unreachable!(),
        }
        let (snapshot, shared) = serde_json::from_value::<(
            tor_simulation::checkpoint::Snapshot,
            tor_simulation::checkpoint::SharedState,
        )>(forged)
        .unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared).is_none(),
            "forged case {kind}"
        );
    }
    let mut missing = serde_json::to_value(game.preparation(actor).unwrap()).unwrap();
    missing.as_object_mut().unwrap().remove("charge");
    assert!(serde_json::from_value::<tor_simulation::combat::Preparation>(missing).is_err());
}

#[test]
fn fear_actions_apply_resist_or_ignore_the_effect_and_always_pay_resolution() {
    use tor_simulation::abilities::{resolve_fear, FearResolution};
    use tor_simulation::creatures::Template;
    use tor_simulation::grants::{Descriptor, Grant, Selector};
    let mut applied = 0;
    let mut resisted = 0;
    for seed in 0..16 {
        for immune in [false, true] {
            let (mut game, actor, target) = game_with_seed(seed);
            if immune {
                let mut build = game.creature(target).unwrap().build().clone();
                let mut templates = build.templates().to_vec();
                let mut ward = Template::new("fearward", 10);
                ward.grants = vec![Grant::Immunity(Selector::Descriptor(Descriptor::Fear))];
                templates.push(ward);
                build.set_templates(templates).unwrap();
                game.rebuild_creature(target, build).unwrap();
            }
            let mut rng = seed;
            let expected = resolve_fear(
                game.creature(actor).unwrap(),
                game.creature(target).unwrap(),
                &game.creature(target).unwrap().derived().protection,
                &mut rng,
                tor_simulation::dice::Edge::default(),
            )
            .unwrap();
            let duration = match expected {
                FearResolution::Applied { duration, .. } => {
                    applied += 1;
                    Some(duration)
                }
                FearResolution::Resisted(_) => {
                    resisted += 1;
                    None
                }
                FearResolution::Immune => {
                    assert!(immune);
                    None
                }
            };
            game.admit_intention(
                actor,
                Action::UseAbility {
                    ability: Ability::Fear,
                    target,
                },
                IntentionOrigin::Human,
            )
            .unwrap();
            game.execute_next_intention().unwrap().outcome.unwrap();
            game.act(target, Action::Wait).unwrap();
            assert_eq!(
                game.creature(target).unwrap().fear().remaining(actor),
                duration
            );
            let costs = game.creature(actor).unwrap().costs();
            assert_eq!(costs.resources().balance(Resource::Focus), 2);
            assert!(costs.reservations().is_empty());
            assert!(game.combat_events().iter().any(|event| matches!(event,
                tor_simulation::combat::CombatEvent::AbilityResolved { ability: Ability::Fear, applied, .. } if *applied == duration.is_some())));
        }
    }
    assert!(applied > 0 && resisted > 0);
}

#[test]
fn ability_disclosure_hides_unseen_participants_and_unobserved_other_combat() {
    use tor_simulation::combat::{AbilityOutcome, DisclosedCombatEvent};
    let (mut game, actor, target) = game();
    game.admit_intention(
        actor,
        Action::UseAbility {
            ability: Ability::MagicBolt,
            target,
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.act(target, Action::Wait).unwrap();
    assert!(game
        .observe(target)
        .unwrap()
        .combat
        .unwrap()
        .events
        .contains(&DisclosedCombatEvent::Ability {
            caster: Some(actor),
            target: Some(target),
            ability: Ability::MagicBolt,
            outcome: AbilityOutcome::Applied,
        }));
    let at = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    };
    game.teleport(actor, at(4)).unwrap();
    game.set_wall(at(3), true).unwrap();
    let observed = game.observe(target).unwrap();
    assert!(observed.visible_actors.iter().all(|seen| seen.id != actor));
    assert!(observed
        .combat
        .unwrap()
        .events
        .contains(&DisclosedCombatEvent::Ability {
            caster: None,
            target: Some(target),
            ability: Ability::MagicBolt,
            outcome: AbilityOutcome::Applied,
        }));
    let other = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        other,
        CreatureIdentity {
            name: "other".into(),
            faction: "neutral".into(),
        },
        game.creature(actor).unwrap().build().clone(),
    )
    .unwrap();
    assert!(game
        .observe(other)
        .unwrap()
        .combat
        .unwrap()
        .events
        .iter()
        .all(|event| !matches!(event, DisclosedCombatEvent::Ability { .. })));
}

#[test]
fn combat_diagnostic_capture_records_real_resolution_costs_and_effects_without_changing_state() {
    use tor_simulation::resolution_diagnostics::ResolutionRecord;
    for ability in [
        Ability::BasicMelee,
        Ability::PowerStrike,
        Ability::MagicBolt,
        Ability::Fear,
    ] {
        let (mut ordinary, actor, target) = game();
        let mut observed = ordinary.clone();
        observed.set_combat_diagnostics(true);
        let action = if ability == Ability::BasicMelee {
            Action::Attack { target }
        } else {
            Action::UseAbility { ability, target }
        };
        let mut owner = None;
        for state in [&mut ordinary, &mut observed] {
            let id = state
                .admit_intention(actor, action, IntentionOrigin::Human)
                .unwrap();
            owner = Some(id);
            state.execute_next_intention().unwrap().outcome.unwrap();
        }
        assert!(
            observed.combat_diagnostics().unwrap().records().is_empty(),
            "preparation is not resolution"
        );
        let preparation = observed.preparation(actor).unwrap().clone();
        for state in [&mut ordinary, &mut observed] {
            state.act(target, Action::Wait).unwrap();
        }
        let diagnostics = observed.combat_diagnostics().unwrap();
        assert_eq!(diagnostics.records().len(), 1);
        assert_eq!(diagnostics.dropped(), 0);
        let record = diagnostics.records().front().unwrap();
        assert_eq!((record.actor, record.target), (actor, target));
        assert_eq!(record.work, preparation.work);
        assert_eq!(record.intention, owner);
        assert_eq!(record.origin_intention, owner);
        assert_eq!(record.preparation, preparation.duration);
        assert_eq!(record.recovery, preparation.recovery);
        assert_eq!(record.charge.is_some(), ability != Ability::BasicMelee);
        assert!(record.tick <= observed.tick());
        assert_eq!(
            record.target_after.health,
            observed.health(target).unwrap().0
        );
        assert_eq!(
            record.target_before.health - record.target_after.health,
            record.damage
        );
        assert_eq!(record.target_before.injury, Some(0));
        assert_eq!(record.target_after.injury, Some(record.damage));
        assert!(!record.trace.truncated());
        if ability == Ability::Fear {
            assert!(
                matches!(record.trace.steps().last(), Some(ResolutionRecord::FearFinished { applied, .. }) if *applied == record.applied)
            );
            assert_eq!(
                record.target_after.fear.contains_key(&actor),
                record.applied
            );
        } else {
            assert!(record
                .trace
                .steps()
                .iter()
                .any(|step| matches!(step, ResolutionRecord::Check(_))));
        }
        if let Some(charge) = record.charge {
            let before = &record.actor_before.resources[charge.resource as usize];
            let after = &record.actor_after.resources[charge.resource as usize];
            assert_eq!(before.balance - before.available, charge.resolution);
            assert_eq!(before.balance - after.balance, charge.resolution);
            assert_eq!(after.balance, after.available);
        }
        let view = observed.observe(actor).unwrap();
        assert_eq!(
            view,
            ordinary.observe(actor).unwrap(),
            "ordinary observations do not disclose traces"
        );
        observed.set_combat_diagnostics(false);
        assert!(observed.combat_diagnostics().is_none());
        assert_eq!(
            observed, ordinary,
            "capture consumes neither game time nor random state"
        );
    }
}

#[test]
fn transient_combat_capture_replays_from_checkpoint_but_is_not_persisted() {
    let (mut game, actor, target) = game();
    game.set_combat_diagnostics(true);
    game.admit_intention(
        actor,
        Action::UseAbility {
            ability: Ability::MagicBolt,
            target,
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
    assert!(restored.combat_diagnostics().is_none());
    restored.set_combat_diagnostics(true);
    game.act(target, Action::Wait).unwrap();
    restored.act(target, Action::Wait).unwrap();
    assert_eq!(game.combat_diagnostics(), restored.combat_diagnostics());
    assert!(!game.combat_diagnostics().unwrap().records().is_empty());
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
    assert!(restored.combat_diagnostics().is_none());
    game.set_combat_diagnostics(false);
    assert_eq!(game, restored);
}

#[test]
fn combat_capture_preserves_resumed_receipt_and_original_payment_owner() {
    let (mut game, actor, target) = game();
    game.set_combat_diagnostics(true);
    let action = Action::UseAbility {
        ability: Ability::MagicBolt,
        target,
    };
    let original = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.pause_preparation(actor).unwrap();
    let resumed = game
        .admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_ne!(original, resumed);
    assert!(game.combat_diagnostics().unwrap().records().is_empty());
    let preparation = game.preparation(actor).unwrap().clone();
    game.act(target, Action::Wait).unwrap();
    let record = game
        .combat_diagnostics()
        .unwrap()
        .records()
        .front()
        .unwrap();
    assert_eq!(record.intention, Some(resumed));
    assert_eq!(record.origin_intention, Some(original));
    assert_eq!(record.started, preparation.started);
    assert_eq!(record.remaining, preparation.remaining);
    assert_eq!(record.tick, record.started + record.remaining);
    let before = &record.actor_before.resources[Resource::Mana as usize];
    let after = &record.actor_after.resources[Resource::Mana as usize];
    assert_eq!(before.balance - after.balance, 1);
    game.set_combat_diagnostics(true);
    assert_eq!(
        game.combat_diagnostics().unwrap().records().len(),
        1,
        "enabling again does not erase evidence"
    );
    game.set_combat_diagnostics(false);
    game.set_combat_diagnostics(true);
    assert!(game.combat_diagnostics().unwrap().records().is_empty());
}

#[test]
fn combat_capture_does_not_fabricate_resolution_for_cancelled_work() {
    let (mut game, actor, target) = game();
    game.set_combat_diagnostics(true);
    let intention = game
        .admit_intention(
            actor,
            Action::UseAbility {
                ability: Ability::MagicBolt,
                target,
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.cancel_intention(actor, intention).unwrap();
    for _ in 0..2 {
        let next = game.next_actor().unwrap();
        game.act(next, Action::Wait).unwrap();
    }
    assert!(game.combat_diagnostics().unwrap().records().is_empty());
}

#[test]
fn combat_capture_forks_keep_their_own_retained_windows() {
    let (mut game, actor, target) = game();
    game.set_combat_diagnostics(true);
    game.act(actor, Action::Attack { target }).unwrap();
    game.act(target, Action::Wait).unwrap();
    let original = game.clone();
    let mut fork = game.clone();
    fork.set_combat_diagnostics(false);
    fork.set_combat_diagnostics(true);
    assert!(fork.combat_diagnostics().unwrap().records().is_empty());
    assert_eq!(game.combat_diagnostics().unwrap().records().len(), 1);
    while fork.next_actor() != Some(actor) {
        fork.act(fork.next_actor().unwrap(), Action::Wait).unwrap();
    }
    fork.act(actor, Action::Attack { target }).unwrap();
    while fork.preparation(actor).is_some() {
        fork.act(fork.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(fork.combat_diagnostics().unwrap().records().len(), 1);
    assert_eq!(
        game, original,
        "a fork cannot alter the original capture window"
    );
    assert_ne!(game.combat_diagnostics(), fork.combat_diagnostics());
}
