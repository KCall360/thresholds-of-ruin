//! Fresh reconstruction regressions. These tests are not recovered historical code.
#[test]
fn abilities_require_source_owned_interaction_semantics() {
    let rules = crate::test_creatures::ruleset();
    let ability = rules.document().abilities.first().unwrap();
    let mut value = serde_json::to_value(ability).unwrap();
    value.as_object_mut().unwrap().remove("interaction");
    assert!(
        serde_json::from_value::<crate::rules::AbilityDef>(value).is_err(),
        "ability interaction must be explicit, independent of AI selection hints"
    );
}

#[test]
fn autonomous_tactics_require_explicit_profile_opt_in() {
    assert!(
        serde_json::from_str::<crate::ai::AiProfile>(r#"{"memory_ticks":1000,"flee_percent":25}"#)
            .is_err(),
        "carrying loot must not implicitly opt an actor into care, gear or looting"
    );
}

use crate::{
    ai::AiProfile,
    behavior::{Behavior, BehaviorPolicy, Disposition, Engagement, IdleBehavior},
    content_ids::AbilityId,
    rules::{Interaction, Ruleset, StrikeSource},
    Action, ActorId, CreatureIdentity, Game,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(disposition: Disposition, memory_ticks: u64) -> Game {
    fixture_with(disposition, memory_ticks, |_, _| {})
}
fn fixture_with(
    disposition: Disposition,
    memory_ticks: u64,
    edit: impl FnOnce(&mut crate::rules::RulesetDocument, &mut crate::creatures::Species),
) -> Game {
    let mut species = crate::test_creatures::species();
    species.behavior = BehaviorPolicy {
        disposition,
        idle: IdleBehavior::Wait,
        memory_ticks,
        ..Default::default()
    };
    let mut document = crate::test_creatures::ruleset().document().clone();
    edit(&mut document, &mut species);
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut game = Game::two_room(42, content.clone());
    let build = crate::creatures::CreatureBuild::new(
        content,
        species_id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    for x in [1, 2] {
        let actor = game
            .spawn_actor(at(x), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("actor {x}"),
                faction: "neighbors".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::new(),
    )
    .unwrap();
    game.configure_ai(
        ActorId(2),
        AiProfile {
            idle: IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    game.refresh_navigation();
    game
}
fn attack(target: ActorId) -> Action {
    Action::UseAbility {
        id: AbilityId::new("basic_melee").unwrap(),
        invocation: crate::abilities::AbilityInvocation::actor_target(target),
        supplies: Default::default(),
    }
}
fn commit_strike(game: &mut Game, actor: ActorId, target: ActorId) -> crate::abilities::Outcome {
    let plan = game
        .ability_plan(actor, &AbilityId::new("basic_melee").unwrap(), target)
        .unwrap();
    game.resolve_strike(
        plan.as_ability().unwrap(),
        0,
        None,
        &mut crate::combat::PreparationProgress::Ability {
            next_strike: 1,
            successful_primaries: 0,
            attempted_riders: [0; 2],
            linked_hold: None,
        },
    )
    .unwrap()
}
fn restore(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint =
        serde_json::from_slice(&serde_json::to_vec(&game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap()
}

#[test]
fn passive_species_wait_until_a_visible_hostile_strike_commits() {
    let mut game = fixture(Disposition::Passive, 500);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("neighbors".into(), BTreeSet::from(["neighbors".into()]))]),
    )
    .unwrap();
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
    let before = game.actors[&ActorId(2)].behavior.clone();
    game.act(ActorId(1), attack(ActorId(2))).unwrap();
    assert_eq!(
        game.actors[&ActorId(2)].behavior,
        before,
        "unresolved preparation grants no provocation"
    );
    while game.preparation(ActorId(1)).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    let engagement = game.actors[&ActorId(2)].behavior.engagement.unwrap();
    assert_eq!(
        (engagement.target, engagement.location),
        (ActorId(1), at(1))
    );
    assert!(matches!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::UseAbility { .. }
    ));
}

#[test]
fn mean_wake_is_one_committed_eligible_decision_and_queries_draw_nothing() {
    let mut game = fixture(Disposition::Mean, 500);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("neighbors".into(), BTreeSet::from(["neighbors".into()]))]),
    )
    .unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().behavior.rng = 0;
    let original = game.clone();
    let first = game.choose_ai(ActorId(2)).unwrap();
    for _ in 0..4 {
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().behavior, first.behavior);
        game.observe(ActorId(2)).unwrap();
    }
    assert_eq!(game, original);
    game.act(ActorId(1), Action::Wait).unwrap();
    game.admit_ai_intention(ActorId(2)).unwrap();
    assert_eq!(
        game.actors[&ActorId(2)].behavior.rng,
        0,
        "admission chooses nothing"
    );
    let queued = restore(&game);
    let execution = game.execute_next_intention().unwrap();
    execution.outcome.unwrap();
    assert_eq!(game.actors[&ActorId(2)].behavior.rng, 0x9e3779b97f4a7c15);
    let mut replay = queued;
    replay.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game, replay);
}

#[test]
fn no_enemy_no_wake_and_multi_cell_self_never_acquires_itself() {
    let mut game = fixture(Disposition::Mean, 500);
    game.teleport(
        ActorId(1),
        Location {
            region: RegionId(2),
            position: Position { x: 4, y: 1, z: 0 },
        },
    )
    .unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("neighbors".into(), BTreeSet::from(["neighbors".into()]))]),
    )
    .unwrap();
    game.set_body(
        ActorId(2),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0]],
            eye: [0, 0, 0],
            mass: 100,
        },
    )
    .unwrap();
    assert!(game
        .observe(ActorId(2))
        .unwrap()
        .visible_actors
        .iter()
        .any(|actor| actor.id == ActorId(2)));
    let before = game.actors[&ActorId(2)].behavior.clone();
    let decision = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(decision.behavior, before);
    assert!(!game.hostile(ActorId(2), ActorId(2)));
}

#[test]
fn private_grudge_changes_only_the_requesters_relationships() {
    let mut calm = fixture(Disposition::Passive, 500);
    let mut grudging = calm.clone();
    grudging
        .actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = Some(Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(1),
        location: at(1),
        last_seen: 0,
    });
    assert!(grudging.hostile(ActorId(2), ActorId(1)));
    assert!(!grudging.hostile(ActorId(1), ActorId(2)));
    assert!(grudging.allied(ActorId(1), ActorId(2)));
    assert!(!grudging.allied(ActorId(2), ActorId(1)));
    assert_eq!(
        calm.observe(ActorId(1)).unwrap(),
        grudging.observe(ActorId(1)).unwrap(),
        "another actor's private grudge cannot change my menus or ally facts"
    );
    let support = AbilityId::new("bind_wounds").unwrap();
    assert_eq!(
        calm.ability_plan(ActorId(1), &support, ActorId(2)),
        grudging.ability_plan(ActorId(1), &support, ActorId(2))
    );
    calm.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("neighbors".into(), BTreeSet::from(["neighbors".into()]))]),
    )
    .unwrap();
    assert!(
        !calm.allied(ActorId(1), ActorId(2)),
        "authored reverse faction hostility still prevents alliance"
    );
}

#[test]
fn engagement_and_zero_rng_survive_controller_reconfiguration_and_checkpoint() {
    let mut game = fixture(Disposition::Passive, 500);
    commit_strike(&mut game, ActorId(1), ActorId(2));
    game.actors.get_mut(&ActorId(2)).unwrap().behavior.rng = 0;
    let before = game.actors[&ActorId(2)].behavior.clone();
    game.combat.ai.remove(&ActorId(2));
    let mut restored = restore(&game);
    restored
        .configure_ai(ActorId(2), AiProfile::general())
        .unwrap();
    assert_eq!(restored.actors[&ActorId(2)].behavior, before);
    assert!(restored.hostile(ActorId(2), ActorId(1)));
}

#[test]
fn behavior_records_require_named_fields_and_licensed_location() {
    let state = Behavior {
        rng: 0,
        engagement: Some(Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1),
            last_seen: 0,
        }),
    };
    let value = serde_json::to_value(&state).unwrap();
    assert_eq!(
        serde_json::from_value::<Behavior>(value.clone()).unwrap(),
        state
    );
    for field in ["rng", "engagement"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Behavior>(missing).is_err());
    }
    for field in ["target", "location", "last_seen"] {
        let mut missing = value.clone();
        missing["engagement"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Behavior>(missing).is_err());
    }
    assert!(serde_json::from_str::<Behavior>(r#"[null,0]"#).is_err());
    assert!(serde_json::from_str::<Behavior>(r#"{"rng":0,"rng":1,"engagement":null}"#).is_err());
    let mut positional = value;
    positional["engagement"]["location"] = serde_json::json!([1, [1, 1, 0]]);
    assert!(serde_json::from_value::<Behavior>(positional).is_err());
}

#[test]
fn non_hostile_rejects_direct_harm_in_primary_failure_rider_and_flag_condition() {
    for branch in ["on_success", "on_failure", "rider", "flag"] {
        let mut document = crate::test_creatures::ruleset().document().clone();
        let mut ability = document
            .abilities
            .iter()
            .find(|ability| ability.id.as_str() == "spark_lance_fire")
            .unwrap()
            .clone();
        ability.interaction = Interaction::NonHostile;
        ability.ai.hostile = false;
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            panic!("authored fixture")
        };
        let damage = strikes[0].on_success.clone();
        strikes[0].on_success.clear();
        strikes[0].on_failure.clear();
        match branch {
            "on_success" => strikes[0].on_success = damage,
            "on_failure" => strikes[0].on_failure = damage,
            "rider" => strikes[0].riders.push(crate::rules::RiderDef {
                condition: None,
                trigger: crate::rules::RiderTrigger::Success,
                check: None,
                effects: damage,
                descriptors: vec![],
            }),
            "flag" => {
                let mut condition = document
                    .conditions
                    .iter()
                    .find(|definition| definition.id.as_str() == "fear")
                    .unwrap()
                    .clone();
                document.flags.push(crate::rules::FlagDef {
                    id: crate::content_ids::FlagId::new("fixture_harm").unwrap(),
                    name: "Fixture harm".into(),
                    modifiers: condition.modifiers.clone(),
                });
                condition.modifiers = vec![crate::rules::Modifier {
                    scope: crate::rules::Scope::Always,
                    op: crate::rules::ModifierOp::Flag {
                        flag: crate::content_ids::FlagId::new("fixture_harm").unwrap(),
                    },
                }];
                *document
                    .conditions
                    .iter_mut()
                    .find(|definition| definition.id == condition.id)
                    .unwrap() = condition.clone();
                strikes[0]
                    .on_success
                    .push(crate::rules::Effect::ApplyCondition {
                        condition: crate::content_ids::ConditionId::new("fear").unwrap(),
                        duration: 100,
                    });
            }
            _ => unreachable!(),
        }
        *document
            .abilities
            .iter_mut()
            .find(|entry| entry.id == ability.id)
            .unwrap() = ability.clone();
        assert!(Ruleset::validate(document).is_err(), "{branch}");
    }
}

#[test]
fn a_committed_miss_and_zero_damage_contact_provoke_without_health_loss() {
    for bonus in [-1000, 1000] {
        let mut game = fixture_with(Disposition::Passive, 500, |_, species| {
            species.melee = crate::test_creatures::natural_melee(
                bonus,
                60,
                crate::combat::DamageType::Impact,
                0,
            );
        });
        let health = game.health(ActorId(2));
        let outcome = commit_strike(&mut game, ActorId(1), ActorId(2));
        assert_eq!(game.health(ActorId(2)), health);
        assert!(outcome.target_participated());
        assert_eq!(
            game.actors[&ActorId(2)].behavior.engagement.unwrap().target,
            ActorId(1)
        );
        if bonus < 0 {
            assert_eq!(outcome.result, crate::abilities::OutcomeStatus::Miss);
        }
    }
}

#[test]
fn non_hostile_target_contact_does_not_provoke_even_with_hostile_ai_hints() {
    let mut game = fixture_with(Disposition::Passive, 500, |document, _| {
        let ability = document
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        ability.interaction = Interaction::NonHostile;
        ability.strikes = StrikeSource::Authored {
            strikes: vec![crate::rules::StrikeDef {
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![crate::rules::Effect::Heal {
                    amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }],
        };
        assert!(ability.ai.hostile);
    });
    commit_strike(&mut game, ActorId(1), ActorId(2));
    assert!(game.actors[&ActorId(2)].behavior.engagement.is_none());
}

#[test]
fn an_attempted_target_rider_provokes_once_through_the_parent_strike() {
    let mut game = fixture_with(Disposition::Passive, 500, |document, _| {
        let ability = document
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        ability.strikes = StrikeSource::Authored {
            strikes: vec![crate::rules::StrikeDef {
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![crate::rules::RiderDef {
                    condition: None,
                    trigger: crate::rules::RiderTrigger::Success,
                    check: Some(crate::rules::RiderCheckDef {
                        skill: crate::rules::SkillSelector::Named {
                            skill: crate::attributes::Skill::HeavyWeaponry,
                        },
                        defense: crate::rules::DefenseKind::Physical,
                    }),
                    effects: vec![],
                    descriptors: vec![],
                }],
            }],
        };
    });
    let outcome = commit_strike(&mut game, ActorId(1), ActorId(2));
    assert!(!outcome.participation.target_participated());
    assert!(outcome.target_rider_participated);
    assert_eq!(
        game.actors[&ActorId(2)]
            .behavior
            .engagement
            .unwrap()
            .last_seen,
        0
    );
    assert_eq!(game.event_log.iter().filter(|event| matches!(event, crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.ability.is_some())).count(), 1);
}

#[test]
fn a_cancelled_preparation_and_unavailable_strike_do_not_provoke() {
    let mut game = fixture(Disposition::Passive, 500);
    let plan = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let admitted = game
        .admit_intention(
            ActorId(1),
            attack(ActorId(2)),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.cancel_intention(ActorId(1), admitted).unwrap();
    assert!(game.actors[&ActorId(2)].behavior.engagement.is_none());
    game.teleport(
        ActorId(2),
        Location {
            region: RegionId(2),
            position: Position { x: 4, y: 1, z: 0 },
        },
    )
    .unwrap();
    let outcome = game
        .resolve_strike(
            plan.as_ability().unwrap(),
            0,
            None,
            &mut crate::combat::PreparationProgress::Ability {
                next_strike: 1,
                successful_primaries: 0,
                attempted_riders: [0; 2],
                linked_hold: None,
            },
        )
        .unwrap();
    assert_eq!(outcome.result, crate::abilities::OutcomeStatus::Miss);
    assert!(!outcome.target_participated());
    assert!(game.actors[&ActorId(2)].behavior.engagement.is_none());
}

#[test]
fn paired_hidden_attackers_do_not_become_identified_grudges() {
    let mut game = fixture_with(Disposition::Passive, 500, |document, _| {
        let ability = document
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        ability.basic_attack = false;
        ability.reach = crate::rules::Reach::Sight { range: 8 };
        ability.strikes = StrikeSource::Authored {
            strikes: vec![crate::rules::StrikeDef {
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![crate::rules::Effect::Damage {
                    damage: crate::rules::DamageExpr::Components {
                        components: vec![crate::rules::DamageComponentExpr {
                            category: crate::combat::DamageType::Impact,
                            descriptor: None,
                            amount: crate::rules::AmountExpr::Fixed { amount: 0 },
                        }],
                    },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }],
        };
    });
    game.teleport(ActorId(2), at(4)).unwrap();
    game.set_cell_light(at(1), false).unwrap();
    let mut other = game.clone();
    other
        .actors
        .get_mut(&ActorId(1))
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .identity
        .name = "different hidden attacker".into();
    for state in [&mut game, &mut other] {
        let view = state.observe(ActorId(2)).unwrap();
        assert!(!view
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(1)));
        let outcome = commit_strike(state, ActorId(1), ActorId(2));
        assert!(outcome.target_participated());
        let view = state.observe(ActorId(2)).unwrap();
        let events = view.combat.unwrap().events;
        assert!(events.iter().any(|event| matches!(
            event,
            crate::combat::DisclosedCombatEvent::UnseenAbility {
                caster: None,
                target: ActorId(2),
                outcome: result,
            } if *result == outcome.result
        )));
        assert!(!events
            .iter()
            .any(|event| matches!(event, crate::combat::DisclosedCombatEvent::Ability { .. })));
        assert!(state.actors[&ActorId(2)].behavior.engagement.is_none());
    }
    assert_eq!(
        game.observe(ActorId(2)).unwrap(),
        other.observe(ActorId(2)).unwrap()
    );
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        other.choose_ai(ActorId(2)).unwrap().action
    );
}

#[test]
fn memory_expiry_preserves_only_the_final_positive_nutrition_interval() {
    for (memory_ticks, clock, expected) in [(50, 100, true), (100, 100, true), (100, 150, false)] {
        let mut game = fixture_with(Disposition::Passive, memory_ticks, |document, _| {
            document.nutrition = serde_json::from_value(serde_json::json!({
                "kind":"enabled","initial":0,"maximum":2000,"hungry_below":300,
                "weak_below":150,"starve_at":-852,"unit_ticks":100,
                "hungry_recovery":{"kind":"normal"},"weak_recovery":{"kind":"normal"},
                "empty_recovery":{"kind":"normal"},
                "regeneration_cost":{"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0}
            }))
            .unwrap();
        });
        game.configure_nutrition(ActorId(2), crate::nutrition::NutritionRequirement::Required)
            .unwrap();
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = Some(Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1),
            last_seen: 0,
        });
        game.tick = clock;
        let settled = game.settled_actor_outcome(ActorId(2)).unwrap();
        assert_eq!(
            settled.nutrition_empty_depletion, expected,
            "memory={memory_ticks}, clock={clock}"
        );
        assert!(settled.state.behavior.engagement.is_none());
    }
}

#[test]
fn partial_attacker_body_requires_disclosed_caster_for_historical_identity() {
    for anchor_lit in [false, true] {
        let mut game = fixture(Disposition::Passive, 500);
        game.teleport(ActorId(2), at(4)).unwrap();
        game.set_body(
            ActorId(1),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0], [2, 0, 0]],
                eye: [0, 0, 0],
                mass: 100,
            },
        )
        .unwrap();
        game.set_cell_light(at(1), anchor_lit).unwrap();
        let view = game.observe(ActorId(2)).unwrap();
        let projected = view
            .visible_actors
            .iter()
            .find(|actor| actor.id == ActorId(1))
            .unwrap();
        assert_eq!(
            projected.disclosure,
            crate::perception::ActorDisclosure::Creature
        );
        assert_eq!(
            view.visible_actors
                .iter()
                .any(|actor| actor.id == ActorId(1) && actor.location == at(1)),
            anchor_lit
        );
        let outcome = commit_strike(&mut game, ActorId(1), ActorId(2));
        assert!(outcome.target_participated());
        let events = game.observe(ActorId(2)).unwrap().combat.unwrap().events;
        if anchor_lit {
            assert!(events.iter().any(|event| matches!(
                event,
                crate::combat::DisclosedCombatEvent::Ability {
                    caster: Some(ActorId(1)),
                    target: Some(ActorId(2)),
                    ..
                }
            )));
            assert_eq!(
                game.actors[&ActorId(2)].behavior.engagement.unwrap().target,
                ActorId(1)
            );
        } else {
            assert!(events.iter().any(|event| matches!(
                event,
                crate::combat::DisclosedCombatEvent::UnseenAbility {
                    caster: None,
                    target: ActorId(2),
                    outcome: result,
                } if *result == outcome.result
            )));
            assert!(!events
                .iter()
                .any(|event| matches!(event, crate::combat::DisclosedCombatEvent::Ability { .. })));
            assert!(game.actors[&ActorId(2)].behavior.engagement.is_none());
        }
    }
}

#[test]
fn carrying_healing_and_loot_does_not_enable_generic_tactics() {
    let mut game = fixture(Disposition::Passive, 500);
    let mut potion = crate::ItemSpec::ordinary("known healing".into());
    potion.class = crate::ItemClass::Potion;
    potion.consumable = Some(crate::ConsumableSpec {
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![crate::rules::Effect::Heal {
            amount: crate::rules::AmountExpr::Fixed { amount: 10 },
        }],
    });
    game.place_item_stack(20, at(2), Some(ActorId(2)), 1, potion)
        .unwrap();
    let maximum = game.health(ActorId(2)).unwrap().1;
    game.apply_damage(
        ActorId(2),
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                u32::try_from(maximum - 1).unwrap(),
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
    game.configure_ai(ActorId(2), AiProfile::general()).unwrap();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Drink {
            item: crate::ItemId(20)
        }
    );
}

#[test]
fn engagement_expiry_segments_contextual_recovery_at_the_actor_active_boundary() {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    let mut game = fixture_with(Disposition::Passive, 50, |_, species| {
        species.modifiers.extend([
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::HealthRecoveryPeriod,
                    value: 100,
                },
            },
            Modifier {
                scope: Scope::WhileNoSensedHostile,
                op: ModifierOp::Add {
                    stat: Stat::HealthRecoveryPeriodPercent,
                    value: -50,
                },
            },
        ]);
    });
    game.apply_damage(
        ActorId(2),
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                10,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let injured = game.health(ActorId(2)).unwrap().0;
    game.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = Some(Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(1),
        location: at(1),
        last_seen: 0,
    });
    game.reconcile_recovery_contexts().unwrap();
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .period(),
        100
    );
    assert_eq!(game.next_creature_event_tick(), Some(50));
    let before = game.clone();
    game.tick = 75;
    let projected = game.creature(ActorId(2)).unwrap().into_owned();
    assert_eq!(projected.health().current(), injured + 1);
    assert_eq!(projected.health_recovery().elapsed(), 0);
    assert_eq!(
        game.actors[&ActorId(2)].active_since,
        0,
        "read projects, never settles"
    );
    let mut committed = before;
    committed.tick = 50;
    committed.settle_creature_time().unwrap();
    assert!(committed.actors[&ActorId(2)].behavior.engagement.is_none());
    assert_eq!(
        committed
            .creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .period(),
        50
    );
    committed.tick = 75;
    committed.settle_creature_time().unwrap();
    assert_eq!(committed.creature(ActorId(2)).unwrap().as_ref(), &projected);
}

#[test]
fn coincident_condition_and_engagement_expiry_rephase_earned_work_once() {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    let mut game = fixture_with(Disposition::Passive, 402, |document, species| {
        document.health_recovery.base_period = 1200;
        document.health_recovery.reduction_per_hd = 0;
        species.modifiers.push(Modifier {
            scope: Scope::WhileNoSensedHostile,
            op: ModifierOp::Add {
                stat: Stat::HealthRecoveryPeriodPercent,
                value: -25,
            },
        });
        let condition = document
            .conditions
            .iter_mut()
            .find(|condition| condition.id.as_str() == "fear")
            .unwrap();
        condition.modifiers = vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::HealthRecoveryPeriod,
                value: 400,
            },
        }];
    });
    game.apply_damage(
        ActorId(2),
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Vital,
                None,
                10,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    game.apply_condition(
        ActorId(2),
        &crate::content_ids::ConditionId::new("fear").unwrap(),
        ActorId(1),
        402,
    )
    .unwrap();
    game.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = Some(Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(1),
        location: at(1),
        last_seen: 0,
    });
    game.reconcile_recovery_contexts().unwrap();
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .period(),
        1600
    );
    game.tick = 402;
    game.settle_creature_time().unwrap();
    let clock = game.creature(ActorId(2)).unwrap().health_recovery();
    assert_eq!(clock.period(), 900);
    assert_eq!(
        clock.elapsed(),
        226,
        "one combined rephase floors 402 * 900 / 1600 only once"
    );
}

#[test]
fn ai_engagement_deadline_overflow_rejects_the_whole_decision() {
    let mut game = fixture(Disposition::Mean, 500);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("neighbors".into(), BTreeSet::from(["neighbors".into()]))]),
    )
    .unwrap();
    game.configure_ai(ActorId(1), AiProfile::default()).unwrap();
    game.tick = u64::MAX - 200;
    for id in [ActorId(1), ActorId(2)] {
        let mut actor = game.actors.get_mut(&id).unwrap();
        actor.active_since = game.tick;
        actor.ready_at = game.tick;
    }
    game.actors.get_mut(&ActorId(1)).unwrap().behavior.rng = 0;
    let decision = game.choose_ai(ActorId(1)).unwrap();
    assert!(decision.behavior.engagement.is_some());
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(1), decision.action),
        Err(crate::GameError::Schedule(
            crate::ScheduleError::TimeExhausted
        ))
    );
    assert_eq!(
        game, before,
        "failed publication consumes neither RNG nor action work"
    );
}

#[test]
fn species_policy_changes_settle_old_memory_and_never_reset_rng_or_resurrect_engagement() {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    let source = fixture_with(Disposition::Passive, 1000, |document, species| {
        document.health_recovery.base_period = 100;
        document.health_recovery.reduction_per_hd = 0;
        document.health_recovery.minimum_period = 1;
        species.modifiers.push(Modifier {
            scope: Scope::WhileNoSensedHostile,
            op: ModifierOp::Add {
                stat: Stat::HealthRecoveryPeriodPercent,
                value: -50,
            },
        });
    });
    let original = source.creature(ActorId(2)).unwrap().build().clone();
    let long = original.species().clone();
    let mut short = long.clone();
    short.id = crate::content_ids::SpeciesId::new("fixture_short_memory").unwrap();
    short.behavior.memory_ticks = 50;
    let content = crate::creatures::Content::new(
        original.ruleset().clone().into(),
        crate::creatures::ScenarioCatalog {
            species: BTreeMap::from([
                (long.id.clone(), long.clone()),
                (short.id.clone(), short.clone()),
            ]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let build = |species| {
        crate::creatures::CreatureBuild::new(
            content.clone(),
            species,
            original.ledger().clone(),
            original.binding(),
            [
                crate::attributes::Skill::Deception,
                crate::attributes::Skill::Persuasion,
            ],
        )
        .unwrap()
    };
    let mut game = Game::two_room(42, content.clone());
    for x in [1, 2] {
        let actor = game
            .spawn_actor(at(x), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("actor {x}"),
                faction: "neighbors".into(),
            },
            build(long.id.clone()),
        )
        .unwrap();
    }
    crate::test_creatures::injure(&mut game, ActorId(2), 10).unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().behavior = Behavior {
        rng: 0,
        engagement: Some(Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1),
            last_seen: 0,
        }),
    };
    game.reconcile_recovery_contexts().unwrap();
    game.configure_ai(ActorId(2), AiProfile::general()).unwrap();
    assert_eq!(game.actors[&ActorId(2)].engagement_deadline(), Some(1000));
    game.tick = 75;
    let replacement = build(short.id);
    let before = crate::diagnostics::work_counts();
    game.rebuild_creature(ActorId(2), replacement).unwrap();
    let rebuilt = crate::diagnostics::work_counts();
    assert!(rebuilt.raw_species_lookups > before.raw_species_lookups);
    assert!(rebuilt.permanent_creature_compiles > before.permanent_creature_compiles);
    let actor = &game.actors[&ActorId(2)];
    assert_eq!(
        actor.behavior,
        Behavior {
            rng: 0,
            engagement: None
        }
    );
    let clock = game.creature(ActorId(2)).unwrap().health_recovery();
    assert_eq!((clock.period(), clock.elapsed()), (50, 37));
    game.rebuild_creature(ActorId(2), build(long.id)).unwrap();
    assert_eq!(
        game.actors[&ActorId(2)].behavior,
        Behavior {
            rng: 0,
            engagement: None
        }
    );
    for id in [ActorId(1), ActorId(2)] {
        game.actors.get_mut(&id).unwrap().ready_at = game.tick;
    }
    assert_eq!(restore(&game), game);
}

fn compiled_species_fixture(hit_dice: usize) -> Game {
    let mut game = fixture(Disposition::Passive, 500);
    for id in [ActorId(1), ActorId(2)] {
        let creature = game.creature(id).unwrap();
        let base = creature.build();
        let build = crate::creatures::CreatureBuild::new(
            base.content().clone(),
            base.species_id().clone(),
            crate::progression::HdLedger::seeded(
                vec![crate::progression::HdSource::Racial; hit_dice],
                42,
            )
            .unwrap(),
            base.binding(),
            *base.starting_skills(),
        )
        .unwrap();
        game.rebuild_creature(id, build).unwrap();
    }
    game
}

#[test]
fn compiled_species_observation_and_ai_queries_do_not_read_raw_species() {
    for count in [2, 256] {
        let game = compiled_species_fixture(count);
        let behavior = game.actors[&ActorId(2)].behavior.clone();
        let before = crate::diagnostics::work_counts();
        for _ in 0..4 {
            assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
            assert!(game.observe(ActorId(2)).unwrap().combat.is_some());
        }
        let after = crate::diagnostics::work_counts();
        assert!(after.observations > before.observations);
        assert!(after.decision_scenes > before.decision_scenes);
        assert_eq!(game.actors[&ActorId(2)].behavior, behavior);
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
    }
}

#[test]
fn compiled_species_deadline_projection_does_not_read_raw_species() {
    for count in [2, 256] {
        let mut game = compiled_species_fixture(count);
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = Some(Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1),
            last_seen: 0,
        });
        let before = crate::diagnostics::work_counts();
        for clock in [1, 499, 500, 501] {
            game.tick = clock;
            let actor = &game.actors[&ActorId(2)];
            assert_eq!(actor.engagement_deadline(), Some(500));
            assert_eq!(actor.engagement_at(clock).is_some(), clock < 500);
            let outcome = game.projected_actor_outcome(ActorId(2)).unwrap();
            assert_eq!(outcome.state.behavior.engagement.is_some(), clock < 500);
        }
        let after = crate::diagnostics::work_counts();
        assert!(game.actors[&ActorId(2)].behavior.engagement.is_some());
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
    }
}
