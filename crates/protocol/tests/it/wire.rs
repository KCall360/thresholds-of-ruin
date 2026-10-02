use tor_protocol::*;

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
