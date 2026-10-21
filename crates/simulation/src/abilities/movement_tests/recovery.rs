//! Translation and the first contextual source share one post-placement rate.
use super::*;
fn recovery_translation_game() -> crate::Game {
    let mut rules = crate::test_creatures::ruleset().document().clone();
    let mut quiet = rules
        .conditions
        .iter()
        .find(|c| c.id.as_str() == "braced")
        .unwrap()
        .clone();
    quiet.id = crate::content_ids::ConditionId::new("quiet_movement_probe").unwrap();
    quiet.family = crate::content_ids::ConditionFamilyId::new("quiet_movement_probe").unwrap();
    quiet.modifiers = vec![crate::rules::Modifier {
        scope: crate::rules::Scope::WhileNoSensedHostile,
        op: crate::rules::ModifierOp::Add {
            stat: crate::rules::Stat::HealthRecoveryPeriodPercent,
            value: -25,
        },
    }];
    rules.conditions.push(quiet);
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(rules).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let mut game = crate::Game::two_room_in_stone(42, content.clone());
    for (x, faction) in [(1, "hero"), (4, "foe")] {
        let id = game
            .spawn_actor(cell(x, 1, 0), std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        let build = crate::creatures::CreatureBuild::new(
            content.clone(),
            species_id.clone(),
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                crate::attributes::Skill::Deception,
                crate::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        game.configure_creature(
            id,
            crate::CreatureIdentity {
                name: faction.into(),
                faction: faction.into(),
            },
            build,
        )
        .unwrap();
    }
    game.configure_run(
        crate::ActorId(1),
        [crate::ActorId(1)].into(),
        None,
        [
            ("hero".into(), ["foe".into()].into()),
            ("foe".into(), ["hero".into()].into()),
        ]
        .into(),
    )
    .unwrap();
    game.set_region_light(tor_world::RegionId(1), false)
        .unwrap();
    crate::test_creatures::injure(&mut game, crate::ActorId(1), 3).unwrap();
    game.tick = 401;
    // Preserve the lazy recovery interval, but keep the manual fixture's next
    // decision at its current tick, as an ordinary scheduler boundary would.
    for id in [crate::ActorId(1), crate::ActorId(2)] {
        game.actors.get_mut(&id).unwrap().ready_at = game.tick;
    }
    assert_eq!(game.actors[&game.next_actor().unwrap()].ready_at, game.tick);
    assert_eq!(game.actors[&crate::ActorId(1)].active_since, 0);
    game
}
fn translation_condition(game: &crate::Game, condition: &str) -> super::super::Outcome {
    use super::super::*;
    Outcome {
        resistance: None,
        actor: crate::ActorId(1),
        target: crate::ActorId(1),
        ability: None,
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        participation: AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![
            ResolvedEffect::Translate {
                mode: MovementMode::Voluntary,
                result: MovementResult::Moved {
                    from: Placement::capture(game, crate::ActorId(1)).unwrap(),
                    to: Placement {
                        location: cell(3, 1, 0),
                        orientation: 0,
                    },
                },
            },
            ResolvedEffect::ApplyCondition {
                id: crate::content_ids::ConditionId::new(condition).unwrap(),
                duration: 500,
            },
        ],
    }
}
#[test]
fn translation_first_contextual_condition_uses_resulting_local_awareness_without_double_rephase() {
    let mut game = recovery_translation_game();
    assert!(!game.senses_hostile(crate::ActorId(1)).unwrap());
    let outcome = translation_condition(&game, "quiet_movement_probe");
    game.apply(outcome).unwrap();
    assert!(game.senses_hostile(crate::ActorId(1)).unwrap());
    let creature = game.creature(crate::ActorId(1)).unwrap();
    assert_eq!(
        (
            creature.health().injury(),
            creature.health_recovery().elapsed(),
            creature.health_recovery().period()
        ),
        (3, 401, 1200)
    );
    assert!(game.recovery_contexts_valid());
    let mut shared = crate::checkpoint::SharedState::default();
    assert_eq!(
        crate::Game::restore_checkpoint(
            game.checkpoint(&mut shared),
            &shared,
            game.content().clone()
        )
        .unwrap(),
        game
    );
}
#[test]
fn translation_contextual_condition_failure_rolls_back_placement_and_old_recovery_fraction() {
    let mut game = recovery_translation_game();
    let before = game.clone();
    let outcome = translation_condition(&game, "missing_condition");
    assert!(game.apply(outcome).is_err());
    assert_eq!(game, before);
}

#[test]
fn first_contextual_condition_samples_only_its_resulting_placement() {
    let mut game = recovery_translation_game();
    // Drop derived caches without touching authoritative state, so neither
    // placement can inherit an earlier perceived-scene result.
    game.lifecycle.scenes = Default::default();
    let outcome = translation_condition(&game, "quiet_movement_probe");
    let before = crate::diagnostics::work_counts();
    game.apply_resolved_outcome(
        outcome,
        crate::lifecycle::ResolutionPublication::Standalone,
        &mut crate::lifecycle::DamageActivityGuard::default(),
    )
    .unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.scenes - before.scenes,
        1,
        "the context policy predicate must not sample the old placement"
    );
    let creature = game.creature(crate::ActorId(1)).unwrap();
    assert_eq!(
        (
            creature.health_recovery().elapsed(),
            creature.health_recovery().period()
        ),
        (401, 1200)
    );
}
