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
        form_choices: Vec::new(),
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

#[test]
fn owner_form_catalog_is_private_read_only_and_preserves_nested_choices() {
    let mut service = Service::new(
        Engine::memory(crate::advancement_test_support::form_scenario(2, false)).unwrap(),
    );
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    control(&mut service, &mut owner);
    let basis = page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let original = [ActorId(1), ActorId(2)].map(|actor| service.engine.state(actor).unwrap());
    let catalog = json!({"kind":"catalog","basis":basis,"catalog":"talent","offset":0});
    let rows = query(&mut service, &mut owner, catalog.clone());
    let root = page(&rows)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["value"]["selection"]["talent"] == "choice_root")
        .unwrap();
    assert_eq!(
        root["value"]["form_choices"],
        json!([{"id":"choice.form_0","name":"Form 0"},{"id":"choice.form_1","name":"Form 1"}])
    );
    let denied = query(&mut service, &mut spectator, catalog);
    no_private_reply(&denied);
    assert!(!serde_json::to_string(&denied)
        .unwrap()
        .contains("choice.form"));
    assert_eq!(
        [ActorId(1), ActorId(2)].map(|actor| service.engine.state(actor).unwrap()),
        original
    );
}

#[test]
fn owner_growth_source_rejects_spectators_lost_control_and_stale_basis() {
    let mut service = Service::new(Engine::memory(scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    let mut other = connect(&mut service, "other", AccessRole::Player, ActorId(1));
    control(&mut service, &mut owner);
    let b = page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let initial = [ActorId(1), ActorId(2)].map(|actor| service.engine.state(actor).unwrap());
    for denied in [&mut spectator, &mut other] {
        let request = command(
            &service,
            denied.id,
            json!({"type":"act","expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
            "action":{"type":"choose_growth_source","basis":b,"source":{"kind":"racial"}}}),
        );
        service.handle(denied.id, "choose-growth".into(), request);
        let messages = drain(denied);
        no_private_reply(&messages);
        assert!(messages.iter().any(|m| m["type"] == "error"));
        assert_eq!(
            [ActorId(1), ActorId(2)].map(|actor| service.engine.state(actor).unwrap()),
            initial
        );
    }
    let mut stale = b;
    stale["build_epoch"] = json!("9999");
    let request = command(
        &service,
        owner.id,
        json!({"type":"act","expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"choose_growth_source","basis":stale,"source":{"kind":"racial"}}}),
    );
    service.handle(owner.id, "stale-growth".into(), request);
    assert!(drain(&mut owner)
        .iter()
        .any(|m| m["code"] == "stale_context"));
    assert_eq!(
        [ActorId(1), ActorId(2)].map(|actor| service.engine.state(actor).unwrap()),
        initial
    );
}

#[test]
fn growth_source_receipt_precedes_reconnect_freshness_without_private_projection() {
    let mut service = Service::new(Engine::memory(scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    control(&mut service, &mut owner);
    let basis = page(&query(&mut service, &mut owner, json!({"kind":"overview"})))["basis"].clone();
    let original = command(
        &service,
        owner.id,
        json!({"type":"act",
        "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
        "action":{"type":"choose_growth_source","basis":basis,"source":{"kind":"racial"}}}),
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
    changed["command"]["action"]["source"] = json!({"kind":"class","class":"warrior"});
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
fn earned_growth_private_totals_stay_controller_only_alongside_shared_paid_history() {
    use crate::journal::{Action, Command};
    let mut engine = Engine::memory(crate::public_plan_test_fixture::growth_scenario()).unwrap();
    for (actor, target, ability) in [
        (1, 2, "fixture_observe_plan"),
        (2, 1, "slow_plan"),
        (1, 2, "growth_finisher"),
    ] {
        engine
            .command(
                "player",
                "test",
                ActorId(actor),
                ability,
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(ActorId(actor)).unwrap(),
                    action: Action::UseAbility {
                        id: tor_simulation::content_ids::AbilityId::new(ability).unwrap(),
                        invocation: crate::journal::AbilityInvocation::ActorTarget {
                            target: tor_simulation::ActorId(target),
                        },
                        supplies: Default::default(),
                    },
                },
            )
            .unwrap_or_else(|error| panic!("growth admission actor{actor} {ability}: {error:?}"));
        assert_eq!(engine.next_intention_actor(), Some(ActorId(actor)));
        engine
            .execute_next_intention()
            .unwrap_or_else(|error| panic!("growth execution actor{actor} {ability}: {error:?}"))
            .expect("growth fixture executed");
    }
    assert!(!engine.alive(ActorId(2)));
    let paid = engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .public_plan
        .unwrap();
    let mut service = Service::new(engine);
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    let mut observer = connect(&mut service, "observer", AccessRole::Player, ActorId(1));
    let mut foreign = connect(&mut service, "foreign", AccessRole::Spectator, ActorId(2));
    control(&mut service, &mut owner);
    for client in [&mut spectator, &mut observer, &mut foreign] {
        drain(client);
        service.handle(client.id, "snapshot-growth".into(), Request::Snapshot);
        let snapshots = drain(client);
        no_private_reply(&snapshots);
        assert!(!serde_json::to_string(&snapshots)
            .unwrap()
            .contains("earned_xp"));
        let snapshot = snapshots
            .iter()
            .find(|message| message["type"] == "snapshot")
            .unwrap();
        let expected = if service.clients[&client.id].actor == Some(ActorId(1)) {
            serde_json::to_value(&paid).unwrap()
        } else {
            Value::Null
        };
        assert_eq!(
            snapshot["snapshot"]["state"]["observation"]["combat"]["public_plan"],
            expected
        );
        let denied = query(&mut service, client, json!({"kind":"overview"}));
        no_private_reply(&denied);
        assert!(!serde_json::to_string(&denied)
            .unwrap()
            .contains("earned_xp"));
        assert!(denied.iter().any(|message| message["type"] == "error"));
    }
    let authorized = query(&mut service, &mut owner, json!({"kind":"overview"}));
    assert_eq!(
        page(&authorized)["entries"][0]["value"]["growth"]["earned_xp"],
        "100"
    );
    assert_eq!(
        page(&authorized)["entries"][0]["value"]["growth"]["spent_xp"],
        "100"
    );
}
fn admit_capacity_ability(
    service: &mut Service,
    client: &mut Connection,
    actor: ActorId,
    id: &str,
    target: u64,
) {
    let action = service.engine.encode_action(
        actor,
        &crate::journal::Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new(id).unwrap(),
            invocation: crate::journal::AbilityInvocation::ActorTarget {
                target: tor_simulation::ActorId(target),
            },
            supplies: Default::default(),
        },
    );
    let request = command(
        service,
        client.id,
        json!({
            "type":"act", "expected_revision":service.engine.revision(actor).unwrap().to_string(),
            "action":action
        }),
    );
    service.handle(client.id, id.into(), request);
    let messages = drain(client);
    assert!(
        messages
            .iter()
            .any(|message| message["type"] == "ack" && message["request_id"] == id),
        "ordinary admission actor{} {id}: {messages:?}",
        actor.0
    );
    assert!(
        !messages.iter().any(|message| message["type"] == "error"),
        "ordinary admission actor{} {id}: {messages:?}",
        actor.0
    );
    assert_eq!(service.engine.next_intention_actor(), Some(actor));
}

fn capacity_service(maximum: bool) -> (Service, Connection, Vec<Connection>) {
    use crate::engine::growth_capacity_transaction_tests as fixture;
    let mut service = Service::new(Engine::memory(fixture::scenario()).unwrap());
    let mut owner = connect(&mut service, "owner", AccessRole::Player, ActorId(1));
    let mut victim = connect(&mut service, "victim", AccessRole::Player, ActorId(2));
    control(&mut service, &mut owner);
    service.handle(victim.id, "control-victim".into(), Request::AcquireControl);
    let replies = drain(&mut victim);
    assert!(
        !replies.iter().any(|message| message["type"] == "error"),
        "victim control: {replies:?}"
    );
    assert_eq!(service.controllers.get(&ActorId(2)), Some(&victim.id));
    admit_capacity_ability(
        &mut service,
        &mut owner,
        ActorId(1),
        "fixture_observe_plan",
        2,
    );
    assert!(
        matches!(service.step(), Step::Progress),
        "ordinary read setup must execute"
    );
    admit_capacity_ability(&mut service, &mut victim, ActorId(2), "slow_plan", 1);
    assert!(
        matches!(service.step(), Step::Progress),
        "ordinary slow-plan setup must execute"
    );
    assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
    assert!(service
        .engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .public_plan
        .is_some());
    if maximum {
        // Test-only strict saved authority, after constructor/control/ordinary
        // setup. Restart pausing must not alter the active fatal control.
        service.engine = fixture::with_maximum_xp(&service.engine);
    }
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator, ActorId(1));
    let mut foreign = connect(&mut service, "foreign", AccessRole::Spectator, ActorId(2));
    for client in [&mut owner, &mut victim, &mut spectator, &mut foreign] {
        drain(client);
    }
    (service, owner, vec![victim, spectator, foreign])
}

#[test]
fn real_fatal_xp_capacity_failure_reaches_attached_controller_once_without_reattach() {
    use crate::engine::growth_capacity_transaction_tests as fixture;
    let (mut control, mut control_owner, _control_others) = capacity_service(false);
    admit_capacity_ability(
        &mut control,
        &mut control_owner,
        ActorId(1),
        "growth_finisher",
        2,
    );
    assert!(
        matches!(control.step(), Step::Progress),
        "identical nonmax Service work must actually complete its fatal effect"
    );
    assert!(
        !control.engine.alive(ActorId(2)),
        "positive Service control reaches death before checking overflow delivery"
    );
    let control_page = query(&mut control, &mut control_owner, json!({"kind":"overview"}));
    assert_eq!(
        page(&control_page)["entries"][0]["value"]["growth"]["held_xp"],
        "100"
    );

    let (mut service, mut owner, mut others) = capacity_service(true);
    admit_capacity_ability(&mut service, &mut owner, ActorId(1), "growth_finisher", 2);
    let before = fixture::authority(&service.engine);
    assert!(matches!(service.step(), Step::Blocked));
    service.poll_saves();
    assert_eq!(fixture::authority(&service.engine), before);
    let messages = drain(&mut owner);
    let errors: Vec<_> = messages
        .iter()
        .filter(|message| message["type"] == "error")
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "attached controller must receive one generic resource_limit from the actual failed step"
    );
    assert_eq!(errors[0]["scope"]["type"], "attached");
    assert!(errors[0]["request_id"].is_null());
    assert_eq!(errors[0]["code"], "resource_limit");
    assert_eq!(
        errors[0]["message"],
        "Simulation resource capacity exhausted"
    );
    let encoded = serde_json::to_string(&messages).unwrap();
    for private in [
        "18446744073709551615",
        "earned_xp",
        "held_xp",
        "spent_xp",
        "highest_earned_tier",
    ] {
        assert!(
            !encoded.contains(private),
            "ordinary public output leaked {private}"
        );
    }
    for client in &mut others {
        let messages = drain(client);
        assert!(
            !messages.iter().any(|message| message["type"] == "error"),
            "queued execution failure belongs to its controller, not other attachments"
        );
        assert!(messages
            .iter()
            .all(|message| message["type"] != "advancement_page"));
    }
    for _ in 0..2 {
        assert!(matches!(service.step(), Step::Blocked));
        service.poll_saves();
        assert_eq!(fixture::authority(&service.engine), before);
        assert!(
            drain(&mut owner)
                .iter()
                .all(|message| message["type"] != "error"),
            "the same blocked work must not spam repeated warnings"
        );
    }
}

#[test]
fn real_capacity_notice_reaches_a_new_controller_after_suspend_and_resume() {
    use crate::engine::growth_capacity_transaction_tests as fixture;
    let (mut service, mut first, _others) = capacity_service(true);
    admit_capacity_ability(&mut service, &mut first, ActorId(1), "growth_finisher", 2);
    assert!(matches!(service.step(), Step::Blocked));
    assert_eq!(
        drain(&mut first)
            .iter()
            .filter(|m| m["type"] == "error" && m["code"] == "resource_limit")
            .count(),
        1
    );
    let before_reset = fixture::authority(&service.engine);
    let old_stream = service.clients[&first.id].context.clone();
    service.handle(first.id, "reset-failed-work".into(), Request::Snapshot);
    assert_ne!(service.clients[&first.id].context, old_stream);
    assert!(drain(&mut first)
        .iter()
        .any(|message| message["type"] == "snapshot"));
    assert!(matches!(service.step(), Step::Blocked));
    assert_eq!(fixture::authority(&service.engine), before_reset);
    assert_eq!(
        drain(&mut first)
            .iter()
            .filter(|message| message["type"] == "error" && message["code"] == "resource_limit")
            .count(),
        1,
        "the same connection's new stream needs its own scoped capacity notice"
    );
    assert!(matches!(service.step(), Step::Blocked));
    assert!(drain(&mut first)
        .iter()
        .all(|message| message["type"] != "error"));
    let admission = service.engine.pending_intentions(ActorId(1))[0]
        .intention
        .clone();
    service.handle(
        first.id,
        "release-failed-work".into(),
        Request::ReleaseControl,
    );
    assert!(matches!(service.step(), Step::Blocked));
    drain(&mut first);
    let mut replacement = connect(&mut service, "replacement", AccessRole::Player, ActorId(1));
    control(&mut service, &mut replacement);
    let request = command(
        &service,
        replacement.id,
        json!({
            "type":"resume_intention", "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
            "intention":admission
        }),
    );
    service.handle(replacement.id, "resume-failed-work".into(), request);
    let resumed = drain(&mut replacement);
    assert!(
        resumed.iter().any(
            |message| message["type"] == "ack" && message["request_id"] == "resume-failed-work"
        ),
        "ordinary resume: {resumed:?}"
    );
    let before = fixture::authority(&service.engine);
    assert!(matches!(service.step(), Step::Blocked));
    service.poll_saves();
    assert_eq!(fixture::authority(&service.engine), before);
    let messages = drain(&mut replacement);
    let errors: Vec<_> = messages.iter().filter(|m| m["type"] == "error").collect();
    assert_eq!(
        errors.len(),
        1,
        "prior delivery cannot suppress a new controller/stream"
    );
    assert_eq!(errors[0]["code"], "resource_limit");
    assert_eq!(
        errors[0]["message"],
        "Simulation resource capacity exhausted"
    );
    assert_eq!(errors[0]["scope"]["type"], "attached");
    assert!(drain(&mut first)
        .iter()
        .all(|message| message["type"] != "error"));
    assert!(matches!(service.step(), Step::Blocked));
    assert!(drain(&mut replacement)
        .iter()
        .all(|message| message["type"] != "error"));
    assert_eq!(fixture::authority(&service.engine), before);
}

#[test]
fn real_capacity_notice_output_refusal_disconnects_without_partial_simulation_effects() {
    use crate::engine::growth_capacity_transaction_tests as fixture;
    let (mut service, mut owner, mut others) = capacity_service(true);
    admit_capacity_ability(&mut service, &mut owner, ActorId(1), "growth_finisher", 2);
    let before = fixture::authority(&service.engine);
    for _ in 0..QUEUE {
        service.send(owner.id, ServerMessage::Waiting { on: Waiting::You });
    }
    assert!(service.clients.contains_key(&owner.id));
    // Inject refusal at the ordinary executor's final output boundary, after
    // the runner's headroom gate. The simulation must still reject atomically.
    assert!(matches!(
        service.execute_queued_intention(ActorId(1)),
        Step::Blocked
    ));
    assert_eq!(fixture::authority(&service.engine), before);
    assert!(!service.clients.contains_key(&owner.id));
    assert!(!service.controllers.contains_key(&ActorId(1)));
    assert!(owner.close.borrow().is_some());
    assert!(service.pending_pauses.contains(&ActorId(1)));
    assert!(
        service.reported_execution_failure.is_none(),
        "refused output is not a delivered notice"
    );
    for client in &mut others {
        assert!(drain(client)
            .iter()
            .all(|message| message["type"] != "error"));
    }
}
