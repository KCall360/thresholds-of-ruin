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
        "strength_acquired",
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
        "action":{"type":"purchase","basis":original,"purchase":{"owner":"0","pool":"advancement",
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
        "action":{"type":"purchase","basis":fresh,"purchase":{"owner":"0","pool":"advancement",
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
        "action":{"type":"purchase","basis":basis,"purchase":{"owner":"0","pool":"advancement",
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
        "action":{"type":"purchase","basis":basis,"purchase":{"owner":"0","pool":"advancement",
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
        expertise_grants: Vec::new(),
        form_choices: Vec::new(),
        id: tor_simulation::content_ids::TalentId::new(CANARY).unwrap(),
        name: "Private confirmation choice".into(),
        prerequisites: vec![],
        requirement: tor_simulation::rules::Requirement::TotalHd {
            minimum: tor_simulation::HdCount(1),
        },
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
    let replacement = json!({"source":{"owner":"0","index":0},"replacement":{
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
                        invocation: crate::journal::AbilityInvocation {
                            aim: crate::journal::AbilityAim::ActorTarget {
                                target: tor_simulation::ActorId(target),
                            },
                            weapon: crate::journal::WeaponSelection::Default,
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
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: tor_simulation::ActorId(target),
                },
                weapon: crate::journal::WeaponSelection::Default,
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
    growth_service(
        crate::engine::growth_capacity_transaction_tests::scenario(),
        maximum,
    )
}

fn growth_service(
    scenario: crate::Scenario,
    maximum: bool,
) -> (Service, Connection, Vec<Connection>) {
    use crate::engine::growth_capacity_transaction_tests as fixture;
    let mut service = Service::new(Engine::memory(scenario).unwrap());
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
fn real_growth_invariant_refusal_reaches_only_its_controller_and_can_be_cancelled() {
    use crate::engine::growth_capacity_transaction_tests as fixture;
    let mut scenario = fixture::scenario();
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["characters"][0]["creature"]["future_growth"] =
        json!({"kind":"selected","source":{"kind":"class","class":"mage"}});
    manifest["creatures"]["species"]["reader"]["melee"]["strikes"][0]["on_success"][0]["damage"]
        ["components"][0]["amount"] = json!({"kind":"fixed","amount":0});
    let mut rules = package.rules().unwrap().document().clone();
    let melee = rules
        .abilities
        .iter_mut()
        .find(|ability| ability.id.as_str() == "basic_melee")
        .unwrap();
    melee.interaction = tor_simulation::rules::Interaction::NonHostile;
    melee.strikes =
        serde_json::from_value(json!({"kind":"natural_melee","additional_riders":[]})).unwrap();
    rules
        .classes
        .iter_mut()
        .find(|class| class.id.as_str() == "mage")
        .unwrap()
        .modifiers
        .push(
            serde_json::from_value(json!({"scope":"always","op":{
                "kind":"add","stat":{"kind":"ability","ability":"basic_melee","stat":"flat_bonus"},
                "value":1
            }}))
            .unwrap(),
        );
    // The package compiler admits the intact current racial build. Only its
    // next genuinely earned Mage ordinal contradicts the harmless interaction.
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    let (mut service, mut owner, mut others) = growth_service(scenario, false);
    admit_capacity_ability(&mut service, &mut owner, ActorId(1), "growth_finisher", 2);
    let before = fixture::authority(&service.engine);
    let queued = service.engine.pending_intentions(ActorId(1));
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
        "the refused execution must report its invariant failure"
    );
    assert_eq!(errors[0]["code"], "invalid_action");
    assert!(errors[0]["request_id"].is_null());
    assert_eq!(
        errors[0]["message"],
        "Simulation state could not be updated; no changes were committed"
    );
    assert!(
        service.save_warning.is_none(),
        "a simulation refusal is not a storage warning"
    );
    for client in &mut others {
        assert!(drain(client)
            .iter()
            .all(|message| message["type"] != "error"));
    }
    assert!(matches!(service.step(), Step::Blocked));
    assert_eq!(fixture::authority(&service.engine), before);
    assert!(drain(&mut owner)
        .iter()
        .all(|message| message["type"] != "error"));
    assert_eq!(service.engine.pending_intentions(ActorId(1)), queued);
    let request = command(
        &service,
        owner.id,
        json!({
            "type":"cancel_intention", "expected_revision":service.engine.revision(ActorId(1)).unwrap().to_string(),
            "intention":queued[0].intention, "step":null
        }),
    );
    service.handle(owner.id, "cancel-refused-growth".into(), request);
    let messages = drain(&mut owner);
    assert!(messages.iter().any(|message| message["type"] == "ack"));
    assert!(messages.iter().all(|message| message["type"] != "error"));
    assert!(service.engine.pending_intentions(ActorId(1)).is_empty());
    assert!(service.engine.alive(ActorId(2)));
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

/// Controlled opposition proves canonical mechanism composition, not autonomous
/// W-fight survivability. Every actor has an ordinary, actor-scoped controller.
#[test]
fn canonical_wraith_earned_regrowth_survives_source_free_service_reopen() {
    use crate::{scenario_package, SavePolicy, Scenario};
    use tor_protocol::IntentionPhase;

    const OWNER: ActorId = ActorId(1);
    const WRAITH: ActorId = ActorId(11);
    const DRAIN: &str = "rogue.energy_drain";

    // This existing read-only test seam captures strict game authority without
    // enabling wizard mode or exposing a new Engine gameplay/inspection API.
    fn saved(service: &Service) -> Value {
        serde_json::to_value(
            crate::engine::Checkpoint::capture(&service.engine).encode("controlled-wraith", 0),
        )
        .unwrap()
    }
    fn creature(saved: &Value, actor: ActorId) -> &Value {
        let actors = saved["game"]["actors"].as_u64().unwrap() as usize;
        &saved["shared"]["actors"][actors][actor.0.to_string()]["combat"]["creature"]
    }
    fn owned_rows(build: &Value, kind: &str) -> Value {
        json!(build["hd"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, row)| row["owner"]["kind"] == kind)
            .map(|(index, row)| json!({"hit_die":row,"choices":build["choices"][index]}))
            .collect::<Vec<_>>())
    }
    fn paid_choice(owner: Value, selection: Value) -> Value {
        json!({"purchase_highwater":1,"purchases":[{
            "id":{"owner":owner,"serial":1},"pool":"advancement","selection":selection
        }],"attribute":null})
    }
    // hd is PERSONAL advancement, independent of the effective body row.
    fn progress(service: &Service, earned: u64, spent: u64, hd: u64) -> Value {
        let snapshot = saved(service);
        let owner = creature(&snapshot, OWNER);
        let xp = &owner["earned_progress"];
        for (field, expected) in [
            ("earned_xp", earned),
            ("spent_xp", spent),
            ("held_xp", earned - spent),
            ("authored_baseline", 2),
            ("highest_applied_tier", 2),
            ("highest_earned_tier", 2),
        ] {
            assert_eq!(xp[field], json!(expected), "{field}");
        }
        assert_eq!(
            owner["build"]["hd"].as_array().unwrap().len() as u64,
            hd + 1
        );
        assert_eq!(
            owned_rows(&owner["build"], "body")
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            owned_rows(&owner["build"], "personal")
                .as_array()
                .unwrap()
                .len() as u64,
            hd
        );
        assert_eq!(owner["dead"], false);
        let view = service.engine.observation(OWNER).unwrap().combat.unwrap();
        assert!(!view.dead && !view.terminal);
        let raw_injury = owner["injury"].as_u64().unwrap();
        assert!(view.hp > 0 && view.hp <= view.max_hp);
        // Saved injury is anchored at active_since; the public view projects
        // ordinary elapsed recovery. Only a no-time reopen requires exact raw
        // injury equality. With no ongoing damaging condition, projection can
        // reduce injury but cannot manufacture it.
        assert!(view.max_hp - view.hp <= raw_injury);
        let actors = snapshot["game"]["actors"].as_u64().unwrap() as usize;
        if snapshot["shared"]["actors"][actors]["1"]["active_since"] == snapshot["game"]["tick"] {
            assert_eq!(view.max_hp - view.hp, raw_injury);
        }
        assert_eq!(view.own_stats.unwrap().restorable_losses.hit_dice, 2 - hd);
        for actor in (3..=12).map(ActorId) {
            assert_eq!(
                creature(&snapshot, actor)["earned_progress"]["earned_xp"],
                0,
                "cooperating controllers cannot earn the owner's kill credit"
            );
        }
        snapshot
    }
    fn lore(service: &Service) -> u8 {
        service
            .engine
            .observation(OWNER)
            .unwrap()
            .combat
            .unwrap()
            .own_stats
            .unwrap()
            .skills
            .iter()
            .find(|row| row.skill.id == "lore")
            .unwrap()
            .rank
    }

    struct Controlled {
        service: Service,
        clients: BTreeMap<ActorId, Connection>,
        acts: usize,
        resolved: BTreeSet<String>,
        events: Vec<CombatEventView>,
        steps: usize,
        resumes: usize,
        branch: BranchId,
        batches: BTreeSet<(String, usize)>,
    }
    impl Controlled {
        fn connect(service: Service, counts: (usize, usize, usize)) -> Self {
            let branch = service.engine.branch().clone();
            let mut case = Self {
                service,
                clients: BTreeMap::new(),
                acts: counts.0,
                resolved: BTreeSet::new(),
                events: Vec::new(),
                steps: counts.1,
                resumes: counts.2,
                branch,
                batches: BTreeSet::new(),
            };
            for actor in case.service.engine.actors() {
                if !case.service.engine.alive(actor) {
                    continue;
                }
                assert!(!case.service.engine.is_ai(actor));
                let mut client = case
                    .service
                    .connect(
                        &Account {
                            role: AccessRole::Player,
                            user: format!("controlled-{}", actor.0),
                            token: "test-only".into(),
                            actors: BTreeSet::from([actor]),
                        },
                        "headless".into(),
                    )
                    .unwrap();
                drain(&mut client);
                case.service
                    .handle(client.id, "attach".into(), Request::Attach { actor });
                assert!(drain(&mut client).iter().any(|m| m["type"] == "snapshot"));
                case.service
                    .handle(client.id, "control".into(), Request::AcquireControl);
                assert!(drain(&mut client).iter().all(|m| m["type"] != "error"));
                assert_eq!(case.service.controllers.get(&actor), Some(&client.id));
                assert_eq!(
                    case.service.clients[&client.id].allowed,
                    BTreeSet::from([actor])
                );
                case.clients.insert(actor, client);
                case.drain();
            }
            case
        }
        fn drain(&mut self) -> Vec<Value> {
            let mut all = Vec::new();
            for client in self.clients.values_mut() {
                assert!(
                    self.service.clients.contains_key(&client.id),
                    "lost expected controller"
                );
                for message in drain(client) {
                    assert_ne!(message["type"], "error", "Service refusal: {message}");
                    let status = &message["update"]["body"]["status"];
                    if message["update"]["body"]["type"] == "intention" {
                        assert_ne!(status["phase"], "failed", "paid work failed: {message}");
                        assert_ne!(
                            status["phase"], "cancelled",
                            "paid work cancelled: {message}"
                        );
                        if status["phase"] == "resolved" {
                            self.resolved
                                .insert(status["intention"].as_str().unwrap().into());
                        }
                    }
                    all.push(message);
                }
            }
            assert!(self.service.save_warning.is_none(), "storage warning");
            all
        }
        fn admit(&mut self, actor: ActorId, action: Value) -> String {
            assert_eq!(self.service.engine.next_actor(), Some(actor));
            assert!(self.service.engine.pending_intentions(actor).is_empty());
            assert!(self.service.engine.input_readiness(actor).admission);
            self.acts += 1;
            assert!(self.acts <= 8_000, "controlled Act admission bound");
            let client = self.clients[&actor].id;
            let revision = self.service.engine.revision(actor).unwrap();
            let wire: tor_protocol::Action = serde_json::from_value(action.clone()).unwrap();
            let decoded = self.service.engine.decode_action(actor, &wire).unwrap();
            let original = crate::journal::Command::AdmitIntention {
                expected_revision: revision,
                action: decoded,
            };
            let request_id = format!("controlled-{}", self.acts);
            let request = command(
                &self.service,
                client,
                json!({"type":"act",
                "expected_revision":revision.to_string(), "action":action}),
            );
            self.service.handle(client, request_id.clone(), request);
            let messages = self.drain();
            let acks: Vec<_> = messages
                .iter()
                .filter(|m| m["type"] == "ack" && m["request_id"] == request_id)
                .collect();
            assert_eq!(acks.len(), 1, "one correlated ordinary Act admission");
            let receipt = &acks[0]["receipt"];
            assert_eq!(receipt["type"], "admitted");
            let admission = receipt["intention"].as_str().unwrap().to_owned();
            // Read-only receipt lookup proves authenticated journal ownership;
            // it does not execute, retry, or inject any gameplay action.
            let user = format!("controlled-{}", actor.0);
            let accepted = self
                .service
                .engine
                .retry(
                    &user,
                    actor,
                    &request_id,
                    self.service.engine.branch(),
                    &original,
                )
                .unwrap()
                .unwrap();
            assert_eq!(accepted.entry.id.0, admission);
            assert_eq!(accepted.entry.actor, actor);
            assert_eq!(accepted.entry.author, Author::User { user });
            assert!(matches!(
                accepted.entry.content,
                crate::journal::JournalContent::IntentionAdmitted { .. }
            ));
            admission
        }
        fn step(&mut self) {
            self.steps += 1;
            assert!(
                self.steps <= 40_000,
                "Service step-call bound, independent of active ticks"
            );
            assert_eq!(self.service.engine.branch(), &self.branch);
            assert!(self.service.travels.is_empty());
            assert!(self.service.pending_pauses.is_empty());
            assert!(self.service.pending_travel_cancellations.is_empty());
            let due = self.service.engine.next_intention_actor();
            if due.is_some() {
                assert_eq!(
                    self.service.engine.next_intention_origin(),
                    Some(tor_simulation::IntentionOrigin::Human)
                );
            }
            let records = self.service.engine.profile_counts().0;
            match self.service.step() {
                Step::Progress => {
                    assert!(due.is_some(), "one previously admitted human execution");
                    assert_eq!(self.service.engine.branch(), &self.branch);
                    assert_eq!(
                        self.service.engine.profile_counts().0,
                        records + 1,
                        "one committed record per successful Service execution"
                    );
                    self.drain(); // Reject failed/cancelled work before reading events.
                    assert!(
                        self.batches.insert((self.branch.0.clone(), records)),
                        "an execution batch is read exactly once"
                    );
                    let state = self.service.engine.state(OWNER).unwrap();
                    // Every successful ordinary/resumed execution clears the prior
                    // event_log in commit_action -> begin_action_boundary. Preserve
                    // this entire batch, including equal events; never deduplicate.
                    self.events
                        .extend(state.observation.combat.as_ref().unwrap().events.clone());
                    let combat = state.observation.combat.unwrap();
                    assert!(
                        !combat.dead && !combat.terminal,
                        "unchanged-kit seed0 failed; do not repair"
                    );
                    // Tick is a public observation field; every ordinary actor's
                    // Wait contributes to the same finite active execution.
                    assert!(
                        state.observation.tick <= 40_000,
                        "controlled active-tick bound"
                    );
                }
                Step::Full(_) => {
                    panic!("all eleven response queues must be drained after each step")
                }
                Step::Blocked => {
                    assert_eq!(
                        self.service.engine.profile_counts().0,
                        records,
                        "blocked Service steps cannot append hidden work"
                    );
                    self.drain();
                    let actor = self
                        .service
                        .engine
                        .next_actor()
                        .expect("live controlled scheduler");
                    assert_eq!(
                        self.service.controllers.get(&actor),
                        Some(&self.clients[&actor].id)
                    );
                    let pending = self.service.engine.pending_intentions(actor);
                    if let Some(work) = pending.first() {
                        assert_eq!(pending.len(), 1, "one paid action at a time");
                        assert!(matches!(
                            work.phase,
                            IntentionPhase::Paused | IntentionPhase::Suspended
                        ));
                        assert!(self
                            .service
                            .engine
                            .input_readiness(actor)
                            .resume
                            .contains(&work.intention));
                        let client = self.clients[&actor].id;
                        let request = command(
                            &self.service,
                            client,
                            json!({"type":"resume_intention",
                            "expected_revision":self.service.engine.revision(actor).unwrap().to_string(),
                            "intention":work.intention}),
                        );
                        self.resumes += 1;
                        assert!(self.resumes <= 8_000, "independent resume-request bound");
                        let request_id = format!("resume-{}", self.resumes);
                        self.service.handle(client, request_id.clone(), request);
                        let messages = self.drain();
                        let acks: Vec<_> = messages
                            .iter()
                            .filter(|m| m["type"] == "ack" && m["request_id"] == request_id)
                            .collect();
                        assert_eq!(acks.len(), 1, "one correlated resume receipt");
                        let ack = acks[0];
                        assert_eq!(ack["receipt"]["type"], "immediate");
                        assert_eq!(ack["receipt"]["actor"], json!(actor));
                        assert_eq!(ack["receipt"]["branch"], json!(self.branch));
                        assert!(ack["receipt"]["entry_id"].is_string());
                        assert!(
                            messages.iter().any(|message| {
                                let body = &message["update"]["body"];
                                body["type"] == "intention"
                                    && body["status"]["actor"] == json!(actor)
                                    && body["status"]["branch"] == json!(self.branch)
                                    && body["status"]["intention"] == json!(work.intention)
                                    && body["status"]["phase"] == "queued"
                            }),
                            "resume queues the original paid intention"
                        );
                        assert!(self
                            .service
                            .engine
                            .pending_intentions(actor)
                            .iter()
                            .any(|queued| queued.intention == work.intention
                                && queued.phase == IntentionPhase::Queued));
                    } else {
                        self.admit(actor, json!({"type":"wait"}));
                    }
                }
            }
        }
        fn turn(&mut self, actor: ActorId) {
            while self.service.engine.next_actor() != Some(actor)
                || !self.service.engine.pending_intentions(actor).is_empty()
            {
                self.step();
            }
        }
        fn act(&mut self, actor: ActorId, action: Value) -> Vec<CombatEventView> {
            self.turn(actor);
            let start = self.events.len();
            let admission = self.admit(actor, action);
            while !self.resolved.contains(&admission)
                || !self.service.engine.pending_intentions(actor).is_empty()
            {
                self.step();
            }
            assert!(
                self.service.engine.actors().into_iter().all(|id| self
                    .service
                    .engine
                    .pending_intentions(id)
                    .is_empty()),
                "no paid work remains at an action boundary"
            );
            self.events[start..].to_vec()
        }
        fn target(&self, observer: ActorId, name: &str, y: i32) -> ActorTarget {
            let view = self.service.engine.observation(observer).unwrap();
            let targets: Vec<_> = view
                .visible_actors
                .iter()
                .filter(|other| {
                    other.id != view.self_target
                        && other.name == name
                        && other.position == (Position { x: 0, y, z: 0 })
                })
                .collect();
            assert_eq!(targets.len(), 1, "one actually disclosed cardinal contact");
            targets[0].id
        }
        fn purchase(&mut self, ordinal: u64, selection: Value) {
            self.turn(OWNER);
            let before = saved(&self.service);
            let owner = self.clients.get_mut(&OWNER).unwrap();
            let basis =
                page(&query(&mut self.service, owner, json!({"kind":"overview"})))["basis"].clone();
            let purchase = json!({"owner":(ordinal - 1).to_string(),"pool":"advancement","selection":selection});
            let preview = query(
                &mut self.service,
                owner,
                json!({"kind":"preview","basis":basis,
                "proposal":{"kind":"purchase","purchase":purchase},"section":"summary","offset":0}),
            );
            assert_eq!(page(&preview)["entries"][0]["kind"], "summary");
            assert_eq!(
                saved(&self.service),
                before,
                "ordinary preview is read-only"
            );
            self.act(
                OWNER,
                json!({"type":"purchase","basis":basis,"purchase":purchase}),
            );
        }
        fn kill(
            &mut self,
            name: &str,
            reward: u64,
            earned: u64,
            spent: u64,
            hd: u64,
        ) -> ActorTarget {
            self.turn(OWNER);
            let target = self.target(OWNER, name, 1);
            for attempt in 0..64 {
                self.turn(OWNER);
                assert_eq!(
                    self.target(OWNER, name, 1),
                    target,
                    "lost target is not a death"
                );
                let stats = self
                    .service
                    .engine
                    .observation(OWNER)
                    .unwrap()
                    .combat
                    .unwrap()
                    .own_stats
                    .unwrap();
                let technique = stats
                    .abilities
                    .iter()
                    .find(|a| a.id == "rattling_blow")
                    .unwrap();
                let paid = technique.cooldown_remaining == 0
                    && technique.cost.as_ref().is_none_or(|cost| {
                        stats.resources.iter().any(|r| {
                            r.resource.id == cost.resource.id
                                && r.available >= cost.start + cost.resolution
                        })
                    });
                let events = self.act(
                    OWNER,
                    json!({"type":"use_ability","supplies":{"kind":"carried"},
                    "id":if paid {"rattling_blow"} else {"basic_melee"},
                    "invocation":{"aim":{"kind":"actor_target","target":target},"weapon":{"kind":"default"}}}),
                );
                let deaths: Vec<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        CombatEventView::Died { actor, .. } => Some(*actor),
                        _ => None,
                    })
                    .collect();
                if !deaths.is_empty() {
                    assert_eq!(deaths, [target], "one real death per finite target");
                    let observed = self.service.engine.observation(OWNER).unwrap().self_target;
                    assert!(events.iter().any(|event| matches!(event,
                        CombatEventView::Ability { caster: Some(caster), target: Some(victim),
                            outcome: AbilityOutcome::Applied, .. } if *caster == observed && *victim == target)),
                        "fatal contact must belong to the admitted owner attack");
                    let history = self
                        .service
                        .engine
                        .history(OWNER, "controlled-1", None, 1)
                        .unwrap();
                    let latest = history
                        .entries
                        .last()
                        .expect("ordinary owner attack history");
                    assert_eq!(latest.actor, OWNER);
                    assert!(matches!(&latest.content,
                        HistoryContent::Action { action: tor_protocol::Action::UseAbility {
                            id, invocation: tor_protocol::AbilityInvocation { aim: tor_protocol::AbilityAim::ActorTarget { target: victim }, weapon: tor_protocol::WeaponSelection::Default }, .. }, .. }
                            if *victim == target && matches!(id.as_str(), "rattling_blow" | "basic_melee")));
                    progress(&self.service, earned + reward, spent, hd);
                    return target;
                }
                let prior_hd = if earned < 160 { 2 } else { 1 };
                progress(&self.service, earned, 0, prior_hd);
                assert!(attempt < 63, "64 attack admissions exhausted for {name}");
            }
            unreachable!()
        }
        fn recover(&mut self) {
            let mut waits = 0;
            loop {
                self.turn(OWNER);
                let view = self.service.engine.observation(OWNER).unwrap();
                let combat = view.combat.as_ref().unwrap();
                if combat.hp == combat.max_hp {
                    return;
                }
                let stats = combat.own_stats.as_ref().unwrap();
                let care = stats
                    .abilities
                    .iter()
                    .find(|a| a.id == "bind_wounds")
                    .unwrap();
                let hungry = matches!(stats.nutrition, NutritionView::Required { reserve, .. } if reserve < 300);
                let action = if hungry {
                    let ration = view
                        .inventory
                        .iter()
                        .find(|item| item.name == "food ration")
                        .expect("unchanged finite ration exhausted");
                    json!({"type":"eat","item":ration.id})
                } else if combat.max_hp - combat.hp >= 2
                    && care.cooldown_remaining == 0
                    && view
                        .inventory
                        .iter()
                        .any(|item| item.name == "clean bandage" && item.quantity > 0)
                {
                    json!({"type":"use_ability","supplies":{"kind":"carried"},"id":"bind_wounds",
                        "invocation":{"aim":{"kind":"actor_target","target":view.self_target},"weapon":{"kind":"default"}}})
                } else {
                    waits += 1;
                    assert!(
                        waits <= 128,
                        "ordinary recovery-Wait bound between DR2 attempts"
                    );
                    json!({"type":"wait"})
                };
                self.act(OWNER, action);
            }
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("controlled-source");
    let path = directory.path().join("controlled-wraith.db");
    let production =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let certificate = {
        let mut manifest = scenario_package::resolve_source(&production)
            .unwrap()
            .manifest;
        manifest.id = "rogue-wraith-controlled-regrowth".into();
        manifest.characters.retain(|character| character.id == 1);
        manifest.characters[0].anchor = "1/start".into();
        let warrior = manifest.characters[0].creature.as_mut().unwrap();
        warrior
            .hit_dice
            .push(serde_json::from_value(json!({"source":"warrior","purchases":[]})).unwrap());
        manifest.procedural_worlds.clear();
        manifest.generation_recipes.clear();
        manifest.generation_groups.clear();
        manifest.stair_pairs.clear();
        manifest.floor_activation = None;
        manifest.unique_retrieval = None;
        manifest.objective = None;
        // Preserve resolved production catalogs, rules, loadout, themes/assets,
        // and actor turn timings. Only placement/control and one empty HD differ.
        let region: scenario_package::RegionDef = serde_json::from_value(json!({
            "id":1,"name":"Controlled canonical W regrowth","size":[13,5,2],
            "chamber":true,"gravity":[0,0,-1],"anchors":{"start":[2,1,0]},
            "actors":(0..10).map(|index| json!({"id":index + 3,"at":[index + 2,2,0],
                "archetype":if index == 8 {"rogue.wraith"} else {"rogue.emu"},
                "controller":"external"})).collect::<Vec<_>>()
        }))
        .unwrap();
        let rules = std::fs::read_to_string(production.join(&manifest.ruleset)).unwrap();
        scenario_package::write_package(&source, &manifest, &[region], &rules).unwrap();
        scenario_package::validate(&source).unwrap()
    };
    let policy = SavePolicy {
        checkpoint_interval: 4096,
        ..Default::default()
    };
    let scenario = scenario_package::load(&source, 0, Some(1), false).unwrap();
    assert_eq!(scenario.package.as_ref().unwrap().certificate, certificate);
    assert_eq!(
        scenario.package.as_ref().unwrap().manifest.id,
        "rogue-wraith-controlled-regrowth"
    );
    assert!(!certificate.content_hash.is_empty() && !certificate.ruleset_hash.is_empty());
    let mut case = Controlled::connect(
        Service::new(Engine::open_with_policy(&path, scenario, policy.clone()).unwrap()),
        (0, 0, 0),
    );
    assert_eq!(case.clients.len(), 11);
    assert_eq!(case.service.engine.scenario_validation(), Some(true));
    assert!(!case.service.engine.wizard_enabled());
    let initial = progress(&case.service, 0, 0, 2);
    let original = creature(&initial, OWNER)["build"].clone();
    let empty = json!({"purchase_highwater":0,"purchases":[],"attribute":null});
    assert_eq!(original["choices"], json!([empty, empty, empty]));
    assert_eq!(original["body_build"]["base"]["id"], "rogue.body.human");
    assert_eq!(
        original["body_build"]["active"],
        original["body_build"]["base"]
    );
    assert_eq!(original["body_build"]["birth_personal_count"], 2);
    assert_eq!(original["body_build"]["next_personal_id"], 3);
    assert_eq!(original["body_build"]["drained"], json!([]));
    assert_eq!(
        original["hd"][0]["owner"],
        json!({"kind":"body","form":"rogue.body.human","ordinal":0})
    );
    for (index, personal_id) in [(1, 1), (2, 2)] {
        assert_eq!(
            original["hd"][index]["owner"],
            json!({"kind":"personal","id":personal_id})
        );
        assert_eq!(original["hd"][index]["health_policy"], "seeded");
    }
    assert_eq!(original["hd"][0]["health_policy"], "maximized");
    assert_eq!(original["restoration"], json!({"kind":"choose_again"}));
    assert!(original["health_root_seed"].is_null());
    assert!(original["body_build"]["health_root"].is_u64());
    assert_eq!(
        original["hd"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["source"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!({"kind":"racial"}),
            json!({"kind":"class","id":"warrior"}),
            json!({"kind":"class","id":"warrior"})
        ]
    );
    // Capture actual constructor seeds for these authored identities; neither
    // their values nor this script claim the process fixture's Health rolls.
    let constructor_builds: BTreeMap<_, _> = case
        .service
        .engine
        .actors()
        .into_iter()
        .map(|actor| (actor, creature(&initial, actor)["build"].clone()))
        .collect();
    assert!(constructor_builds
        .values()
        .all(|build| build["health_root_seed"].is_null()
            && build["body_build"]["health_root"].is_u64()
            && build["hd"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["health_seed"].is_u64() && row["owner"].is_object())));
    let wraith = creature(&initial, WRAITH);
    assert_eq!(wraith["build"]["hd"].as_array().unwrap().len(), 5);
    assert!(wraith["build"]["hd"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["source"] == json!({"kind":"racial"})));
    assert_eq!(
        wraith["build"]["choices"],
        json!([
            paid_choice(
                json!({"kind":"body","form":"rogue.body.wraith","ordinal":0}),
                json!({"kind":"talent","talent":DRAIN})
            ),
            empty,
            empty,
            empty,
            empty
        ])
    );
    assert_eq!(wraith["earned_progress"]["peak_threat"], 4);
    assert_eq!(owned_rows(&wraith["build"], "personal"), json!([]));
    for (ordinal, row) in wraith["build"]["hd"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            row["owner"],
            json!({"kind":"body","form":"rogue.body.wraith","ordinal":ordinal})
        );
    }
    let w_overview = query(
        &mut case.service,
        case.clients.get_mut(&WRAITH).unwrap(),
        json!({"kind":"overview"}),
    );
    assert!(
        w_overview
            .iter()
            .any(|message| message["type"] == "advancement_page"),
        "zero-personal-HD owned Wraith must receive its authorized overview: {w_overview:?}"
    );
    assert_eq!(
        page(&w_overview)["entries"][0]["value"]["growth"]["highest_applied_tier"],
        "0"
    );
    assert_eq!(
        page(&w_overview)["entries"][0]["value"]["growth"]["body_package_hit_dice"],
        "5"
    );
    assert_eq!(
        page(&w_overview)["entries"][0]["value"]["growth"]["highest_earned_tier"],
        "0"
    );
    assert_eq!(
        page(&w_overview)["entries"][0]["value"]["points"],
        json!({"earned":"0","spent":"0","unspent":"0"})
    );
    assert!(case
        .service
        .engine
        .observation(WRAITH)
        .unwrap()
        .inventory
        .is_empty());
    for actor in (3..=12).filter(|id| *id != 11).map(ActorId) {
        assert_eq!(
            creature(&initial, actor)["earned_progress"]["peak_threat"],
            1
        );
    }
    assert_eq!(
        case.service
            .engine
            .observation(OWNER)
            .unwrap()
            .inventory
            .iter()
            .map(|item| (item.name.clone(), item.quantity))
            .collect::<BTreeMap<_, _>>(),
        BTreeMap::from([
            ("mace".into(), 1),
            ("ring mail".into(), 1),
            ("clean bandage".into(), 3),
            ("food ration".into(), 1)
        ])
    );
    // The effective body slot is locked; purchases fund stable personal owners.
    case.purchase(2, json!({"kind":"talent","talent":"rattling_blow"}));
    case.purchase(3, json!({"kind":"skill_rank","skill":"lore"}));
    let paid = creature(&progress(&case.service, 0, 0, 2), OWNER)["build"].clone();
    assert_eq!(paid["hd"], original["hd"]);
    assert_eq!(paid["health_root_seed"], original["health_root_seed"]);
    assert_eq!(lore(&case.service), 1);
    assert_eq!(
        paid["choices"],
        json!([
            empty,
            paid_choice(
                json!({"kind":"personal","id":1}),
                json!({"kind":"talent","talent":"rattling_blow"})
            ),
            paid_choice(
                json!({"kind":"personal","id":2}),
                json!({"kind":"skill_rank","skill":"lore"})
            )
        ])
    );
    for index in 0..8 {
        if index != 0 {
            case.act(OWNER, json!({"type":"move","direction":"east"}));
        }
        let target = case.kill("emu", 20, index * 20, 0, 2);
        if index == 0 {
            let before = saved(&case.service);
            let client = case.clients[&OWNER].id;
            let request = command(
                &case.service,
                client,
                json!({"type":"act",
                "expected_revision":case.service.engine.revision(OWNER).unwrap().to_string(),
                "action":{"type":"use_ability","supplies":{"kind":"carried"},"id":"basic_melee",
                    "invocation":{"aim":{"kind":"actor_target","target":target},"weapon":{"kind":"default"}}}}),
            );
            case.service
                .handle(client, "dead-target-no-credit".into(), request);
            assert!(drain(case.clients.get_mut(&OWNER).unwrap())
                .iter()
                .any(|m| m["type"] == "error"));
            case.drain();
            assert_eq!(
                saved(&case.service),
                before,
                "rejected dead target changes nothing"
            );
        }
    }
    let before_drain = progress(&case.service, 160, 0, 2);
    assert_eq!(creature(&before_drain, OWNER)["build"], paid);
    case.act(OWNER, json!({"type":"move","direction":"east"}));
    let w_target = case.target(OWNER, "wraith", 1);
    let mut survived = false;
    for attempt in 0..16 {
        case.recover();
        case.turn(WRAITH);
        let owner_view = case.service.engine.observation(OWNER).unwrap();
        let full = owner_view.combat.as_ref().unwrap();
        assert_eq!(full.hp, full.max_hp);
        let victim = case.target(WRAITH, "Warrior", -1);
        let events = case.act(
            WRAITH,
            json!({"type":"use_ability","supplies":{"kind":"carried"},"id":DRAIN,
            "invocation":{"aim":{"kind":"actor_target","target":victim},"weapon":{"kind":"default"}}}),
        );
        let actual_hd = case
            .service
            .engine
            .observation(OWNER)
            .unwrap()
            .combat
            .unwrap()
            .own_stats
            .unwrap()
            .hit_dice
            .total;
        if actual_hd == 2 {
            assert!(events.iter().any(|event| matches!(event,
                CombatEventView::Ability { caster: Some(caster), target: Some(target), ability,
                    outcome: AbilityOutcome::Unaffected, strike: Some(0) }
                    if *caster == w_target && *target == owner_view.self_target && ability.id == DRAIN)),
                "ring mail absorbs this contact's damage without preventing its rider");
            let drained = progress(&case.service, 160, 0, 1);
            let primary: Vec<_> = drained["game"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|event| event.get("Resolved"))
                .filter(|event| {
                    event["strike"] == 0
                        && event["outcome"]["actor"] == WRAITH.0
                        && event["outcome"]["target"] == OWNER.0
                        && event["outcome"]["ability"] == DRAIN
                })
                .collect();
            assert_eq!(primary.len(), 1, "one real primary, not an immunity bypass");
            assert_eq!(primary[0]["outcome"]["check"]["success"], true);
            assert_eq!(
                primary[0]["outcome"]["effects"],
                json!([{"kind":"damage","amount":0}])
            );
            let build = &creature(&drained, OWNER)["build"];
            assert_eq!(build["hd"], json!([paid["hd"][0], paid["hd"][1]]));
            assert_eq!(owned_rows(build, "body"), owned_rows(&paid, "body"));
            assert_eq!(
                owned_rows(build, "personal"),
                json!([owned_rows(&paid, "personal")[0]])
            );
            assert_eq!(
                build["body_build"]["drained"],
                json!([{
                    "hit_die":paid["hd"][2],"choices":paid["choices"][2]
                }])
            );
            assert_eq!(
                build["drained_hd"],
                json!([]),
                "owned loss is not copied into legacy drain storage"
            );
            assert_eq!(
                build["body_build"]["health_root"],
                paid["body_build"]["health_root"]
            );
            assert_eq!(
                build["choices"],
                json!([paid["choices"][0], paid["choices"][1]])
            );
            assert_eq!(build["health_root_seed"], paid["health_root_seed"]);
            assert_eq!(lore(&case.service), 0);
            assert!(case.service.engine.alive(WRAITH));
            survived = true;
            break;
        }
        progress(&case.service, 160, 0, 2);
        assert!(
            attempt < 15,
            "16 real DR2 attempts exhausted; do not search seeds"
        );
    }
    assert!(survived);
    assert!(case.service.engine.pending_intentions(WRAITH).is_empty());
    assert!(case
        .service
        .engine
        .observation(WRAITH)
        .unwrap()
        .combat
        .unwrap()
        .preparation_remaining
        .is_none());
    let drain_end = case.events.len();
    case.kill("wraith", 35, 160, 0, 1);
    assert!(
        case.events[drain_end..].iter().all(|event| !matches!(event,
        CombatEventView::Ability { ability, .. } if ability.id == DRAIN)),
        "W is Wait-only after survived drain"
    );
    assert!(!case.service.engine.alive(WRAITH));
    let at_195 = progress(&case.service, 195, 0, 1);
    assert_eq!(
        creature(&at_195, OWNER)["build"]["hd"],
        json!([original["hd"][0], original["hd"][1]])
    );
    assert_eq!(
        creature(&at_195, OWNER)["build"]["choices"],
        json!([paid["choices"][0], paid["choices"][1]])
    );
    assert_eq!(
        creature(&at_195, OWNER)["build"]["health_root_seed"],
        original["health_root_seed"]
    );
    assert_eq!(lore(&case.service), 0);
    let owner_state = case.service.engine.state(OWNER).unwrap();
    let client = case.clients[&OWNER].id;
    case.service
        .handle(client, "durable-195".into(), Request::Save);
    case.service.engine.flush().unwrap();
    case.service.poll_saves();
    assert!(case
        .drain()
        .iter()
        .any(|m| m["type"] == "ack" && m["request_id"] == "durable-195"));
    assert_eq!(saved(&case.service), at_195, "Save advances no game time");
    let counts = (case.acts, case.steps, case.resumes);
    drop(case);
    std::fs::remove_dir_all(&source).unwrap();
    assert!(!source.exists());
    let engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy).unwrap();
    assert!(
        engine.recovery_profile().records_replayed > 0,
        "this one4096 execution must replay real journal records"
    );
    let service = Service::new(engine);
    assert_eq!(
        saved(&service),
        at_195,
        "source-free exact game/shared authority, seeds, injury and boundaries"
    );
    assert_eq!(service.engine.scenario_validation(), Some(true));
    assert_eq!(service.engine.state(OWNER).unwrap(), owner_state);
    let mut case = Controlled::connect(service, counts);
    assert_eq!(
        saved(&case.service),
        at_195,
        "normal reconnect/control does not suspend settled work"
    );
    progress(&case.service, 195, 0, 1);
    assert_eq!(lore(&case.service), 0);
    case.act(OWNER, json!({"type":"move","direction":"east"}));
    case.kill("emu", 20, 195, 200, 2);
    let restored = progress(&case.service, 215, 200, 2);
    let build = &creature(&restored, OWNER)["build"];
    assert_eq!(
        build["hd"], original["hd"],
        "exact original ordinal source and Health seed"
    );
    assert_eq!(build["health_root_seed"], original["health_root_seed"]);
    // ChooseAgain returns the funded owner and original Health, not its removed choices.
    let restored_empty = json!({"purchase_highwater":1,"purchases":[],"attribute":null});
    assert_eq!(
        build["choices"],
        json!([paid["choices"][0], paid["choices"][1], restored_empty])
    );
    assert_eq!(lore(&case.service), 0);
    assert_eq!(build["body_build"]["drained"], json!([]));
    assert_eq!(
        build["body_build"]["health_root"],
        original["body_build"]["health_root"]
    );
    assert_eq!(
        build["body_build"]["personal_restoration"],
        json!({"kind":"choose_again"})
    );
    let overview = query(
        &mut case.service,
        case.clients.get_mut(&OWNER).unwrap(),
        json!({"kind":"overview"}),
    );
    assert_eq!(
        page(&overview)["entries"][0]["value"]["points"],
        json!({"earned":"2","spent":"1","unspent":"1"})
    );
    case.purchase(3, json!({"kind":"skill_rank","skill":"lore"}));
    let mut repaid = paid.clone();
    repaid["choices"][2]["purchase_highwater"] = json!(2);
    repaid["choices"][2]["purchases"][0]["id"]["serial"] = json!(2);
    assert_eq!(
        creature(&progress(&case.service, 215, 200, 2), OWNER)["build"],
        repaid
    );
    assert_eq!(lore(&case.service), 1);
    case.act(OWNER, json!({"type":"wait"}));
    progress(&case.service, 215, 200, 2);
}
