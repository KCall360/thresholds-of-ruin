use tor_protocol::*;

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
        serde_json::from_str::<Action>(r#"{"type":"take","item":10}"#).unwrap(),
        Action::Take {
            item: 10,
            quantity: None
        }
    );
    assert_eq!(
        serde_json::from_str::<Action>(r#"{"type":"drop","item":10,"quantity":3}"#).unwrap(),
        Action::Drop {
            item: 10,
            quantity: Some(3)
        }
    );
    for text in [
        r#"{"type":"take","item":10,"quantity":-1}"#,
        r#"{"type":"take","item":10,"quantity":1.5}"#,
        r#"{"type":"take","item":10,"quantity":18446744073709551616}"#,
        r#"{"type":"drop","item":10,"identity":"healing"}"#,
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
        branch: BranchId("branch".into()),
        command
    }));
    assert!(serde_json::from_str::<Command>(r#"{"type":"rename_place","expected_revision":7,"key":"opaque-cell","name":"Quiet Reverie","region":3}"#).is_err());
}

#[test]
fn spectators_are_not_permitted_wizard_commands() {
    assert!(!AccessRole::Spectator.permits(&Request::Command {
        branch: BranchId("branch".into()),
        command: Command::Wizard {
            expected_revision: 0,
            operation: "rewind initial".into()
        }
    }));
}

#[test]
fn clients_cannot_claim_backend_authorship_or_supply_an_author() {
    let forged = r#"{"type":"annotate","anchor":{"type":"state","revision":0},"text":"spoiler","source":"backend"}"#;
    assert!(serde_json::from_str::<Command>(forged).is_err());
    let forged = r#"{"type":"annotate","anchor":{"type":"state","revision":0},"text":"spoiler","author":{"type":"backend","component":"simulation"}}"#;
    assert!(serde_json::from_str::<Command>(forged).is_err());
}

#[test]
fn user_notes_default_to_private_and_round_trip_as_plain_text() {
    let source = r#"{"type":"annotate","anchor":{"type":"state","revision":0},"text":"<script>not executable</script>\nlook"}"#;
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
        "user":"test", "actors":[1]});
    assert!(serde_json::from_value::<ServerMessage>(old).is_err());
}

#[test]
fn combat_facts_are_data_not_prose() {
    let events: Vec<CombatEventView> = serde_json::from_str(
        r#"[{"type":"attack","attacker":2,"target":null,"outcome":"no_injury"},
            {"type":"interrupted","actor":1},{"type":"died","actor":2}]"#,
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
        r#"{"type":"died","actor":2,"message":"The scout died."}"#
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
        request_id: "save".into(),
        receipt: RequestReceipt::Immediate { entry_id: None },
    };
    let wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["receipt"]["type"], "immediate");
    assert_eq!(
        serde_json::from_value::<ServerMessage>(wire).unwrap(),
        response
    );
}
