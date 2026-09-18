use tor_protocol::*;

#[test]
fn wizard_wire_rejects_arbitrary_archetypes_and_forged_properties() {
    for operation in [
        serde_json::json!({"type":"place_item","kind":"sword","position":{"region":1,"x":1,"y":1,"z":0}}),
        serde_json::json!({"type":"spawn_actor","position":{"region":1,"x":1,"y":1,"z":0},"turn_ticks":100,"god":true}),
    ] {
        assert!(serde_json::from_value::<WizardOperation>(operation).is_err());
    }
    assert!(!AccessRole::Spectator.permits(&Request::Command {
        branch: BranchId("branch".into()),
        command: Command::Wizard {
            expected_revision: 0,
            operation: WizardOperation::Rewind { target: None }
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
