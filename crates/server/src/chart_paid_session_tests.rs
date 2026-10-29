//! Real authored potion -> paid Service -> chart reply -> source-deleted reopen.
//! Synthetic authored source supplies detection effects and explicit magic; no privileged injection.
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
    manifest["spatial_scopes"] = json!({"study":{"members":[1],"rooms":{"1":[{"region":1,"min":[0,0,0],"max":[31,31,7]}]},"passages":[],"doorways":[]}});
    manifest["creatures"]["species"]["figure"]["detection_appearance"] =
        json!({"name":"fixture figure","asset":null});
    // Item class/device effects alone do not satisfy the settled Magic census.
    // Authored magic makes the ground positive and own-pack exclusion real.
    manifest["archetypes"]["ember"]["magical"] = json!(true);
    manifest["archetypes"]["draught"]["magical"] = json!(true);
    manifest["archetypes"]["draught"]["consumable"] = json!({"discovery":"self_effect","effects":[
        if kind=="detect_monsters"{json!({"kind":kind,"condition":"fixture.monster_sense"})}else{json!({"kind":kind})}
    ]});
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    // Stay inside the authored side ceiling while placing subjects beyond
    // sight16 on both axes; eight heights retain a genuinely multipage wall chart.
    region["size"] = json!([32, 32, 8]);
    region["actors"][0]["at"] = json!([25, 25, 0]);
    region["actors"][0]["creature"]["name"] = json!("PRIVATE peer personal name");
    // An ordinary non-creature external body keeps a same-tick scheduler peer
    // ready after actor 2 moves. It is not a monster-census subject.
    region["actors"].as_array_mut().unwrap().push(json!({
        "id":3,"at":[27,27,0],"turn_ticks":100,"controller":"external"
    }));
    region["items"] = json!([
        {"id":10,"at":[1,2,0],"archetype":"draught","quantity":3,"carried_by":1},
        {"id":30,"at":[28,25,0],"archetype":"ember"}
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
        base: observed.base.clone(),
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
    let revision = wire.service.engine.causal_boundary(OWNER).unwrap();
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
        "ground magic only; own magical potion excluded"
    );
    assert_eq!(rows(&first)[0].mark, ChartMark::Magic);
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
        base: observed.base.clone(),
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
        // Item preparation is deliberately not cancellable. A storage failure
        // preserves that exact admitted plan; use the established control-loss
        // pause and a NEW ordinary ResumeIntention transaction, not a fabricated
        // cancellation, fresh dose, or production automatic retry.
        let original_intention = match &admitted {
            RequestReceipt::Admitted { intention, .. } => intention.clone(),
            _ => panic!("ordinary paid admission"),
        };
        let pending = wire.snapshot(OWNER);
        assert!(!pending.readiness.cancel.contains(&original_intention));
        assert!(wire
            .service
            .engine
            .pending_intentions(OWNER)
            .iter()
            .any(|status| { status.intention == original_intention && status.phase.active() }));
        ack(wire.send(OWNER, "storage-pause", Request::ReleaseControl));
        ack(wire.send(OWNER, "storage-reacquire", Request::AcquireControl));
        wire.service.run_until_blocked();
        let paused = wire.snapshot(OWNER);
        assert!(paused.readiness.resume.contains(&original_intention));
        assert!(!paused.readiness.cancel.contains(&original_intention));
        assert_eq!(nothing_items(&wire.checkpoint())["10"]["quantity"], 3);
        assert!(!nothing_identified(&paused, dose));
        let inactive = paused
            .state
            .observation
            .charts
            .iter()
            .find(|summary| summary.kind == ChartKind::Magic)
            .unwrap();
        assert_eq!(inactive.status, ChartStatus::Inactive);
        assert!(inactive.chart.is_none() && inactive.latest_attempt.is_none());
        let resume = Request::Command {
            context: paused.reply_context().input,
            branch: paused.branch.clone(),
            command: Command::ResumeIntention {
                intention: original_intention,
            },
        };
        let resumed = ack(wire.send(OWNER, "storage-resume-new", resume.clone()));
        assert!(
            resumed.entry_id().is_some(),
            "new user transaction is durable"
        );
        assert_ne!(
            resumed.entry_id(),
            admitted.entry_id(),
            "the explicit new user transaction has its own durable receipt"
        );
        let completed = wire.finish(OWNER, "storage-dose", &request);
        assert_eq!(
            completed.entry_id(),
            admitted.entry_id(),
            "resuming preserves the original paid work identity"
        );
        assert!(matches!(
            completed,
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
        assert_eq!(
            summary.latest_attempt.as_ref().unwrap().status,
            ChartAttemptStatus::Positive
        );
        let published = wire.checkpoint();
        assert_eq!(ack(wire.send(OWNER, "storage-resume-new", resume)), resumed);
        assert_eq!(ack(wire.send(OWNER, "storage-dose", request)), completed);
        assert_eq!(
            wire.checkpoint(),
            published,
            "both exact receipt retries preserve one paid acquisition/consumption"
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
    assert_eq!(
        summary.latest_attempt.as_ref().unwrap().status,
        ChartAttemptStatus::Positive
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

fn aggregate_history_source(root: &Path, streaming: bool) -> crate::Scenario {
    let base = if streaming {
        crate::scenario_package::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor"),
            42,
            None,
            false,
        )
        .unwrap()
    } else {
        chart_source(&root.join("seed"), "detect_magic")
    };
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["archetypes"] = json!({
        "magic_dose":{"class":"potion","name":"potion of magic detection","identity":"fixture.magic",
            "magical":true,"stackable":true,"appearance_pool":"fixture.magic",
            "consumable":{"discovery":"self_effect","effects":[{"kind":"detect_magic"}]}},
        "food_scroll":{"class":"scroll","name":"scroll of food detection","identity":"fixture.food",
            "magical":true,"stackable":true,"appearance_pool":"fixture.food",
            "readable":{"discovery":"self_effect","effects":[{"kind":"detect_food"}]}},
        "mapping_scroll":{"class":"scroll","name":"scroll of magic mapping","identity":"fixture.mapping",
            "magical":true,"stackable":true,"appearance_pool":"fixture.mapping",
            "readable":{"discovery":"on_use","effects":[{"kind":"map_topology"}]}},
        "ration":{"class":"food","name":"ration"},
        "sigil":{"class":"misc","name":"magic sigil","magical":true}
    });
    manifest["appearance_pools"] = json!({
        "fixture.magic":{"appearances":["brass potion"]},
        "fixture.food":{"appearances":["scroll titled FOOD"]},
        "fixture.mapping":{"appearances":["scroll titled MAP"]}
    });
    let mut regions = package.region_defs().unwrap();
    let mut scopes = serde_json::Map::new();
    for region in &mut regions {
        let mut value = serde_json::to_value(&*region).unwrap();
        let id = value["id"].as_u64().unwrap();
        value["actors"] = json!([]);
        let carried_at = if streaming { [2, 1, 0] } else { [1, 2, 0] };
        value["items"] = if id == 1 {
            json!([
                {"id":10,"at":carried_at,"archetype":"magic_dose","quantity":3,"carried_by":1},
                {"id":11,"at":carried_at,"archetype":"food_scroll","quantity":4,"carried_by":1},
                {"id":12,"at":carried_at,"archetype":"mapping_scroll","quantity":5,"carried_by":1}
            ])
        } else {
            json!([])
        };
        if id == if streaming { 4 } else { 1 } {
            value["items"].as_array_mut().unwrap().extend([
                json!({"id":40,"at":[15,1,0],"archetype":"ration"}),
                json!({"id":41,"at":[15,1,0],"archetype":"sigil"}),
            ]);
        }
        if streaming && id == 4 {
            // A nonchamber all-room corridor has no wall facts. Give the paid
            // Mapping witness one real authored feature; an open door leaves
            // the ordinary movement/streaming path unobstructed.
            value["doors"] = json!([{"id":400,"at":[10,1,0],"open":true,"height":1}]);
        }
        let size = value["size"].as_array().unwrap();
        let max: Vec<_> = size.iter().map(|v| v.as_i64().unwrap() - 1).collect();
        scopes.insert(format!("hall-{id}"), json!({
            "members":[id],"rooms":{(id.to_string()):[{"region":id,"min":[0,0,0],"max":max}]},
            "passages":[],"doorways":[],"mapping":{"room_frames":[{"room":id,"region":id,"up":"up"}]}
        }));
        *region = serde_json::from_value(value).unwrap();
    }
    manifest["spatial_scopes"] = Value::Object(scopes);
    let rules = &package.rules_source;
    let checked =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, rules).unwrap();
    checked.check().unwrap();
    checked.build(42, false).unwrap();
    let destination = root.join("complete");
    crate::scenario_package::write_package(
        &destination,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        rules,
    )
    .unwrap();
    crate::scenario_package::check(&destination).unwrap();
    let mut result = crate::scenario_package::load(&destination, 42, None, true).unwrap();
    if streaming {
        result.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
    }
    result
}

fn aggregate_history_items(wire: &mut Wire) -> [ItemTarget; 3] {
    let inventory = wire.snapshot(OWNER).state.observation.inventory.clone();
    [3, 4, 5].map(|quantity| {
        inventory
            .iter()
            .find(|i| i.quantity == quantity)
            .unwrap()
            .id
    })
}
fn acquire_three_histories(wire: &mut Wire, items: [ItemTarget; 3]) {
    wire.act("aggregate-food", Action::Read { item: items[1] });
    wire.act("aggregate-magic", Action::Drink { item: items[0] });
    wire.act("aggregate-mapping", Action::Read { item: items[2] });
    let saved = wire.checkpoint();
    let owners = saved["shared"]["chart_history"]["owners"]
        [saved["game"]["chart_history"].as_u64().unwrap() as usize]
        .as_array()
        .unwrap();
    let owner = owners.iter().find(|o| o["actor"] == 1).unwrap();
    for kind in ["food", "magic", "mapping"] {
        let slot = &owner["slots"][kind];
        assert!(
            slot["latest"].is_u64(),
            "real paid {kind} producer must retain a positive root"
        );
        assert_eq!(slot["attempt"]["status"]["kind"], "positive");
    }
}

#[test]
fn paid_combined_food_magic_mapping_roots_use_whole_checkpoint_exact_ceiling() {
    let temp = tempfile::tempdir().unwrap();
    let scenario = aggregate_history_source(&temp.path().join("source"), false);
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(
            temp.path().join("combined.db"),
            scenario,
            crate::SavePolicy {
                checkpoint_interval: 0,
                ..Default::default()
            },
        )
        .unwrap(),
        [OWNER],
    );
    let items = aggregate_history_items(&mut wire);
    acquire_three_histories(&mut wire, items);
    wire.service.engine.flush().unwrap();
    let old = wire.checkpoint();
    let old_roots = old["shared"]["chart_history"]["roots"].as_array().unwrap();
    assert!(old_roots.iter().any(|r| r["kind"] == "food"));
    assert!(old_roots.iter().any(|r| r["kind"] == "magic"));
    let mapping = old_roots.iter().find(|r| r["kind"] == "mapping").unwrap();
    assert_eq!(
        mapping["points"].as_array().unwrap().len(),
        1024,
        "32x32x8 certified room retains all four side faces at eight heights, beyond one wire page"
    );
    let request = wire.request(OWNER, Action::Drink { item: items[0] });
    ack(wire.send(OWNER, "aggregate-new-magic", request));
    let before = wire.checkpoint();
    let state = wire.service.engine.state(OWNER).unwrap();
    let accepted = wire.service.engine.save_status().accepted_sequence;
    let exact = {
        let guard = crate::storage::HistoryAdmissionTestLimit::new(0);
        assert_eq!(
            wire.service
                .engine
                .execute_next_intention()
                .unwrap_err()
                .code,
            ErrorCode::StorageFailure
        );
        guard.required_bytes().unwrap()
    };
    assert_eq!(wire.checkpoint(), before);
    {
        let guard = crate::storage::HistoryAdmissionTestLimit::new(exact - 1);
        assert_eq!(
            wire.service
                .engine
                .execute_next_intention()
                .unwrap_err()
                .code,
            ErrorCode::StorageFailure
        );
        assert_eq!(guard.required_bytes(), Some(exact));
    }
    assert_eq!(wire.checkpoint(), before);
    assert_eq!(wire.service.engine.state(OWNER).unwrap(), state);
    assert_eq!(
        wire.service.engine.save_status().accepted_sequence,
        accepted
    );
    {
        let guard = crate::storage::HistoryAdmissionTestLimit::new(exact);
        assert!(matches!(wire.service.step(), Step::Progress));
        assert_eq!(guard.required_bytes(), Some(exact));
    }
    assert_eq!(
        wire.service.engine.save_status().accepted_sequence,
        accepted + 1
    );
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)["10"]["quantity"], 1);
    let roots = after["shared"]["chart_history"]["roots"]
        .as_array()
        .unwrap();
    for old_root in old_roots {
        assert!(roots.iter().any(|root| ["kind","acquired_tick","scope","components","points"]
            .iter().all(|field| root[*field] == old_root[*field])),
            "retained boundaries preserve the old acquisition; pool reference indices may be renumbered");
    }
    assert!(roots.iter().filter(|r| r["kind"] == "magic").count() >= 2);
    assert!(
        exact
            > serde_json::to_vec(&after["shared"]["chart_history"])
                .unwrap()
                .len(),
        "admission is the actual entire checkpoint, not a new standalone chart ceiling"
    );
}

#[test]
fn paid_history_guard_checks_every_unwritten_detached_row_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let scenario = aggregate_history_source(&temp.path().join("source"), true);
    // Keep the real writer pending using its ordinary validated policy, without
    // races against a blocked SQLite transaction or replacing the storage path.
    let hour = std::time::Duration::from_secs(3600);
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(
            temp.path().join("pending.db"),
            scenario,
            crate::SavePolicy {
                checkpoint_interval: 0,
                target_interval: hour,
                max_unsaved_age: hour,
                idle_interval: hour,
                ..Default::default()
            },
        )
        .unwrap(),
        [OWNER],
    );
    let items = aggregate_history_items(&mut wire);
    for step in 0..76 {
        wire.act(
            &format!("pending-east-{step}"),
            Action::Move {
                direction: Direction::East,
            },
        );
    }
    assert_eq!(wire.service.engine.region_counts().unwrap().detached, 2);
    acquire_three_histories(&mut wire, items);
    let saved = wire.checkpoint();
    let mapping = saved["shared"]["chart_history"]["roots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|root| root["kind"] == "mapping")
        .unwrap();
    let points = mapping["points"].as_array().unwrap();
    assert_eq!(points.len(), 1, "ordinary room interiors remain omitted");
    assert_eq!(points[0]["mapping"]["tag"], "doorway");
    assert_eq!(
        points[0]["source"],
        json!({"region":4,"position":{"x":10,"y":1,"z":0}}),
        "the retained positive comes from the actual authored open door"
    );
    let captured = crate::engine::Checkpoint::capture(&wire.service.engine);
    assert_eq!(
        captured.records.len(),
        2,
        "both detached records remain genuine unwritten rows"
    );
    let lengths: Vec<_> = captured
        .records
        .values()
        .map(|r| serde_json::to_vec(&**r).unwrap().len())
        .collect();
    let exact = *lengths.iter().max().unwrap();
    assert!(exact > 1);
    assert_eq!(wire.service.engine.save_status().durable_sequence, 0);
    let request = wire.request(OWNER, Action::Drink { item: items[0] });
    ack(wire.send(OWNER, "pending-new-magic", request));
    let before = wire.checkpoint();
    let state = wire.service.engine.state(OWNER).unwrap();
    let accepted = wire.service.engine.save_status().accepted_sequence;
    {
        let guard = crate::storage::RegionAdmissionTestLimit::new(exact - 1);
        assert_eq!(
            wire.service
                .engine
                .execute_next_intention()
                .unwrap_err()
                .code,
            ErrorCode::StorageFailure
        );
        assert_eq!(guard.measurements().1, exact);
    }
    assert_eq!(
        wire.checkpoint(),
        before,
        "cost, knowledge, attempts and roots are atomic with pending-row admission"
    );
    assert_eq!(wire.service.engine.state(OWNER).unwrap(), state);
    assert_eq!(
        wire.service.engine.save_status().accepted_sequence,
        accepted
    );
    assert_eq!(nothing_items(&before)["10"]["quantity"], 2);
    {
        let guard = crate::storage::RegionAdmissionTestLimit::new(exact);
        assert!(matches!(wire.service.step(), Step::Progress));
        assert_eq!(
            guard.measurements(),
            (2, exact),
            "every separate row passes its exact existing payload ceiling"
        );
    }
    assert_eq!(
        wire.service.engine.save_status().accepted_sequence,
        accepted + 1
    );
    assert_eq!(nothing_items(&wire.checkpoint())["10"]["quantity"], 1);
    wire.service.engine.flush().unwrap();
}

#[path = "detached_chart_admission_tests.rs"]
mod detached_admission;
