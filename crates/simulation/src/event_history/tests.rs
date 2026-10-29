use super::*;
use crate::abilities::{AttemptedParticipation, Outcome, OutcomeStatus, ResolvedEffect};
use crate::content_ids::{AbilityId, TemplateId};
use crate::perception::{ActorDisclosure, PerceptionValue};
use crate::rules::{Modifier, ModifierOp, Scope};
use tor_world::{Location, Position, RegionId};

fn at(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}
fn fixture() -> Game {
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let mut invisible =
        crate::creatures::Template::new(TemplateId::new("event_invisible").unwrap(), 0);
    invisible.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Perception {
            value: PerceptionValue::Invisible { enabled: true },
        },
    });
    let mut detecting =
        crate::creatures::Template::new(TemplateId::new("event_detecting").unwrap(), 0);
    detecting.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Perception {
            value: PerceptionValue::SeeInvisible { enabled: true },
        },
    });
    let mut guised = crate::creatures::Template::new(TemplateId::new("event_guised").unwrap(), 0);
    guised.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Perception {
            value: PerceptionValue::ObjectGuise {
                skill: crate::attributes::Skill::Stealth,
                edge: 0,
                appearance: Some(crate::perception::GuiseAppearance {
                    name: "closed chest".into(),
                    description: "An ordinary chest.".into(),
                    class: crate::ItemClass::Tool,
                    asset: None,
                }),
            },
        },
    });
    let mut caster = crate::creatures::Template::new(TemplateId::new("event_caster").unwrap(), 0);
    caster.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: AbilityId::new("spark_lance_fire").unwrap(),
        },
    });
    let mut rules = crate::test_creatures::ruleset().document().clone();
    rules.conditions.push(serde_json::from_value(serde_json::json!({
        "id":"event_detecting", "family":"event_detecting", "name":"Detecting",
        "lifecycle":"timed", "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
        "restrictions":[], "preparation":"preserve", "modifiers":[{
            "scope":"always", "op":{"kind":"perception","value":{"kind":"see_invisible","enabled":true}}
        }], "stacking":{"kind":"longest"}, "source_scoped":false,
        "descriptors":[], "capacity":"reject"
    })).unwrap());
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(rules).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: [
                (invisible.id.clone(), invisible),
                (detecting.id.clone(), detecting),
                (guised.id.clone(), guised),
                (caster.id.clone(), caster),
            ]
            .into(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content.clone(),
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
    let mut game = Game::two_room_in_stone(42, content);
    for location in [at(1, 1), at(2, 1), at(2, 2)] {
        let id = game
            .spawn_actor(location, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            id,
            crate::CreatureIdentity {
                name: format!("actor {}", id.0),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game
}
fn template(game: &mut Game, actor: ActorId, name: Option<&str>) {
    let mut build = game.creature(actor).unwrap().build().clone();
    build
        .set_templates(
            name.map(|name| vec![TemplateId::new(name).unwrap()])
                .unwrap_or_default(),
        )
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
}
fn damage(actor: ActorId, target: ActorId, amount: u32) -> Outcome {
    Outcome {
        resistance: None,
        actor,
        target,
        ability: Some(AbilityId::new("basic_melee").unwrap()),
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        participation: AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![ResolvedEffect::Damage { amount }],
    }
}
fn events(game: &Game, actor: ActorId) -> Vec<DisclosedCombatEvent> {
    game.observe(actor).unwrap().combat.unwrap().events
}
fn restored(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}
fn host_damage(game: &mut Game, amount: u32) {
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    game.apply_outcome(damage(ActorId(1), ActorId(2), amount))
        .unwrap();
    game.seal_participant_events().unwrap();
    game.host_region_continuation = false;
}
fn capture(game: &mut Game, actor: ActorId, witness: &EventObservationWitness) -> Observation {
    let (mut view, _, proof) = game.observe_event_scene(actor).unwrap();
    game.record_event_observation(witness, &proof, &mut view)
        .unwrap();
    view
}

#[test]
fn unchanged_unbound_origins_reuse_work_but_changed_senses_and_bindings_do_not() {
    let mut game = fixture();
    let origin = game.capture_event_origin(ActorId(1), ActorId(2));
    let before = crate::diagnostics::work_counts();
    let reused = game.reuse_event_origin(ActorId(1), ActorId(2), Some(&origin));
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.scenes, before.scenes);
    assert_eq!(after.body_cells, before.body_cells);
    assert_eq!(after.actor_disclosures, before.actor_disclosures);
    assert_eq!(reused.source_target, Some(ActorReferenceKind::Creature));
    template(&mut game, ActorId(2), Some("event_invisible"));
    let refreshed = game.reuse_event_origin(ActorId(1), ActorId(2), Some(&reused));
    assert_eq!(refreshed.source_target, Some(ActorReferenceKind::Contact));
    assert_eq!(origin.source_target, Some(ActorReferenceKind::Creature));
    let mut bound = refreshed;
    bound.event = Some(EventKey::capture(&CombatEvent::Died { actor: ActorId(2) }));
    let before = crate::diagnostics::work_counts();
    let unbound = game.reuse_event_origin(ActorId(1), ActorId(2), Some(&bound));
    assert!(unbound.event.is_none());
    assert!(crate::diagnostics::work_counts().actor_disclosures > before.actor_disclosures);
}

#[test]
fn active_recipient_retains_visible_or_unseen_feedback_from_a_frozen_source() {
    for invisible in [false, true] {
        let mut game = fixture();
        game.teleport(
            ActorId(1),
            Location {
                region: RegionId(2),
                position: Position { x: 4, y: 1, z: 0 },
            },
        )
        .unwrap();
        game.teleport(ActorId(2), at(0, 1)).unwrap();
        game.teleport(ActorId(3), at(0, 0)).unwrap();
        if invisible {
            template(&mut game, ActorId(1), Some("event_invisible"));
        }
        game.transition_regions(
            &crate::RegionTransition {
                active: [RegionId(1)].into(),
                loaded: [RegionId(1), RegionId(2)].into(),
            },
            &mut crate::MemoryRecords::default(),
        )
        .unwrap();
        assert!(game.actor_frozen(ActorId(1)));
        assert!(!game.actor_frozen(ActorId(2)));
        let origin = game.capture_event_origin(ActorId(1), ActorId(2));
        assert_eq!(origin.source_target, None);
        assert_eq!(
            origin.target_source,
            (!invisible).then_some(ActorReferenceKind::Creature)
        );
        game.apply(damage(ActorId(1), ActorId(2), 1)).unwrap();
        let received = events(&game, ActorId(2));
        if invisible {
            assert!(received.iter().any(|event| matches!(
                event,
                DisclosedCombatEvent::UnseenAbility {
                    caster: None,
                    target: ActorId(2),
                    ..
                }
            )));
        } else {
            assert!(received.iter().any(|event| matches!(
                event,
                DisclosedCombatEvent::Ability {
                    caster: Some(ActorId(1)),
                    caster_reference: Some(ActorReferenceKind::Creature),
                    ..
                }
            )));
        }
        assert!(events(&game, ActorId(1)).iter().any(|event| matches!(
            event,
            DisclosedCombatEvent::Ability {
                caster: Some(ActorId(1)),
                target: None,
                ..
            }
        )));
    }
}

#[test]
fn rider_refreshes_after_primary_reveal_without_upgrading_primary_or_contact_plan() {
    for contact in [false, true] {
        let mut game = fixture();
        let plan = game
            .ability_plan(
                ActorId(1),
                &AbilityId::new("basic_melee").unwrap(),
                ActorId(2),
            )
            .unwrap();
        let mut ability = plan.as_ability().unwrap().clone();
        ability.strikes.truncate(1);
        ability.strikes[0].check = None;
        ability.strikes[0].on_success = vec![crate::abilities::PlannedEffect::Heal {
            amount: crate::abilities::PlannedAmount::Fixed { amount: 1 },
        }];
        ability.strikes[0].on_failure.clear();
        ability.strikes[0].riders = vec![crate::abilities::FrozenRider {
            trigger: crate::rules::RiderTrigger::Success,
            condition: None,
            check: None,
            effects: vec![crate::abilities::PlannedEffect::Damage {
                damage: crate::damage::DamageSpec::new(
                    vec![crate::damage::DamageComponent::fixed(
                        crate::combat::DamageType::Impact,
                        None,
                        1_000_000,
                    )],
                    None,
                )
                .unwrap(),
            }],
            descriptors: vec![],
        }];
        if contact {
            ability.invocation = crate::abilities::AbilityInvocation::contact_target(ActorId(2));
        }
        template(&mut game, ActorId(2), Some("event_guised"));
        assert!(game.actors[&ActorId(2)].guise.appearance().is_some());
        game.resolve_strike(
            &ability,
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
        assert!(game.actors[&ActorId(2)].guise.appearance().is_none());
        let history = events(&game, ActorId(1));
        let references: Vec<_> = game.event_receipts.observers[&ActorId(1)]
            .entries
            .iter()
            .filter_map(|entry| match entry.key.kind {
                EventKind::Resolved => Some(entry.target),
                _ => None,
            })
            .collect();
        let expected = if contact {
            ActorReferenceKind::Contact
        } else {
            ActorReferenceKind::Creature
        };
        assert_eq!(references, vec![None, Some(expected)]);
        assert!(history.iter().any(|event| matches!(
            event,
            DisclosedCombatEvent::Ability {
                target: None,
                target_reference: None,
                ..
            }
        )));
        assert!(
            history.iter().any(|event| matches!(event,
            DisclosedCombatEvent::Died { actor: ActorId(2), reference } if *reference == expected))
        );
    }
}

#[test]
fn target_detection_between_primary_and_rider_refreshes_only_the_rider_source() {
    let mut game = fixture();
    template(&mut game, ActorId(1), Some("event_invisible"));
    let plan = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let mut ability = plan.as_ability().unwrap().clone();
    ability.strikes.truncate(1);
    ability.strikes[0].check = None;
    ability.strikes[0].on_success = vec![crate::abilities::PlannedEffect::ApplyCondition {
        id: crate::content_ids::ConditionId::new("event_detecting").unwrap(),
        duration: 100,
    }];
    ability.strikes[0].on_failure.clear();
    ability.strikes[0].riders = vec![crate::abilities::FrozenRider {
        trigger: crate::rules::RiderTrigger::Success,
        condition: None,
        check: None,
        effects: vec![crate::abilities::PlannedEffect::Heal {
            amount: crate::abilities::PlannedAmount::Fixed { amount: 1 },
        }],
        descriptors: vec![],
    }];
    game.resolve_strike(
        &ability,
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
    let references: Vec<_> = game.event_receipts.observers[&ActorId(2)]
        .entries
        .iter()
        .filter_map(|entry| (entry.key.kind == EventKind::Resolved).then_some(entry.source))
        .collect();
    assert_eq!(
        references,
        vec![
            Some(ActorReferenceKind::Contact),
            Some(ActorReferenceKind::Creature)
        ]
    );
    assert!(events(&game, ActorId(2)).iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::UnseenAbility {
            caster: Some(ActorId(1)),
            ..
        }
    )));
}

#[test]
fn missing_receipts_never_reclassify_on_free_reads_or_later_commit() {
    let mut game = fixture();
    host_damage(&mut game, 1);
    let cold = game.clone();
    for _ in 0..3 {
        assert!(events(&game, ActorId(3)).is_empty());
    }
    assert_eq!(game, cold);
    let witness = game.event_observation_witness();
    assert!(!capture(&mut game, ActorId(3), &witness)
        .combat
        .unwrap()
        .events
        .is_empty());
    game.seal_event_observations().unwrap();
    let saved = events(&game, ActorId(3));
    template(&mut game, ActorId(1), Some("event_invisible"));
    assert_eq!(events(&game, ActorId(3)), saved);
    assert_eq!(events(&restored(&game), ActorId(3)), saved);
}

#[test]
fn same_generation_append_keeps_the_hidden_prefix_frozen() {
    let mut game = fixture();
    template(&mut game, ActorId(2), Some("event_invisible"));
    game.apply(damage(ActorId(1), ActorId(2), 1)).unwrap();
    let first = events(&game, ActorId(1));
    assert!(matches!(
        first[0],
        DisclosedCombatEvent::Ability {
            target_reference: Some(ActorReferenceKind::Contact),
            ..
        }
    ));
    let generation = game.event_receipts.generation;
    template(&mut game, ActorId(1), Some("event_detecting"));
    game.apply(damage(ActorId(1), ActorId(2), 1)).unwrap();
    let after = events(&game, ActorId(1));
    assert_eq!(game.event_receipts.generation, generation);
    assert_eq!(after[0], first[0]);
    assert!(matches!(
        after[1],
        DisclosedCombatEvent::Ability {
            target_reference: Some(ActorReferenceKind::Creature),
            ..
        }
    ));
    assert_eq!(events(&restored(&game), ActorId(1)), after);
}

#[test]
fn anonymous_lethal_hit_and_death_stay_anonymous_beside_visible_corpse() {
    let mut game = fixture();
    template(&mut game, ActorId(2), Some("event_invisible"));
    game.apply(damage(ActorId(1), ActorId(2), 1_000_000))
        .unwrap();
    let view = game.observe(ActorId(1)).unwrap();
    let history = &view.combat.as_ref().unwrap().events;
    assert!(history.iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Ability {
            target: Some(ActorId(2)),
            target_reference: Some(ActorReferenceKind::Contact),
            ..
        }
    )));
    assert!(history.iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Died {
            actor: ActorId(2),
            reference: ActorReferenceKind::Contact
        }
    )));
    assert!(view.ground_items.iter().any(|item| item
        .corpse
        .as_ref()
        .is_some_and(|corpse| corpse.actor == ActorId(2))));
    assert_eq!(events(&restored(&game), ActorId(1)), *history);
}

#[test]
fn public_act_and_apply_seal_before_snapshot_and_keep_queries_pure() {
    let mut game = fixture();
    game.act(ActorId(1), crate::Action::Wait).unwrap();
    game.apply(damage(ActorId(2), ActorId(1), 1)).unwrap();
    let before = game.clone();
    let rng = game.rules_rng;
    let tick = game.tick();
    let expected = events(&game, ActorId(1));
    for _ in 0..3 {
        assert_eq!(events(&game, ActorId(1)), expected);
    }
    assert_eq!(game, before);
    assert_eq!((game.rules_rng, game.tick()), (rng, tick));
    assert_eq!(events(&restored(&game), ActorId(1)), expected);
    assert!(game.event_origins.entries.is_empty());
}

#[test]
fn host_event_identity_requires_the_disclosed_reference_cell() {
    for anchor_lit in [false, true] {
        let mut game = fixture();
        game.teleport(ActorId(2), at(4, 1)).unwrap();
        game.teleport(ActorId(3), at(4, 2)).unwrap();
        game.set_body(
            ActorId(1),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0], [2, 0, 0]],
                eye: [0, 0, 0],
                mass: 100,
            },
        )
        .unwrap();
        game.set_cell_light(at(1, 1), anchor_lit).unwrap();
        host_damage(&mut game, 1);
        let witness = game.event_observation_witness();
        let (mut view, scene, proof) = game.observe_event_scene(ActorId(3)).unwrap();
        assert!(view.visible_actors.iter().any(|actor| {
            actor.id == ActorId(1) && actor.disclosure == ActorDisclosure::Creature
        }));
        assert_eq!(
            scene.iter().any(|cell| cell.location == at(1, 1)),
            anchor_lit
        );
        let before = crate::diagnostics::work_counts();
        game.record_event_observation(&witness, &proof, &mut view)
            .unwrap();
        assert_eq!(crate::diagnostics::work_counts().scenes, before.scenes);
        let third_party = view.combat.unwrap().events;
        if anchor_lit {
            assert!(third_party.iter().any(|event| matches!(
                event,
                DisclosedCombatEvent::Ability {
                    caster: Some(ActorId(1)),
                    target: Some(ActorId(2)),
                    ..
                }
            )));
        } else {
            assert!(third_party.is_empty());
            assert!(events(&game, ActorId(2)).iter().any(|event| matches!(
                event,
                DisclosedCombatEvent::UnseenAbility {
                    caster: None,
                    target: ActorId(2),
                    ..
                }
            )));
        }
        game.seal_event_observations().unwrap();
        let reopened = restored(&game);
        for observer in [ActorId(2), ActorId(3)] {
            assert_eq!(events(&reopened, observer), events(&game, observer));
        }
    }
}

#[test]
fn event_observer_proof_binds_owner_generation_and_visibility() {
    let mut game = fixture();
    host_damage(&mut game, 1);
    let witness = game.event_observation_witness();
    let (view, _, proof) = game.observe_event_scene(ActorId(3)).unwrap();
    let mut wrong_owner = view.clone();
    wrong_owner.actor = ActorId(1);
    assert!(game
        .record_event_observation(&witness, &proof, &mut wrong_owner)
        .is_err());
    let before = game.clone();
    template(&mut game, ActorId(3), Some("event_detecting"));
    assert!(game
        .record_event_observation(&witness, &proof, &mut view.clone())
        .is_err());
    assert_eq!(game.event_receipts, before.event_receipts);
}

#[test]
fn observer_capture_rollback_and_unchanged_reads_preserve_shared_receipts() {
    let mut game = fixture();
    host_damage(&mut game, 1);
    let committed = game.clone();
    let rows = game.event_receipts.observers.clone();
    assert!(game.event_receipts.observers.shares_storage(&rows));
    let mut candidate = game.clone();
    let witness = candidate.event_observation_witness();
    capture(&mut candidate, ActorId(3), &witness);
    assert!(!candidate.event_receipts.observers.shares_storage(&rows));
    assert!(game.event_receipts.observers.shares_storage(&rows));
    drop(candidate); // A rejected host/storage candidate has no publication.
    assert_eq!(game, committed);
    let _ = game.observe(ActorId(3)).unwrap();
    assert!(game.event_receipts.observers.shares_storage(&rows));
}

#[test]
fn known_ability_and_death_only_batches_capture_without_extra_scenes() {
    let mut game = fixture();
    host_damage(&mut game, 1);
    let witness = game.event_observation_witness();
    let (mut view, _, proof) = game.observe_event_scene(ActorId(3)).unwrap();
    let counts = crate::diagnostics::work_counts();
    game.record_event_observation(&witness, &proof, &mut view)
        .unwrap();
    assert_eq!(
        crate::diagnostics::work_counts().ordinary_scenes,
        counts.ordinary_scenes
    );
    assert!(!view.combat.unwrap().events.is_empty());
    game.seal_event_observations().unwrap();
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    let origin = game.capture_event_origin(ActorId(1), ActorId(2));
    game.push_event(CombatEvent::Died { actor: ActorId(2) }, Some(origin))
        .unwrap();
    game.seal_participant_events().unwrap();
    game.host_region_continuation = false;
    assert!(game.observation_knowledge_candidates().unwrap().is_empty());
    let witness = game.event_observation_witness();
    let (mut view, _, proof) = game.observe_event_scene(ActorId(3)).unwrap();
    let counts = crate::diagnostics::work_counts();
    game.record_event_observation(&witness, &proof, &mut view)
        .unwrap();
    assert_eq!(
        crate::diagnostics::work_counts().ordinary_scenes,
        counts.ordinary_scenes
    );
    assert!(matches!(
        view.combat.unwrap().events.as_slice(),
        [DisclosedCombatEvent::Died { .. }]
    ));
}

#[test]
fn restored_receipts_reject_stale_generation_kind_owner_and_watermark() {
    let mut game = fixture();
    game.apply(damage(ActorId(1), ActorId(2), 1)).unwrap();
    let baseline = serde_json::to_value(&game.event_receipts).unwrap();
    let mut alternative_kind = baseline.clone();
    alternative_kind["observers"]["1"]["entries"][0]["key"]["kind"] =
        serde_json::json!({"resolved": null});
    assert!(
        serde_json::from_value::<EventReceipts>(alternative_kind).is_err(),
        "event kind must use its canonical string operand, never an externally tagged object"
    );
    for field in [
        "generation",
        "through",
        "kind",
        "actor",
        "participant",
        "source",
    ] {
        let mut value = baseline.clone();
        let row = &mut value["observers"]["1"];
        match field {
            "generation" => row["generation"] = serde_json::json!(99),
            "through" => row["through"] = serde_json::json!(99),
            "kind" => row["entries"][0]["key"]["kind"] = serde_json::json!("died"),
            "actor" => row["entries"][0]["key"]["actor"] = serde_json::json!(3),
            "participant" => row["entries"][0]["participant"] = serde_json::json!(false),
            "source" => row["entries"][0]["source"] = serde_json::json!("contact"),
            _ => unreachable!(),
        }
        let mut corrupt = game.clone();
        corrupt.event_receipts = serde_json::from_value(value).unwrap();
        assert!(!corrupt.event_receipts_valid(), "{field}");
        let mut shared = crate::checkpoint::SharedState::default();
        assert!(Game::restore_checkpoint(
            corrupt.checkpoint(&mut shared),
            &shared,
            corrupt.content().clone()
        )
        .is_err());
    }
}

#[test]
fn short_or_missing_receipt_does_not_fall_back_to_current_visibility() {
    let mut game = fixture();
    game.apply(damage(ActorId(1), ActorId(2), 1)).unwrap();
    assert_eq!(
        game.actor_disclosure(ActorId(1), ActorId(2)).unwrap(),
        ActorDisclosure::Creature
    );
    game.event_receipts
        .observers
        .get_mut(&ActorId(1))
        .unwrap()
        .through = 0;
    assert!(events(&game, ActorId(1)).is_empty());
    game.event_receipts.observers.remove(&ActorId(1));
    assert!(events(&game, ActorId(1)).is_empty());
}

#[test]
fn changed_visibility_omits_third_party_death_but_keeps_causal_participant() {
    let mut game = fixture();
    host_damage(&mut game, 1_000_000);
    let witness = game.event_observation_witness();
    let third = capture(&mut game, ActorId(3), &witness);
    assert!(third.combat.unwrap().events.is_empty());
    assert!(events(&game, ActorId(1)).iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Died {
            actor: ActorId(2),
            ..
        }
    )));
    game.seal_event_observations().unwrap();
    assert!(events(&restored(&game), ActorId(3)).is_empty());
}

#[test]
fn frozen_prefix_cannot_be_removed_and_failed_receipt_capacity_is_atomic() {
    let mut game = fixture();
    game.apply(damage(ActorId(1), ActorId(2), 1)).unwrap();
    let before = game.clone();
    assert!(game.take_event_suffix(0).is_err());
    assert_eq!(game, before);
    game.event_receipts.bindings = crate::limits::EVENT_RECEIPTS;
    let full = game.clone();
    assert!(game.apply(damage(ActorId(1), ActorId(2), 1)).is_err());
    assert_eq!(game, full);
}

#[test]
fn ordinary_actor_visibility_epoch_ignores_receipt_and_knowledge_writes() {
    let mut game = fixture();
    assert!(game.actors.contextual_recovery_owners().next().is_none());
    let before = game.actors.visibility_epoch();
    game.actors
        .get_mut(&ActorId(3))
        .unwrap()
        .knowledge
        .insert("fixture_fact".into());
    assert_eq!(game.actors.visibility_epoch(), before);
    template(&mut game, ActorId(3), Some("event_detecting"));
    assert_ne!(game.actors.visibility_epoch(), before);
    host_damage(&mut game, 1);
    let witness = game.event_observation_witness();
    let before = game.actors.visibility_epoch();
    capture(&mut game, ActorId(3), &witness);
    assert_eq!(game.actors.visibility_epoch(), before);
}

#[test]
fn cold_and_warm_free_queries_produce_identical_host_receipts() {
    let base = fixture();
    let mut cold = base.clone();
    let mut warm = base;
    for id in [ActorId(1), ActorId(2), ActorId(3)] {
        warm.observe(id).unwrap();
    }
    for game in [&mut cold, &mut warm] {
        host_damage(game, 1);
        let witness = game.event_observation_witness();
        for id in [ActorId(1), ActorId(2), ActorId(3)] {
            capture(game, id, &witness);
        }
        game.seal_event_observations().unwrap();
    }
    assert_eq!(cold.event_receipts, warm.event_receipts);
    assert_eq!(cold, warm);
}

#[test]
fn own_condition_interruption_and_escape_feedback_need_no_counterpart_identity() {
    let mut game = fixture();
    template(&mut game, ActorId(2), Some("event_invisible"));
    game.apply_effects(
        ActorId(1),
        &[crate::rules::Effect::ApplyCondition {
            condition: crate::content_ids::ConditionId::new("readied").unwrap(),
            duration: 100,
        }],
    )
    .unwrap();
    assert!(events(&game, ActorId(1))
        .iter()
        .any(|event| matches!(event, DisclosedCombatEvent::Condition { .. })));
    game.push_event(
        CombatEvent::Interrupted {
            actor: ActorId(1),
            intention: None,
        },
        None,
    )
    .unwrap();
    game.push_event(
        CombatEvent::EscapeResolved {
            actor: ActorId(1),
            hold: crate::linked_hold::HoldId(1),
            success: true,
            intention: None,
        },
        None,
    )
    .unwrap();
    game.seal_participant_events().unwrap();
    let history = events(&game, ActorId(1));
    assert!(history.iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Interrupted { actor: ActorId(1) }
    )));
    assert!(history.iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::EscapeResolved { success: true, .. }
    )));
    assert!(events(&game, ActorId(3)).is_empty());
}

#[test]
fn participant_suffix_cost_does_not_rescan_or_copy_a_unique_retained_prefix() {
    let mut game = fixture();
    // Internal suffix publication avoids the deliberate public transaction COW.
    // The completed prefix is immutable while the same generation grows.
    for _ in 0..32 {
        game.push_event(
            CombatEvent::Interrupted {
                actor: ActorId(1),
                intention: None,
            },
            None,
        )
        .unwrap();
        game.seal_participant_events().unwrap();
    }
    let counts = crate::diagnostics::work_counts();
    game.push_event(
        CombatEvent::Interrupted {
            actor: ActorId(1),
            intention: None,
        },
        None,
    )
    .unwrap();
    game.seal_participant_events().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.event_receipt_cow_bindings,
        counts.event_receipt_cow_bindings
    );
    assert!(
        after.event_receipt_binding_inspections - counts.event_receipt_binding_inspections < 16
    );
    assert_eq!(game.event_receipts.observers[&ActorId(1)].entries.len(), 33);
}

/// Publish through the real primary/rider/seal/provocation path while keeping
/// both participants alive. A real check (hit or miss) witnesses participation;
/// no damage or secondary rider can obscure the source-domain boundary.
fn provoke_with_zero_damage_primary(game: &mut Game) {
    let plan = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let mut ability = plan.as_ability().unwrap().clone();
    ability.strikes.truncate(1);
    assert!(ability.strikes[0].check.is_some());
    ability.strikes[0].on_success.clear();
    ability.strikes[0].on_failure.clear();
    ability.strikes[0].riders.clear();
    game.resolve_strike(
        &ability,
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
}

#[test]
fn same_batch_reused_strike_ordinal_provokes_only_its_own_reference_domain() {
    use crate::perception::ActorReferenceKind::{Contact, Creature};
    // Both transitions need independent event identity. Same-domain cases are
    // positive controls, including two anonymous events with no public ordinal.
    for (first, second) in [
        (Contact, Creature),
        (Creature, Contact),
        (Contact, Contact),
        (Creature, Creature),
    ] {
        let mut game = fixture();
        let conceal = |game: &mut Game, reference| {
            template(
                game,
                ActorId(1),
                (reference == Contact).then_some("event_invisible"),
            );
        };
        conceal(&mut game, first);
        provoke_with_zero_damage_primary(&mut game);
        assert_eq!(
            game.actors[&ActorId(2)]
                .behavior
                .engagement
                .unwrap()
                .reference,
            first
        );
        let generation = game.event_receipts.generation;
        let prefix = game.event_log.clone();
        conceal(&mut game, second);
        // A subsequent receipt must independently license a new engagement; an
        // old Contact may neither suppress a Creature nor license the reverse.
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = None;
        provoke_with_zero_damage_primary(&mut game);
        assert_eq!(game.event_receipts.generation, generation);
        assert_eq!(&game.event_log[..prefix.len()], prefix.as_slice());
        assert_eq!(game.event_log.len(), prefix.len() + 1);
        assert_eq!(game.actors[&ActorId(2)].behavior.engagement.map(|entry| entry.reference),
            Some(second), "{first:?} -> {second:?} must use the current published event, not a matching earlier strike");
        assert_eq!(
            game.event_log
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Resolved {
                        strike: Some(0),
                        ..
                    }
                ))
                .count(),
            2,
            "separate primary publications intentionally reuse ordinal zero"
        );
    }
}

fn host_source_activity(game: &mut Game, ceiling: SourceAbilityDisclosure) {
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    let origin = game
        .capture_event_origin(ActorId(1), ActorId(2))
        .with_ability_disclosure(ceiling);
    game.apply_resolved_with_disclosure(
        damage(ActorId(1), ActorId(2), 1),
        crate::lifecycle::ResolutionPublication::Primary { strike: 0 },
        &mut crate::lifecycle::DamageActivityGuard::default(),
        origin,
    )
    .unwrap();
    game.seal_participant_events().unwrap();
    game.host_region_continuation = false;
}

#[test]
fn source_disclosure_raw_records_require_exact_current_named_fields() {
    use serde_json::json;
    for (ceiling, spelling) in [
        (SourceAbilityDisclosure::Public, "public"),
        (SourceAbilityDisclosure::InitiatorOnly, "initiator_only"),
        (SourceAbilityDisclosure::Concealed, "concealed"),
    ] {
        let event = CombatEvent::Resolved {
            outcome: damage(ActorId(1), ActorId(2), 1),
            strike: Some(0),
            source_disclosure: ceiling,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["Resolved"].as_object().unwrap().len(), 3);
        assert_eq!(value["Resolved"]["source_disclosure"], spelling);
        assert_eq!(
            serde_json::from_value::<CombatEvent>(value.clone()).unwrap(),
            event
        );
        for field in ["outcome", "strike", "source_disclosure"] {
            let mut missing = value.clone();
            missing["Resolved"].as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<CombatEvent>(missing).is_err(),
                "{field}"
            );
        }
        for malformed in [
            json!(null),
            json!({(spelling):null}),
            json!([spelling]),
            json!("known"),
            json!(true),
            json!(1),
        ] {
            let mut invalid = value.clone();
            invalid["Resolved"]["source_disclosure"] = malformed;
            assert!(serde_json::from_value::<CombatEvent>(invalid).is_err());
        }
        let mut positional = value.clone();
        positional["Resolved"] = json!([
            value["Resolved"]["outcome"],
            value["Resolved"]["strike"],
            spelling
        ]);
        assert!(serde_json::from_value::<CombatEvent>(positional).is_err());
        let mut extra = value.clone();
        extra["Resolved"]["item"] = json!(1);
        assert!(serde_json::from_value::<CombatEvent>(extra).is_err());
        let text = serde_json::to_string(&value).unwrap();
        let old = format!("\"source_disclosure\":\"{spelling}\"");
        let duplicate = text.replacen(&old, &format!("{old},{old}"), 1);
        assert_ne!(duplicate, text);
        assert!(serde_json::from_str::<CombatEvent>(&duplicate).is_err());
        if ceiling != SourceAbilityDisclosure::Public {
            let mut no_ability = value;
            no_ability["Resolved"]["outcome"]["ability"] = json!(null);
            assert!(serde_json::from_value::<CombatEvent>(no_ability).is_err());
        }
    }
}

#[test]
fn source_disclosure_ceiling_names_only_licensed_historical_observers() {
    for ceiling in [
        SourceAbilityDisclosure::Public,
        SourceAbilityDisclosure::InitiatorOnly,
        SourceAbilityDisclosure::Concealed,
    ] {
        let mut game = fixture();
        host_source_activity(&mut game, ceiling);
        let witness = game.event_observation_witness();
        capture(&mut game, ActorId(3), &witness);
        game.seal_event_observations().unwrap();
        for observer in [ActorId(1), ActorId(2), ActorId(3)] {
            let disclosed = events(&game, observer);
            assert_eq!(disclosed.len(), 1);
            if ceiling.permits(observer, ActorId(1)) {
                assert!(
                    matches!(&disclosed[0], DisclosedCombatEvent::Ability { id, .. }
                    if id.as_str() == "basic_melee")
                );
            } else {
                assert!(matches!(
                    &disclosed[0],
                    DisclosedCombatEvent::AnonymousAbility {
                        caster: Some(ActorId(1)),
                        caster_reference: Some(ActorReferenceKind::Creature),
                        target: Some(ActorId(2)),
                        target_reference: Some(ActorReferenceKind::Creature),
                        outcome: OutcomeStatus::Applied,
                    }
                ));
            }
            assert_eq!(events(&restored(&game), observer), disclosed);
        }
    }
}

#[test]
fn restricted_unseen_source_keeps_neutral_personal_feedback_without_hidden_caster_hit() {
    let mut hidden_source = fixture();
    template(&mut hidden_source, ActorId(1), Some("event_invisible"));
    host_source_activity(&mut hidden_source, SourceAbilityDisclosure::Concealed);
    let received = events(&hidden_source, ActorId(2));
    assert!(
        matches!(received.as_slice(), [DisclosedCombatEvent::AnonymousAbility {
        caster_reference, target: Some(ActorId(2)),
        target_reference: Some(ActorReferenceKind::Creature), outcome: OutcomeStatus::Applied, ..
    }] if *caster_reference != Some(ActorReferenceKind::Creature))
    );
    assert!(!received
        .iter()
        .any(|event| matches!(event, DisclosedCombatEvent::UnseenAbility { .. })));
    for ceiling in [
        SourceAbilityDisclosure::InitiatorOnly,
        SourceAbilityDisclosure::Concealed,
    ] {
        let mut hidden_target = fixture();
        template(&mut hidden_target, ActorId(2), Some("event_invisible"));
        host_source_activity(&mut hidden_target, ceiling);
        assert!(events(&hidden_target, ActorId(1)).is_empty());
        assert!(matches!(
            events(&hidden_target, ActorId(2)).as_slice(),
            [DisclosedCombatEvent::AnonymousAbility {
                target: Some(ActorId(2)),
                ..
            }]
        ));
    }
}

#[test]
fn later_identification_reads_restore_and_rollback_never_upgrade_source_history() {
    let mut game = fixture();
    host_source_activity(&mut game, SourceAbilityDisclosure::Concealed);
    let witness = game.event_observation_witness();
    capture(&mut game, ActorId(3), &witness);
    game.seal_event_observations().unwrap();
    let before = [ActorId(1), ActorId(2), ActorId(3)].map(|actor| events(&game, actor));
    for actor in [ActorId(1), ActorId(2), ActorId(3)] {
        game.learn_identity(actor, "fixture.later_identified_source")
            .unwrap();
        let knowledge = game.actors[&actor].recognized_abilities.clone();
        let view = game.observe(actor).unwrap();
        game.record_observation_knowledge(&view).unwrap();
        assert_eq!(game.actors[&actor].recognized_abilities, knowledge);
    }
    for candidate in [&game, &restored(&game)] {
        for (index, actor) in [ActorId(1), ActorId(2), ActorId(3)].into_iter().enumerate() {
            assert_eq!(events(candidate, actor), before[index]);
        }
    }
    let committed = game.clone();
    let result = game.with_recovery_transition(true, |candidate| {
        let origin = candidate
            .capture_event_origin(ActorId(1), ActorId(2))
            .with_ability_disclosure(SourceAbilityDisclosure::InitiatorOnly);
        candidate.apply_resolved_with_disclosure(
            damage(ActorId(1), ActorId(2), 1),
            crate::lifecycle::ResolutionPublication::Primary { strike: 0 },
            &mut crate::lifecycle::DamageActivityGuard::default(),
            origin,
        )?;
        candidate.seal_participant_events()?;
        assert_eq!(events(candidate, ActorId(1)).len(), before[0].len() + 1);
        Err::<(), _>(GameError::IdentityExhausted)
    });
    assert_eq!(result, Err(GameError::IdentityExhausted));
    assert_eq!(game, committed);
}

#[test]
fn source_receipts_reject_missing_ids_substitution_and_ceiling_or_recipient_forgery() {
    use serde_json::json;
    for ceiling in [
        SourceAbilityDisclosure::Public,
        SourceAbilityDisclosure::InitiatorOnly,
        SourceAbilityDisclosure::Concealed,
    ] {
        let mut game = fixture();
        host_source_activity(&mut game, ceiling);
        let witness = game.event_observation_witness();
        capture(&mut game, ActorId(3), &witness);
        game.seal_event_observations().unwrap();
        let good = serde_json::to_value(&game.event_receipts).unwrap();
        for owner in ["1", "2", "3"] {
            let entry = &good["observers"][owner]["entries"][0];
            assert!(entry.get("disclosed_ability").is_some());
            assert_eq!(
                entry["key"]["source_disclosure"],
                serde_json::to_value(ceiling).unwrap()
            );
            let receipt_fields = [
                "index",
                "key",
                "source",
                "target",
                "participant",
                "disclosed_ability",
            ];
            let key_fields = [
                "kind",
                "actor",
                "target",
                "participated",
                "source_disclosure",
            ];
            assert_eq!(entry.as_object().unwrap().len(), receipt_fields.len());
            assert_eq!(entry["key"].as_object().unwrap().len(), key_fields.len());
            for field in receipt_fields {
                let mut missing = good.clone();
                missing["observers"][owner]["entries"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    serde_json::from_value::<EventReceipts>(missing).is_err(),
                    "receipt {field}"
                );
            }
            for field in key_fields {
                let mut missing = good.clone();
                missing["observers"][owner]["entries"][0]["key"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    serde_json::from_value::<EventReceipts>(missing).is_err(),
                    "key {field}"
                );
            }
            for invalid in [
                json!(null),
                json!("publci"),
                json!({"public":null}),
                json!(["public"]),
                json!(7),
            ] {
                let mut malformed = good.clone();
                malformed["observers"][owner]["entries"][0]["key"]["source_disclosure"] = invalid;
                assert!(serde_json::from_value::<EventReceipts>(malformed).is_err());
            }
            for path in ["", "/key"] {
                let mut malformed = good.clone();
                let fields = entry.pointer(path).unwrap().as_object().unwrap();
                *malformed["observers"][owner]["entries"][0]
                    .pointer_mut(path)
                    .unwrap() = json!(fields.values().collect::<Vec<_>>());
                assert!(serde_json::from_value::<EventReceipts>(malformed).is_err());
                let mut surplus = good.clone();
                surplus["observers"][owner]["entries"][0]
                    .pointer_mut(path)
                    .unwrap()["extra"] = json!(true);
                assert!(serde_json::from_value::<EventReceipts>(surplus).is_err());
            }
            let text = serde_json::to_string(entry).unwrap();
            let field = format!("\"disclosed_ability\":{}", entry["disclosed_ability"]);
            let duplicate = text.replacen(&field, &format!("{field},{field}"), 1);
            assert_ne!(duplicate, text);
            assert!(serde_json::from_str::<EventReceipt>(&duplicate).is_err());
            for replacement in [
                json!("spark_lance_fire"),
                if entry["disclosed_ability"].is_null() {
                    json!("basic_melee")
                } else {
                    json!(null)
                },
            ] {
                let mut invalid = good.clone();
                invalid["observers"][owner]["entries"][0]["disclosed_ability"] = replacement;
                let mut corrupt = game.clone();
                corrupt.event_receipts = serde_json::from_value(invalid).unwrap();
                assert!(!corrupt.event_receipts_valid());
                let mut shared = crate::checkpoint::SharedState::default();
                assert!(Game::restore_checkpoint(
                    corrupt.checkpoint(&mut shared),
                    &shared,
                    corrupt.content().clone()
                )
                .is_err());
            }
        }
        if ceiling != SourceAbilityDisclosure::Public {
            let mut forged = good;
            forged["observers"]["2"]["entries"][0]["key"]["source_disclosure"] = json!("public");
            forged["observers"]["2"]["entries"][0]["disclosed_ability"] = json!("basic_melee");
            let mut corrupt = game.clone();
            corrupt.event_receipts = serde_json::from_value(forged).unwrap();
            assert!(
                !corrupt.event_receipts_valid(),
                "a forged receipt cannot relax raw event authority"
            );
        }
    }
}

#[test]
fn raw_source_ceiling_survives_originless_suffix_transfer_without_upgrade() {
    let mut game = fixture();
    game.begin_event_batch().unwrap();
    let origin = game
        .capture_event_origin(ActorId(1), ActorId(2))
        .with_ability_disclosure(SourceAbilityDisclosure::Concealed);
    game.push_event(
        CombatEvent::Resolved {
            outcome: damage(ActorId(1), ActorId(2), 1),
            strike: Some(0),
            source_disclosure: SourceAbilityDisclosure::Public,
        },
        Some(origin),
    )
    .unwrap();
    let (events, _) = game.take_event_suffix(0).unwrap();
    assert!(matches!(
        &events[0],
        CombatEvent::Resolved {
            source_disclosure: SourceAbilityDisclosure::Concealed,
            ..
        }
    ));
    game.append_event_suffix(events, vec![None]).unwrap();
    game.seal_event_observations().unwrap();
    assert!(matches!(
        &game.event_log[0],
        CombatEvent::Resolved {
            source_disclosure: SourceAbilityDisclosure::Concealed,
            ..
        }
    ));
    assert!(game.event_receipts.observers.values().all(|row| row
        .entries
        .iter()
        .all(|entry| entry.disclosed_ability.is_none())));
    assert!(game.event_receipts_valid());
    assert_eq!(restored(&game), game);
}

#[test]
fn typed_checkpoint_rejects_source_grant_substitution_and_initiator_to_spectator_transplant() {
    let mut game = fixture();
    host_source_activity(&mut game, SourceAbilityDisclosure::InitiatorOnly);
    let witness = game.event_observation_witness();
    capture(&mut game, ActorId(3), &witness);
    game.seal_event_observations().unwrap();
    let grant = game.event_receipts.observers[&ActorId(1)].entries[0]
        .disclosed_ability
        .clone();
    assert!(grant.is_some());
    for mutation in [
        "different_grant",
        "different_raw_ability",
        "spectator_grant",
        "no_ability",
    ] {
        let mut corrupt = game.clone();
        match mutation {
            "different_grant" => {
                corrupt
                    .event_receipts
                    .observers
                    .get_mut(&ActorId(1))
                    .unwrap()
                    .entries[0]
                    .disclosed_ability = Some(AbilityId::new("spark_lance_fire").unwrap());
            }
            "different_raw_ability" => {
                let CombatEvent::Resolved { outcome, .. } = &mut corrupt.event_log[0] else {
                    unreachable!()
                };
                outcome.ability = Some(AbilityId::new("spark_lance_fire").unwrap());
            }
            "spectator_grant" => {
                corrupt
                    .event_receipts
                    .observers
                    .get_mut(&ActorId(3))
                    .unwrap()
                    .entries[0]
                    .disclosed_ability = grant.clone();
            }
            _ => {
                let CombatEvent::Resolved { outcome, .. } = &mut corrupt.event_log[0] else {
                    unreachable!()
                };
                outcome.ability = None;
                // Raw authority must be invalid even without any receipt to compare.
                corrupt.event_receipts.observers.clear();
                corrupt.event_receipts.bindings = 0;
            }
        }
        assert!(!corrupt.event_receipts_valid(), "{mutation}");
        let mut shared = crate::checkpoint::SharedState::default();
        assert!(
            Game::restore_checkpoint(
                corrupt.checkpoint(&mut shared),
                &shared,
                corrupt.content().clone()
            )
            .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn shared_event_geometry_preserves_exact_proof_fields_and_committed_receipts() {
    let mut game = fixture();
    host_damage(&mut game, 1);
    let observer = ActorId(3);
    let (shared_view, shared_scene, shared_proof) =
        game.observe_event_shared_scene(observer).unwrap();
    let (owned_view, owned_scene, owned_proof) = game.observe_event_scene(observer).unwrap();
    assert_eq!(shared_view, owned_view);
    assert_eq!(*shared_scene, owned_scene);
    assert!(
        !shared_proof.entries.is_empty(),
        "fixture must exercise real event authority"
    );
    assert_eq!(shared_proof.observer, owned_proof.observer);
    assert_eq!(shared_proof.witness, owned_proof.witness);
    assert_eq!(shared_proof.entries, owned_proof.entries);
    let witness = game.event_observation_witness();
    let mut shared_game = game.clone();
    let mut owned_game = game;
    let mut shared_view = shared_view;
    let mut owned_view = owned_view;
    shared_game
        .record_event_observation(&witness, &shared_proof, &mut shared_view)
        .unwrap();
    owned_game
        .record_event_observation(&witness, &owned_proof, &mut owned_view)
        .unwrap();
    assert_eq!(shared_view, owned_view);
    shared_game.seal_event_observations().unwrap();
    owned_game.seal_event_observations().unwrap();
    assert_eq!(shared_game.event_receipts, owned_game.event_receipts);
    assert_eq!(
        events(&restored(&shared_game), observer),
        events(&restored(&owned_game), observer)
    );
}

#[test]
fn commit_event_facts_match_rich_authority_without_materialization() {
    let mut game = fixture();
    host_damage(&mut game, 1);
    let observer = ActorId(3);
    let witness = game.event_observation_witness();
    let (mut rich, scene, proof) = game.observe_event_shared_scene(observer).unwrap();
    let original = game.clone();
    let before = crate::diagnostics::work_counts().observations;
    let before_vectors = crate::diagnostics::work_counts().actor_reference_vectors;
    let facts = game.capture_commit_event_facts(observer).unwrap();
    assert_eq!(
        crate::diagnostics::work_counts().actor_reference_vectors,
        before_vectors,
        "event authority must not allocate an intermediate reference row vector"
    );
    let materialized = crate::diagnostics::work_counts().observations - before;
    assert_eq!(
        game, original,
        "capturing authority must remain a pure read"
    );
    assert_eq!(facts.observer(), observer);
    assert_eq!(facts.tick(), game.tick());
    assert_eq!(facts.scene(), &scene);
    assert_eq!(facts.proof.observer, proof.observer);
    assert_eq!(facts.proof.witness, proof.witness);
    assert_eq!(facts.proof.entries, proof.entries);
    assert!(
        facts.proof.entries.iter().any(|entry| {
            entry.key.actor == ActorId(1)
                && entry.key.target == ActorId(2)
                && entry.source == Some(ActorReferenceKind::Creature)
                && entry.target == Some(ActorReferenceKind::Creature)
        }),
        "ordinary fixture must grant the known source and target identities"
    );
    let mut lightweight = game.clone();
    let mut oracle = game;
    lightweight
        .record_commit_event_facts(&witness, &facts)
        .unwrap();
    assert_ne!(
        lightweight.event_receipts, original.event_receipts,
        "the nonempty candidate must really extend the observer's historical authority"
    );
    assert!(
        events(&original, observer).is_empty(),
        "a rejected candidate cannot publish receipts"
    );
    oracle
        .record_event_observation(&witness, &proof, &mut rich)
        .unwrap();
    assert_eq!(lightweight.event_receipts, oracle.event_receipts);
    assert!(events(&lightweight, observer).iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Ability {
            caster: Some(ActorId(1)),
            target: Some(ActorId(2)),
            ..
        }
    )));
    lightweight.seal_event_observations().unwrap();
    oracle.seal_event_observations().unwrap();
    assert_eq!(
        events(&restored(&lightweight), observer),
        events(&restored(&oracle), observer)
    );
    assert_eq!(
        materialized, 0,
        "commit event authority must not materialize a rich Observation"
    );
}

#[test]
fn commit_event_facts_keep_hidden_identity_and_reject_stale_or_wrong_owner() {
    let mut hidden = fixture();
    template(&mut hidden, ActorId(1), Some("event_invisible"));
    host_damage(&mut hidden, 1);
    let observer = ActorId(3);
    let witness = hidden.event_observation_witness();
    let (mut rich, scene, proof) = hidden.observe_event_shared_scene(observer).unwrap();
    let facts = hidden.capture_commit_event_facts(observer).unwrap();
    assert_eq!(facts.scene(), &scene);
    assert_eq!(facts.proof.observer, proof.observer);
    assert_eq!(facts.proof.witness, proof.witness);
    assert_eq!(facts.proof.entries, proof.entries);
    assert!(
        facts.proof.entries.is_empty(),
        "a third party must not identify the invisible source"
    );
    let original = hidden.clone();
    let mut candidate = hidden.clone();
    candidate
        .record_commit_event_facts(&witness, &facts)
        .unwrap();
    hidden
        .record_event_observation(&witness, &proof, &mut rich)
        .unwrap();
    assert_eq!(candidate.event_receipts, hidden.event_receipts);
    assert!(events(&candidate, observer).is_empty());
    let victim_sources: Vec<_> = candidate.event_receipts.observers[&ActorId(2)]
        .entries
        .iter()
        .filter(|entry| entry.key.kind == EventKind::Resolved)
        .map(|entry| entry.source)
        .collect();
    assert_eq!(
        victim_sources,
        vec![Some(ActorReferenceKind::Contact)],
        "the adjacent invisible source grants tactile Contact, not identified Creature authority"
    );
    assert!(events(&candidate, ActorId(2)).iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::UnseenAbility {
            caster: Some(ActorId(1)),
            target: ActorId(2),
            ..
        }
    )));
    assert_eq!(
        candidate.event_receipts, original.event_receipts,
        "empty hidden proof must remain a no-op, not manufacture a receipt watermark"
    );
    let retained = original.event_receipts.observers.clone();
    drop(candidate);
    assert!(original.event_receipts.observers.shares_storage(&retained));
    assert!(events(&original, observer).is_empty());

    let mut game = fixture();
    host_damage(&mut game, 1);
    let witness = game.event_observation_witness();
    let facts = game.capture_commit_event_facts(observer).unwrap();
    let receipts = game.event_receipts.clone();
    let mut wrong_owner = facts.clone();
    wrong_owner.observer = ActorId(1);
    assert!(game
        .record_commit_event_facts(&witness, &wrong_owner)
        .is_err());
    assert_eq!(game.event_receipts, receipts);
    let mut elapsed = game.clone();
    elapsed.tick += 1;
    assert!(elapsed.record_commit_event_facts(&witness, &facts).is_err());
    assert_eq!(elapsed.event_receipts, receipts);
    template(&mut game, observer, Some("event_detecting"));
    let current_witness = game.event_observation_witness();
    assert!(game
        .record_commit_event_facts(&current_witness, &facts)
        .is_err());
    assert_eq!(game.event_receipts, receipts);
}

mod boundary_effects_contracts {
    use super::*;
    use crate::ObservationChange;

    fn rich(
        game: &Game,
        observer: ActorId,
    ) -> (
        crate::Observation,
        std::sync::Arc<Vec<tor_world::SightCell>>,
        Vec<crate::detection::DetectionView>,
    ) {
        let (observation, scene) = game.observe_shared_scene(observer).unwrap();
        let charts = [
            crate::chart::Kind::Monsters,
            crate::chart::Kind::Food,
            crate::chart::Kind::Magic,
            crate::chart::Kind::Mapping,
        ]
        .into_iter()
        .map(|kind| game.detection_chart(observer, kind).unwrap())
        .collect();
        (observation, scene, charts)
    }
    fn with_remote() -> (Game, ActorId) {
        let mut game = fixture();
        game.world
            .add_region(tor_world::Region {
                id: RegionId(3),
                name: "isolated".into(),
                bounds: tor_world::Extent::new(6, 3, 1).unwrap(),
            })
            .unwrap();
        let remote = game
            .spawn_actor(
                Location {
                    region: RegionId(3),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        (game, remote)
    }
    #[test]
    fn remote_actor_change_certifies_equal_without_rich_projection() {
        let (before, remote) = with_remote();
        let observer = ActorId(1);
        let mut after = before.clone();
        after.actors.get_mut(&remote).unwrap().location.position.x += 1;
        assert_eq!(
            rich(&before, observer),
            rich(&after, observer),
            "positive cold full-view oracle"
        );
        assert_ne!(
            before.actors[&remote].location,
            after.actors[&remote].location
        );
        let count = crate::diagnostics::work_counts().observations;
        let effects = after.boundary_effects_since(&before);
        assert_eq!(effects.changed_actors().collect::<Vec<_>>(), vec![remote]);
        assert_eq!(
            effects.observation_change(observer).unwrap(),
            ObservationChange::Unchanged
        );
        assert_eq!(
            crate::diagnostics::work_counts().observations - count,
            0,
            "authority must not materialize rich views"
        );
    }
    #[test]
    fn adjacent_and_apparent_subject_changes_take_cold_path() {
        let before = fixture();
        let observer = ActorId(1);
        let subject = ActorId(2);
        assert!(before
            .observe(observer)
            .unwrap()
            .visible_actors
            .iter()
            .any(|row| row.id == subject));
        let mut after = before.clone();
        after.actors.get_mut(&subject).unwrap().asset = Some("changed".into());
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::NeedsProjection
        );
        let mut before = fixture();
        template(&mut before, subject, Some("event_guised"));
        assert!(
            before
                .observe(observer)
                .unwrap()
                .apparent_objects
                .iter()
                .any(|row| row.subject == subject),
            "positive apparent-object authority"
        );
        let mut after = before.clone();
        after.actors.get_mut(&subject).unwrap().location.position.x += 1;
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::NeedsProjection
        );
    }
    #[test]
    fn lighting_and_chart_roots_remain_conservative() {
        let before = fixture();
        let observer = ActorId(1);
        let mut after = before.clone();
        after.set_region_light(RegionId(1), false).unwrap();
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::NeedsProjection
        );
        let mut after = before.clone();
        after
            .chart_history
            .entry(observer)
            .or_default()
            .record_attempt(
                crate::chart_history::Kind::Food,
                crate::chart_history::Attempt {
                    attempted_at: after.tick,
                    status: crate::chart_history::AttemptStatus::Empty,
                },
                None,
            )
            .unwrap();
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::NeedsProjection
        );
    }
    #[test]
    fn tick_origin_and_local_physics_have_exact_or_conservative_witnesses() {
        let before = fixture();
        let observer = ActorId(1);
        let mut after = before.clone();
        after.tick += 1;
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::Changed
        );
        let mut after = before.clone();
        after.actors.get_mut(&observer).unwrap().location.position.x = 0;
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::Changed
        );
        let mut after = before.clone();
        after.physics.displaced.insert(observer);
        assert_ne!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::NeedsProjection
        );
        let mut after = before.clone();
        after.physics.displaced.insert(ActorId(2));
        assert_eq!(rich(&before, observer), rich(&after, observer));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::Unchanged
        );
    }
    #[test]
    fn linked_hold_and_expired_receipt_views_cannot_be_certified_unchanged() {
        let mut species = crate::test_creatures::species();
        let flag = crate::content_ids::FlagId::new("magical").unwrap();
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Flag { flag: flag.clone() },
        });
        let build = crate::test_creatures::build(species);
        let mut before = Game::two_room_in_stone(42, build.content().clone());
        for x in [1, 2] {
            let id = before
                .spawn_actor(at(x, 1), std::num::NonZeroU64::new(100).unwrap())
                .unwrap();
            before
                .configure_creature(
                    id,
                    crate::CreatureIdentity {
                        name: format!("holder {x}"),
                        faction: "neutral".into(),
                    },
                    build.clone(),
                )
                .unwrap();
        }
        let mut after = before.clone();
        let id = after
            .apply_linked_hold(
                ActorId(1),
                ActorId(2),
                crate::linked_hold::HoldPolicy {
                    sustaining: flag,
                    preparation: 100,
                    recovery: 100,
                    protection: 400,
                },
            )
            .unwrap()
            .expect("positive sustaining hold fixture");
        assert_eq!(after.linked_holds(ActorId(2))[0].id, id);
        assert_ne!(rich(&before, ActorId(2)), rich(&after, ActorId(2)));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(ActorId(2))
                .unwrap(),
            ObservationChange::NeedsProjection
        );
        let mut before = fixture();
        host_damage(&mut before, 1);
        let witness = before.event_observation_witness();
        let previous = capture(&mut before, ActorId(3), &witness);
        assert!(
            !previous.combat.unwrap().events.is_empty(),
            "positive third-party receipt"
        );
        let mut after = before.clone();
        after.begin_event_batch().unwrap();
        assert!(after
            .observe(ActorId(3))
            .unwrap()
            .combat
            .unwrap()
            .events
            .is_empty());
        assert_ne!(rich(&before, ActorId(3)), rich(&after, ActorId(3)));
        assert_eq!(
            after
                .boundary_effects_since(&before)
                .observation_change(ActorId(3))
                .unwrap(),
            ObservationChange::NeedsProjection
        );
    }
    #[test]
    fn tick_fast_path_preserves_malformed_projection_rejection() {
        let before = fixture();
        let observer = ActorId(1);
        assert!(before.observe(observer).is_ok(), "valid positive baseline");
        let mut after = before.clone();
        after
            .actors
            .get_mut(&observer)
            .unwrap()
            .anatomy
            .slots
            .push(crate::content_ids::EquipmentSlot::new("missing_fixture_slot").unwrap());
        after.tick += 1;
        assert!(after.observe(observer).is_err());
        assert!(after
            .boundary_effects_since(&before)
            .observation_change(observer)
            .is_err());
    }
    #[test]
    fn frozen_lifecycle_keeps_historical_charts_but_not_display_tick() {
        let mut frozen = fixture();
        let observer = ActorId(1);
        frozen
            .chart_history
            .entry(observer)
            .or_default()
            .record_attempt(
                crate::chart_history::Kind::Food,
                crate::chart_history::Attempt {
                    attempted_at: 0,
                    status: crate::chart_history::AttemptStatus::Empty,
                },
                None,
            )
            .unwrap();
        let mut records = crate::MemoryRecords::default();
        frozen
            .apply_region_transition(
                &crate::RegionTransition {
                    active: [RegionId(2)].into(),
                    loaded: [RegionId(1), RegionId(2)].into(),
                },
                &mut records,
            )
            .unwrap();
        assert!(
            frozen.actor_frozen(observer),
            "positive frozen observer fixture"
        );
        let mut later = frozen.clone();
        later.tick = 50;
        let old = rich(&frozen, observer);
        let current = rich(&later, observer);
        assert_eq!(
            old.2, current.2,
            "historical chart content does not advance with world tick"
        );
        assert_ne!(old.0.tick, current.0.tick);
        assert_eq!(later.actor_clock(observer), frozen.actor_clock(observer));
        assert_eq!(
            later
                .boundary_effects_since(&frozen)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::Changed
        );
        let mut thawed = later.clone();
        thawed
            .apply_region_transition(
                &crate::RegionTransition {
                    active: [RegionId(1), RegionId(2)].into(),
                    loaded: [RegionId(1), RegionId(2)].into(),
                },
                &mut records,
            )
            .unwrap();
        assert!(!thawed.actor_frozen(observer));
        assert_ne!(later.actor_clock(observer), thawed.actor_clock(observer));
        assert_eq!(later.tick, thawed.tick);
        assert_eq!(
            thawed
                .boundary_effects_since(&later)
                .observation_change(observer)
                .unwrap(),
            ObservationChange::NeedsProjection
        );
    }
}

#[test]
fn committed_event_knowledge_matches_rich_commit_without_materialization() {
    let mut game = fixture();
    let observer = ActorId(3);
    let ability = AbilityId::new("spark_lance_fire").unwrap();
    template(&mut game, ActorId(1), Some("event_caster"));
    assert!(!game.actors[&observer]
        .recognized_abilities
        .contains(&ability));
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    let mut outcome = damage(ActorId(1), ActorId(2), 1);
    outcome.ability = Some(ability.clone());
    game.apply_outcome(outcome).unwrap();
    game.seal_participant_events().unwrap();
    game.host_region_continuation = false;
    let witness = game.event_observation_witness();
    let (mut rich, _, proof) = game.observe_event_shared_scene(observer).unwrap();
    let facts = game.capture_commit_event_facts(observer).unwrap();
    game.record_committed_event_knowledge(observer).unwrap();
    assert!(
        !game.actors[&observer]
            .recognized_abilities
            .contains(&ability),
        "pure capture and uncommitted event identity cannot teach an observer"
    );
    let mut rejected = game.clone();
    rejected.tick += 1;
    assert!(rejected
        .record_commit_event_facts(&witness, &facts)
        .and_then(|()| rejected.record_committed_event_knowledge(observer))
        .is_err());
    assert!(!rejected.actors[&observer]
        .recognized_abilities
        .contains(&ability));
    assert_eq!(rejected.event_receipts, game.event_receipts);
    let mut oracle = game.clone();
    oracle
        .record_event_observation(&witness, &proof, &mut rich)
        .unwrap();
    oracle.record_observation_knowledge(&rich).unwrap();
    let before = crate::diagnostics::work_counts().observations;
    game.record_commit_event_facts(&witness, &facts).unwrap();
    game.record_committed_event_knowledge(observer).unwrap();
    assert!(game.actors[&observer]
        .recognized_abilities
        .contains(&ability));
    assert_eq!(game, oracle);
    assert_eq!(
        crate::diagnostics::work_counts().observations,
        before,
        "committing historical knowledge must not construct an Observation"
    );
    let known = game.actors[&observer].recognized_abilities.clone();
    game.record_committed_event_knowledge(observer).unwrap();
    assert!(game.actors[&observer]
        .recognized_abilities
        .shares_storage(&known));
    game.seal_event_observations().unwrap();
    assert!(restored(&game).actors[&observer]
        .recognized_abilities
        .contains(&ability));
}

#[test]
fn committed_event_knowledge_ignores_noncombat_owners_like_ordinary_views() {
    let mut game = fixture();
    let observer = game
        .spawn_actor(at(3, 2), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let ability = AbilityId::new("spark_lance_fire").unwrap();
    template(&mut game, ActorId(1), Some("event_caster"));
    assert!(!game.actors[&observer]
        .recognized_abilities
        .contains(&ability));
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    let mut outcome = damage(ActorId(1), ActorId(2), 1);
    outcome.ability = Some(ability.clone());
    game.apply_outcome(outcome).unwrap();
    game.seal_participant_events().unwrap();
    game.host_region_continuation = false;
    let witness = game.event_observation_witness();
    let facts = game.capture_commit_event_facts(observer).unwrap();
    game.record_commit_event_facts(&witness, &facts).unwrap();
    assert!(
        game.disclosed_event_history(observer)
            .iter()
            .any(|event| matches!(event,
                DisclosedCombatEvent::Ability { id, .. } if id == &ability
            )),
        "fixture must retain a real licensed ability receipt for the plain observer"
    );
    let view = game.observe(observer).unwrap();
    assert!(view.combat.is_none());
    let mut oracle = game.clone();
    oracle.record_observation_knowledge(&view).unwrap();
    game.record_committed_event_knowledge(observer).unwrap();
    assert_eq!(game, oracle);
    assert!(!game.actors[&observer]
        .recognized_abilities
        .contains(&ability));
    assert!(matches!(
        game.record_committed_event_knowledge(ActorId(999)),
        Err(GameError::UnknownActor)
    ));
}

#[test]
fn sealed_observation_scene_preserves_geometry_without_actor_classification() {
    let mut game = fixture();
    game.seal_event_observations().unwrap();
    game.lifecycle.scenes = crate::observation::ActorScenes::default();
    let observer = ActorId(3);
    let retained = game.clone();
    let before = crate::diagnostics::work_counts();
    let scene = game.capture_observation_scene_shared(observer).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.perception_projections, before.perception_projections);
    assert_eq!(after.observations, before.observations);
    assert_eq!(
        scene,
        *game.capture_commit_event_facts(observer).unwrap().scene()
    );
    assert_eq!(game, retained);
    let missing = ActorId(u64::MAX);
    assert_eq!(
        game.capture_observation_scene_shared(missing).unwrap_err(),
        game.capture_commit_event_facts(missing).unwrap_err()
    );
    host_damage(&mut game, 1);
    assert!(game.event_origins.open, "positive unexpected-open fixture");
    assert_eq!(
        game.capture_observation_scene_shared(observer).unwrap(),
        *game.capture_commit_event_facts(observer).unwrap().scene()
    );
}
