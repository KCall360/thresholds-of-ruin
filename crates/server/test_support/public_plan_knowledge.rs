//! Committed knowledge is saved authority, while observation queries remain pure.
use super::*;
use crate::public_plan_test_fixture as fixture;

fn act(engine: &mut Engine, actor: ActorId, action: Action) {
    engine
        .command(
            "player",
            "test",
            actor,
            &new_id().0,
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(actor).unwrap(),
                action,
            },
        )
        .unwrap();
}
fn knows(game: &Game, actor: ActorId, id: &str) -> bool {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let value = serde_json::to_value((snapshot, shared)).unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    value[1]["actors"][actors][actor.0.to_string()]["recognized_abilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|ability| ability == id)
}
fn before_wait_completion_with(known: bool) -> Engine {
    let mut engine = Engine::memory(fixture::scenario(known)).unwrap();
    act(&mut engine, ActorId(1), Action::Wait);
    act(
        &mut engine,
        ActorId(2),
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap(),
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: SimActor(1),
                },
                weapon: crate::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    );
    for _ in 0..4 {
        if engine.game.tick() >= 200 {
            break;
        }
        assert_eq!(engine.next_actor(), Some(ActorId(1)));
        act(&mut engine, ActorId(1), Action::Wait);
    }
    assert_eq!(engine.game.tick(), 200);
    assert_eq!(engine.next_actor(), Some(ActorId(1)));
    assert_eq!(knows(&engine.game, ActorId(1), "slow_plan"), known);
    engine
}
#[test]
fn public_plan_committed_wait_records_only_already_disclosed_ability_knowledge() {
    let mut engine = before_wait_completion_with(false);
    act(&mut engine, ActorId(1), Action::Wait);
    let state = engine.state(ActorId(1)).unwrap();
    assert_eq!(state.observation.tick, 300);
    assert!(
        state
            .observation
            .combat
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(event,
        tor_protocol::CombatEventView::Ability { ability, .. } if ability.id == "slow_plan")),
        "the ordinary committed view already licenses this identity"
    );
    assert!(
        knows(&engine.game, ActorId(1), "slow_plan"),
        "a committed Wait must save exactly the names it publicly disclosed"
    );
}
#[test]
fn public_plan_uncommitted_candidate_views_and_live_queries_do_not_learn() {
    let engine = before_wait_completion_with(false);
    let mut candidate = Candidate::capture(&engine);
    candidate
        .game
        .act(SimActor(1), tor_simulation::Action::Wait)
        .unwrap();
    let candidate_before = candidate.game.clone();
    let live_before = engine.game.clone();
    for _ in 0..3 {
        let view = candidate.revision_view(ActorId(1)).unwrap();
        assert!(view.observation.combat.unwrap().events.iter().any(|event| matches!(event,
            tor_simulation::combat::DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "slow_plan")));
        engine.state(ActorId(1)).unwrap();
    }
    assert!(!knows(&candidate.game, ActorId(1), "slow_plan"));
    assert_eq!(candidate.game, candidate_before);
    assert_eq!(engine.game, live_before);
}
#[test]
fn public_plan_fixture_separates_harmless_knowledge_from_hostile_growth() {
    let scenario = fixture::growth_scenario();
    let rules = scenario.package.unwrap().rules().unwrap();
    for (id, interaction) in [
        ("slow_plan", tor_simulation::rules::Interaction::NonHostile),
        (
            "growth_finisher",
            tor_simulation::rules::Interaction::HostileToTarget,
        ),
    ] {
        assert_eq!(
            rules
                .ability(&tor_simulation::content_ids::AbilityId::new(id).unwrap())
                .unwrap()
                .interaction,
            interaction,
            "the fixture must declare the intended interaction of {id}"
        );
    }
}
#[test]
fn public_plan_already_known_completion_adds_no_observer_projection_work() {
    let mut engine = before_wait_completion_with(true);
    let actor = ActorId(1);
    let (_, profile) = engine
        .command_profiled(
            "player",
            "test",
            actor,
            "known-completion",
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(actor).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    assert_eq!(engine.game.tick(), 300);
    assert!(knows(&engine.game, actor, "slow_plan"));
    assert_eq!(
        profile.actors_observed, 0,
        "known events need no knowledge projections"
    );
    assert_eq!(
        profile.perception_calls, 0,
        "no hidden observe pass for an already licensed identity"
    );
}

#[test]
fn public_plan_final_candidate_reuses_disclosed_views_and_keeps_the_live_game_unchanged() {
    let engine = before_wait_completion_with(false);
    let live = engine.game.clone();
    let mut candidate = Candidate::capture(&engine);
    candidate
        .game
        .act(SimActor(1), tor_simulation::Action::Wait)
        .unwrap();
    let view = Arc::new(candidate.revision_view(ActorId(1)).unwrap());
    candidate.observations.insert(ActorId(1), view.clone());
    let mut profile = CommandProfile::default();
    candidate
        .record_observation_knowledge(Some(&mut profile))
        .unwrap();
    assert!(knows(&candidate.game, ActorId(1), "slow_plan"));
    assert!(Arc::ptr_eq(&view, &candidate.observations[&ActorId(1)]));
    assert_eq!(
        profile.actors_observed, 0,
        "the final cached view is the authority"
    );
    assert_eq!(engine.game, live);
    let once = candidate.game.clone();
    candidate
        .record_observation_knowledge(Some(&mut profile))
        .unwrap();
    assert_eq!(candidate.game, once);
    assert_eq!(profile.actors_observed, 0);
}

#[test]
fn public_plan_failed_storage_admission_discards_wait_knowledge_and_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let mut engine = before_wait_completion_with(false)
        .attach_profile_save_with_policy(
            directory.path().join("rejected-observation.db"),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
    let before = engine.game.clone();
    let revision = engine.causal_boundary(ActorId(1)).unwrap();
    let counts = engine.profile_counts();
    let receipts = engine.receipts.len();
    for _ in 0..2 {
        let error = engine
            .command(
                "player",
                "test",
                ActorId(1),
                "rejected-knowledge",
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: revision,
                    action: Action::Wait,
                },
            )
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StorageFailure);
        assert_eq!(engine.game, before);
        assert!(!knows(&engine.game, ActorId(1), "slow_plan"));
        assert_eq!(engine.causal_boundary(ActorId(1)).unwrap(), revision);
        assert_eq!(engine.profile_counts(), counts);
        assert_eq!(engine.receipts.len(), receipts);
    }
}
// These exercise real streaming and commit boundaries, not a process transport.
fn streaming_observer_scenario() -> Scenario {
    let mut scenario = fixture::scenario(false);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.ai_profiles.insert(
        "witness".into(),
        serde_json::from_value(serde_json::json!({
            "idle": "wait", "care": true, "equipment": true, "loot": true, "flee_percent":0
        }))
        .unwrap(),
    );
    let mut regions = package.region_defs().unwrap();
    let mut gallery = regions[1].clone();
    gallery.id = 3;
    gallery.name = "Observer gallery".into();
    gallery.items.clear();
    let mut observer = serde_json::to_value(&regions[0].actors[0]).unwrap();
    observer["id"] = serde_json::json!(3);
    observer["at"] = serde_json::json!([2, 1, 0]);
    // External controllers get explicit observing reference points at startup.
    // A neutral AI can instead remain genuinely unbuilt until acquired.
    observer["controller"] = serde_json::json!("ai");
    observer["ai"] = serde_json::json!("witness");
    observer["creature"]["name"] = serde_json::json!("witness");
    observer["creature"]["templates"] = serde_json::json!([]);
    gallery.actors = vec![serde_json::from_value(observer).unwrap()];
    // Its incoming-only view reaches entry chamber1; region1's own portal
    // still leads to gallery2, so it cannot disclose or load gallery3 early.
    let mut inert = gallery.clone();
    inert.id = 4;
    inert.name = "Inert acquisition".into();
    inert.actors.clear();
    inert.portals.clear();
    regions.extend([gallery, inert]);
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(manifest, regions, &package.rules_source)
            .unwrap(),
    ));
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}
fn before_streaming_completion() -> Engine {
    let mut engine = Engine::memory(streaming_observer_scenario()).unwrap();
    assert!(
        !engine.game.has_actor(SimActor(3)),
        "the final acquisition must genuinely add its observer"
    );
    // Lazy streaming has not declared this disconnected frontier at all yet.
    assert_eq!(engine.game.region_state(tor_world::RegionId(3)), None);
    act(&mut engine, ActorId(1), Action::Wait);
    act(
        &mut engine,
        ActorId(2),
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap(),
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: SimActor(1),
                },
                weapon: crate::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    );
    for _ in 0..4 {
        if engine.game.tick() >= 200 {
            break;
        }
        assert_eq!(engine.next_actor(), Some(ActorId(1)));
        act(&mut engine, ActorId(1), Action::Wait);
    }
    assert_eq!(engine.game.tick(), 200);
    engine
}
fn uncommitted_streaming_completion(host: bool) -> (Engine, Candidate) {
    let engine = before_streaming_completion();
    let mut candidate = Candidate::capture(&engine);
    if host {
        assert!(matches!(
            candidate
                .game
                .act_for_host(SimActor(1), tor_simulation::Action::Wait)
                .unwrap(),
            CandidateStep::Settled(_)
        ));
    } else {
        candidate
            .game
            .act(SimActor(1), tor_simulation::Action::Wait)
            .unwrap();
    }
    assert_eq!(candidate.game.tick(), 300);
    assert_slow_plan_completion(&candidate.game);
    (engine, candidate)
}
fn assert_slow_plan_completion(game: &Game) {
    assert_eq!(
        game.combat_events()
            .iter()
            .filter(|event| matches!(event,
                tor_simulation::combat::CombatEvent::Resolved { outcome, strike: Some(0), .. }
                    if outcome.actor == SimActor(2) && outcome.target == SimActor(1)
                        && outcome.ability.as_ref().is_some_and(|id| id.as_str() == "slow_plan")
            ))
            .count(),
        1,
        "the exact ordinary completion must have executed once"
    );
}
fn host_completion_with_existing_observer() -> (Engine, Candidate, ActorId) {
    let mut engine = before_streaming_completion();
    let mut candidate = Candidate::capture(&engine);
    candidate
        .transition_with(
            engine.regions.as_mut(),
            BTreeSet::from([tor_world::RegionId(3)]),
            None,
        )
        .unwrap();
    assert!(candidate.game.has_actor(SimActor(3)));
    assert!(!knows(&candidate.game, ActorId(3), "slow_plan"));
    let mut last_actor = None;
    for _ in 0..4 {
        if candidate.game.tick() >= 300 {
            break;
        }
        let actor = candidate.game.next_actor().unwrap();
        assert!(matches!(actor, SimActor(1) | SimActor(3)));
        assert!(matches!(
            candidate
                .game
                .act_for_host(actor, tor_simulation::Action::Wait)
                .unwrap(),
            CandidateStep::Settled(_)
        ));
        last_actor = Some(ActorId(actor.0));
        candidate.observations.clear();
    }
    assert_eq!(candidate.game.tick(), 300);
    assert_slow_plan_completion(&candidate.game);
    let view = candidate.revision_view(ActorId(3)).unwrap();
    assert!(
        candidate
            .game
            .event_observation_pending(&cold_event_proof(&candidate.game, ActorId(3))),
        "the existing observer must retain a valid event-time proof"
    );
    assert!(
        !discloses_slow_plan(&view),
        "free projections cannot record receipts"
    );
    (engine, candidate, last_actor.unwrap())
}
// The oracle captures the exact current event-time receipt proof independently
// of the runtime presentation cache; that cache need not retain unused proofs.
fn cold_event_proof(game: &Game, actor: ActorId) -> tor_simulation::EventObservation {
    game.observe_event_shared_scene(SimActor(actor.0))
        .unwrap()
        .2
}
fn discloses_slow_plan(view: &CachedObservation) -> bool {
    view.observation.combat.as_ref().is_some_and(|combat| combat.events.iter().any(|event| matches!(event,
        tor_simulation::combat::DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "slow_plan")))
}
fn commit_streaming_wait(engine: &mut Engine, candidate: Candidate, actor: ActorId) {
    let entry = JournalEntry {
        intention_suspensions: vec![],
        intention_ends: vec![],
        id: new_id(),
        branch: candidate.branch().clone(),
        actor,
        tick: engine.game.tick(),
        author: Author::User {
            user: "player".into(),
        },
        audience: Audience::Actor,
        content: JournalContent::Action {
            action: Action::Wait,
            event: crate::journal::Event::Waited,
        },
    };
    engine
        .commit_candidate(candidate, entry, None, None)
        .unwrap();
}
#[test]
fn public_plan_final_region_acquisition_cannot_invent_sealed_observer_history() {
    assert_late_acquisition_cannot_learn(false);
}
#[test]
fn public_plan_host_final_region_acquisition_cannot_reconstruct_an_event_time_witness() {
    assert_late_acquisition_cannot_learn(true);
}
fn assert_late_acquisition_cannot_learn(host: bool) {
    let (mut engine, mut candidate) = uncommitted_streaming_completion(host);
    let mut profile = CommandProfile::default();
    candidate
        .transition_with(
            engine.regions.as_mut(),
            BTreeSet::from([tor_world::RegionId(3)]),
            Some(&mut profile),
        )
        .unwrap();
    assert_eq!(profile.region_changes, 1);
    assert!(candidate.game.has_actor(SimActor(3)));
    let view = candidate.revision_view(ActorId(3)).unwrap();
    assert!(!discloses_slow_plan(&view));
    assert!(!candidate
        .game
        .event_observation_pending(&cold_event_proof(&candidate.game, ActorId(3))));
    commit_streaming_wait(&mut engine, candidate, ActorId(1));
    assert!(!discloses_slow_plan(
        &engine.revision_view(ActorId(3)).unwrap()
    ));
    assert!(!knows(&engine.game, ActorId(3), "slow_plan"));
}
#[test]
fn public_plan_host_final_publication_licenses_an_existing_event_time_observer() {
    let (mut engine, candidate, actor) = host_completion_with_existing_observer();
    commit_streaming_wait(&mut engine, candidate, actor);
    assert!(discloses_slow_plan(
        &engine.revision_view(ActorId(3)).unwrap()
    ));
    assert!(knows(&engine.game, ActorId(3), "slow_plan"));
}
#[test]
fn public_plan_pre_transition_proof_does_not_license_an_occluded_final_view() {
    let (mut engine, mut candidate, actor) = host_completion_with_existing_observer();
    let transient = Arc::new(candidate.revision_view(ActorId(3)).unwrap());
    assert!(candidate
        .game
        .event_observation_pending(&cold_event_proof(&candidate.game, ActorId(3))));
    assert!(!discloses_slow_plan(&transient));
    candidate.observations.insert(ActorId(3), transient);
    for y in 0..3 {
        for z in 0..2 {
            candidate
                .game
                .set_wall(
                    tor_world::Location {
                        region: tor_world::RegionId(3),
                        position: tor_world::Position { x: 1, y, z },
                    },
                    true,
                )
                .unwrap();
        }
    }
    let mut profile = CommandProfile::default();
    candidate
        .transition_with(
            engine.regions.as_mut(),
            BTreeSet::from([tor_world::RegionId(3), tor_world::RegionId(4)]),
            Some(&mut profile),
        )
        .unwrap();
    assert_eq!(
        profile.region_changes, 1,
        "the final cache must be reconciled by a real acquisition"
    );
    assert!(!discloses_slow_plan(
        &candidate.revision_view(ActorId(3)).unwrap()
    ));
    assert!(
        candidate
            .observations
            .get(&ActorId(3))
            .is_none_or(|view| !discloses_slow_plan(view)),
        "a cached view, when retained, must not preserve the transient disclosure"
    );
    assert!(!candidate
        .game
        .event_observation_pending(&cold_event_proof(&candidate.game, ActorId(3))));
    commit_streaming_wait(&mut engine, candidate, actor);
    assert!(!knows(&engine.game, ActorId(3), "slow_plan"));
}
fn private_work_scenario() -> Scenario {
    let mut scenario = fixture::scenario(false);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.archetypes.insert("inert_mail".into(), toml::from_str(
        "class = 'armor'\nname = 'private mail'\nequipment = { slot = 'body_armor', modifiers = [] }"
    ).unwrap());
    let mut regions = package.region_defs().unwrap();
    regions[0].items.push(
        toml::from_str("id = 10\nat = [2,1,0]\narchetype = 'inert_mail'\ncarried_by = 2").unwrap(),
    );
    let mut clock = regions[0].actors[0].clone();
    clock.id = 3;
    clock.at = [4, 1, 0];
    clock.creature.as_mut().unwrap().name = "clock figure".into();
    regions[0].actors.push(clock);
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(manifest, regions, &package.rules_source)
            .unwrap(),
    ));
    scenario
}
fn admit(engine: &mut Engine, actor: ActorId, action: Action) {
    engine
        .command(
            "player",
            "test",
            actor,
            &new_id().0,
            &engine.branch().clone(),
            Command::AdmitIntention {
                expected_boundary: engine.causal_boundary(actor).unwrap(),
                action,
            },
        )
        .unwrap();
    engine.execute_next_intention().unwrap().unwrap();
}
fn observer_snapshot(mut service: crate::Service, role: AccessRole) -> Vec<Snapshot> {
    let account = crate::Account {
        user: "observer".into(),
        token: "test-only".into(),
        role,
        actors: BTreeSet::from([ActorId(1)]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    assert!(matches!(
        client.messages.try_recv().unwrap(),
        ServerMessage::Welcome { .. }
    ));
    let mut snapshots = Vec::new();
    for request in [
        Request::Attach { actor: ActorId(1) },
        Request::Snapshot,
        Request::Snapshot,
    ] {
        service.handle(client.id, "observe".into(), request);
        let ServerMessage::Snapshot { snapshot, .. } = client.messages.try_recv().unwrap() else {
            panic!("ordinary snapshot reply")
        };
        snapshots.push(*snapshot);
    }
    snapshots
}
#[test]
fn public_plan_idle_and_private_work_have_equal_paid_replies_and_later_snapshots() {
    let baseline = Engine::memory(private_work_scenario()).unwrap();
    // Existing detached diagnostic copies retain the same archive salts and
    // branch, so compare genuine public references rather than redact them.
    let mut twins = [baseline.diagnostic_copy(), baseline.diagnostic_copy()];
    for engine in &mut twins {
        act(engine, ActorId(1), Action::Wait);
    }
    act(&mut twins[0], ActorId(2), Action::Wait);
    let slot = twins[1]
        .game
        .observe(SimActor(2))
        .unwrap()
        .interactions
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.id.as_str() == "body_armor")
        .unwrap();
    admit(
        &mut twins[1],
        ActorId(2),
        Action::Equip {
            item: 10,
            slot: slot as u16,
        },
    );
    for engine in &mut twins {
        for _ in 0..8 {
            if engine.next_actor() == Some(ActorId(1)) {
                break;
            }
            let actor = engine.next_actor().unwrap();
            act(engine, actor, Action::Wait);
        }
        assert_eq!(engine.next_actor(), Some(ActorId(1)));
        assert_eq!(engine.game.tick(), 100);
    }
    assert_eq!(
        twins[0].state(ActorId(1)).unwrap().observation,
        twins[1].state(ActorId(1)).unwrap().observation
    );
    assert!(twins[0].game.preparation(SimActor(2)).is_none());
    assert!(twins[1]
        .game
        .preparation(SimActor(2))
        .unwrap()
        .plan
        .as_ability()
        .is_none());
    for engine in &mut twins {
        admit(
            engine,
            ActorId(1),
            Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("fixture_observe_plan").unwrap(),
                invocation: crate::journal::AbilityInvocation {
                    aim: crate::journal::AbilityAim::ActorTarget {
                        target: SimActor(2),
                    },
                    weapon: crate::journal::WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
        );
        for _ in 0..8 {
            if engine.game.public_plan_observation(SimActor(1)).is_some() {
                break;
            }
            let actor = engine.next_actor().unwrap();
            assert_ne!(actor, ActorId(1));
            act(engine, actor, Action::Wait);
        }
        assert_eq!(engine.game.tick(), 200);
        let record = engine.game.public_plan_observation(SimActor(1)).unwrap();
        assert_eq!(record.observed_at, 200);
        assert!(matches!(
            record.state,
            tor_simulation::public_plan::PublicPlanState::NoPublicPlan
        ));
        assert_eq!(
            engine
                .game
                .creature(SimActor(1))
                .unwrap()
                .conditions()
                .instances()
                .filter(|condition| condition.condition.as_str() == "readied")
                .count(),
            1
        );
    }
    let expected = twins[0].state(ActorId(1)).unwrap().observation;
    assert_eq!(
        expected,
        twins[1].state(ActorId(1)).unwrap().observation,
        "paid Readied, resources, timing and public facts agree"
    );
    let record = expected
        .combat
        .as_ref()
        .unwrap()
        .public_plan
        .as_ref()
        .unwrap()
        .clone();
    for engine in &mut twins {
        for role in [AccessRole::Player, AccessRole::Spectator] {
            for snapshot in observer_snapshot(crate::Service::new(engine.diagnostic_copy()), role) {
                assert_eq!(
                    snapshot
                        .state
                        .observation
                        .combat
                        .as_ref()
                        .unwrap()
                        .public_plan
                        .as_ref(),
                    Some(&record)
                );
                assert_eq!(
                    serde_json::to_vec(
                        &snapshot
                            .state
                            .observation
                            .combat
                            .as_ref()
                            .unwrap()
                            .public_plan
                    )
                    .unwrap(),
                    serde_json::to_vec(&Some(&record)).unwrap()
                );
            }
        }
        for _ in 0..8 {
            if engine.game.tick() >= 400 {
                break;
            }
            let actor = engine.next_actor().unwrap();
            act(engine, actor, Action::Wait);
        }
        assert!(engine.game.tick() >= 400);
        for snapshot in observer_snapshot(
            crate::Service::new(engine.diagnostic_copy()),
            AccessRole::Spectator,
        ) {
            assert_eq!(
                snapshot
                    .state
                    .observation
                    .combat
                    .as_ref()
                    .unwrap()
                    .public_plan
                    .as_ref(),
                Some(&record)
            );
        }
    }
}

fn asymmetric_public_plan_engine(caster_lit: bool) -> Engine {
    asymmetric_public_plan_engine_with(caster_lit, |_, _| {})
}
fn asymmetric_public_plan_engine_with(
    caster_lit: bool,
    modify: impl FnOnce(&mut tor_simulation::rules::RulesetDocument, &mut serde_json::Value),
) -> Engine {
    let mut scenario = fixture::scenario(false);
    let package = scenario.package.take().unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(&package.rules_source).unwrap();
    rules
        .constants
        .default_abilities
        .retain(|id| id.as_str() != "fixture_observe_plan");
    let mut manifest = package.manifest.clone();
    let mut creatures = serde_json::to_value(&manifest.creatures).unwrap();
    creatures["templates"]["reader_only"] = serde_json::json!({"priority":0,"kind":null,"modifiers":[
        {"scope":"always","op":{"kind":"grant_ability","ability":"fixture_observe_plan"}}
    ]});
    modify(&mut rules, &mut creatures);
    manifest.creatures = serde_json::from_value(creatures).unwrap();
    let mut owner =
        serde_json::to_value(manifest.characters[0].creature.as_ref().unwrap()).unwrap();
    owner["templates"] = serde_json::json!(["reader_only"]);
    manifest.characters[0].creature = Some(serde_json::from_value(owner).unwrap());
    let mut regions = package.region_defs().unwrap();
    regions[0].actors[0].at = [4, 1, 0];
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            manifest,
            regions,
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    let mut engine = Engine::memory(scenario).unwrap();
    engine
        .game
        .set_region_light(tor_world::RegionId(1), false)
        .unwrap();
    for z in 0..2 {
        engine
            .game
            .set_cell_light(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 4, y: 1, z },
                },
                true,
            )
            .unwrap();
        if caster_lit {
            engine
                .game
                .set_cell_light(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x: 1, y: 1, z },
                    },
                    true,
                )
                .unwrap();
        }
    }
    assert!(engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .visible_actors
        .iter()
        .any(|actor| actor.id == SimActor(2)));
    assert_eq!(
        engine
            .game
            .observe(SimActor(2))
            .unwrap()
            .visible_actors
            .iter()
            .any(|actor| actor.id == SimActor(1)),
        caster_lit,
        "asymmetric visibility is a fixture prerequisite"
    );
    assert!(
        !knows(&engine.game, ActorId(2), "fixture_observe_plan"),
        "selected subject does not already own or know W05"
    );
    engine
}
fn complete_asymmetric_public_plan(engine: &mut Engine) {
    complete_asymmetric_action(engine);
    assert!(engine.game.public_plan_observation(SimActor(1)).is_some());
    assert!(engine
        .game
        .creature(SimActor(1))
        .unwrap()
        .conditions()
        .instances()
        .any(|entry| entry.condition.as_str() == "readied"));
}
fn complete_asymmetric_action(engine: &mut Engine) {
    admit(
        engine,
        ActorId(1),
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("fixture_observe_plan").unwrap(),
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: SimActor(2),
                },
                weapon: crate::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    );
    act(engine, ActorId(2), Action::Wait);
}
fn subject_discloses_fixture_observe_plan(engine: &Engine) -> bool {
    engine.game.observe(SimActor(2)).unwrap().combat.unwrap().events.iter().any(|event| matches!(event,
        tor_simulation::combat::DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "fixture_observe_plan"))
}
#[test]
fn public_plan_review_unseen_source_only_effect_does_not_disclose_or_license_to_subject() {
    let mut engine = asymmetric_public_plan_engine(false);
    complete_asymmetric_public_plan(&mut engine);
    assert!(
        !subject_discloses_fixture_observe_plan(&engine),
        "merely being selected is not personal sensation from a Source-only effect"
    );
    assert!(
        !knows(&engine.game, ActorId(2), "fixture_observe_plan"),
        "committed observer knowledge must not acquire the undisclosed name"
    );
}
#[test]
fn public_plan_review_visible_source_only_effect_still_discloses_and_licenses() {
    let mut engine = asymmetric_public_plan_engine(true);
    complete_asymmetric_public_plan(&mut engine);
    let events = engine
        .game
        .observe(SimActor(2))
        .unwrap()
        .combat
        .unwrap()
        .events;
    assert!(matches!(events.as_slice(), [
        tor_simulation::combat::DisclosedCombatEvent::Ability {
            caster: Some(SimActor(1)),
            target: Some(SimActor(2)),
            id,
            outcome: tor_simulation::abilities::OutcomeStatus::Applied,
            strike: Some(0),
            ..
        }
    ] if id.as_str() == "fixture_observe_plan"));
    assert!(knows(&engine.game, ActorId(2), "fixture_observe_plan"));
}

#[derive(Clone, Copy)]
enum ParticipationFixture {
    PersonalDifficulty,
    DormantTargetFailure,
    TargetDefense,
    TargetOpposed,
    TargetOwned,
    AttackMiss,
    TargetZero,
    ImmuneResistance,
    ImmuneTargetPayload,
    ImmunePersonalDifficulty,
    ImmuneDormantTargetFailure,
    ImmuneSourcePayload,
    SelectedTargetFailure,
}
fn participation_engine(case: ParticipationFixture) -> Engine {
    participation_engine_with_visibility(case, false)
}
fn participation_engine_with_visibility(case: ParticipationFixture, caster_lit: bool) -> Engine {
    asymmetric_public_plan_engine_with(caster_lit, |rules, creatures| {
        use tor_simulation::rules::{Effect, StrikeSource};
        let ability = rules
            .abilities
            .iter_mut()
            .find(|ability| ability.id.as_str() == "fixture_observe_plan")
            .unwrap();
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            panic!("authored fixture")
        };
        let strike = &mut strikes[0];
        let difficulty = serde_json::json!({"kind":"difficulty","formula":{"base":0,"skill":null,"attribute":null}});
        let defense = serde_json::json!({"kind":"defense","defense":"physical"});
        let (actor, against) = match case {
            ParticipationFixture::PersonalDifficulty
            | ParticipationFixture::DormantTargetFailure
            | ParticipationFixture::ImmunePersonalDifficulty
            | ParticipationFixture::ImmuneDormantTargetFailure => ("user", difficulty.clone()),
            ParticipationFixture::SelectedTargetFailure => (
                "user",
                serde_json::json!({"kind":"difficulty","formula":{"base":1000,"skill":null,"attribute":null}}),
            ),
            ParticipationFixture::TargetOwned => ("target", difficulty),
            ParticipationFixture::TargetOpposed => (
                "user",
                serde_json::json!({"kind":"opposed","skill":{"kind":"named","skill":"athletics"}}),
            ),
            _ => ("user", defense),
        };
        strike.check = if matches!(
            case,
            ParticipationFixture::TargetZero
                | ParticipationFixture::ImmuneTargetPayload
                | ParticipationFixture::ImmuneSourcePayload
        ) {
            None
        } else {
            Some(serde_json::from_value(serde_json::json!({"actor":actor,"skill":{"kind":"named","skill":"athletics"},"against":against})).unwrap())
        };
        strike.on_failure = strike.on_success.clone();
        if matches!(
            case,
            ParticipationFixture::DormantTargetFailure
                | ParticipationFixture::ImmuneDormantTargetFailure
                | ParticipationFixture::SelectedTargetFailure
        ) {
            strike.on_failure = vec![Effect::Heal {
                amount: tor_simulation::rules::AmountExpr::Fixed { amount: 0 },
            }];
        }
        if matches!(
            case,
            ParticipationFixture::TargetZero | ParticipationFixture::ImmuneTargetPayload
        ) {
            strike.on_success = vec![Effect::Heal {
                amount: tor_simulation::rules::AmountExpr::Fixed { amount: 0 },
            }];
            strike.on_failure.clear();
        }
        if matches!(case, ParticipationFixture::ImmuneSourcePayload) {
            strike.on_failure.clear();
        }
        if matches!(case, ParticipationFixture::AttackMiss) {
            strike.on_success = vec![serde_json::from_value(serde_json::json!({"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}})).unwrap()];
            strike.on_failure.clear();
            creatures["templates"]["reader_only"]["modifiers"].as_array_mut().unwrap().push(serde_json::json!({
                "scope":"always","op":{"kind":"add","stat":{"kind":"ability","ability":"fixture_observe_plan","stat":"check_bonus"},"value":-1000}
            }));
        }
        if matches!(
            case,
            ParticipationFixture::ImmuneResistance
                | ParticipationFixture::ImmuneTargetPayload
                | ParticipationFixture::ImmunePersonalDifficulty
                | ParticipationFixture::ImmuneDormantTargetFailure
                | ParticipationFixture::ImmuneSourcePayload
        ) {
            strike
                .descriptors
                .push(tor_simulation::content_ids::DescriptorId::new("fear").unwrap());
            creatures["species"]["reader"]["modifiers"].as_array_mut().unwrap().push(serde_json::json!({
                "scope":"always","op":{"kind":"immunity","selector":{"kind":"descriptor","descriptor":"fear"}}
            }));
        }
    })
}
fn assert_participation_disclosure(case: ParticipationFixture, expected: bool) {
    let mut engine = participation_engine(case);
    complete_asymmetric_action(&mut engine);
    let outcome = engine
        .game
        .combat_events()
        .iter()
        .find_map(|event| match event {
            tor_simulation::combat::CombatEvent::Resolved { outcome, .. }
                if outcome
                    .ability
                    .as_ref()
                    .is_some_and(|id| id.as_str() == "fixture_observe_plan") =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .expect("ordinary authored resolution reached");
    match case {
        ParticipationFixture::PersonalDifficulty | ParticipationFixture::DormantTargetFailure => {
            assert!(outcome.check.as_ref().unwrap().success);
            assert!(engine.game.public_plan_observation(SimActor(1)).is_some());
        }
        ParticipationFixture::AttackMiss => {
            assert_eq!(
                outcome.result,
                tor_simulation::abilities::OutcomeStatus::Miss
            );
            assert!(outcome.effects.is_empty());
        }
        ParticipationFixture::ImmuneResistance
        | ParticipationFixture::ImmuneTargetPayload
        | ParticipationFixture::ImmunePersonalDifficulty
        | ParticipationFixture::ImmuneDormantTargetFailure
        | ParticipationFixture::ImmuneSourcePayload => {
            assert!(
                outcome.check.is_none(),
                "immunity must still short-circuit the check"
            );
            assert!(outcome.effects.is_empty());
            assert_eq!(
                outcome.result,
                tor_simulation::abilities::OutcomeStatus::Unaffected
            );
        }
        ParticipationFixture::TargetZero => {
            assert!(
                matches!(
                    outcome.effects.as_slice(),
                    [tor_simulation::abilities::ResolvedEffect::Heal { amount: 0 }]
                ),
                "zero-valued resolved effects remain in the existing kernel: {:?}",
                outcome.effects
            );
            assert_eq!(
                outcome.result,
                tor_simulation::abilities::OutcomeStatus::Unaffected
            );
        }
        ParticipationFixture::SelectedTargetFailure => {
            assert!(!outcome.check.as_ref().unwrap().success);
            assert!(matches!(
                outcome.effects.as_slice(),
                [tor_simulation::abilities::ResolvedEffect::Heal { amount: 0 }]
            ));
            assert!(engine.game.public_plan_observation(SimActor(1)).is_none());
        }
        _ => assert!(outcome.check.is_some()),
    }
    let view = engine.game.observe(SimActor(2)).unwrap();
    let sensations: Vec<_> = view
        .combat
        .unwrap()
        .events
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                tor_simulation::combat::DisclosedCombatEvent::UnseenAbility { .. }
            )
        })
        .collect();
    assert_eq!(
        sensations.len(),
        usize::from(expected),
        "anonymous sensation must follow actual target participation exactly once"
    );
    if expected {
        assert_eq!(
            sensations[0],
            tor_simulation::combat::DisclosedCombatEvent::UnseenAbility {
                caster: None,
                target: SimActor(2),
                outcome: outcome.result,
            }
        );
    }
    assert!(
        !subject_discloses_fixture_observe_plan(&engine),
        "target sensation does not identify the unseen source's technique"
    );
    assert!(engine.game.observe(SimActor(1)).unwrap().combat.unwrap().events.iter().any(|event| matches!(event,
        tor_simulation::combat::DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "fixture_observe_plan")), "the source keeps its own action event");
    assert!(
        !knows(&engine.game, ActorId(2), "fixture_observe_plan"),
        "anonymous feedback must not teach the hidden technique"
    );
}
#[test]
fn public_plan_review_personal_dc_source_only_stays_private() {
    assert_participation_disclosure(ParticipationFixture::PersonalDifficulty, false);
}
#[test]
fn public_plan_review_dormant_target_failure_does_not_disclose_source_success() {
    assert_participation_disclosure(ParticipationFixture::DormantTargetFailure, false);
}
#[test]
fn public_plan_review_target_defense_discloses_source_payload() {
    assert_participation_disclosure(ParticipationFixture::TargetDefense, true);
}
#[test]
fn public_plan_review_target_opposed_discloses_source_payload() {
    assert_participation_disclosure(ParticipationFixture::TargetOpposed, true);
}
#[test]
fn public_plan_review_target_owned_check_discloses_source_payload() {
    assert_participation_disclosure(ParticipationFixture::TargetOwned, true);
}
#[test]
fn public_plan_review_attack_miss_preserves_target_sensation() {
    assert_participation_disclosure(ParticipationFixture::AttackMiss, true);
}
#[test]
fn public_plan_review_zero_target_payload_preserves_target_sensation() {
    assert_participation_disclosure(ParticipationFixture::TargetZero, true);
}
#[test]
fn public_plan_review_immune_resistance_preserves_target_sensation() {
    assert_participation_disclosure(ParticipationFixture::ImmuneResistance, true);
}
#[test]
fn public_plan_review_immune_target_payload_preserves_target_sensation() {
    assert_participation_disclosure(ParticipationFixture::ImmuneTargetPayload, true);
}

#[test]
fn public_plan_review_immune_personal_dc_stays_private() {
    assert_participation_disclosure(ParticipationFixture::ImmunePersonalDifficulty, false);
}
#[test]
fn public_plan_review_immune_dormant_target_branch_stays_private() {
    assert_participation_disclosure(ParticipationFixture::ImmuneDormantTargetFailure, false);
}
#[test]
fn public_plan_review_immune_source_payload_stays_private() {
    assert_participation_disclosure(ParticipationFixture::ImmuneSourcePayload, false);
}
#[test]
fn public_plan_review_selected_target_failure_discloses_actual_attempt() {
    assert_participation_disclosure(ParticipationFixture::SelectedTargetFailure, true);
}

#[test]
fn public_plan_previously_learned_technique_survives_anonymous_target_sensation() {
    use tor_simulation::abilities::OutcomeStatus;
    use tor_simulation::combat::DisclosedCombatEvent;
    let mut engine = participation_engine_with_visibility(ParticipationFixture::TargetZero, true);
    complete_asymmetric_action(&mut engine);
    let disclosed = engine
        .game
        .observe(SimActor(2))
        .unwrap()
        .combat
        .unwrap()
        .events;
    assert!(
        matches!(disclosed.as_slice(), [DisclosedCombatEvent::Ability {
        caster: Some(SimActor(1)), target: Some(SimActor(2)), id,
        outcome: OutcomeStatus::Unaffected, strike: Some(0), ..
    }] if id.as_str() == "fixture_observe_plan")
    );
    assert!(knows(&engine.game, ActorId(2), "fixture_observe_plan"));
    for z in 0..2 {
        engine
            .game
            .set_cell_light(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z },
                },
                false,
            )
            .unwrap();
    }
    let now = engine.game.observe(SimActor(2)).unwrap();
    assert!(!now
        .visible_actors
        .iter()
        .any(|actor| actor.id == SimActor(1)));
    assert_eq!(
        now.combat.unwrap().events,
        disclosed,
        "the existing identified event keeps its original receipt"
    );
    engine.observations = ObservationCache::default();
    for _ in 0..8 {
        if engine.next_actor() == Some(ActorId(1)) {
            break;
        }
        let actor = engine.next_actor().unwrap();
        act(&mut engine, actor, Action::Wait);
    }
    assert_eq!(engine.next_actor(), Some(ActorId(1)));
    complete_asymmetric_action(&mut engine);
    assert_eq!(
        engine
            .game
            .observe(SimActor(2))
            .unwrap()
            .combat
            .unwrap()
            .events,
        vec![DisclosedCombatEvent::UnseenAbility {
            caster: None,
            target: SimActor(2),
            outcome: OutcomeStatus::Unaffected,
        }],
        "prior technique knowledge cannot attribute a new hidden action"
    );
    assert!(
        knows(&engine.game, ActorId(2), "fixture_observe_plan"),
        "anonymous feedback must preserve legitimate previously learned knowledge"
    );
}

#[derive(Clone, Copy)]
enum RiderParticipationFixture {
    Damage,
    ZeroHeal,
    Defense,
    ImmuneDefense,
    ImmunePayload,
    Untriggered,
    PrimaryImmune,
}
fn assert_committed_rider_disclosure(case: RiderParticipationFixture, expected: bool) {
    assert_committed_rider_disclosure_with_visibility(case, expected, false);
}
fn assert_committed_rider_disclosure_with_visibility(
    case: RiderParticipationFixture,
    expected: bool,
    caster_lit: bool,
) {
    use tor_simulation::abilities::{OutcomeStatus, ResolvedEffect};
    use tor_simulation::combat::{CombatEvent, DisclosedCombatEvent};
    use tor_simulation::rules::{
        AmountExpr, Effect, RiderCheckDef, RiderDef, RiderTrigger, StrikeSource,
    };
    let mut engine = asymmetric_public_plan_engine_with(caster_lit, |rules, creatures| {
        let ability = rules
            .abilities
            .iter_mut()
            .find(|ability| ability.id.as_str() == "fixture_observe_plan")
            .unwrap();
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        let strike = &mut strikes[0];
        let checked = matches!(
            case,
            RiderParticipationFixture::Defense | RiderParticipationFixture::ImmuneDefense
        );
        let primary_immune = matches!(case, RiderParticipationFixture::PrimaryImmune);
        let immune = matches!(
            case,
            RiderParticipationFixture::ImmuneDefense
                | RiderParticipationFixture::ImmunePayload
                | RiderParticipationFixture::PrimaryImmune
        );
        let untriggered = matches!(case, RiderParticipationFixture::Untriggered);
        if untriggered {
            strike.check = Some(serde_json::from_value(serde_json::json!({"actor":"user","skill":{"kind":"named","skill":"athletics"},"against":{"kind":"difficulty","formula":{"base":0,"skill":null,"attribute":null}}})).unwrap());
            strike.on_failure = strike.on_success.clone();
        }
        if primary_immune {
            strike
                .descriptors
                .push(tor_simulation::content_ids::DescriptorId::new("fear").unwrap());
        }
        let effects = if checked {
            vec![]
        } else if matches!(
            case,
            RiderParticipationFixture::Damage
                | RiderParticipationFixture::Untriggered
                | RiderParticipationFixture::PrimaryImmune
        ) {
            vec![serde_json::from_value(serde_json::json!({"kind":"damage","damage":{"kind":"components","components":[{"category":"vital","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}})).unwrap()]
        } else {
            vec![Effect::Heal {
                amount: AmountExpr::Fixed { amount: 0 },
            }]
        };
        strike.riders.push(RiderDef {
            cancellable: false,
            condition: None,
            trigger: if untriggered { RiderTrigger::Failure } else { RiderTrigger::Success },
            check: checked.then(|| serde_json::from_value::<RiderCheckDef>(serde_json::json!({"skill":{"kind":"named","skill":"athletics"},"defense":"physical"})).unwrap()),
            effects,
            descriptors: if immune && !primary_immune { vec![tor_simulation::content_ids::DescriptorId::new("fear").unwrap()] } else { vec![] },
        });
        if immune {
            creatures["species"]["reader"]["modifiers"].as_array_mut().unwrap().push(serde_json::json!({"scope":"always","op":{"kind":"immunity","selector":{"kind":"descriptor","descriptor":"fear"}}}));
        }
    });
    if matches!(case, RiderParticipationFixture::PrimaryImmune) {
        complete_asymmetric_action(&mut engine);
        assert!(engine.game.public_plan_observation(SimActor(1)).is_none());
    } else {
        complete_asymmetric_public_plan(&mut engine);
    }
    let mut primary = None;
    let mut riders = Vec::new();
    for event in engine.game.combat_events() {
        let CombatEvent::Resolved {
            outcome, strike, ..
        } = event
        else {
            continue;
        };
        if outcome.actor != SimActor(1) || outcome.target != SimActor(2) {
            continue;
        }
        if outcome
            .ability
            .as_ref()
            .is_some_and(|id| id.as_str() == "fixture_observe_plan")
        {
            assert!(primary.replace(outcome).is_none());
            assert_eq!(*strike, Some(0));
        } else if outcome.ability.is_none() {
            assert!(strike.is_none(), "rider remains a non-public payload event");
            riders.push(outcome);
        }
    }
    let primary = primary.expect("actual primary resolution");
    assert_eq!(
        primary.result,
        if matches!(case, RiderParticipationFixture::PrimaryImmune) {
            OutcomeStatus::Unaffected
        } else {
            OutcomeStatus::Applied
        },
        "primary outcome is immutable"
    );
    assert_eq!(primary.target_rider_participated, expected);
    if matches!(
        case,
        RiderParticipationFixture::Untriggered | RiderParticipationFixture::PrimaryImmune
    ) {
        if matches!(case, RiderParticipationFixture::Untriggered) {
            assert!(primary.check.as_ref().unwrap().success);
        }
        assert!(
            riders.is_empty(),
            "the untriggered or blocked rider was not attempted"
        );
    } else {
        assert!(primary.check.is_none());
        assert_eq!(riders.len(), 1, "exactly the selected rider was committed");
        let rider = riders[0];
        match case {
            RiderParticipationFixture::Damage => assert!(matches!(
                rider.effects.as_slice(),
                [ResolvedEffect::Damage { amount: 1 }]
            )),
            RiderParticipationFixture::ZeroHeal => {
                assert!(matches!(
                    rider.effects.as_slice(),
                    [ResolvedEffect::Heal { amount: 0 }]
                ));
                assert_eq!(rider.result, OutcomeStatus::Unaffected);
            }
            RiderParticipationFixture::Defense => {
                let check = rider.check.as_ref().expect("actual target-defense check");
                assert!(rider.effects.is_empty());
                assert_eq!(
                    rider.result,
                    if check.success {
                        OutcomeStatus::Unaffected
                    } else {
                        OutcomeStatus::Miss
                    }
                );
            }
            RiderParticipationFixture::ImmuneDefense | RiderParticipationFixture::ImmunePayload => {
                assert!(rider.check.is_none() && rider.effects.is_empty());
                assert_eq!(rider.result, OutcomeStatus::Unaffected);
            }
            RiderParticipationFixture::Untriggered | RiderParticipationFixture::PrimaryImmune => {
                unreachable!()
            }
        }
    }
    let view = engine.game.observe(SimActor(2)).unwrap();
    let events: Vec<_> = view
        .combat
        .unwrap()
        .events
        .into_iter()
        .filter(|event| {
            matches!(event, DisclosedCombatEvent::UnseenAbility { .. })
                || matches!(event,
                    DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "fixture_observe_plan")
        })
        .collect();
    assert_eq!(
        events.len(),
        usize::from(expected),
        "executed rider participation must license exactly one primary event"
    );
    if expected {
        if caster_lit {
            assert!(
                matches!(
                    &events[0],
                    DisclosedCombatEvent::Ability {
                        caster: Some(SimActor(1)),
                        target: Some(SimActor(2)),
                        outcome: OutcomeStatus::Applied,
                        strike: Some(0),
                        ..
                    }
                ),
                "identified source preserves the primary event and strike"
            );
        } else {
            assert_eq!(
                events[0],
                DisclosedCombatEvent::UnseenAbility {
                    caster: None,
                    target: SimActor(2),
                    outcome: OutcomeStatus::Applied,
                },
                "rider sensation preserves primary status without naming the technique"
            );
        }
    }
    assert_eq!(
        knows(&engine.game, ActorId(2), "fixture_observe_plan"),
        caster_lit && expected,
        "only the identified primary event may teach the technique"
    );
    assert_eq!(
        subject_discloses_fixture_observe_plan(&engine),
        caster_lit && expected
    );
    let source_events = engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .combat
        .unwrap()
        .events;
    let source_primaries: Vec<_> = source_events
        .iter()
        .filter(|event| {
            matches!(event,
                DisclosedCombatEvent::Ability { id, .. } if id.as_str() == "fixture_observe_plan"
            )
        })
        .collect();
    assert_eq!(
        source_primaries.len(),
        1,
        "the source retains exactly its own primary event"
    );
    assert!(
        matches!(source_primaries[0], DisclosedCombatEvent::Ability {
        caster: Some(SimActor(1)), target: Some(SimActor(2)), outcome, strike: Some(0), ..
    } if *outcome == primary.result)
    );
}
#[test]
fn public_plan_rider_review_identified_source_licenses_primary_once() {
    assert_committed_rider_disclosure_with_visibility(
        RiderParticipationFixture::Damage,
        true,
        true,
    );
}
#[test]
fn public_plan_rider_review_executed_damage_preserves_anonymous_primary_once() {
    assert_committed_rider_disclosure(RiderParticipationFixture::Damage, true);
}
#[test]
fn public_plan_rider_review_executed_zero_heal_preserves_anonymous_primary_once() {
    assert_committed_rider_disclosure(RiderParticipationFixture::ZeroHeal, true);
}
#[test]
fn public_plan_rider_review_executed_defense_check_preserves_anonymous_primary_once() {
    assert_committed_rider_disclosure(RiderParticipationFixture::Defense, true);
}
#[test]
fn public_plan_rider_review_immune_defense_check_preserves_anonymous_primary_once() {
    assert_committed_rider_disclosure(RiderParticipationFixture::ImmuneDefense, true);
}
#[test]
fn public_plan_rider_review_immune_target_payload_preserves_anonymous_primary_once() {
    assert_committed_rider_disclosure(RiderParticipationFixture::ImmunePayload, true);
}
#[test]
fn public_plan_rider_review_untriggered_target_payload_remains_private() {
    assert_committed_rider_disclosure(RiderParticipationFixture::Untriggered, false);
}
#[test]
fn public_plan_rider_review_blocked_primary_does_not_invent_rider_participation() {
    assert_committed_rider_disclosure(RiderParticipationFixture::PrimaryImmune, false);
}

fn pay_growth_observation(engine: &mut Engine) {
    for (actor, target, id) in [(1, 2, "fixture_observe_plan"), (2, 1, "slow_plan")] {
        engine
            .command(
                "player",
                "test",
                ActorId(actor),
                id,
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_boundary: engine.causal_boundary(ActorId(actor)).unwrap(),
                    action: Action::UseAbility {
                        id: tor_simulation::content_ids::AbilityId::new(id).unwrap(),
                        invocation: crate::journal::AbilityInvocation {
                            aim: crate::journal::AbilityAim::ActorTarget {
                                target: SimActor(target),
                            },
                            weapon: crate::journal::WeaponSelection::Default,
                        },
                        supplies: Default::default(),
                    },
                },
            )
            .unwrap_or_else(|error| panic!("growth setup admission actor{actor} {id}: {error:?}"));
        assert_eq!(engine.next_intention_actor(), Some(ActorId(actor)));
        engine
            .execute_next_intention()
            .unwrap_or_else(|error| panic!("growth setup execution actor{actor} {id}: {error:?}"))
            .expect("growth setup executed");
    }
    assert!(engine.game.public_plan_observation(SimActor(1)).is_some());
    assert!(!knows(&engine.game, ActorId(1), "slow_plan"));
}
fn growth_finisher(engine: &Engine) -> Command {
    Command::Act {
        expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
        action: Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("growth_finisher").unwrap(),
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: SimActor(2),
                },
                weapon: crate::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    }
}
fn finish_growth(engine: &mut Engine, request: &str, command: Command) -> crate::CommandResult {
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            request,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}
#[test]
fn earned_growth_keeps_paid_unknown_plan_frozen_while_learning_own_grant_and_restoring() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("paid-growth.db");
        let scenario = fixture::growth_scenario();
        let policy = crate::SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, scenario.clone(), policy.clone()).unwrap();
        pay_growth_observation(&mut engine);
        let historical = engine
            .game
            .public_plan_observation(SimActor(1))
            .unwrap()
            .clone();
        assert!(!serde_json::to_string(&historical)
            .unwrap()
            .contains("Slow Plan"));
        let command = growth_finisher(&engine);
        let receipt = finish_growth(&mut engine, "earned-fatal", command.clone());
        assert!(
            !engine.game.alive(SimActor(2)),
            "the same admitted command must actually publish the fatal effect"
        );
        let owner = engine.game.creature(SimActor(1)).unwrap();
        assert_eq!(
            owner.build().ledger().total_hd(),
            tor_simulation::HdCount(2)
        );
        assert_eq!(owner.earned_progress().earned_xp(), 100);
        assert_eq!(owner.earned_progress().spent_xp(), 100);
        assert!(
            knows(&engine.game, ActorId(1), "slow_plan"),
            "growth commits the new own ability license"
        );
        assert_eq!(
            engine.game.public_plan_observation(SimActor(1)),
            Some(&historical)
        );
        let complete = engine.game.clone();
        for _ in 0..2 {
            engine.state(ActorId(1)).unwrap();
            engine.state(ActorId(2)).unwrap();
            assert_eq!(
                engine.game, complete,
                "projection cannot refold growth or rewrite history"
            );
        }
        let duplicate = finish_growth(&mut engine, "earned-fatal", command.clone());
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, receipt.entry);
        assert_eq!(engine.game, complete);
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            complete
        );
        engine.flush().unwrap();
        drop(engine);
        let mut restored = Engine::open_with_policy(&path, scenario, policy).unwrap();
        assert_eq!(restored.game, complete);
        assert!(finish_growth(&mut restored, "earned-fatal", command).duplicate);
        restored.enable_wizard().unwrap();
        let revision = restored.causal_boundary(ActorId(1)).unwrap();
        finish_growth(
            &mut restored,
            "rewind-growth",
            Command::Wizard {
                display_basis: None,
                expected_boundary: revision,
                operation: crate::journal::WizardOperation::Rewind { target: None },
            },
        );
        assert!(restored.game.alive(SimActor(2)));
        assert_eq!(
            restored
                .game
                .creature(SimActor(1))
                .unwrap()
                .earned_progress()
                .earned_xp(),
            0
        );
        assert!(restored.game.public_plan_observation(SimActor(1)).is_none());
        assert!(!knows(&restored.game, ActorId(1), "slow_plan"));
    }
}
#[test]
fn earned_growth_store_refusal_rolls_back_death_paid_record_knowledge_and_receipt_together() {
    let mut engine = Engine::memory(fixture::growth_scenario()).unwrap();
    pay_growth_observation(&mut engine);
    let command = growth_finisher(&engine);
    let mut accepted = Engine::replay(engine.archive.clone(), None, None).unwrap();
    finish_growth(&mut accepted, "positive-control", command.clone());
    assert!(!accepted.game.alive(SimActor(2)));
    assert_eq!(
        accepted
            .game
            .creature(SimActor(1))
            .unwrap()
            .earned_progress()
            .spent_xp(),
        100
    );
    assert!(knows(&accepted.game, ActorId(1), "slow_plan"));
    let directory = tempfile::tempdir().unwrap();
    let mut engine = engine
        .attach_profile_save_with_policy(
            directory.path().join("refused-growth.db"),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
    let before = engine.game.clone();
    let revision = engine.causal_boundary(ActorId(1)).unwrap();
    let counts = engine.profile_counts();
    let receipts = engine.receipts.len();
    for _ in 0..2 {
        let failure = engine
            .command(
                "player",
                "test",
                ActorId(1),
                "refused-fatal",
                &engine.branch().clone(),
                command.clone(),
            )
            .unwrap_err();
        assert_eq!(failure.code, ErrorCode::StorageFailure);
        assert_eq!(engine.game, before);
        assert_eq!(engine.causal_boundary(ActorId(1)).unwrap(), revision);
        assert_eq!(engine.profile_counts(), counts);
        assert_eq!(engine.receipts.len(), receipts);
        assert!(!knows(&engine.game, ActorId(1), "slow_plan"));
    }
}
