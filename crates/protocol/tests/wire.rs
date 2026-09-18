use tor_protocol::*;

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
