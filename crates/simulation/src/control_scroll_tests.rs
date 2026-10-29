use super::*;

fn condition() -> ConditionId {
    ConditionId::new("fixture.confused").unwrap()
}

#[test]
fn control_specs_reject_invalid_duration_radius_and_extra_fields() {
    let valid = serde_json::json!({"kind":"hold_nearby","condition":"fixture.confused","duration":1000,"radius":2});
    assert!(serde_json::from_value::<ControlSpec>(valid.clone()).is_ok());
    for (field, value) in [("duration", 0), ("radius", 0), ("radius", 9)] {
        let mut bad = valid.clone();
        bad[field] = value.into();
        assert!(serde_json::from_value::<ControlSpec>(bad).is_err());
    }
    let mut bad = valid;
    bad["resist_scare"] = true.into();
    assert!(serde_json::from_value::<ControlSpec>(bad).is_err());
}

#[test]
fn confusion_charge_replaces_instead_of_banking_and_miss_retains_it() {
    let mut state = ControlState::default();
    let payload = ConditionPayload {
        condition: condition(),
        duration: 2000,
    };
    state.arm_confusion(payload.clone());
    state.arm_confusion(payload.clone());
    assert_eq!(state.take_confusion(false), None);
    assert_eq!(state.take_confusion(true), Some(payload));
    assert_eq!(state.take_confusion(true), None);
}

#[test]
fn scare_approach_receipt_survives_serialization_and_only_departure_rearms() {
    let mut state = ControlState::default();
    assert_eq!(
        state.enter_scared(&[ItemId(7)], &[ItemId(7)]).unwrap(),
        vec![ItemId(7)]
    );
    assert!(state
        .enter_scared(&[ItemId(7)], &[ItemId(7)])
        .unwrap()
        .is_empty());
    let encoded = serde_json::to_value(&state).unwrap();
    let mut restored: ControlState = serde_json::from_value(encoded).unwrap();
    assert!(restored
        .enter_scared(&[ItemId(7)], &[ItemId(7)])
        .unwrap()
        .is_empty());
    assert!(restored.enter_scared(&[], &[]).unwrap().is_empty());
    assert_eq!(
        restored.enter_scared(&[ItemId(7)], &[ItemId(7)]).unwrap(),
        vec![ItemId(7)]
    );
}

#[test]
fn control_state_is_strict_and_contact_ids_are_sorted_unique_bounded() {
    let positive = serde_json::to_value(ControlState::default()).unwrap();
    assert!(serde_json::from_value::<ControlState>(positive.clone()).is_ok());
    for field in ["confusion", "held", "flee", "scare_contacts"] {
        let mut value = positive.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ControlState>(value).is_err());
    }
    for ids in [vec![7, 7], vec![7, 6], vec![0], vec![u64::MAX], vec![1; 65]] {
        let mut value = positive.clone();
        value["scare_contacts"] = serde_json::json!(ids
            .into_iter()
            .map(|item| serde_json::json!({"item":item,"attempted":true}))
            .collect::<Vec<_>>());
        assert!(serde_json::from_value::<ControlState>(value).is_err());
    }
}

fn at(x: i32) -> Location {
    Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x, y: 1, z: 0 },
    }
}
fn game() -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    for (id, restrictions, preparation) in [
        ("fixture.scared", vec![], "preserve"),
        ("fixture.held", vec!["voluntary_actions"], "pause"),
        ("fixture.confused", vec![], "preserve"),
        ("fixture.scoped", vec![], "preserve"),
        ("fixture.permanent", vec![], "preserve"),
    ] {
        document.conditions.push(
            serde_json::from_value(serde_json::json!({
                "id":id,"family":id,"name":id,"lifecycle":"timed",
                "modifiers":[],"stacking":{"kind":"keep"},"source_scoped":false,
                "descriptors":[],"capacity":"reject",
                "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                "restrictions":restrictions,"preparation":preparation
            }))
            .unwrap(),
        );
    }
    for condition in &mut document.conditions {
        if condition.id.as_str() == "fixture.scoped" {
            condition.source_scoped = true;
        }
        if condition.id.as_str() == "fixture.permanent" {
            condition.timing = crate::rules::ConditionTiming::UntilRemoved;
        }
    }
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: Default::default(),
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
    for x in [1, 4] {
        let id = game
            .spawn_actor(at(x), std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            id,
            crate::CreatureIdentity {
                name: format!("subject{x}"),
                faction: format!("side{x}"),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.combat
        .hostility
        .insert("side1".into(), ["side4".into()].into());
    game.combat
        .hostility
        .insert("side4".into(), ["side1".into()].into());
    game.configure_ai(
        ActorId(2),
        crate::ai::AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    game
}
fn scare(game: &mut Game) -> ItemId {
    let mut spec = crate::ItemSpec::ordinary("unread page".into());
    spec.class = crate::ItemClass::Scroll;
    spec.stackable = true;
    spec.ground_scare = Some(ConditionPayload {
        condition: ConditionId::new("fixture.scared").unwrap(),
        duration: 1000,
    });
    spec.readable = Some(crate::ReadableSpec {
        control: Some(ControlSpec::Nothing {}),
        descriptors: vec![],
        effects: vec![],
        identify: None,
        discovery: crate::ItemDiscovery::Never,
    });
    game.place_item_stack(80, at(1), None, 1, spec).unwrap()
}

#[test]
fn occupied_scroll_applies_no_roll_flee_snapshot_and_queries_do_not_refresh() {
    let mut game = game();
    scare(&mut game);
    let rng = game.rules_rng;
    game.teleport(ActorId(2), at(2)).unwrap();
    game.reconcile_control_contacts().unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    assert_eq!(
        game.rules_rng, rng,
        "Scare has no resistance or activation roll"
    );
    let stored = serde_json::to_value(&game.actors[&ActorId(2)].control).unwrap();
    assert!(stored["flee"].get("target").is_none());
    assert!(
        stored["flee"].get("actor").is_none(),
        "the forced objective stores location, not occupant identity"
    );
    game.advance_physics(600).unwrap();
    for _ in 0..3 {
        game.observe(ActorId(2)).unwrap();
        game.reconcile_control_contacts().unwrap();
    }
    game.advance_physics(1000).unwrap();
    assert_eq!(
        game.forced_flee_location(ActorId(2)),
        None,
        "standing in range does not refresh the timer"
    );
    game.teleport(ActorId(2), at(4)).unwrap();
    game.teleport(ActorId(2), at(2)).unwrap();
    assert_eq!(
        game.forced_flee_location(ActorId(2)),
        Some(at(1)),
        "a genuine return is a new approach"
    );
}

#[test]
fn removing_ground_scroll_does_not_cancel_earned_flee_or_track_occupant() {
    let mut game = game();
    let item = scare(&mut game);
    game.teleport(ActorId(2), at(2)).unwrap();
    game.reconcile_control_contacts().unwrap();
    game.items.remove(&item);
    game.teleport(ActorId(1), at(3)).unwrap();
    game.reconcile_control_contacts().unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    game.advance_physics(1000).unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), None);
}

#[test]
fn ordinary_actor_has_no_forced_ai_lock_and_nothing_read_still_owns_one_paid_unit() {
    let mut game = game();
    let item = scare(&mut game);
    let ordinary = game
        .spawn_actor(at(3), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.reconcile_control_contacts().unwrap();
    assert_eq!(game.forced_flee_location(ordinary), None);
    // A readable's frozen control mode is checked against the exact held item.
    game.items
        .edit(item, |item| {
            item.location = ItemLocation::Carried(ActorId(1))
        })
        .unwrap();
    let plan = game
        .plan_work(ActorId(1), crate::Work::Read { item })
        .unwrap();
    assert!(plan.preparation() > 0);
    assert!(game.item_allocation(&plan).is_ok());
    let mut forged = plan;
    if let crate::abilities::Plan::Read { control, .. } = &mut forged {
        *control = Some(ControlSpec::Aggravate {});
    }
    assert!(
        game.item_allocation(&forged).is_err(),
        "a read cannot substitute a different control payload"
    );
}

#[test]
fn scare_timer_and_approach_survive_checkpoint_and_inactive_floor_time() {
    let mut game = game();
    scare(&mut game);
    game.teleport(ActorId(2), at(2)).unwrap();
    game.reconcile_control_contacts().unwrap();
    // This timer fixture advances physics without executing scheduler actions;
    // establish the matching authored ready boundary before saving it.
    for actor in [ActorId(1), ActorId(2)] {
        game.actors.get_mut(&actor).unwrap().ready_at = 300;
    }
    game.advance_physics(300).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut game = Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    game.reconcile_control_contacts().unwrap();
    let mut records = crate::streaming::MemoryRecords::default();
    let regions = [tor_world::RegionId(1), tor_world::RegionId(2)].into();
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: Default::default(),
            loaded: regions,
        },
        &mut records,
    )
    .unwrap();
    game.advance_physics(3000).unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: [tor_world::RegionId(1)].into(),
            loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
        },
        &mut records,
    )
    .unwrap();
    game.advance_physics(3699).unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    game.advance_physics(3700).unwrap();
    game.reconcile_control_contacts().unwrap();
    assert_eq!(
        game.forced_flee_location(ActorId(2)),
        None,
        "restore or reactivation cannot rearm an unbroken approach"
    );
}

#[test]
fn hold_release_uses_the_shared_condition_and_preserves_unrelated_conditions() {
    let mut game = game();
    let held = ConditionPayload {
        condition: ConditionId::new("fixture.held").unwrap(),
        duration: 1000,
    };
    assert!(game
        .apply_control_condition(ActorId(1), ActorId(2), &held)
        .unwrap());
    let owned = game.owned_control_condition(ActorId(2), &held.condition);
    game.actors.get_mut(&ActorId(2)).unwrap().control.held = owned;
    game.apply_control_condition(
        ActorId(1),
        ActorId(2),
        &ConditionPayload {
            condition: condition(),
            duration: 2000,
        },
    )
    .unwrap();
    assert!(!game.condition_allows_voluntary_input(ActorId(2)));
    game.release_control_hold(ActorId(2)).unwrap();
    assert!(game.condition_allows_voluntary_input(ActorId(2)));
    assert!(game
        .creature(ActorId(2))
        .unwrap()
        .conditions()
        .has_active(&condition()));
    let before = game.clone();
    game.release_control_hold(ActorId(2)).unwrap();
    assert_eq!(game, before, "a later rider or hook cannot release twice");
}

#[test]
fn reading_scare_wastes_one_unit_through_the_ordinary_paid_read_path() {
    let mut game = game();
    let item = scare(&mut game);
    game.items
        .edit(item, |item| {
            item.location = ItemLocation::Carried(ActorId(1));
            item.quantity = 2;
        })
        .unwrap();
    game.combat.ai.clear();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 10_000;
    let issued = game
        .admit_intention(
            ActorId(1),
            crate::Action::Read { item },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, issued);
    done.outcome.unwrap();
    assert_eq!(
        game.items[&item].quantity, 1,
        "the wasted read still pays one scroll"
    );
    assert!(game.tick > 0, "the wasted read is not a free command");
    assert_eq!(game.actors[&ActorId(1)].control, ControlState::default());
}

#[test]
fn first_occupancy_can_trigger_but_swaps_and_reoccupancy_cannot_repeat_approach() {
    let mut state = ControlState::default();
    assert!(state.enter_scared(&[ItemId(7)], &[]).unwrap().is_empty());
    assert_eq!(
        state.enter_scared(&[ItemId(7)], &[ItemId(7)]).unwrap(),
        vec![ItemId(7)]
    );
    assert!(state.enter_scared(&[ItemId(7)], &[]).unwrap().is_empty());
    assert!(state
        .enter_scared(&[ItemId(7)], &[ItemId(7)])
        .unwrap()
        .is_empty());
    state.enter_scared(&[], &[]).unwrap();
    assert_eq!(
        state.enter_scared(&[ItemId(7)], &[ItemId(7)]).unwrap(),
        vec![ItemId(7)]
    );
}

#[test]
fn old_control_ownership_cannot_reanimate_after_condition_removal_or_expiry() {
    let mut game = game();
    scare(&mut game);
    game.teleport(ActorId(2), at(2)).unwrap();
    game.reconcile_control_contacts().unwrap();
    let scared = ConditionId::new("fixture.scared").unwrap();
    let stale = game.actors[&ActorId(2)].control.clone();
    game.apply_effects(
        ActorId(2),
        &[crate::rules::Effect::RemoveCondition {
            condition: scared.clone(),
        }],
    )
    .unwrap();
    game.apply_control_condition(
        ActorId(1),
        ActorId(2),
        &ConditionPayload {
            condition: scared,
            duration: 1000,
        },
    )
    .unwrap();
    assert_eq!(
        game.forced_flee_location(ActorId(2)),
        None,
        "unrelated new generation cannot inherit an old location"
    );
    // Even a stale in-memory copy cannot mistake the later condition generation.
    game.actors.get_mut(&ActorId(2)).unwrap().control = stale;
    assert_eq!(game.forced_flee_location(ActorId(2)), None);
    let mut settled = game.actor_outcome(ActorId(2)).unwrap();
    settled.state.control.reconcile_conditions(
        settled
            .state
            .combat
            .as_ref()
            .map(|combat| combat.creature().conditions()),
    );
    game.apply_outcome(settled).unwrap();
    let held = ConditionPayload {
        condition: ConditionId::new("fixture.held").unwrap(),
        duration: 1000,
    };
    game.apply_control_condition(ActorId(1), ActorId(2), &held)
        .unwrap();
    let owned = game.owned_control_condition(ActorId(2), &held.condition);
    game.actors.get_mut(&ActorId(2)).unwrap().control.held = owned;
    game.advance_physics(1000).unwrap();
    game.apply_control_condition(ActorId(1), ActorId(2), &held)
        .unwrap();
    game.release_control_hold(ActorId(2)).unwrap();
    assert!(
        !game.condition_allows_voluntary_input(ActorId(2)),
        "later unrelated Hold survives the old release hook"
    );
}

#[test]
fn control_payload_rejects_source_scoped_condition_ownership() {
    let fixture = game();
    let mut document = fixture.content.rules().document().clone();
    let payload = ConditionPayload {
        condition: ConditionId::new("fixture.scared").unwrap(),
        duration: 1000,
    };
    assert!(payload.valid(fixture.content.rules()));
    document
        .conditions
        .iter_mut()
        .find(|condition| condition.id == payload.condition)
        .unwrap()
        .source_scoped = true;
    let rules = crate::rules::Ruleset::validate(document).unwrap();
    assert!(!payload.valid(&rules));
}

#[test]
fn first_control_condition_generation_zero_survives_json_and_checkpoint_bytes() {
    for held in [false, true] {
        let mut game = game();
        let payload = ConditionPayload {
            condition: ConditionId::new(if held {
                "fixture.held"
            } else {
                "fixture.scared"
            })
            .unwrap(),
            duration: 1000,
        };
        assert!(game
            .apply_control_condition(ActorId(1), ActorId(2), &payload)
            .unwrap());
        let owned = game
            .owned_control_condition(ActorId(2), &payload.condition)
            .unwrap();
        assert_eq!(owned.generation, 0, "first ordinary condition lifetime");
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .conditions()
                .next_generation(),
            1
        );
        let mut actor = game.actors.get_mut(&ActorId(2)).unwrap();
        if held {
            actor.control.held = Some(owned);
        } else {
            actor.control.flee = Some(FleeSource {
                owned,
                location: at(1),
            });
        }
        drop(actor);
        let control = game.actors[&ActorId(2)].control.clone();
        let encoded = serde_json::to_value(&control).unwrap();
        assert_eq!(
            serde_json::from_value::<ControlState>(encoded.clone()).unwrap(),
            control
        );
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot =
            serde_json::from_slice(&serde_json::to_vec(&game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        assert_eq!(restored.actors[&ActorId(2)].control, control);

        let mut exhausted = encoded.clone();
        let owned = if held {
            &mut exhausted["held"]
        } else {
            &mut exhausted["flee"]["owned"]
        };
        owned["generation"] = serde_json::json!(u64::MAX);
        assert!(serde_json::from_value::<ControlState>(exhausted).is_err());

        let mut forged = encoded;
        let owned = if held {
            &mut forged["held"]
        } else {
            &mut forged["flee"]["owned"]
        };
        owned["generation"] = serde_json::json!(1);
        game.actors.get_mut(&ActorId(2)).unwrap().control = serde_json::from_value(forged).unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot =
            serde_json::from_slice(&serde_json::to_vec(&game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
        assert!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
            "an unallocated generation cannot claim the existing condition"
        );
    }
}

#[test]
fn stored_control_ownership_rejects_permanent_or_source_scoped_conditions() {
    let mut canonical = game();
    scare(&mut canonical);
    canonical.teleport(ActorId(2), at(2)).unwrap();
    canonical.reconcile_control_contacts().unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = canonical.checkpoint(&mut shared);
    assert!(Game::restore_checkpoint(snapshot, &shared, canonical.content().clone()).is_ok());
    for name in ["fixture.permanent", "fixture.scoped"] {
        for held in [false, true] {
            let mut invalid = game();
            let condition = ConditionId::new(name).unwrap();
            if name.ends_with("permanent") {
                invalid
                    .apply_persistent_condition(ActorId(2), &condition, ActorId(1))
                    .unwrap();
            } else {
                invalid
                    .apply_condition(ActorId(2), &condition, ActorId(1), 1000)
                    .unwrap();
            }
            let instance = invalid
                .creature(ActorId(2))
                .unwrap()
                .conditions()
                .instances()
                .find(|entry| entry.condition == condition)
                .unwrap()
                .clone();
            let owned = OwnedCondition {
                condition,
                generation: instance.generation,
            };
            if held {
                invalid.actors.get_mut(&ActorId(2)).unwrap().control.held = Some(owned);
            } else {
                invalid.actors.get_mut(&ActorId(2)).unwrap().control.flee = Some(FleeSource {
                    owned,
                    location: at(1),
                });
            }
            let mut shared = crate::checkpoint::SharedState::default();
            let snapshot = invalid.checkpoint(&mut shared);
            assert!(
                Game::restore_checkpoint(snapshot, &shared, invalid.content().clone()).is_err(),
                "{name}/{held}"
            );
        }
    }
}

#[test]
fn horizontal_body_receives_one_hold_check_and_one_token_consumption() {
    let mut base = game();
    base.teleport(ActorId(2), at(3)).unwrap();
    base.set_body(
        ActorId(2),
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    assert_eq!(
        base.observe(ActorId(1))
            .unwrap()
            .visible_actors
            .iter()
            .filter(|actor| actor.id == ActorId(2))
            .count(),
        2,
        "both horizontal body cells must qualify for the old duplicate path"
    );
    let token = ConditionId::new("distracted").unwrap();
    base.apply_condition(ActorId(1), &token, ActorId(1), 1000)
        .unwrap();
    assert_eq!(
        base.creature(ActorId(1))
            .unwrap()
            .conditions()
            .qualifying_tokens(base.content.rules(), ActorId(1), false)
            .len(),
        1
    );
    base.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = Some(crate::behavior::Engagement {
        target: ActorId(1),
        reference: crate::perception::ActorReferenceKind::Creature,
        location: at(1),
        last_seen: base.tick,
    });
    let spec = ControlSpec::HoldNearby {
        condition: ConditionId::new("fixture.held").unwrap(),
        duration: 1000,
        radius: 2,
    };
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..128 {
        // The independent target-cardinality oracle invokes the shared check
        // kernel exactly once, without the area/visible-occurrence collector.
        let mut expected = base.clone();
        expected.rules_rng = seed;
        let landed = expected.control_lore_check(ActorId(1), ActorId(2)).unwrap();
        assert_ne!(expected.rules_rng, seed);
        assert!(!expected
            .creature(ActorId(1))
            .unwrap()
            .conditions()
            .has_active(&token));
        let mut actual = base.clone();
        actual.rules_rng = seed;
        assert_eq!(
            actual.resolve_control_read(ActorId(1), &spec).unwrap(),
            landed
        );
        assert_eq!(
            actual.rules_rng, expected.rules_rng,
            "one Lore roll per creature"
        );
        assert_eq!(
            actual.creature(ActorId(1)).unwrap().conditions(),
            expected.creature(ActorId(1)).unwrap().conditions(),
            "the one paid check consumes the captured next-check source"
        );
        assert_eq!(!actual.condition_allows_voluntary_input(ActorId(2)), landed);
        outcomes.insert(landed);
    }
    assert_eq!(
        outcomes,
        [false, true].into(),
        "resistance cannot retry another body cell"
    );
}

#[test]
fn tall_eye_hold_uses_body_plane_and_has_landed_and_resisted_outcomes() {
    let mut base = game();
    base.teleport(ActorId(2), at(3)).unwrap();
    for actor in [ActorId(1), ActorId(2)] {
        base.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
    }
    base.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .behavior
        .engagement = Some(crate::behavior::Engagement {
        target: ActorId(1),
        reference: crate::perception::ActorReferenceKind::Creature,
        location: at(1),
        last_seen: 0,
    });
    let spec = ControlSpec::HoldNearby {
        condition: ConditionId::new("fixture.held").unwrap(),
        duration: 1000,
        radius: 2,
    };
    let mut outcomes = std::collections::BTreeSet::new();
    for seed in 0..128 {
        let mut trial = base.clone();
        trial.rules_rng = seed;
        let before = trial.rules_rng;
        let landed = trial.resolve_control_read(ActorId(1), &spec).unwrap();
        assert_ne!(
            trial.rules_rng, before,
            "the ordinary tall target must actually receive the Lore test"
        );
        assert_eq!(!trial.condition_allows_voluntary_input(ActorId(2)), landed);
        outcomes.insert(landed);
    }
    assert_eq!(outcomes, [false, true].into());
}

#[test]
fn committed_unavailable_attack_releases_hold_but_retains_confusion_charge() {
    let mut game = game();
    let held = ConditionPayload {
        condition: ConditionId::new("fixture.held").unwrap(),
        duration: 1000,
    };
    game.apply_control_condition(ActorId(1), ActorId(2), &held)
        .unwrap();
    let owned = game.owned_control_condition(ActorId(2), &held.condition);
    game.actors.get_mut(&ActorId(2)).unwrap().control.held = owned;
    let charge = ConditionPayload {
        condition: condition(),
        duration: 2000,
    };
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .control
        .arm_confusion(charge.clone());
    let outcome = crate::abilities::Outcome {
        actor: ActorId(1),
        target: ActorId(2),
        ability: Some(crate::content_ids::AbilityId::new("basic_melee").unwrap()),
        intention: None,
        result: crate::abilities::OutcomeStatus::Miss,
        check: None,
        resistance: None,
        participation: crate::abilities::AttemptedParticipation::blocked(
            None,
            crate::abilities::UnselectedReason::UnavailableTarget,
        ),
        target_rider_participated: false,
        effects: vec![],
    };
    game.control_on_attack(&outcome).unwrap();
    assert!(game.condition_allows_voluntary_input(ActorId(2)));
    assert!(game.actors[&ActorId(2)].control.held.is_none());
    assert_eq!(game.actors[&ActorId(1)].control.confusion, Some(charge));
}

#[test]
fn no_ground_scare_avoids_neighborhood_work_and_shared_actor_writes() {
    // A nonempty store is not sufficient: carried scare scrolls and ordinary
    // ground items do not supply ground-scare contacts.
    for fixture in 0..3 {
        let mut game = game();
        match fixture {
            0 => {} // Ordinary starting gear remains; absence is checked below.
            1 => {
                let item = scare(&mut game);
                game.items
                    .edit(item, |item| {
                        item.location = ItemLocation::Carried(ActorId(1));
                    })
                    .unwrap();
                assert!(game.items[&item].spec.ground_scare.is_some());
            }
            2 => {
                game.place_item_stack(
                    81,
                    at(1),
                    None,
                    1,
                    crate::ItemSpec::ordinary("ordinary stone".into()),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(game.alive(ActorId(2)));
        assert!(!game.actor_frozen(ActorId(2)));
        assert!(game.combat.ai.contains_key(&ActorId(2)));
        assert!(game.actors[&ActorId(2)].control.scare_contacts.is_empty());
        assert!(!game
            .items
            .values()
            .any(|item| matches!(item.location, ItemLocation::Ground(_))
                && item.spec.ground_scare.is_some()));
        let expected = game.clone(); // Deliberately retain a shared actor table.
        let rng = game.rules_rng;
        let before = crate::diagnostics::work_counts();
        game.reconcile_control_contacts().unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            game, expected,
            "absent-feature reconciliation changes no authoritative state"
        );
        assert_eq!(game.rules_rng, rng);
        assert_eq!(
            after.control_contact_neighbor_probes - before.control_contact_neighbor_probes,
            0
        );
        assert_eq!(after.actor_table_copies - before.actor_table_copies, 0);
    }
}

#[test]
fn absent_ground_scare_clears_only_eligible_receipts_and_preserves_earned_flee() {
    let mut game = game();
    let item = scare(&mut game);
    game.teleport(ActorId(2), at(2)).unwrap();
    game.reconcile_control_contacts().unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    assert_eq!(game.actors[&ActorId(2)].control.scare_contacts.len(), 1);
    assert!(game.actors[&ActorId(2)].control.scare_contacts[0].attempted);
    // Ordinary actors are excluded by the existing eligibility gate; their
    // historical receipts must not be silently cleared by the absent path.
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .control
        .enter_scared(&[item], &[])
        .unwrap();
    game.items.remove(&item).unwrap();
    let mut expected = game.clone();
    expected
        .actors
        .get_mut(&ActorId(2))
        .unwrap()
        .control
        .scare_contacts
        .clear();
    let rng = game.rules_rng;
    let before = crate::diagnostics::work_counts();
    game.reconcile_control_contacts().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(game, expected);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    assert_eq!(
        after.control_contact_neighbor_probes - before.control_contact_neighbor_probes,
        0
    );
}

#[test]
fn unchanged_present_scare_contact_does_not_copy_shared_actors() {
    let mut game = game();
    scare(&mut game);
    game.teleport(ActorId(2), at(2)).unwrap();
    game.reconcile_control_contacts().unwrap();
    assert_eq!(game.forced_flee_location(ActorId(2)), Some(at(1)));
    assert_eq!(game.actors[&ActorId(2)].control.scare_contacts.len(), 1);
    let expected = game.clone();
    let rng = game.rules_rng;
    let before = crate::diagnostics::work_counts();
    game.reconcile_control_contacts().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(game, expected);
    assert_eq!(game.rules_rng, rng);
    assert!(
        after.control_contact_neighbor_probes > before.control_contact_neighbor_probes,
        "present-feature path must still inspect current contacts"
    );
    assert_eq!(after.actor_table_copies - before.actor_table_copies, 0);
}

#[test]
fn absent_ground_scare_still_rejects_unresolvable_recipient_body() {
    let mut game = game();
    let actor = &game.actors[&ActorId(2)];
    assert!(game
        .body_cells(actor.location, actor.orientation, &actor.body)
        .is_some());
    // Fault injection after a valid positive fixture: a multi-cell body in an
    // absent region cannot resolve its neighboring component.
    {
        let mut actor = game.actors.get_mut(&ActorId(2)).unwrap();
        actor.location.region = tor_world::RegionId(999);
        actor.body.cells = vec![[0, 0, 0], [1, 0, 0]];
    }
    let actor = &game.actors[&ActorId(2)];
    assert!(game
        .body_cells(actor.location, actor.orientation, &actor.body)
        .is_none());
    assert!(!game
        .items
        .values()
        .any(|item| matches!(item.location, ItemLocation::Ground(_))
            && item.spec.ground_scare.is_some()));
    let expected = game.clone();
    assert_eq!(
        game.reconcile_control_contacts(),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, expected);
}
