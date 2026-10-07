use tor_protocol::*;

#[test]
fn error_scope_distinguishes_transport_unattached_and_disclosed_host_errors_strictly() {
    for scope in [
        serde_json::json!({"type":"transport"}),
        serde_json::json!({"type":"unattached"}),
        serde_json::json!({"type":"attached", "context": {
            "input":{"stream":{"stream":"attachment","epoch":"1"},"readiness_revision":"2"},
            "actor":"1","branch":"current","cursor":{"sequence":"3","tick":"0"},"revision":"0" }}),
    ] {
        let value = serde_json::json!({"type":"error","scope":scope,"request_id":null,"code":"invalid_request","message":"Rejected"});
        let message: ServerMessage = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(message).unwrap(), value);
    }
    for invalid in [
        serde_json::json!({"type":"attached"}),
        serde_json::json!({"type":"unattached","actor":"1"}),
        serde_json::json!({"type":"transport","context":null}),
        serde_json::json!({"type":"unknown"}),
    ] {
        assert!(
            serde_json::from_value::<ErrorScope>(invalid.clone()).is_err(),
            "invalid scope: {invalid}"
        );
    }
}

#[test]
fn errors_require_explicit_scope_instead_of_an_implicit_attachment() {
    let error = serde_json::json!({"type":"error", "request_id":null, "code":"invalid_request", "message":"Rejected"});
    assert!(
        serde_json::from_value::<ServerMessage>(error).is_err(),
        "an error must explicitly name its scope"
    );
}

#[test]
fn successful_replies_require_the_current_disclosed_context() {
    for reply in [
        serde_json::json!({"type":"ack", "request_id":"operation", "receipt": {
            "type":"immediate", "actor":"1", "branch":"original", "entry_id":null }}),
        serde_json::json!({"type":"history", "request_id":"query", "page":{"entries":[], "older_before":null}}),
        serde_json::json!({"type":"palette", "request_id":null, "palette":{"revision":"1", "body":{"type":"full", "assets":[]}}}),
    ] {
        assert!(
            serde_json::from_value::<ServerMessage>(reply.clone()).is_err(),
            "successful reply must require current context: {reply}"
        );
    }
}

#[test]
fn reply_context_requires_current_attachment_state_and_rejects_implicit_fields() {
    let context = serde_json::json!({
        "input": {"stream": {"stream": "attachment", "epoch": "3"}, "readiness_revision": "8"},
        "actor": "2", "branch": "current", "cursor": {"sequence": "17", "tick": "20"}, "revision": "9",
    });
    let decoded: ReplyContext = serde_json::from_value(context.clone()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), context);
    for field in ["input", "actor", "branch", "cursor", "revision"] {
        let mut incomplete = context.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ReplyContext>(incomplete).is_err(),
            "missing {field}"
        );
    }
    let mut forged = context;
    forged["receipt"] = serde_json::json!({"branch": "original"});
    assert!(serde_json::from_value::<ReplyContext>(forged).is_err());
}

#[test]
fn queued_intention_controls_round_trip_opaque_identity_and_reject_extra_authority() {
    for command in [
        Command::ResumeIntention {
            expected_revision: 7,
            intention: IntentionId("admission".into()),
        },
        Command::CancelIntention {
            expected_revision: 7,
            intention: IntentionId("admission".into()),
        },
    ] {
        let encoded = serde_json::to_value(&command).unwrap();
        assert_eq!(
            serde_json::from_value::<Command>(encoded.clone()).unwrap(),
            command
        );
        assert!(encoded["intention"].is_string());
        let mut forged = encoded;
        forged["actor"] = serde_json::json!(2);
        assert!(serde_json::from_value::<Command>(forged).is_err());
    }
}

#[test]
fn transfers_accept_optional_counts_but_reject_forged_identity_and_invalid_numbers() {
    assert_eq!(
        serde_json::from_str::<Action>(r#"{"type":"take","item":"10"}"#).unwrap(),
        Action::Take {
            item: 10,
            quantity: None
        }
    );
    assert_eq!(
        serde_json::from_str::<Action>(r#"{"type":"drop","item":"10","quantity":"3"}"#).unwrap(),
        Action::Drop {
            item: 10,
            quantity: Some(3)
        }
    );
    for text in [
        r#"{"type":"take","item":"10","quantity":-1}"#,
        r#"{"type":"take","item":"10","quantity":1.5}"#,
        r#"{"type":"take","item":"10","quantity":"18446744073709551616"}"#,
        r#"{"type":"drop","item":"10","identity":"healing"}"#,
    ] {
        assert!(serde_json::from_str::<Action>(text).is_err());
    }
}

#[test]
fn place_names_use_opaque_keys_and_remain_read_only_for_spectators() {
    let command = Command::RenamePlace {
        expected_revision: 7,
        key: "opaque-cell".into(),
        name: "Quiet Reverie".into(),
    };
    assert_eq!(
        serde_json::from_str::<Command>(&serde_json::to_string(&command).unwrap()).unwrap(),
        command
    );
    assert!(!AccessRole::Spectator.permits(&Request::Command {
        context: InputContext {
            stream: StreamContext {
                stream: StreamId("authority-test".into()),
                epoch: 1
            },
            readiness_revision: 0,
        },
        branch: BranchId("branch".into()),
        command
    }));
    assert!(serde_json::from_str::<Command>(r#"{"type":"rename_place","expected_revision":"7","key":"opaque-cell","name":"Quiet Reverie","region":3}"#).is_err());
}

#[test]
fn spectators_are_not_permitted_wizard_commands() {
    assert!(!AccessRole::Spectator.permits(&Request::Command {
        context: InputContext {
            stream: StreamContext {
                stream: StreamId("authority-test".into()),
                epoch: 1
            },
            readiness_revision: 0,
        },
        branch: BranchId("branch".into()),
        command: Command::Wizard {
            expected_revision: 0,
            operation: "rewind initial".into()
        }
    }));
}

#[test]
fn clients_cannot_claim_backend_authorship_or_supply_an_author() {
    let forged = r#"{"type":"annotate","anchor":{"type":"state","revision":"0"},"text":"spoiler","source":"backend"}"#;
    assert!(serde_json::from_str::<Command>(forged).is_err());
    let forged = r#"{"type":"annotate","anchor":{"type":"state","revision":"0"},"text":"spoiler","author":{"type":"backend","component":"simulation"}}"#;
    assert!(serde_json::from_str::<Command>(forged).is_err());
}

#[test]
fn user_notes_default_to_private_and_round_trip_as_plain_text() {
    let source = r#"{"type":"annotate","anchor":{"type":"state","revision":"0"},"text":"<script>not executable</script>\nlook"}"#;
    let command: Command = serde_json::from_str(source).unwrap();
    assert!(matches!(
        command,
        Command::Annotate {
            source: ClientSource::User,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
            ..
        }
    ));
    let encoded = serde_json::to_string(&command).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&encoded).unwrap(), command);
}

#[test]
fn clients_cannot_choose_a_role_and_welcome_requires_server_authority() {
    let forged = serde_json::json!({"type":"hello", "protocol":PROTOCOL_VERSION,
        "token":"spectator", "frontend":"text", "role":"player"});
    assert!(serde_json::from_value::<ClientMessage>(forged).is_err());
    let old = serde_json::json!({"type":"welcome", "protocol":PROTOCOL_VERSION,
        "user":"test", "actors":["1"]});
    assert!(serde_json::from_value::<ServerMessage>(old).is_err());
}

#[test]
fn combat_facts_are_data_not_prose() {
    let events: Vec<CombatEventView> = serde_json::from_str(
        r#"[{"type":"attack","attacker":"2","target":null,"outcome":"no_injury"},
            {"type":"interrupted","actor":"1"},{"type":"died","actor":"2"}]"#,
    )
    .unwrap();
    assert_eq!(
        events,
        [
            CombatEventView::Attack {
                attacker: Some(ActorId(2)),
                target: None,
                outcome: AttackOutcome::NoInjury,
            },
            CombatEventView::Interrupted { actor: ActorId(1) },
            CombatEventView::Died { actor: ActorId(2) },
        ]
    );
    assert_eq!(
        serde_json::to_string(&(Injury::BadlyWounded, ObjectiveKind::RetrieveAndReturn)).unwrap(),
        r#"["badly_wounded","retrieve_and_return"]"#
    );
    // Prose fields are gone, and unknown event fields are refused.
    assert!(serde_json::from_str::<CombatEventView>(
        r#"{"type":"died","actor":"2","message":"The scout died."}"#
    )
    .is_err());
}

#[test]
fn protocol_22_says_whose_move_it_is_where_the_exit_is_and_where_names_came_from() {
    let waiting = ServerMessage::Waiting {
        on: Waiting::Others,
    };
    let text = serde_json::to_string(&waiting).unwrap();
    assert_eq!(text, r#"{"type":"waiting","on":"others"}"#);
    assert_eq!(
        serde_json::from_str::<ServerMessage>(&text).unwrap(),
        waiting
    );
    for on in ["you", "others", "unclaimed", "paused", "stopped"] {
        let text = format!(r#"{{"type":"waiting","on":"{on}"}}"#);
        assert!(serde_json::from_str::<ServerMessage>(&text).is_ok(), "{on}");
    }

    let place = PlaceView {
        key: "opaque-cell".into(),
        name: "Threshold".into(),
        origin: PlaceNameOrigin::Authored,
    };
    let text = serde_json::to_string(&place).unwrap();
    assert_eq!(
        text,
        r#"{"key":"opaque-cell","name":"Threshold","origin":"authored"}"#
    );
    // Every place says where its name came from.
    assert!(serde_json::from_str::<PlaceView>(r#"{"key":"k","name":"n"}"#).is_err());

    let combat = |exit: Option<&str>| CombatView {
        hp: 1,
        max_hp: 1,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![],
        events: vec![],
        objective: Some(ObjectiveKind::ReachExit),
        exit: exit.map(str::to_owned),
        victory: false,
        dead: false,
        terminal: false,
    };
    let with = serde_json::to_value(combat(Some("exit-cell"))).unwrap();
    assert_eq!(with["exit"], "exit-cell");
    let without = serde_json::to_value(combat(None)).unwrap();
    assert!(without.get("exit").is_none());
    assert_eq!(
        serde_json::from_value::<CombatView>(without).unwrap(),
        combat(None)
    );
}

#[test]
fn gameplay_acceptance_has_a_typed_receipt_and_an_opaque_intention_identity() {
    let response = ServerMessage::Ack {
        context: ReplyContext {
            input: InputContext {
                stream: StreamContext {
                    stream: StreamId("attachment".into()),
                    epoch: 1,
                },
                readiness_revision: 3,
            },
            actor: ActorId(7),
            branch: BranchId("current-branch".into()),
            cursor: StreamCursor {
                sequence: 4,
                tick: 10,
            },
            revision: 2,
        },
        request_id: "request".into(),
        receipt: RequestReceipt::Admitted {
            actor: ActorId(7),
            branch: BranchId("branch".into()),
            intention: IntentionId("opaque-intention".into()),
            entry_id: EntryId("admission-record".into()),
            phase: IntentionPhase::Queued,
        },
    };
    let wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["receipt"]["type"], "admitted");
    assert_eq!(wire["receipt"]["phase"], "queued");
    assert_eq!(wire["receipt"]["intention"], "opaque-intention");
    assert!(wire.get("entry_id").is_none());
    assert_eq!(
        serde_json::from_value::<ServerMessage>(wire).unwrap(),
        response
    );
    assert!(serde_json::from_value::<ServerMessage>(serde_json::json!({
        "type": "ack", "request_id": "request", "entry_id": "old-completion"
    }))
    .is_err());
}

#[test]
fn immediate_completion_is_distinct_from_admitted_gameplay() {
    let response = ServerMessage::Ack {
        context: ReplyContext {
            input: InputContext {
                stream: StreamContext {
                    stream: StreamId("attachment".into()),
                    epoch: 0,
                },
                readiness_revision: 0,
            },
            actor: ActorId(1),
            branch: BranchId("branch-1".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            revision: 0,
        },
        request_id: "save".into(),
        receipt: RequestReceipt::Immediate {
            actor: ActorId(1),
            branch: BranchId("branch-1".into()),
            entry_id: None,
        },
    };
    let wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["receipt"]["type"], "immediate");
    assert_eq!(wire["receipt"]["actor"], "1");
    assert_eq!(wire["receipt"]["branch"], "branch-1");
    for required in ["actor", "branch"] {
        let mut missing = wire.clone();
        missing["receipt"].as_object_mut().unwrap().remove(required);
        assert!(serde_json::from_value::<ServerMessage>(missing).is_err());
    }
    assert_eq!(
        serde_json::from_value::<ServerMessage>(wire).unwrap(),
        response
    );
}

#[test]
fn readiness_is_required_and_does_not_accept_extra_authority() {
    let samples: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/wire-v27.json")).unwrap();
    let snapshot = samples["server"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["type"] == "snapshot")
        .unwrap()
        .clone();
    let mut missing = snapshot.clone();
    missing["snapshot"]
        .as_object_mut()
        .unwrap()
        .remove("readiness");
    assert!(serde_json::from_value::<ServerMessage>(missing).is_err());
    let mut forged = snapshot.clone();
    forged["snapshot"]["readiness"]["actor"] = serde_json::json!(999);
    assert!(serde_json::from_value::<ServerMessage>(forged).is_err());
    for field in ["revision", "admission", "resume", "cancel"] {
        let mut missing = snapshot.clone();
        missing["snapshot"]["readiness"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<ServerMessage>(missing).is_err());
    }
}

#[test]
fn a_command_requires_the_stream_and_readiness_context_it_was_built_from() {
    let request = serde_json::json!({
        "type": "command", "branch": "branch-1",
        "command": {"type": "act", "expected_revision": "0", "action": {"type": "wait"}}
    });
    assert!(
        serde_json::from_value::<Request>(request.clone()).is_err(),
        "a bare branch/revision must not authorize a newly submitted command"
    );
    let mut contextual = request;
    contextual["context"] = serde_json::json!({
        "stream": {"stream": "current-attachment", "epoch": "3"}, "readiness_revision": "7",
    });
    let decoded: Request = serde_json::from_value(contextual.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), contextual);
    for field in ["stream", "readiness_revision"] {
        let mut missing = contextual.clone();
        missing["context"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Request>(missing).is_err());
    }
    let mut forged = contextual;
    forged["context"]["has_control"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Request>(forged).is_err());
}

#[test]
fn wire_64_bit_unsigned_identities_and_cursors_use_decimal_strings() {
    assert_eq!(
        serde_json::to_value(ActorId(u64::MAX)).unwrap(),
        serde_json::json!("18446744073709551615")
    );
    assert_eq!(
        serde_json::to_value(StreamCursor {
            sequence: u64::MAX,
            tick: 9007199254740993
        })
        .unwrap(),
        serde_json::json!({"sequence":"18446744073709551615","tick":"9007199254740993"})
    );
    assert_eq!(
        serde_json::to_value(StreamContext {
            stream: StreamId("attachment".into()),
            epoch: u64::MAX
        })
        .unwrap(),
        serde_json::json!({"stream":"attachment","epoch":"18446744073709551615"})
    );
    assert_eq!(
        serde_json::to_value(InputContext {
            stream: StreamContext {
                stream: StreamId("attachment".into()),
                epoch: 0
            },
            readiness_revision: u64::MAX
        })
        .unwrap()["readiness_revision"],
        serde_json::json!("18446744073709551615")
    );
}

#[test]
fn wire_64_bit_actor_decoding_rejects_numbers_and_noncanonical_strings() {
    for input in [
        serde_json::json!(1),
        serde_json::json!(u64::MAX),
        serde_json::json!(""),
        serde_json::json!("01"),
        serde_json::json!("+1"),
        serde_json::json!("-0"),
        serde_json::json!("-1"),
        serde_json::json!("1.0"),
        serde_json::json!("1e3"),
        serde_json::json!(" 1"),
        serde_json::json!("1 "),
        serde_json::json!("18446744073709551616"),
        serde_json::json!("١"),
    ] {
        assert!(
            serde_json::from_value::<ActorId>(input.clone()).is_err(),
            "accepted {input}"
        );
    }
    for value in [
        0,
        1,
        9007199254740991,
        9007199254740992,
        9007199254740993,
        u64::MAX,
    ] {
        assert_eq!(
            serde_json::from_value::<ActorId>(serde_json::json!(value.to_string())).unwrap(),
            ActorId(value)
        );
    }
}

#[test]
fn wire_64_bit_optional_quantities_preserve_missing_and_null() {
    for input in [
        serde_json::json!({"type":"take","item":"18446744073709551615"}),
        serde_json::json!({"type":"take","item":"18446744073709551615","quantity":null}),
    ] {
        assert_eq!(
            serde_json::from_value::<Action>(input).unwrap(),
            Action::Take {
                item: u64::MAX,
                quantity: None
            }
        );
    }
    let input = serde_json::json!({"type":"drop","item":"18446744073709551615","quantity":"9007199254740993"});
    assert_eq!(
        serde_json::from_value::<Action>(input).unwrap(),
        Action::Drop {
            item: u64::MAX,
            quantity: Some(9007199254740993)
        }
    );
    assert!(serde_json::from_value::<Action>(
        serde_json::json!({"type":"take","item":"1","quantity":1})
    )
    .is_err());
}

#[test]
fn wire_64_bit_signed_motion_keeps_extremes_and_small_scalars_are_numbers() {
    let motion = MotionView {
        velocity: [i64::MIN, 0, i64::MAX],
        units_per_cell: 256,
        displaced: false,
        impacted: false,
    };
    let expected = serde_json::json!({"velocity":["-9223372036854775808","0","9223372036854775807"],"units_per_cell":256,"displaced":false,"impacted":false});
    assert_eq!(serde_json::to_value(&motion).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<MotionView>(expected).unwrap(),
        motion
    );
    for value in [
        serde_json::json!(0),
        serde_json::json!("-0"),
        serde_json::json!("+1"),
        serde_json::json!("01"),
        serde_json::json!("-01"),
        serde_json::json!("9223372036854775808"),
        serde_json::json!("-9223372036854775809"),
    ] {
        assert!(serde_json::from_value::<MotionView>(serde_json::json!({"velocity":[value,"0","0"],"units_per_cell":256,"displaced":false,"impacted":false})).is_err());
    }
}

#[test]
fn capability_limits_are_explicit_bounded_and_required_in_welcome() {
    let capabilities = ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16);
    assert!(capabilities.is_valid());
    assert_eq!(capabilities.max_request_bytes, MAX_REQUEST_BYTES as u32);
    assert_eq!(
        capabilities.max_retained_state_bytes,
        MAX_STATE_BYTES as u32
    );
    assert_eq!(
        capabilities.max_history_page_entries,
        MAX_HISTORY_PAGE as u32
    );
    for invalid in [
        ServerCapabilities {
            max_response_bytes: 0,
            ..capabilities
        },
        ServerCapabilities {
            max_response_bytes: MAX_RESPONSE_BYTES as u32 + 1,
            ..capabilities
        },
        ServerCapabilities {
            max_connections: 0,
            ..capabilities
        },
        ServerCapabilities {
            max_request_bytes: 0,
            ..capabilities
        },
        ServerCapabilities {
            max_request_bytes: MAX_REQUEST_BYTES as u32 + 1,
            ..capabilities
        },
        ServerCapabilities {
            max_history_page_entries: 0,
            ..capabilities
        },
        ServerCapabilities {
            max_retained_state_bytes: MAX_STATE_BYTES as u32 + 1,
            ..capabilities
        },
    ] {
        assert!(!invalid.is_valid());
    }
    let missing = serde_json::json!({"type":"welcome", "protocol":PROTOCOL_VERSION,
        "user":"test", "actors":["1"], "role":"player"});
    assert!(serde_json::from_value::<ServerMessage>(missing).is_err());
}
