use tor_protocol::{
    Action, ActorId, Anchor, AnnotationCategory, Audience, ClientSource, Direction,
};
use tor_server::journal::{Command, WizardOperation};
use tor_server::{wire_adapter::DecodedCommand, Engine, Scenario};

#[test]
fn ordinary_commands_preserve_every_payload_at_the_wire_boundary() {
    use tor_protocol::Command as Wire;
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let scope = engine.target_scope(ActorId(1));
    let basis = tor_protocol::AdvancementBasis {
        actor: ActorId(1),
        branch: engine.branch().clone(),
        stream: tor_protocol::StreamId("original-preview-stream".into()),
        reset: u64::MAX,
        build_epoch: u64::MAX,
    };
    let purchase = tor_protocol::PurchaseOperation {
        owner: 255,
        pool: "advancement".into(),
        selection: tor_protocol::PurchaseChoice::SkillRank {
            skill: "perception".into(),
        },
    };
    let replacement = tor_protocol::ReplacementOperation {
        source: tor_protocol::ReplacementSource {
            owner: 255,
            index: 255,
        },
        replacement: Some(tor_protocol::ReplacementPurchase {
            pool: "advancement".into(),
            selection: tor_protocol::PurchaseChoice::Talent {
                talent: "health_foundation".into(),
            },
        }),
    };
    let mut commands = vec![
        Wire::ProjectileChoices {
            expected_revision: u64::MAX,
            query: tor_protocol::ProjectileChoiceQuery {
                ability: "fixture.launch".into(),
                phase: tor_protocol::ProjectileChoicePhase::Ammunition {
                    launcher: tor_protocol::EquippedProjectileWeapon {
                        item: scope.item(tor_simulation::ItemId(u64::MAX)),
                        socket: 63,
                    },
                },
                offset: u32::MAX,
            },
        },
        Wire::Advancement {
            expected_revision: u64::MAX,
            query: tor_protocol::AdvancementQuery::Overview {},
        },
        Wire::Advancement {
            expected_revision: u64::MAX,
            query: tor_protocol::AdvancementQuery::Preview {
                basis: basis.clone(),
                proposal: tor_protocol::AdvancementProposal::Replacement {
                    replacement: replacement.clone(),
                },
                section: tor_protocol::AdvancementPreviewSection::Removed,
                offset: 255,
            },
        },
        Wire::ResumeIntention {
            expected_revision: u64::MAX,
            intention: tor_protocol::IntentionId("original-admission".into()),
        },
        Wire::CancelIntention {
            step: None,
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
        Action::Purchase {
            basis: Box::new(basis.clone()),
            purchase,
        },
        Action::Respec {
            basis: Box::new(basis),
            replacement,
            supplies: tor_protocol::SupplySelection::Explicit {
                items: vec![tor_protocol::SelectedSupply {
                    cost_ordinal: 0,
                    item: scope.item(tor_simulation::ItemId(u64::MAX)),
                }],
            },
        },
        Action::UseAbility {
            id: "basic_melee".into(),
            invocation: tor_protocol::AbilityInvocation {
                aim: tor_protocol::AbilityAim::ActorTarget {
                    target: scope.actor(tor_simulation::ActorId(u64::MAX)),
                },
                weapon: tor_protocol::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
        Action::SetDoor {
            door: scope.door(u64::MAX),
            open: true,
        },
        Action::Move {
            direction: Direction::North,
        },
        Action::Take {
            item: scope.item(tor_simulation::ItemId(u64::MAX)),
            quantity: Some(u64::MAX),
        },
        Action::Drop {
            item: scope.item(tor_simulation::ItemId(u64::MAX)),
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
        let reconstructed = match backend {
            DecodedCommand::Backend(command) => engine.encode_command(ActorId(1), command).unwrap(),
            DecodedCommand::ProjectileChoices {
                expected_revision,
                query,
            } => Wire::ProjectileChoices {
                expected_revision,
                query,
            },
            DecodedCommand::Advancement {
                expected_revision,
                query,
            } => Wire::Advancement {
                expected_revision,
                query,
            },
            DecodedCommand::AbilityWeaponPreview { .. }
            | DecodedCommand::CreatureCatalog { .. }
            | DecodedCommand::CreatureInspection { .. }
            | DecodedCommand::StrikeInspection { .. }
            | DecodedCommand::CombatDiagnostics { .. } => {
                panic!("read-only query is not a journal command")
            }
            DecodedCommand::Gameplay {
                expected_revision,
                action,
            } => Wire::Act {
                expected_revision,
                action,
            },
        };
        assert_eq!(reconstructed, wire);
    }
}

#[test]
fn fresh_gameplay_resolves_to_admission_and_developer_spelling_is_normalized() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let wire = tor_protocol::Command::Act {
        expected_revision: 0,
        action: Action::Wait,
    };
    assert_eq!(
        engine
            .resolve_command(
                ActorId(1),
                engine.branch(),
                tor_server::wire_adapter::decode_command(&wire).unwrap()
            )
            .unwrap(),
        Command::AdmitIntention {
            expected_revision: 0,
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
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let backend = Command::Wizard {
        expected_revision: u64::MAX,
        operation: WizardOperation::SetGravity {
            region: u64::MAX,
            vector: [0, 0, -1],
        },
    };
    let wire = engine.encode_command(ActorId(1), backend.clone()).unwrap();
    assert_eq!(
        tor_server::wire_adapter::decode_command(&wire).unwrap(),
        DecodedCommand::Backend(backend)
    );
    assert!(
        tor_server::wire_adapter::decode_command(&tor_protocol::Command::Wizard {
            expected_revision: 0,
            operation: "invalid developer operation".into()
        })
        .is_err()
    );
    assert!(engine
        .encode_command(ActorId(1), Command::PausePreparation)
        .is_err());
}

#[test]
fn backend_actions_use_independent_types_and_numeric_save_payloads() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    use std::any::TypeId;
    use tor_server::journal::{Action as BackendAction, Direction as BackendDirection};
    assert_ne!(TypeId::of::<BackendAction>(), TypeId::of::<Action>());
    assert_ne!(TypeId::of::<BackendDirection>(), TypeId::of::<Direction>());
    let action = BackendAction::UseAbility {
        id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
        invocation: tor_server::journal::AbilityInvocation {
            aim: tor_server::journal::AbilityAim::ActorTarget {
                target: tor_simulation::ActorId(u64::MAX),
            },
            weapon: tor_server::journal::WeaponSelection::Default,
        },
        supplies: Default::default(),
    };
    let command = Command::AdmitIntention {
        expected_revision: u64::MAX,
        action,
    };
    let stored = serde_json::to_value(&command).unwrap();
    assert_eq!(
        stored["action"]["invocation"],
        serde_json::json!({"aim":{
            "kind": "actor_target", "target": u64::MAX,
        },"weapon":{"kind":"default"}})
    );
    assert!(stored["action"].get("target").is_none());
    assert_eq!(serde_json::from_value::<Command>(stored).unwrap(), command);
    let wire = engine.encode_command(ActorId(1), command.clone()).unwrap();
    assert!(tor_server::wire_adapter::decode_command(&wire)
        .unwrap()
        .matches(&command, &engine.target_scope(ActorId(1))));
    let wire = serde_json::to_value(wire).unwrap();
    assert_eq!(
        wire["action"]["invocation"],
        serde_json::json!({"aim":{
            "kind": "actor_target",
            "target": engine.target_scope(ActorId(1))
                .actor(tor_simulation::ActorId(u64::MAX)).to_string(),
        },"weapon":{"kind":"default"}})
    );
    assert!(wire["action"].get("target").is_none());
}

#[test]
fn readonly_creature_query_cannot_be_resolved_into_a_journal_command() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    let query = tor_server::wire_adapter::decode_command(&tor_protocol::Command::Wizard {
        expected_revision: before.revision,
        operation: "creature inspect 999".into(),
    })
    .unwrap();
    assert!(matches!(
        query,
        DecodedCommand::CreatureInspection {
            target: ActorId(999),
            ..
        }
    ));
    assert!(!query.matches(
        &Command::Wizard {
            expected_revision: before.revision,
            operation: WizardOperation::Rewind { target: None }
        },
        &engine.target_scope(ActorId(1))
    ));
    let failure = engine
        .resolve_command(ActorId(1), engine.branch(), query)
        .unwrap_err();
    assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidRequest);
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn readonly_advancement_query_cannot_be_resolved_into_a_journal_command() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    let wire = tor_protocol::Command::Advancement {
        expected_revision: before.revision,
        query: tor_protocol::AdvancementQuery::Overview {},
    };
    let query = tor_server::wire_adapter::decode_command(&wire).unwrap();
    assert_eq!(
        query,
        DecodedCommand::Advancement {
            expected_revision: before.revision,
            query: tor_protocol::AdvancementQuery::Overview {},
        }
    );
    assert!(!query.matches(
        &Command::Act {
            expected_revision: before.revision,
            action: tor_server::journal::Action::Wait
        },
        &engine.target_scope(ActorId(1))
    ));
    let failure = engine
        .resolve_command(ActorId(1), engine.branch(), query)
        .unwrap_err();
    assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidRequest);
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
}
