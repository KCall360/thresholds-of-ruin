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
