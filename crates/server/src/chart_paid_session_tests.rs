//! Real authored potion -> paid Service -> chart reply -> source-deleted reopen.
//! Synthetic source expands geometry only; no privileged condition/item injection.
use super::*;
use tor_protocol::chart_pages::*;
fn chart_source(root: &Path, kind: &str) -> crate::Scenario {
    let base = scenario(true);
    let source = base.package.as_ref().unwrap();
    let mut rules = source.rules().unwrap().document().clone();
    rules.conditions.push(serde_json::from_value(json!({
        "id":"fixture.monster_sense","family":"fixture.monster_sense","name":"Monster sense",
        "detecting_monsters":true,"hallucinating":false,"lifecycle":"timed","modifiers":[],
        "stacking":{"kind":"refresh"},"source_scoped":false,"descriptors":[],"capacity":"reject",
        "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},"restrictions":[],"preparation":"preserve"
    })).unwrap());
    let rules = toml::to_string(
        &tor_simulation::rules::Ruleset::validate(rules)
            .unwrap()
            .document(),
    )
    .unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["spatial_scopes"] = json!({"study":{"members":[1],"rooms":{"1":[{"region":1,"min":[0,0,0],"max":[89,4,1]}]},"passages":[],"doorways":[]}});
    manifest["creatures"]["species"]["figure"]["detection_appearance"] =
        json!({"name":"fixture figure","asset":null});
    manifest["archetypes"]["draught"]["consumable"] = json!({"discovery":"self_effect","effects":[
        if kind=="detect_monsters"{json!({"kind":kind,"condition":"fixture.monster_sense"})}else{json!({"kind":kind})}
    ]});
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["size"] = json!([90, 5, 2]);
    region["actors"][0]["at"] = json!([70, 2, 0]);
    region["actors"][0]["creature"]["name"] = json!("PRIVATE peer personal name");
    // An ordinary non-creature external body keeps a same-tick scheduler peer
    // ready after actor 2 moves. It is not a monster-census subject.
    region["actors"].as_array_mut().unwrap().push(json!({
        "id":3,"at":[75,2,0],"turn_ticks":100,"controller":"external"
    }));
    region["items"] = json!([
        {"id":10,"at":[1,2,0],"archetype":"draught","quantity":3,"carried_by":1},
        {"id":30,"at":[80,2,0],"archetype":"ember"}
    ]);
    regions[0] = serde_json::from_value(region).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.check().unwrap();
    package.build(42, false).unwrap();
    crate::scenario_package::write_package(
        root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}
fn act_as(wire: &mut Wire, actor: ActorId, id: &str, action: Action) -> RequestReceipt {
    let request = wire.request(actor, action);
    let receipt = ack(wire.send(actor, id, request.clone()));
    match receipt {
        RequestReceipt::Immediate { .. }
        | RequestReceipt::Admitted {
            phase: IntentionPhase::Resolved,
            ..
        } => receipt,
        _ => wire.finish(actor, id, &request),
    }
}
fn potion(wire: &mut Wire) -> ItemTarget {
    wire.snapshot(OWNER)
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Potion)
        .unwrap()
        .id
}
fn query(wire: &Wire, actor: ActorId, kind: ChartKind) -> ChartQuery {
    let client = &wire.service.clients[&wire.clients[&actor].id];
    let observed = client.last_observation.as_ref().unwrap();
    let chart = observed
        .state
        .observation
        .charts
        .iter()
        .find(|s| s.kind == kind)
        .unwrap()
        .chart
        .clone()
        .expect("advertised chart");
    ChartQuery {
        context: client.context.clone().unwrap(),
        branch: observed.branch.clone(),
        base: observed.base,
        chart,
        cursor: None,
    }
}
fn page(wire: &mut Wire, actor: ActorId, query: ChartQuery) -> ChartPage {
    wire.send(actor, "chart", Request::Chart { query })
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::ChartPage { page, .. } => Some(*page),
            ServerMessage::Error { code, message, .. } => panic!("{code:?}: {message}"),
            _ => None,
        })
        .unwrap()
}
fn rows(page: &ChartPage) -> Vec<ChartRow> {
    page.records
        .iter()
        .filter_map(|r| match r {
            ChartRecord::Row(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn paid_monster_chart_is_nonvisual_nonactionable_pure_and_republishes_far_peer_movement() {
    let temp = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(
        Engine::memory(chart_source(&temp.path().join("source"), "detect_monsters")).unwrap(),
    );
    let dose = potion(&mut wire);
    wire.act("detect", Action::Drink { item: dose });
    let snapshot = wire.snapshot(OWNER);
    assert!(
        snapshot.state.observation.visible_actors.is_empty(),
        "far subject must not be ordinarily visible"
    );
    let summary = snapshot
        .state
        .observation
        .charts
        .iter()
        .find(|s| s.kind == ChartKind::Monsters)
        .unwrap();
    assert_eq!(
        (summary.mode, summary.status),
        (ChartMode::Live, ChartStatus::Available)
    );
    let request = query(&wire, OWNER, ChartKind::Monsters);
    let before = wire.checkpoint();
    let prepared = wire
        .service
        .engine
        .prepared_detection_charts(OWNER)
        .unwrap();
    let first = page(&mut wire, OWNER, request.clone());
    assert!(Arc::ptr_eq(
        &prepared,
        &wire
            .service
            .engine
            .prepared_detection_charts(OWNER)
            .unwrap()
    ));
    assert_eq!(
        wire.checkpoint(),
        before,
        "pure chart request must not change RNG, knowledge or pending state"
    );
    let found = rows(&first);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].mark,
        ChartMark::Monster {
            name: "fixture figure".into(),
            asset: None
        }
    );
    let encoded = serde_json::to_string(&first).unwrap();
    assert!(!encoded.contains("PRIVATE"));
    assert!(
        !encoded.contains("actor_id") && !encoded.contains("target") && !encoded.contains("region")
    );
    for turn in 0..4 {
        if wire.service.engine.next_actor() == Some(OTHER) {
            break;
        }
        let next = wire.service.engine.next_actor().unwrap();
        act_as(&mut wire, next, &format!("owner-turn-{turn}"), Action::Wait);
    }
    assert_eq!(wire.service.engine.next_actor(), Some(OTHER));
    let revision = wire.service.engine.revision(OWNER).unwrap();
    let tick = wire.service.engine.state(OWNER).unwrap().observation.tick;
    act_as(
        &mut wire,
        OTHER,
        "far-move",
        Action::Move {
            direction: Direction::East,
        },
    );
    let snapshot = wire.snapshot(OWNER);
    assert!(snapshot.state.observation.visible_actors.is_empty());
    assert_eq!(
        snapshot.state.observation.tick, tick,
        "third ready body keeps far movement at the same global tick"
    );
    assert!(snapshot.state.revision > revision);
    let moved_query = query(&wire, OWNER, ChartKind::Monsters);
    let moved = page(&mut wire, OWNER, moved_query);
    assert_eq!(
        rows(&moved)[0].position,
        found[0].position.map(|[x, y, z]| [x + 1, y, z])
    );
    assert_ne!(first.chart, moved.chart);
    assert_eq!(
        wire.service
            .prepare_chart_page(wire.clients[&OWNER].id, request)
            .err()
            .unwrap()
            .code,
        ErrorCode::StaleContext
    );
    // The normal stationary-Wait path must publish expiry rather than leave an
    // obsolete live chart merely because no ordinary visible peer changed.
    for turn in 0..64 {
        let state = wire.service.engine.state(OWNER).unwrap();
        if state
            .observation
            .charts
            .iter()
            .find(|s| s.kind == ChartKind::Monsters)
            .unwrap()
            .status
            == ChartStatus::Inactive
        {
            break;
        }
        let next = wire.service.engine.next_actor().unwrap();
        act_as(&mut wire, next, &format!("expire-{turn}"), Action::Wait);
    }
    let expired = wire.snapshot(OWNER);
    let summary = expired
        .state
        .observation
        .charts
        .iter()
        .find(|s| s.kind == ChartKind::Monsters)
        .unwrap();
    assert_eq!(summary.status, ChartStatus::Inactive);
    assert!(summary.chart.is_none() && summary.acquired_at.is_none());
}
#[test]
fn historical_magic_is_frozen_across_movement_source_deleted_reopen_and_spectator_query() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    let path = temp.path().join("game.db");
    let source = chart_source(&root, "detect_magic");
    let mut wire = Wire::new(
        Engine::open_with_policy(
            &path,
            source,
            crate::SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let dose = potion(&mut wire);
    wire.act("detect-magic", Action::Drink { item: dose });
    wire.snapshot(OWNER);
    let request = query(&wire, OWNER, ChartKind::Magic);
    let first = page(&mut wire, OWNER, request);
    assert_eq!(first.mode, ChartMode::Historical);
    assert_eq!(
        rows(&first).len(),
        1,
        "ground magic only; own potion excluded"
    );
    act_as(
        &mut wire,
        OWNER,
        "move-owner",
        Action::Move {
            direction: Direction::East,
        },
    );
    wire.snapshot(OWNER);
    let request = query(&wire, OWNER, ChartKind::Magic);
    let later = page(&mut wire, OWNER, request);
    assert_eq!(
        (later.acquired_at, rows(&later)),
        (first.acquired_at, rows(&first))
    );
    let saved = wire.checkpoint();
    wire.service
        .engine
        .test_restore_checkpoint(&serde_json::to_vec(&saved).unwrap())
        .unwrap();
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, true);
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let engine = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&engine), saved);
    let mut service = Service::new(engine);
    let mut spectator = service
        .connect(
            &Account {
                role: AccessRole::Spectator,
                user: "historical-reader".into(),
                token: "fixture".into(),
                actors: BTreeSet::from([OWNER]),
            },
            "headless".into(),
        )
        .unwrap();
    service.handle(
        spectator.id,
        "attach".into(),
        Request::Attach { actor: OWNER },
    );
    while spectator.messages.try_recv_frame().is_ok() {}
    let client = &service.clients[&spectator.id];
    let observed = client.last_observation.as_ref().unwrap();
    let summary = observed
        .state
        .observation
        .charts
        .iter()
        .find(|s| s.kind == ChartKind::Magic)
        .unwrap();
    assert_eq!(
        (summary.mode, summary.acquired_at),
        (ChartMode::Historical, Some(first.acquired_at))
    );
    let q = ChartQuery {
        context: client.context.clone().unwrap(),
        branch: observed.branch.clone(),
        base: observed.base,
        chart: summary.chart.clone().unwrap(),
        cursor: None,
    };
    let before = checkpoint(&service.engine);
    service.handle(spectator.id, "read".into(), Request::Chart { query: q });
    let restored = std::iter::from_fn(|| spectator.messages.try_recv_frame().ok())
        .find_map(|frame| match decode_response(&frame.text).unwrap() {
            ServerMessage::ChartPage { page, .. } => Some(page),
            ServerMessage::Error { code, message, .. } => panic!("{code:?}: {message}"),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (restored.acquired_at, rows(&restored)),
        (first.acquired_at, rows(&first))
    );
    assert_eq!(checkpoint(&service.engine), before);
}

#[test]
fn paid_magic_history_encoder_rejection_rolls_back_then_exact_retry_commits_once() {
    paid_magic_history_encoder_rollback(true);
}

#[test]
fn paid_magic_history_encoder_rejection_allows_explicit_new_user_transaction() {
    paid_magic_history_encoder_rollback(false);
}

fn paid_magic_history_encoder_rollback(exact_retry: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let source = chart_source(&temporary.path().join("source"), "detect_magic");
    let mut wire = Wire::new(
        Engine::open_with_policy(
            temporary.path().join("game.db"),
            source,
            crate::SavePolicy {
                checkpoint_interval: 0,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let dose = potion(&mut wire);
    let request = wire.request(OWNER, Action::Drink { item: dose });
    let admitted = ack(wire.send(OWNER, "storage-dose", request.clone()));
    assert!(matches!(admitted, RequestReceipt::Admitted { .. }));
    let mut rejected = None;
    // Admit normal peer waits as needed, but execute one actual Engine boundary
    // at a time so the assertion freezes precisely the paid completion candidate.
    for turn in 0..16 {
        if wire.service.engine.next_intention_actor().is_none() {
            let next = wire.service.engine.next_actor().unwrap();
            let wait = wire.request(next, Action::Wait);
            ack(wire.send(next, &format!("storage-peer-{turn}"), wait));
        }
        wire.service.engine.flush().unwrap();
        let before = wire.checkpoint();
        let state = wire.service.engine.state(OWNER).unwrap();
        let history = wire
            .service
            .engine
            .history(OWNER, "magic-owner-1", None, 100)
            .unwrap();
        let accepted = wire.service.engine.save_status().accepted_sequence;
        let guard = crate::storage::HistoryAdmissionTestLimit::new(0);
        match wire.service.engine.execute_next_intention() {
            Ok(Some(_)) => {
                assert!(
                    guard.required_bytes().is_none(),
                    "no historical book changed before completion"
                );
            }
            Ok(None) => panic!("explicitly queued ordinary work must execute"),
            Err(error) => {
                assert_eq!(error.code, ErrorCode::StorageFailure);
                let exact = guard
                    .required_bytes()
                    .expect("real paid candidate reached actual encoder");
                assert!(exact > 0);
                assert_eq!(
                    wire.checkpoint(),
                    before,
                    "tick, RNG, items, knowledge, books and retained history roll back"
                );
                assert_eq!(wire.service.engine.state(OWNER).unwrap(), state);
                assert_eq!(
                    wire.service
                        .engine
                        .history(OWNER, "magic-owner-1", None, 100)
                        .unwrap(),
                    history
                );
                assert_eq!(
                    wire.service.engine.save_status().accepted_sequence,
                    accepted
                );
                assert_eq!(nothing_items(&before)["10"]["quantity"], 3);
                let item = state
                    .observation
                    .inventory
                    .iter()
                    .find(|item| item.id == dose)
                    .unwrap();
                assert!(!item.identified);
                rejected = Some((before, state, history, accepted, exact));
                break;
            }
        }
    }
    let (before, state, history, accepted, exact) =
        rejected.expect("paid Magic acquisition was not attempted");
    {
        let guard = crate::storage::HistoryAdmissionTestLimit::new(exact - 1);
        let error = wire.service.engine.execute_next_intention().unwrap_err();
        assert_eq!(error.code, ErrorCode::StorageFailure);
        assert_eq!(
            guard.required_bytes(),
            Some(exact),
            "retry has same complete serialized size"
        );
        assert_eq!(wire.checkpoint(), before);
        assert_eq!(wire.service.engine.state(OWNER).unwrap(), state);
        assert_eq!(
            wire.service
                .engine
                .history(OWNER, "magic-owner-1", None, 100)
                .unwrap(),
            history
        );
        assert_eq!(
            wire.service.engine.save_status().accepted_sequence,
            accepted
        );
    }
    if !exact_retry {
        // A failed completion retains the original queue. Cancel it explicitly
        // through the ordinary user protocol before admitting a NEW transaction;
        // resetting this test limit does not install any production auto-retry.
        for actor in wire.clients.keys().copied().collect::<Vec<_>>() {
            let pending = wire.snapshot(actor).readiness.cancel.clone();
            for (ordinal, intention) in pending.into_iter().enumerate() {
                let snapshot = wire.snapshot(actor);
                let request = Request::Command {
                    context: snapshot.reply_context().input,
                    branch: snapshot.branch.clone(),
                    command: Command::CancelIntention {
                        expected_revision: snapshot.state.revision,
                        intention,
                        step: None,
                    },
                };
                ack(wire.send(
                    actor,
                    &format!("storage-cancel-{}-{ordinal}", actor.0),
                    request,
                ));
            }
            assert!(wire.service.engine.pending_intentions(actor).is_empty());
        }
        assert_eq!(nothing_items(&wire.checkpoint())["10"]["quantity"], 3);
        let cancelled = ack(wire.send(OWNER, "storage-dose", request));
        assert!(matches!(
            cancelled,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Cancelled,
                ..
            }
        ));
        let resumed = wire.act("storage-dose-new", Action::Drink { item: dose });
        assert!(matches!(
            resumed,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Resolved,
                ..
            }
        ));
        let after = wire.checkpoint();
        assert_eq!(nothing_items(&after)["10"]["quantity"], 2);
        let state = wire.service.engine.state(OWNER).unwrap();
        assert!(
            state
                .observation
                .inventory
                .iter()
                .find(|item| item.id == dose)
                .unwrap()
                .identified
        );
        let summary = state
            .observation
            .charts
            .iter()
            .find(|s| s.kind == ChartKind::Magic)
            .unwrap();
        assert_eq!(
            (summary.mode, summary.status),
            (ChartMode::Historical, ChartStatus::Available)
        );
        return;
    }
    {
        let guard = crate::storage::HistoryAdmissionTestLimit::new(exact);
        assert!(
            matches!(wire.service.step(), Step::Progress),
            "exact encoder limit must admit the same paid candidate"
        );
        assert_eq!(guard.required_bytes(), Some(exact));
    }
    assert_eq!(
        wire.service.engine.save_status().accepted_sequence,
        accepted + 1
    );
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)["10"]["quantity"], 2);
    assert!(after["game"]["tick"].as_u64().unwrap() > before["game"]["tick"].as_u64().unwrap());
    let completed = wire.snapshot(OWNER);
    assert!(
        completed
            .state
            .observation
            .inventory
            .iter()
            .find(|item| item.id == dose)
            .unwrap()
            .identified
    );
    let summary = completed
        .state
        .observation
        .charts
        .iter()
        .find(|s| s.kind == ChartKind::Magic)
        .unwrap();
    assert_eq!(
        (summary.mode, summary.status),
        (ChartMode::Historical, ChartStatus::Available)
    );
    let resolved = ack(wire.send(OWNER, "storage-dose", request.clone()));
    assert!(matches!(
        resolved,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Resolved,
            ..
        }
    ));
    let published = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "storage-dose", request)), resolved);
    assert_eq!(
        wire.checkpoint(),
        published,
        "accepted receipt retry cannot acquire or consume twice"
    );
}
