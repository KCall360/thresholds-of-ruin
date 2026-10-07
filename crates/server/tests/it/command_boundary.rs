use tor_protocol::{
    Action, ActorId, Anchor, AnnotationCategory, Audience, ClientSource, Direction,
};
use tor_server::journal::{Command, WizardOperation};

#[test]
fn ordinary_commands_preserve_every_payload_at_the_wire_boundary() {
    use tor_protocol::Command as Wire;
    let mut commands = vec![
        Wire::ResumeIntention {
            expected_revision: u64::MAX,
            intention: tor_protocol::IntentionId("original-admission".into()),
        },
        Wire::CancelIntention {
            expected_revision: u64::MAX,
            intention: tor_protocol::IntentionId("original-admission".into()),
        },
        Wire::RenamePlace {
            expected_revision: u64::MAX,
            key: "opaque-key".into(),
            name: "North 🌒\nHall".into(),
        },
        Wire::Travel {
            expected_revision: u64::MAX,
            destination: "remembered-place".into(),
        },
    ];
    for action in [
        Action::Attack {
            target: ActorId(u64::MAX),
        },
        Action::SetDoor {
            door: u64::MAX,
            open: true,
        },
        Action::Move {
            direction: Direction::North,
        },
        Action::Take {
            item: u64::MAX,
            quantity: Some(u64::MAX),
        },
        Action::Drop {
            item: u64::MAX,
            quantity: None,
        },
        Action::Wait,
    ] {
        commands.push(Wire::Act {
            expected_revision: u64::MAX,
            action,
        });
    }
    for anchor in [
        Anchor::State { revision: u64::MAX },
        Anchor::Entry {
            id: tor_protocol::EntryId("opaque-entry".into()),
        },
    ] {
        for source in [ClientSource::User, ClientSource::Frontend] {
            for audience in [Audience::Private, Audience::Actor] {
                for category in [
                    AnnotationCategory::Note,
                    AnnotationCategory::Bookmark,
                    AnnotationCategory::Explanation,
                ] {
                    commands.push(Wire::Annotate {
                        anchor: anchor.clone(),
                        text: "annotated 🌒\nstate".into(),
                        source,
                        audience,
                        category,
                    });
                }
            }
        }
    }
    for wire in commands {
        let backend = tor_server::wire_adapter::decode_command(&wire).unwrap();
        let reconstructed = tor_server::wire_adapter::encode_command(backend).unwrap();
        assert_eq!(reconstructed, wire);
    }
}

#[test]
fn wire_gameplay_decodes_to_admission_and_developer_spelling_is_normalized() {
    let wire = tor_protocol::Command::Act {
        expected_revision: 7,
        action: Action::Wait,
    };
    assert_eq!(
        tor_server::wire_adapter::decode_command(&wire).unwrap(),
        Command::AdmitIntention {
            expected_revision: 7,
            action: tor_server::journal::Action::Wait,
        }
    );
    let operation = WizardOperation::SetGravity {
        region: 2,
        vector: [0, 0, -1],
    };
    let compact = serde_json::to_string(&operation).unwrap();
    let pretty = serde_json::to_string_pretty(&operation).unwrap();
    let decode = |operation| {
        tor_server::wire_adapter::decode_command(&tor_protocol::Command::Wizard {
            expected_revision: 7,
            operation,
        })
        .unwrap()
    };
    assert_ne!(compact, pretty);
    assert_eq!(decode(compact), decode(pretty));
}

#[test]
fn developer_commands_are_parsed_and_backend_only_commands_stay_private() {
    let backend = Command::Wizard {
        expected_revision: u64::MAX,
        operation: WizardOperation::SetGravity {
            region: u64::MAX,
            vector: [0, 0, -1],
        },
    };
    let wire = tor_server::wire_adapter::encode_command(backend.clone()).unwrap();
    assert_eq!(
        tor_server::wire_adapter::decode_command(&wire).unwrap(),
        backend
    );
    assert!(
        tor_server::wire_adapter::decode_command(&tor_protocol::Command::Wizard {
            expected_revision: 0,
            operation: "invalid developer operation".into()
        })
        .is_err()
    );
    assert!(tor_server::wire_adapter::encode_command(Command::PausePreparation).is_err());
}

#[test]
fn backend_actions_use_independent_types_and_numeric_save_payloads() {
    use std::any::TypeId;
    use tor_server::journal::{Action as BackendAction, Direction as BackendDirection};
    assert_ne!(TypeId::of::<BackendAction>(), TypeId::of::<Action>());
    assert_ne!(TypeId::of::<BackendDirection>(), TypeId::of::<Direction>());
    let action = BackendAction::Attack {
        target: tor_simulation::ActorId(u64::MAX),
    };
    let command = Command::AdmitIntention {
        expected_revision: u64::MAX,
        action,
    };
    let stored = serde_json::to_value(&command).unwrap();
    assert_eq!(stored["action"]["target"], serde_json::json!(u64::MAX));
    assert_eq!(serde_json::from_value::<Command>(stored).unwrap(), command);
    let wire = tor_server::wire_adapter::encode_command(command.clone()).unwrap();
    assert_eq!(
        tor_server::wire_adapter::decode_command(&wire).unwrap(),
        command
    );
    assert_eq!(
        serde_json::to_value(wire).unwrap()["action"]["target"],
        u64::MAX.to_string()
    );
}
