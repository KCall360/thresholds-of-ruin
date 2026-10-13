//! Complete wire examples have one maintained source in the protocol reference.
//! Exercise the production bounded decoders, not merely JSON syntax validation.
use tor_protocol::{decode_request, decode_response};

fn validate_examples(markdown: &str) -> Result<(usize, usize), String> {
    let mut active = None;
    let mut body = String::new();
    let (mut clients, mut servers) = (0, 0);
    for (number, line) in markdown.lines().enumerate() {
        if let Some((client, start)) = active {
            if line == "```" {
                let result = if client {
                    clients += 1;
                    decode_request(&body).map(|_| ())
                } else {
                    servers += 1;
                    decode_response(&body).map(|_| ())
                };
                result.map_err(|error| format!("wire example at line {start}: {error}"))?;
                active = None;
                body.clear();
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else if matches!(line, "```json client-message" | "```json server-message") {
            active = Some((line == "```json client-message", number + 1));
        } else if line.starts_with("```json ") {
            return Err(format!("unknown JSON example role at line {}", number + 1));
        }
    }
    if let Some((_, start)) = active {
        return Err(format!("unclosed wire example at line {start}"));
    }
    Ok((clients, servers))
}

#[test]
fn complete_documented_wire_examples_use_the_real_decoders() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/reference/protocol.md"
    ))
    .expect("the current protocol reference must exist");
    let (clients, servers) = validate_examples(&source).expect("documented wire schema");
    assert!(clients > 0, "missing complete client examples");
    assert!(servers > 0, "missing complete server examples");
}

#[test]
fn documentation_validator_rejects_valid_json_with_invalid_wire_contracts() {
    let valid = serde_json::json!({
        "type": "request", "request_id": "example-attack",
        "request": {
            "type": "command", "context": {
                "stream": {"stream": "example-stream", "epoch": "1"},
                "readiness_revision": "0"
            }, "branch": "example-branch",
            "command": {"type": "act", "expected_revision": "0", "action": {"supplies":{"kind":"carried"},
                "type": "use_ability", "id": "basic_melee",
                "target": tor_protocol::ActorTarget::from_digest([0; 32])
            }}
        }
    });
    let example = |message: &serde_json::Value| format!("```json client-message\n{message}\n```\n");
    assert_eq!(validate_examples(&example(&valid)).unwrap(), (1, 0));
    let mut numeric_revision = valid.clone();
    numeric_revision["request"]["command"]["expected_revision"] = serde_json::json!(0);
    let mut numeric_target = valid.clone();
    numeric_target["request"]["command"]["action"]["target"] = serde_json::json!(1);
    let mut missing_context = valid.clone();
    missing_context["request"]
        .as_object_mut()
        .unwrap()
        .remove("context");
    let mut unknown_field = valid;
    unknown_field["request"]["command"]["action"]["secret"] = serde_json::json!(true);
    for invalid in [
        numeric_revision,
        numeric_target,
        missing_context,
        unknown_field,
    ] {
        assert!(
            validate_examples(&example(&invalid)).is_err(),
            "accepted {invalid}"
        );
    }
    let missing_reply_context = r#"```json server-message
{"type":"palette","request_id":null,"palette":{"revision":"1","body":{"type":"full","assets":[]}}}
```"#;
    assert!(validate_examples(missing_reply_context).is_err());
}

#[test]
fn documentation_validator_rejects_unclosed_examples_and_ignores_fragments() {
    assert!(validate_examples("```json client-message\n{}").is_err());
    assert!(validate_examples("```json client-message\n{}\n```").is_err());
    assert!(validate_examples("```json client-mesage\n{}\n```").is_err());
    assert_eq!(
        validate_examples("```json\n{\"fragment\":true}\n```\n").unwrap(),
        (0, 0)
    );
}
