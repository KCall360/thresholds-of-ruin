//! Ordinary Service boundary tests for private owner advancement.
use super::*;
use serde_json::{json, Value};

use crate::advancement_test_support::scenario;

fn drain(client: &mut Connection) -> Vec<Value> {
    std::iter::from_fn(|| client.messages.try_recv().ok())
        .map(|message| {
            let encoded = tor_protocol::encode_response(&message, None, 1024 * 1024).unwrap();
            assert_eq!(
                tor_protocol::decode_response(&encoded.text).unwrap(),
                message
            );
            serde_json::from_str(&encoded.text).unwrap()
        })
        .collect()
}
fn connect(service: &mut Service, user: &str, role: AccessRole, actor: ActorId) -> Connection {
    let mut client = service
        .connect(
            &Account {
                role,
                user: user.into(),
                token: "test-only".into(),
                actors: BTreeSet::from([ActorId(1), ActorId(2)]),
            },
            "headless".into(),
        )
        .unwrap();
    drain(&mut client);
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    let messages = drain(&mut client);
    assert!(
        messages.iter().any(|m| m["type"] == "snapshot"),
        "valid attached fixture: {messages:?}"
    );
    client
}
fn control(service: &mut Service, client: &mut Connection) {
    service.handle(client.id, "acquire".into(), Request::AcquireControl);
    let messages = drain(client);
    assert!(
        !messages.iter().any(|m| m["type"] == "error"),
        "valid control: {messages:?}"
    );
    assert_eq!(service.controllers.get(&ActorId(1)), Some(&client.id));
}
fn command(service: &Service, id: u64, command: Value) -> Request {
    serde_json::from_str(
        &json!({"type":"command","context":service.input_context(id).unwrap(),
        "branch":service.engine.branch(),"command":command})
        .to_string(),
    )
    .unwrap()
}
fn query(service: &mut Service, client: &mut Connection, query: Value) -> Vec<Value> {
    let actor = service.clients[&client.id].actor.unwrap();
    let request = command(
        service,
        client.id,
        json!({"type":"advancement",
        "expected_revision":service.engine.revision(actor).unwrap().to_string(),"query":query}),
    );
    service.handle(client.id, "owner-page".into(), request);
    drain(client)
}
fn page(messages: &[Value]) -> &Value {
    &messages
        .iter()
        .find(|m| m["type"] == "advancement_page")
        .expect("authorized owner page")["page"]
}
fn no_private_reply(messages: &[Value]) {
    assert!(messages.iter().all(|m| m["type"] != "advancement_page"));
    let bytes = serde_json::to_string(messages).unwrap();
    for key in [
        "build_epoch",
        "resulting_build_epoch",
        "starting_skills",
        "refund_count",
        "replacement",
    ] {
        assert!(
            !bytes.contains(key),
            "private field {key} in rejected reply: {bytes}"
        );
    }
}

#[test]
fn owner_advancement_overview_is_controller_only_and_read_only() {
    let mut service = Service::new(Engine::memory(scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    let mut unowned = connect(
        &mut service,
        "second-player",
        AccessRole::Player,
        ActorId(1),
    );
    let mut foreign = connect(
        &mut service,
        "foreign-player",
        AccessRole::Player,
        ActorId(2),
    );
    control(&mut service, &mut owner);
    for denied in [&mut spectator, &mut unowned, &mut foreign] {
        drain(denied);
        let messages = query(&mut service, denied, json!({"kind":"overview"}));
        no_private_reply(&messages);
        assert!(messages.iter().any(|m| m["type"] == "error"));
    }
    let before = [ActorId(1), ActorId(2)].map(|a| service.engine.state(a).unwrap());
    let counts = service.engine.profile_counts();
    let histories =
        [ActorId(1), ActorId(2)].map(|a| service.engine.history(a, "owner", None, 50).unwrap());
    let messages = query(&mut service, &mut owner, json!({"kind":"overview"}));
    let current = page(&messages);
    assert_eq!(current["basis"]["actor"], json!("1"));
    assert_eq!(current["query"], json!({"kind":"overview"}));
    assert_eq!(current["total"], json!(1));
    assert_eq!(
        [ActorId(1), ActorId(2)].map(|a| service.engine.state(a).unwrap()),
        before
    );
    assert_eq!(service.engine.profile_counts(), counts);
    assert_eq!(
        [ActorId(1), ActorId(2)].map(|a| service.engine.history(a, "owner", None, 50).unwrap()),
        histories
    );
    // Control loss invalidates fresh reads even if this attachment is retained.
    service.handle(owner.id, "release".into(), Request::ReleaseControl);
    drain(&mut owner);
    control(&mut service, &mut unowned);
    let messages = query(&mut service, &mut owner, json!({"kind":"overview"}));
    no_private_reply(&messages);
    assert!(messages.iter().any(|m| m["code"] == "not_controller"));
    assert_eq!(
        page(&query(
            &mut service,
            &mut unowned,
            json!({"kind":"overview"})
        ))["basis"]["actor"],
        json!("1")
    );
}

#[test]
fn owner_advancement_foreign_and_reconnected_basis_do_not_authorize_fresh_purchase() {
    let mut service = Service::new(Engine::memory(scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    control(&mut service, &mut owner);
    let original =
        page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let mut foreign = original.clone();
    foreign["actor"] = json!("2");
    let bad = query(
        &mut service,
        &mut owner,
        json!({"kind":"ledger","basis":foreign,"offset":0}),
    );
    no_private_reply(&bad);
    service.disconnect(owner.id);
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    control(&mut service, &mut owner);
    let fresh = page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    assert_ne!(fresh["stream"], original["stream"]);
    let request = command(
        &service,
        owner.id,
        json!({"type":"act",
        "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"purchase","basis":original,"purchase":{"owner":0,"pool":"advancement",
            "selection":{"kind":"skill_rank","skill":"perception"}}}}),
    );
    let before = service.engine.state(ActorId(1)).unwrap();
    let counts = service.engine.profile_counts();
    service.handle(owner.id, "stale-preview".into(), request);
    let rejected = drain(&mut owner);
    no_private_reply(&rejected);
    assert!(rejected.iter().any(|m| m["type"] == "error"));
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
    assert_eq!(service.engine.profile_counts(), counts);
    let valid = query(
        &mut service,
        &mut owner,
        json!({"kind":"ledger","basis":fresh.clone(),"offset":0}),
    );
    assert_eq!(page(&valid)["total"], json!(0));
    // Same actor/operation with its new basis is admissible: stale rejection
    // cannot be satisfied by a blanket unsupported-action or unfunded error.
    let request = command(
        &service,
        owner.id,
        json!({"type":"act",
        "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"purchase","basis":fresh,"purchase":{"owner":0,"pool":"advancement",
            "selection":{"kind":"skill_rank","skill":"perception"}}}}),
    );
    service.handle(owner.id, "fresh-preview".into(), request);
    let accepted = drain(&mut owner);
    assert!(
        accepted
            .iter()
            .any(|m| m["type"] == "ack" && m["request_id"] == "fresh-preview"),
        "fresh control: {accepted:?}"
    );
    assert!(!accepted.iter().any(|m| m["type"] == "error"));
}

#[test]
fn owner_advancement_matching_receipt_precedes_reconnect_freshness_without_private_projection() {
    let mut service = Service::new(Engine::memory(scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    control(&mut service, &mut owner);
    let basis = page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let original = command(
        &service,
        owner.id,
        json!({"type":"act",
        "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"purchase","basis":basis,"purchase":{"owner":0,"pool":"advancement",
            "selection":{"kind":"skill_rank","skill":"perception"}}}}),
    );
    service.handle(owner.id, "durable-request".into(), original.clone());
    let messages = drain(&mut owner);
    let receipt = messages
        .iter()
        .find(|m| m["type"] == "ack" && m["request_id"] == "durable-request")
        .expect("valid initial admission")["receipt"]
        .clone();
    assert_eq!(receipt["type"], json!("admitted"));
    let before = service.engine.state(ActorId(1)).unwrap();
    service.disconnect(owner.id);
    let mut reconnect = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    // No control, and deliberately retain the old input context and basis.
    service.handle(reconnect.id, "durable-request".into(), original.clone());
    let duplicate = drain(&mut reconnect);
    no_private_reply(&duplicate);
    let duplicate_receipt = &duplicate
        .iter()
        .find(|m| m["type"] == "ack" && m["request_id"] == "durable-request")
        .expect("authenticated matching duplicate wins before freshness")["receipt"];
    for field in ["actor", "branch", "entry_id", "intention"] {
        assert_eq!(
            duplicate_receipt[field], receipt[field],
            "receipt identity {field}"
        );
    }
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
    let mut changed = serde_json::to_value(original.clone()).unwrap();
    changed["command"]["action"]["basis"]["stream"] = json!("different-preview-stream");
    service.handle(
        reconnect.id,
        "durable-request".into(),
        serde_json::from_value(changed).unwrap(),
    );
    let conflict = drain(&mut reconnect);
    no_private_reply(&conflict);
    assert!(conflict.iter().any(|m| m["code"] == "request_conflict"));
    let mut foreign = connect(&mut service, "another-user", AccessRole::Player, ActorId(1));
    service.handle(foreign.id, "durable-request".into(), original);
    let rejected = drain(&mut foreign);
    no_private_reply(&rejected);
    assert!(!rejected.iter().any(|m| m["type"] == "ack"));
    assert!(rejected.iter().any(|m| m["type"] == "error"));
}

#[test]
fn advancement_boundary_private_purchase_is_absent_from_history_pages_and_anchors() {
    let mut service = Service::new(Engine::memory(scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    control(&mut service, &mut owner);
    let public = service
        .engine
        .annotate_backend(
            ActorId(1),
            "test",
            Anchor::State {
                revision: service.engine.revision(ActorId(1)).unwrap(),
            },
            AnnotationCategory::Note,
            "public chronology control",
        )
        .unwrap();
    let before = service
        .engine
        .history(ActorId(1), "spectator", None, 1)
        .unwrap();
    assert_eq!(before.entries.len(), 1);
    assert_eq!(before.entries[0].id, public.id);
    let basis = page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let request = command(
        &service,
        owner.id,
        json!({"type":"act",
        "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"purchase","basis":basis,"purchase":{"owner":0,"pool":"advancement",
            "selection":{"kind":"skill_rank","skill":"perception"}}}}),
    );
    service.handle(owner.id, "private-choice".into(), request);
    let admitted = drain(&mut owner);
    assert!(admitted.iter().any(|row| row["type"] == "ack"));
    no_private_reply(&admitted);
    let revisions = service
        .engine
        .actors()
        .into_iter()
        .map(|actor| (actor, service.engine.revision(actor).unwrap()))
        .collect();
    let started = service.engine.execute_next_intention().unwrap().unwrap();
    assert!(matches!(
        started.entry.content,
        crate::journal::JournalContent::IntentionStarted { .. }
    ));
    service.action_result_update(&revisions, &started).unwrap();
    no_private_reply(&drain(&mut spectator));
    assert!(service.engine.disclose_entry(&started.entry).is_none());
    for user in ["owner", "spectator", "unrelated"] {
        let history = service.engine.history(ActorId(1), user, None, 1).unwrap();
        assert_eq!(
            history, before,
            "private work must not displace public pagination for {user}"
        );
        assert!(
            service
                .engine
                .history(ActorId(1), user, Some(&started.entry.id), 1)
                .is_err(),
            "private resolution is not a public anchor"
        );
        assert!(!started.entry.visible_to(ActorId(1), user));
    }
}

#[test]
fn owner_advancement_active_paused_and_resumed_respec_stays_out_of_ordinary_views() {
    const CANARY: &str = "private_respec_confirmation";
    let mut fixture = crate::advancement_test_support::replacement_scenario(false);
    let package = fixture.package.as_ref().unwrap();
    let mut rules = package.rules().unwrap().document().clone();
    rules.talents.push(tor_simulation::rules::TalentDef {
        id: tor_simulation::content_ids::TalentId::new(CANARY).unwrap(),
        name: "Private confirmation choice".into(),
        prerequisites: vec![],
        requirement: tor_simulation::rules::Requirement::TotalHd { minimum: 1 },
        modifiers: vec![],
    });
    fixture.package = Some(std::sync::Arc::new(
        crate::scenario_package::Package::from_parts(
            package.manifest.clone(),
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    let mut service = Service::new(Engine::memory(fixture).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    let mut observer = connect(&mut service, "observer", AccessRole::Spectator, ActorId(2));
    control(&mut service, &mut owner);
    let public = service
        .engine
        .annotate_backend(
            ActorId(1),
            "test",
            Anchor::State {
                revision: service.engine.revision(ActorId(1)).unwrap(),
            },
            AnnotationCategory::Note,
            "public chronology control",
        )
        .unwrap();
    let original_basis =
        page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let replacement = json!({"source":{"owner":0,"index":0},"replacement":{
        "pool":"advancement","selection":{"kind":"talent","talent":CANARY}}});
    let preview = query(
        &mut service,
        &mut owner,
        json!({"kind":"preview","basis":original_basis,
        "proposal":{"kind":"replacement","replacement":replacement},"section":"summary","offset":0}),
    );
    assert_eq!(
        page(&preview)["entries"][0]["value"]["proposal"]["replacement"],
        replacement
    );
    let original = command(
        &service,
        owner.id,
        json!({"type":"act",
        "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"respec","basis":original_basis,"replacement":replacement,"supplies":{"kind":"carried"}}}),
    );
    service.handle(owner.id, "private-respec".into(), original.clone());
    let admission = drain(&mut owner);
    let receipt = admission
        .iter()
        .find(|message| message["type"] == "ack")
        .unwrap()["receipt"]
        .clone();
    assert_eq!(receipt["type"], json!("admitted"));
    let intention = receipt["intention"].clone();
    let public_only = |messages: &[Value]| {
        no_private_reply(messages);
        let bytes = serde_json::to_string(messages).unwrap();
        for private in [CANARY, "\"basis\"", "\"respec\"", "\"refunds\""] {
            assert!(
                !bytes.contains(private),
                "private respec data {private}: {bytes}"
            );
        }
    };
    public_only(&admission);
    let mut private_entries = Vec::new();
    for phase in 0..3 {
        let active = phase != 1;
        if phase == 1 {
            service.handle(owner.id, "release".into(), Request::ReleaseControl);
            service.apply_pending_pauses();
            service.refresh_readiness();
            public_only(&drain(&mut owner));
            let denied = query(&mut service, &mut owner, json!({"kind":"overview"}));
            public_only(&denied);
            assert!(denied
                .iter()
                .any(|message| message["code"] == "not_controller"));
            service.handle(owner.id, "reacquire".into(), Request::AcquireControl);
        }
        if phase == 2 {
            let resume = command(
                &service,
                owner.id,
                json!({"type":"resume_intention",
                "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),"intention":intention}),
            );
            service.handle(owner.id, "resume-private-respec".into(), resume);
        }
        if active {
            let revisions = service
                .engine
                .actors()
                .into_iter()
                .map(|actor| (actor, service.engine.revision(actor).unwrap()))
                .collect();
            let executed = service.engine.execute_next_intention().unwrap().unwrap();
            if phase == 0 {
                assert!(matches!(
                    executed.entry.content,
                    crate::journal::JournalContent::IntentionStarted { .. }
                ));
            } else {
                assert!(matches!(
                    executed.entry.content,
                    crate::journal::JournalContent::IntentionContinued { .. }
                ));
            }
            service.action_result_update(&revisions, &executed).unwrap();
            service.refresh_readiness();
            private_entries.push(executed.entry);
        }
        for client in [&mut owner, &mut spectator, &mut observer] {
            let updates = drain(client);
            public_only(&updates);
            if service.clients[&client.id].actor == Some(ActorId(1)) && active {
                assert!(
                    updates
                        .iter()
                        .any(|message| message["update"]["body"]["type"] == "intention"),
                    "active work must actually publish ordinary intention status"
                );
            }
            service.handle(client.id, "ordinary-snapshot".into(), Request::Snapshot);
            let messages = drain(client);
            public_only(&messages);
            let snapshot = &messages
                .iter()
                .find(|message| message["type"] == "snapshot")
                .unwrap()["snapshot"];
            let observation = &snapshot["state"]["observation"];
            assert!(observation["interactions"].is_object());
            assert_eq!(observation["interactions"]["preparation"], Value::Null);
            if service.clients[&client.id].actor == Some(ActorId(1)) {
                assert_eq!(observation["combat"]["preparation_remaining"], json!("600"));
                assert_eq!(observation["combat"]["preparation_active"], json!(active));
                assert_eq!(snapshot["intentions"].as_array().unwrap().len(), 1);
                assert_eq!(snapshot["intentions"][0]["intention"], intention);
                assert_eq!(
                    snapshot["intentions"][0]["phase"],
                    json!(if active { "started" } else { "paused" })
                );
            } else {
                assert!(
                    observation["visible_actors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|actor| actor["name"] == "owner"),
                    "unrelated observer can actually see the preparing actor"
                );
            }
            service.handle(
                client.id,
                "ordinary-history".into(),
                Request::History {
                    before: None,
                    limit: 1,
                },
            );
            let messages = drain(client);
            public_only(&messages);
            let history = &messages
                .iter()
                .find(|message| message["type"] == "history")
                .unwrap()["page"];
            if service.clients[&client.id].actor == Some(ActorId(1)) {
                assert_eq!(history["entries"].as_array().unwrap().len(), 1);
                assert_eq!(history["entries"][0]["id"], json!(public.id));
                assert_eq!(history["older_before"], Value::Null);
            }
            for entry in &private_entries {
                assert!(service.engine.disclose_entry(entry).is_none());
                let actor = service.clients[&client.id].actor.unwrap();
                let user = &service.clients[&client.id].user;
                assert!(!entry.visible_to(actor, user));
                assert!(service
                    .engine
                    .history(actor, user, Some(&entry.id), 1)
                    .is_err());
            }
        }
        for denied in [&mut spectator, &mut observer] {
            let messages = query(&mut service, denied, json!({"kind":"overview"}));
            public_only(&messages);
            assert!(messages.iter().any(|message| message["type"] == "error"));
        }
        let messages = query(&mut service, &mut owner, json!({"kind":"overview"}));
        let progress = &page(&messages)["entries"][0]["value"]["respec"];
        assert_eq!(
            progress["basis"], original_basis,
            "snapshot resets never replace the accepted basis"
        );
        assert_eq!(progress["replacement"], replacement);
        assert_eq!(progress["intention"], intention);
        assert_eq!(progress["active"], json!(active));
        service.handle(owner.id, "private-respec".into(), original.clone());
        let duplicate = drain(&mut owner);
        public_only(&duplicate);
        assert!(duplicate.iter().any(|message| message["type"] == "ack"));
    }
}
