//! Real service authorization, pagination and source-local work boundaries.
use super::*;
use crate::creature_view::strike_page_tests::{page_work, scenario};
use serde_json::{json, Value};

fn service(abilities: usize, maximum: bool) -> (Service, Connection) {
    let mut engine = Engine::memory(scenario(abilities, maximum)).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let client = connect(&mut service, "wizard", AccessRole::Wizard);
    (service, client)
}
fn connect(service: &mut Service, user: &str, role: AccessRole) -> Connection {
    let mut client = service
        .connect(
            &Account {
                role,
                user: user.into(),
                token: "test".into(),
                actors: BTreeSet::from([ActorId(1)]),
            },
            "headless".into(),
        )
        .unwrap();
    while client.messages.try_recv().is_ok() {}
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: ActorId(1) },
    );
    while client.messages.try_recv().is_ok() {}
    client
}
fn request(service: &Service, client: u64, source: Value, cursor: Value) -> Value {
    json!({"type":"command","context":service.input_context(client).unwrap(),"branch":service.engine.branch(),
        "command":{"type":"inspect_strike_source","expected_revision":service.reply_context(client).unwrap().revision.to_string(),
            "actor":"1","target_revision":service.reply_context(client).unwrap().revision.to_string(),"source":source,"cursor":cursor}})
}
fn query(service: &mut Service, client: &mut Connection, value: Value) -> Vec<Value> {
    let request: Request =
        serde_json::from_value(value).expect("typed private page request must decode");
    service.handle(client.id, "\\".repeat(128), request);
    std::iter::from_fn(|| client.messages.try_recv().ok())
        .map(|message| {
            let encoded = tor_protocol::encode_response(&message, None, 196_608)
                .expect("request-correlated entire page envelope is bounded");
            assert_eq!(
                tor_protocol::decode_response(&encoded.text).unwrap(),
                message
            );
            serde_json::from_str(&encoded.text).unwrap()
        })
        .collect()
}
fn page(messages: &[Value]) -> &Value {
    &messages
        .iter()
        .find(|message| message["type"] == "creature_strike_page")
        .expect("one private detail page")["page"]
}
fn all_entries(service: &mut Service, client: &mut Connection, source: Value) -> Vec<Value> {
    let mut cursor = Value::Null;
    let mut result = Vec::new();
    let mut handle = None;
    let mut starts = std::collections::BTreeSet::new();
    for _ in 0..=656 {
        let input = request(service, client.id, source.clone(), cursor.clone());
        let before = page_work();
        let lookups = tor_simulation::diagnostics::work_counts().descriptor_lookups;
        let messages = query(service, client, input);
        let page = page(&messages);
        assert!(
            starts.insert(page["start"].to_string()),
            "cursor must advance without duplicate pages"
        );
        let current = page["inspection"].clone();
        if let Some(prior) = &handle {
            assert_eq!(prior, &current);
        } else {
            handle = Some(current);
        }
        assert_eq!(page["source"], source);
        let entries = page["entries"].as_array().unwrap();
        assert!((1..=16).contains(&entries.len()));
        let work = page_work();
        assert_eq!(work.authority_projections - before.authority_projections, 1);
        assert!(work.owned_authority_projections - before.owned_authority_projections <= 1);
        assert_eq!(
            work.entries - before.entries,
            entries.len(),
            "no eager unrelated source entry projection"
        );
        assert!(work.fields - before.fields <= 128);
        assert!(work.copied_string_bytes - before.copied_string_bytes <= 172_032);
        assert!(tor_simulation::diagnostics::work_counts().descriptor_lookups - lookups <= 288);
        result.extend(entries.iter().cloned());
        cursor = page["next"].clone();
        if cursor.is_null() {
            return result;
        }
    }
    panic!("bounded source must terminate explicitly");
}

#[test]
fn ordered_strikes_detail_pages_reconstruct_all_128_sources_without_catalog_expansion() {
    let (mut service, mut client) = service(128, false);
    let before = service.engine.state(ActorId(1)).unwrap();
    let mut total = 0;
    for ability in 0..128 {
        let entries = all_entries(
            &mut service,
            &mut client,
            json!({"kind":"ability","id":format!("page_ability_{ability:03}")}),
        );
        assert_eq!(entries.len(), 258);
        for strike in 0..2 {
            let start = strike * 129;
            assert_eq!(entries[start]["kind"], "strike_header");
            assert_eq!(entries[start]["strike"], strike);
            assert_eq!(entries[start]["descriptor_count"], 128);
            for index in 0..128 {
                let row = &entries[start + 1 + index];
                assert_eq!(row["kind"], "strike_descriptor");
                assert_eq!(row["strike"], strike);
                assert_eq!(row["index"], index);
                assert_eq!(
                    row["descriptor"]["id"],
                    crate::creature_view::strike_page_tests::descriptor_id(index)
                );
                assert_eq!(row["descriptor"]["name"], "\\".repeat(256));
                total += 1;
            }
        }
    }
    assert_eq!(total, 32_768);
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn ordered_strikes_detail_maximum_source_preserves_every_check_rider_component_and_unlock() {
    for (abilities, source) in [
        (0, json!({"kind":"natural"})),
        (1, json!({"kind":"ability","id":"page_ability_000"})),
    ] {
        let (mut service, mut client) = service(abilities, true);
        let entries = all_entries(&mut service, &mut client, source);
        assert_eq!(entries.len(), 656);
        let mut index = 0;
        for strike in 0..16 {
            let header = &entries[index];
            index += 1;
            assert_eq!(header["kind"], "strike_header");
            assert_eq!(header["strike"], strike);
            assert_eq!(header["offset"], strike.to_string());
            assert_eq!(header["check"]["fields"].as_array().unwrap().len(), 5);
            assert_eq!(header["success_count"], 0);
            assert_eq!(header["failure_count"], 0);
            assert_eq!(header["rider_count"], 8);
            assert_eq!(header["unlock"].is_null(), strike == 0);
            if strike > 0 {
                assert_eq!(header["unlock"]["id"], format!("page_unlock_{strike}"));
                assert_eq!(header["unlock"]["name"], "\\".repeat(256));
            }
            for descriptor in 0..16 {
                assert_eq!(entries[index]["kind"], "strike_descriptor");
                assert_eq!(entries[index]["index"], descriptor);
                index += 1;
            }
            for rider in 0..8 {
                let header = &entries[index];
                index += 1;
                assert_eq!(header["kind"], "rider_header");
                assert_eq!(header["strike"], strike);
                assert_eq!(header["rider"], rider);
                assert_eq!(header["trigger"], "success");
                assert_eq!(header["check"]["fields"].as_array().unwrap().len(), 3);
                assert_eq!(header["effect_count"], 2);
                assert_eq!(header["descriptor_count"], 0);
                for effect in 0..2 {
                    let row = &entries[index];
                    index += 1;
                    assert_eq!(row["kind"], "rider_effect");
                    assert_eq!(row["rider"], rider);
                    assert_eq!(row["index"], effect);
                    let fields = row["row"]["fields"].as_array().unwrap();
                    assert_eq!(fields.len(), 32);
                    assert_eq!(fields[0]["id"], "primary_component_0");
                }
            }
        }
        assert_eq!(index, entries.len());
    }
}

#[test]
fn ordered_strikes_detail_pages_are_private_and_unknown_ungranted_sources_indistinguishable() {
    let (mut service, mut wizard) = service(1, false);
    let mut player = connect(&mut service, "player", AccessRole::Player);
    let mut spectator = connect(&mut service, "spectator", AccessRole::Spectator);
    for client in [&mut player, &mut spectator] {
        for source in [
            json!({"kind":"natural"}),
            json!({"kind":"ability","id":"never_exists"}),
        ] {
            let input = request(&service, client.id, source, Value::Null);
            let messages = query(&mut service, client, input);
            assert!(messages
                .iter()
                .any(|message| message["type"] == "error" && message["code"] == "unauthorized"));
            assert!(messages
                .iter()
                .all(|message| message["type"] != "creature_strike_page"));
        }
    }
    let mut errors = Vec::new();
    for id in ["never_exists", "falter"] {
        let input = request(
            &service,
            wizard.id,
            json!({"kind":"ability","id":id}),
            Value::Null,
        );
        let messages = query(&mut service, &mut wizard, input);
        let error = messages.iter().find(|m| m["type"] == "error").unwrap();
        errors.push((error["code"].clone(), error["message"].clone()));
    }
    assert_eq!(errors[0], errors[1]);
    let input = request(
        &service,
        wizard.id,
        json!({"kind":"ability","id":"page_ability_000"}),
        Value::Null,
    );
    let messages = query(&mut service, &mut wizard, input);
    page(&messages);
    assert!(player.messages.try_recv().is_err());
    assert!(spectator.messages.try_recv().is_err());
    let ordinary = serde_json::to_value(service.engine.state(ActorId(1)).unwrap())
        .unwrap()
        .to_string();
    assert!(!ordinary.contains("strike_header") && !ordinary.contains("page-handle"));
}

#[test]
fn ordered_strikes_detail_handle_rejects_reset_reconnect_and_wrong_source_without_restarting() {
    let (mut service, mut client) = service(2, false);
    let source = json!({"kind":"ability","id":"page_ability_000"});
    let input = request(&service, client.id, source.clone(), Value::Null);
    let first = query(&mut service, &mut client, input);
    let cursor = page(&first)["next"].clone();
    assert!(!cursor.is_null());
    for change in ["source", "target", "revision", "branch", "stream"] {
        let mut input = request(&service, client.id, source.clone(), cursor.clone());
        match change {
            "source" => input["command"]["source"]["id"] = json!("page_ability_001"),
            "target" => input["command"]["actor"] = json!("999"),
            "revision" => input["command"]["target_revision"] = json!("999"),
            "branch" => input["branch"] = json!("other-branch"),
            _ => input["context"]["stream"]["epoch"] = json!("999"),
        }
        let messages = query(&mut service, &mut client, input);
        assert!(messages.iter().any(|m| m["type"] == "error"), "{change}");
        assert!(
            messages.iter().all(|m| m["type"] != "creature_strike_page"),
            "{change}"
        );
    }
    service.handle(client.id, "snapshot".into(), Request::Snapshot);
    while client.messages.try_recv().is_ok() {}
    let input = request(&service, client.id, source.clone(), cursor.clone());
    let messages = query(&mut service, &mut client, input);
    assert!(messages.iter().any(|m| m["type"] == "error"));
    let mut reconnect = connect(&mut service, "wizard", AccessRole::Wizard);
    let input = request(&service, reconnect.id, source.clone(), cursor);
    let messages = query(&mut service, &mut reconnect, input);
    assert!(messages.iter().any(|m| m["type"] == "error"));
    let input = request(&service, reconnect.id, source, Value::Null);
    let messages = query(&mut service, &mut reconnect, input);
    assert_eq!(
        page(&messages)["start"],
        json!({"kind":"strike_header","strike":0})
    );
}

#[test]
fn ordered_strikes_selected_melee_ability_page_links_to_separate_selected_source() {
    let (mut service, mut client) = service(1, false);
    let entries = all_entries(
        &mut service,
        &mut client,
        json!({"kind":"ability","id":"basic_melee"}),
    );
    assert_eq!(entries, vec![json!({"kind":"selected_melee"})]);
    let selected = all_entries(&mut service, &mut client, json!({"kind":"selected_melee"}));
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["kind"], "strike_header");
    assert!(selected[0]["check"].is_null());
    assert_eq!(
        selected[0]["success_count"], 0,
        "checkless nondamaging natural source remains genuine"
    );
}

fn observe(service: &mut Service, client: &mut Connection) {
    service.handle(client.id, "detail-observe".into(), Request::Observe);
    while let Ok(reply) = client.messages.try_recv() {
        assert!(!matches!(reply, ServerMessage::Error { .. }), "{reply:?}");
    }
}

fn wizard(service: &mut Service, client: &mut Connection, operation: &str) {
    service.handle(
        client.id,
        uuid::Uuid::new_v4().to_string(),
        Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Wizard {
                expected_revision: service.reply_context(client.id).unwrap().revision,
                operation: operation.into(),
            },
        },
    );
    let replies: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
    assert!(
        !replies
            .iter()
            .any(|reply| matches!(reply, ServerMessage::Error { .. })),
        "{replies:?}"
    );
    observe(service, client);
}

#[test]
fn ordered_strikes_detail_grant_loss_rewind_and_replacement_invalidate_real_handles() {
    let mut engine =
        Engine::memory(crate::creature_view::strike_page_tests::revocable_scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, "wizard", AccessRole::Wizard);
    let source = json!({"kind":"ability","id":"page_ability_001"});
    let input = request(&service, client.id, source.clone(), Value::Null);
    let first = query(&mut service, &mut client, input);
    let cursor = page(&first)["next"].clone();
    // A read-only retry is deterministic inside the same capability.
    let input = request(&service, client.id, source.clone(), cursor.clone());
    let next = query(&mut service, &mut client, input.clone());
    let retried = query(&mut service, &mut client, input);
    assert_eq!(page(&next), page(&retried));
    wizard(
        &mut service,
        &mut client,
        "creature template 1 revocable off",
    );
    let granted =
        serde_json::to_value(service.engine.inspect_creature(ActorId(1)).unwrap()).unwrap();
    assert!(!granted["stats"]["abilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|ability| ability["id"] == "page_ability_001"));
    let input = request(&service, client.id, source.clone(), cursor.clone());
    let stale = query(&mut service, &mut client, input);
    assert!(stale.iter().any(|m| m["type"] == "error"));
    let mut errors = Vec::new();
    for id in ["page_ability_001", "never_exists"] {
        let input = request(
            &service,
            client.id,
            json!({"kind":"ability","id":id}),
            Value::Null,
        );
        let replies = query(&mut service, &mut client, input);
        let error = replies.iter().find(|m| m["type"] == "error").unwrap();
        errors.push((error["code"].clone(), error["message"].clone()));
    }
    assert_eq!(errors[0], errors[1]);
    wizard(&mut service, &mut client, "rewind initial");
    let input = request(&service, client.id, source.clone(), cursor);
    let stale = query(&mut service, &mut client, input);
    assert!(stale.iter().any(|m| m["type"] == "error"));
    let input = request(&service, client.id, source.clone(), Value::Null);
    let reopened = query(&mut service, &mut client, input);
    assert_ne!(page(&first)["inspection"], page(&reopened)["inspection"]);
    let old = page(&reopened)["next"].clone();
    let input = request(&service, client.id, json!({"kind":"natural"}), Value::Null);
    page(&query(&mut service, &mut client, input));
    let input = request(&service, client.id, source, old);
    let replaced = query(&mut service, &mut client, input);
    assert!(
        replaced.iter().any(|m| m["type"] == "error"),
        "only one active source handle per connection"
    );
}

fn service_with_page_limit(
    abilities: usize,
    maximum: bool,
    frame_bytes: usize,
) -> (Service, Connection) {
    let mut engine = Engine::memory(scenario(abilities, maximum)).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::with_outbound_limits(
        engine,
        crate::OutboundLimits {
            frame_bytes,
            client_bytes: 16 * 1024 * 1024,
            total_bytes: 32 * 1024 * 1024,
        },
    )
    .unwrap();
    let client = connect(&mut service, "wizard", AccessRole::Wizard);
    assert!(
        service.clients.contains_key(&client.id),
        "ordinary attachment must fit via normal snapshot transfer"
    );
    (service, client)
}

fn bounded_page_query(
    service: &mut Service,
    client: &mut Connection,
    input: Value,
    frame_bytes: usize,
) -> Vec<Value> {
    let input: Request = serde_json::from_value(input).unwrap();
    service.handle(client.id, "\\".repeat(128), input);
    assert!(
        service.clients.contains_key(&client.id),
        "private detail capacity must not disconnect the client"
    );
    std::iter::from_fn(|| client.messages.try_recv_frame().ok())
        .map(|frame| {
            assert!(
                frame.text.len() <= frame_bytes,
                "complete request-correlated frame exceeds advertised capacity"
            );
            assert!(
                frame.following.is_empty(),
                "ordinary private pages are not snapshot-chunked"
            );
            serde_json::from_str(&frame.text).unwrap()
        })
        .collect()
}

#[test]
fn ordered_strikes_detail_64k_transport_reconstructs_maximum_components_and_escaped_refs() {
    let (mut service, mut client) = service_with_page_limit(0, true, 65_536);
    let source = json!({"kind":"natural"});
    let mut cursor = Value::Null;
    let mut entries = Vec::new();
    for _ in 0..=656 {
        let input = request(&service, client.id, source.clone(), cursor);
        let messages = bounded_page_query(&mut service, &mut client, input, 65_536);
        let page = page(&messages);
        entries.extend(page["entries"].as_array().unwrap().iter().cloned());
        cursor = page["next"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(entries.len(), 656);
    assert!(entries.iter().any(|entry| entry["row"]["fields"]
        .as_array()
        .is_some_and(|fields| fields.len() == 32)));
    assert!(entries
        .iter()
        .any(|entry| entry["descriptor"]["name"] == "\\".repeat(256)));
}

#[test]
fn ordered_strikes_detail_adapts_to_smaller_complete_transport_budget_before_projection() {
    let (mut service, mut client) = service_with_page_limit(1, false, 4096);
    let source = json!({"kind":"ability","id":"page_ability_000"});
    let before = service.engine.state(ActorId(1)).unwrap();
    let mut cursor = Value::Null;
    let mut entries = Vec::new();
    for _ in 0..=656 {
        let input = request(&service, client.id, source.clone(), cursor);
        let messages = bounded_page_query(&mut service, &mut client, input, 4096);
        let page = page(&messages);
        entries.extend(page["entries"].as_array().unwrap().iter().cloned());
        cursor = page["next"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(entries.len(), 258);
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn ordered_strikes_detail_indivisible_entry_too_large_returns_bounded_error_without_disconnect() {
    let (mut service, mut client) = service_with_page_limit(0, true, 1024);
    let before = service.engine.state(ActorId(1)).unwrap();
    let input = request(&service, client.id, json!({"kind":"natural"}), Value::Null);
    let messages = bounded_page_query(&mut service, &mut client, input, 1024);
    assert!(messages.iter().any(|message| message["type"] == "error"));
    assert!(messages
        .iter()
        .all(|message| message["type"] != "creature_strike_page"));
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
    assert!(
        service.clients[&client.id].strike_inspection.is_none(),
        "no incomplete capability is installed"
    );
}

#[test]
fn ordered_strikes_detail_512_byte_host_closes_cleanly_when_correlated_error_cannot_fit() {
    let (mut service, mut client) = service_with_page_limit(0, true, 512);
    let state = service.engine.state(ActorId(1)).unwrap();
    let history = service
        .engine
        .history(ActorId(1), "wizard", None, 100)
        .unwrap();
    let input: Request = serde_json::from_value(request(
        &service,
        client.id,
        json!({"kind":"natural"}),
        Value::Null,
    ))
    .unwrap();
    service.handle(client.id, "\\".repeat(128), input);
    assert!(!service.clients.contains_key(&client.id));
    assert!(matches!(
        *client.close.borrow(),
        Some(DisconnectMode::Abort)
    ));
    assert!(
        client.messages.try_recv_frame().is_err(),
        "no partial private reply escapes an aborted output queue"
    );
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        service
            .engine
            .history(ActorId(1), "wizard", None, 100)
            .unwrap(),
        history
    );
}

#[test]
fn ordered_strikes_detail_ordinary_unequip_changes_selected_source_and_retires_handle() {
    let mut engine =
        Engine::memory(crate::creature_view::strike_page_tests::equipped_scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, "wizard", AccessRole::Wizard);
    service.handle(client.id, "control".into(), Request::AcquireControl);
    while client.messages.try_recv().is_ok() {}
    let source = json!({"kind":"selected_melee"});
    let input = request(&service, client.id, source.clone(), Value::Null);
    let initial = query(&mut service, &mut client, input);
    let page = page(&initial);
    assert_eq!(page["resolved"]["kind"], "weapon");
    let cursor = json!({"inspection":page["inspection"],"position":page["start"]});
    let item = service
        .engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .interactions
        .expect("own interaction view")
        .inventory
        .into_iter()
        .find(|item| item.equipped_slot == Some(0))
        .unwrap()
        .item;
    service.handle(
        client.id,
        "unequip".into(),
        Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Act {
                action: Action::Unequip { item },
            },
        },
    );
    service.run_until_blocked();
    while let Ok(reply) = client.messages.try_recv() {
        assert!(!matches!(reply, ServerMessage::Error { .. }), "{reply:?}");
    }
    assert!(service
        .engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .interactions
        .expect("own interaction view")
        .inventory
        .iter()
        .all(|item| item.equipped_slot.is_none()));
    observe(&mut service, &mut client);
    let input = request(&service, client.id, source.clone(), cursor);
    let stale = query(&mut service, &mut client, input);
    assert!(stale.iter().any(|m| m["type"] == "error"));
    let input = request(&service, client.id, source, Value::Null);
    let reopened = query(&mut service, &mut client, input);
    assert_eq!(self::page(&reopened)["resolved"]["kind"], "natural");
    assert_ne!(self::page(&reopened)["inspection"], page["inspection"]);
}

#[test]
fn ordered_strikes_detail_other_target_unload_and_reload_reject_old_identity() {
    let mut engine =
        Engine::memory(crate::creature_view::strike_page_tests::unload_target_scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, "wizard", AccessRole::Wizard);
    let source = json!({"kind":"ability","id":"page_ability_000"});
    let target_request = |service: &Service, client, cursor| {
        let mut value = request(service, client, source.clone(), cursor);
        value["command"]["actor"] = json!("2");
        value["command"]["target_revision"] =
            json!(service.reply_context(client).unwrap().revision.to_string());
        value
    };
    let input = target_request(&service, client.id, Value::Null);
    let first = query(&mut service, &mut client, input);
    let message = first
        .iter()
        .find(|m| m["type"] == "creature_strike_page")
        .unwrap();
    assert_eq!(message["context"]["actor"], "1");
    assert_eq!(message["page"]["actor"], "2");
    let cursor = page(&first)["next"].clone();
    wizard(&mut service, &mut client, "teleport 1 3 1 1 0");
    assert!(
        service.engine.inspect_creature(ActorId(2)).is_err(),
        "the real streaming transition must unload the target"
    );
    let input = target_request(&service, client.id, cursor.clone());
    let stale = query(&mut service, &mut client, input);
    assert!(stale.iter().any(|m| m["type"] == "error"));
    wizard(&mut service, &mut client, "teleport 1 1 1 1 0");
    assert!(service.engine.inspect_creature(ActorId(2)).is_ok());
    let input = target_request(&service, client.id, cursor);
    assert!(query(&mut service, &mut client, input)
        .iter()
        .any(|m| m["type"] == "error"));
    let input = target_request(&service, client.id, Value::Null);
    let fresh = query(&mut service, &mut client, input);
    assert_ne!(page(&first)["inspection"], page(&fresh)["inspection"]);
}

#[test]
fn ordered_strikes_detail_changed_pinned_package_new_run_cannot_reuse_old_handle() {
    let (mut original, mut client) = service(1, false);
    let source = json!({"kind":"ability","id":"page_ability_000"});
    let input = request(&original, client.id, source.clone(), Value::Null);
    let first = query(&mut original, &mut client, input);
    let old_catalog = original.engine.strike_catalog_identity();
    let cursor = page(&first)["next"].clone();
    drop(client);
    drop(original);
    // Catalogs are immutable during a run. Only a supported new run selects a
    // changed pinned package; this is not a fabricated live catalog mutation.
    let mut engine =
        Engine::memory(crate::creature_view::strike_page_tests::changed_catalog_scenario())
            .unwrap();
    engine.enable_wizard().unwrap();
    assert_ne!(engine.strike_catalog_identity().2, old_catalog.2);
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, "wizard", AccessRole::Wizard);
    let input = request(&service, client.id, source.clone(), cursor);
    assert!(query(&mut service, &mut client, input)
        .iter()
        .any(|m| m["type"] == "error"));
    let input = request(&service, client.id, source, Value::Null);
    let fresh = query(&mut service, &mut client, input);
    assert_eq!(
        page(&fresh)["resolved"]["ability"]["name"],
        "Changed pinned technique"
    );
    assert_ne!(page(&first)["inspection"], page(&fresh)["inspection"]);
}

#[test]
fn ordered_strikes_detail_both_primary_rider_triggers_survive_different_pages_losslessly() {
    let mut engine =
        Engine::memory(crate::creature_view::strike_page_tests::mixed_trigger_scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, "wizard", AccessRole::Wizard);
    let source = json!({"kind":"ability","id":"page_ability_000"});
    let mut cursor = Value::Null;
    let mut trigger_pages = Vec::new();
    let mut effects = Vec::new();
    let mut primary = Vec::new();
    for index in 0..=656 {
        let input = request(&service, client.id, source.clone(), cursor);
        let replies = query(&mut service, &mut client, input);
        let page = page(&replies);
        for entry in page["entries"].as_array().unwrap() {
            if entry["kind"] == "success_effect" || entry["kind"] == "failure_effect" {
                primary.push((entry["kind"].clone(), entry["row"].clone(), index));
            }
            if entry["kind"] == "rider_header" {
                trigger_pages.push((entry["trigger"].clone(), index));
            }
            if entry["kind"] == "rider_effect" {
                effects.push(entry["row"].clone());
            }
        }
        cursor = page["next"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(
        trigger_pages
            .iter()
            .map(|(trigger, _)| trigger.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["success", "failure"]
    );
    assert_ne!(
        trigger_pages[0].1, trigger_pages[1].1,
        "both branches must cross a real page boundary"
    );
    assert_eq!(primary.len(), 16);
    for (index, (kind, row, page_index)) in primary[..15].iter().enumerate() {
        assert_eq!(kind, "success_effect");
        assert_eq!(row["id"], "heal");
        assert_eq!(row["fields"][0]["value"]["value"], (index + 1).to_string());
        assert_eq!(*page_index, 0);
    }
    assert_eq!(primary[15].0, "failure_effect");
    assert_eq!(primary[15].1["fields"][0]["value"]["value"], "Impact: 33");
    assert_eq!(
        primary[15].2, 1,
        "primary failure starts on the next real page"
    );
    assert_eq!(effects.len(), 15);
    assert!(effects[..14].iter().all(|effect| effect["id"] == "heal"));
    assert_eq!(effects[14]["id"], "damage");
    assert_eq!(effects[14]["fields"][0]["value"]["value"], "Impact: 77");
}

#[test]
fn natural_melee_pages_keep_species_rows_and_added_riders_distinct_across_fragments() {
    let mut engine =
        Engine::memory(crate::creature_view::strike_page_tests::natural_melee_scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let mut service = Service::new(engine);
    let mut client = connect(&mut service, "wizard", AccessRole::Wizard);
    let before = service.engine.state(ActorId(1)).unwrap();
    let source = json!({"kind":"ability","id":"page_ability_000"});
    let input = request(&service, client.id, source.clone(), Value::Null);
    let first = query(&mut service, &mut client, input);
    let resolved = &page(&first)["resolved"];
    assert_eq!(resolved["kind"], "natural_melee");
    assert_eq!(resolved["species"]["id"], "page_subject");
    assert_eq!(resolved["ability"]["id"], "page_ability_000");
    assert_eq!(resolved["natural_riders"], json!([1, 0]));
    assert_eq!(resolved["additional_riders"], 1);
    let entries = all_entries(&mut service, &mut client, source);
    let headers: Vec<_> = entries
        .iter()
        .filter(|e| e["kind"] == "strike_header")
        .collect();
    assert_eq!(
        headers.len(),
        2,
        "dormant authored tail remains inspectable"
    );
    assert_eq!(headers[0]["rider_count"], 2);
    assert_eq!(headers[1]["rider_count"], 1);
    assert_eq!(headers[1]["unlock"]["id"], "natural_tail");
    let effects: Vec<_> = entries
        .iter()
        .filter(|e| e["kind"] == "rider_effect")
        .map(|e| {
            (
                e["strike"].as_u64().unwrap(),
                e["rider"].as_u64().unwrap(),
                e["row"]["fields"][0]["value"]["value"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(effects, [(0, 0, "1"), (0, 1, "2"), (1, 0, "2")]);
    let conditions: Vec<_> = entries
        .iter()
        .filter(|entry| entry["kind"] == "rider_header")
        .map(|entry| {
            assert!(entry.as_object().unwrap().contains_key("condition"));
            (
                entry["strike"].clone(),
                entry["rider"].clone(),
                entry["condition"].clone(),
            )
        })
        .collect();
    let gate = json!({"kind":"sequence_successful_hits","minimum":2});
    assert_eq!(
        conditions,
        [
            (json!(0), json!(0), Value::Null),
            (json!(0), json!(1), gate.clone()),
            (json!(1), json!(0), gate)
        ]
    );
    assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
}
