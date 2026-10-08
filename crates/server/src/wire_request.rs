//! Resolve saved requests before consulting current target availability.
use super::{valid_label, ActorId, BranchId, Command, CommandResult, Engine, Failure};
use crate::wire_adapter::{self, DecodedCommand, TargetScope};
use tor_protocol::ErrorCode;

impl Engine {
    /// Derivation is independent of current actor/target existence for history and retries.
    pub fn target_scope(&self, actor: ActorId) -> TargetScope {
        TargetScope::new(
            uuid::Uuid::parse_str(&self.archive.view_salt).expect("validated archive privacy salt"),
            tor_simulation::ActorId(actor.0),
        )
    }

    pub fn encode_action(
        &self,
        actor: ActorId,
        action: &crate::journal::Action,
    ) -> tor_protocol::Action {
        wire_adapter::encode_action(action, &self.target_scope(actor))
    }

    pub fn decode_action(
        &self,
        actor: ActorId,
        action: &tor_protocol::Action,
    ) -> Result<crate::journal::Action, Failure> {
        wire_adapter::decode_action(
            action,
            &self.target_scope(actor),
            &self.revision_view(actor)?.observation,
        )
    }

    pub fn encode_command(
        &self,
        actor: ActorId,
        command: Command,
    ) -> Result<tor_protocol::Command, &'static str> {
        wire_adapter::encode_command(command, &self.target_scope(actor))
    }

    /// The caller still controls disclosure to the entry's owning actor and audience.
    pub fn disclose_entry(
        &self,
        entry: &crate::journal::JournalEntry,
    ) -> Option<tor_protocol::HistoryEntry> {
        wire_adapter::disclose_entry(entry, &self.target_scope(entry.actor))
    }

    pub(crate) fn retry_decoded(
        &self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: &DecodedCommand,
    ) -> Result<Option<CommandResult>, Failure> {
        if !valid_label(user) || !valid_label(frontend) || !valid_label(request_id) {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid identity or request ID",
            ));
        }
        let scope = self.target_scope(actor);
        self.retry_matching(user, actor, request_id, branch, |original| {
            command.matches(original, &scope)
        })
    }

    /// Fresh resolution follows request metadata checks and reads the cached native disclosure.
    pub fn resolve_command(
        &self,
        actor: ActorId,
        branch: &BranchId,
        command: DecodedCommand,
    ) -> Result<Command, Failure> {
        self.check_metadata(actor, branch, command.revision_requirement())?;
        match command {
            DecodedCommand::Backend(command) => Ok(command),
            DecodedCommand::Gameplay {
                expected_revision,
                action,
            } => Ok(Command::AdmitIntention {
                expected_revision,
                action: self.decode_action(actor, &action)?,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{journal::Action, Scenario};
    use tor_protocol::{Action as WireAction, Command as WireCommand};

    fn spawn_then_fork_must_not_rebind_target(
        first: crate::journal::WizardOperation,
        second: crate::journal::WizardOperation,
    ) {
        use crate::journal::{JournalContent, WizardOperation, WizardResult};
        let actor = ActorId(1);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fork-identities.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        };
        let mut engine =
            Engine::open_with_policy(&path, Scenario::two_room(42), policy.clone()).unwrap();
        engine.enable_wizard().unwrap();
        let wizard = |engine: &mut Engine, request: &str, operation| {
            let command = Command::Wizard {
                expected_revision: engine.revision(actor).unwrap(),
                operation,
            };
            engine
                .command(
                    "player",
                    "test",
                    actor,
                    request,
                    &engine.branch().clone(),
                    command,
                )
                .unwrap()
        };
        let target = |engine: &Engine, result: &CommandResult| {
            let JournalContent::Wizard { result, .. } = &result.entry.content else {
                panic!("fixture must create a wizard entity")
            };
            let action = match result {
                WizardResult::ItemPlaced { item } => Action::Take {
                    item: *item,
                    quantity: None,
                },
                WizardResult::DoorPlaced { door } => Action::SetDoor {
                    door: *door,
                    open: true,
                },
                WizardResult::ActorSpawned { actor } => Action::Attack {
                    target: tor_simulation::ActorId(actor.0),
                },
                _ => panic!("fixture must create an interaction target"),
            };
            (engine.encode_action(actor, &action), action)
        };
        let created = wizard(&mut engine, "first", first);
        let (old_reference, old_action) = target(&engine, &created);
        assert_eq!(
            engine.decode_action(actor, &old_reference).unwrap(),
            old_action
        );
        wizard(
            &mut engine,
            "rewind",
            WizardOperation::Rewind { target: None },
        );
        let rewound = engine.state(actor).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let mut engine =
            Engine::open_with_policy(&path, Scenario::two_room(42), policy.clone()).unwrap();
        assert_eq!(engine.state(actor).unwrap(), rewound);
        engine.enable_wizard().unwrap();
        let recreated = wizard(&mut engine, "second", second);
        let (new_reference, new_action) = target(&engine, &recreated);
        assert_eq!(
            engine.decode_action(actor, &new_reference).unwrap(),
            new_action
        );
        assert_ne!(
            old_reference, new_reference,
            "An abandoned entity must not become a different entity"
        );
        assert_eq!(
            engine
                .decode_action(actor, &old_reference)
                .unwrap_err()
                .code,
            ErrorCode::InvalidAction
        );
        let current = engine.state(actor).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let engine = Engine::open_with_policy(&path, Scenario::two_room(42), policy).unwrap();
        assert_eq!(engine.state(actor).unwrap(), current);
        assert_eq!(
            engine.decode_action(actor, &new_reference).unwrap(),
            new_action
        );
        assert_eq!(
            engine
                .decode_action(actor, &old_reference)
                .unwrap_err()
                .code,
            ErrorCode::InvalidAction
        );
    }

    #[test]
    fn rewind_does_not_rebind_an_abandoned_item_reference() {
        use crate::journal::{Position, WizardItem, WizardOperation};
        let position = Position {
            region: 1,
            x: 2,
            y: 1,
            z: 0,
        };
        spawn_then_fork_must_not_rebind_target(
            WizardOperation::PlaceItem {
                position,
                kind: WizardItem::Token,
            },
            WizardOperation::PlaceItem {
                position,
                kind: WizardItem::Tablet,
            },
        );
    }

    #[test]
    fn rewind_does_not_rebind_an_abandoned_actor_reference() {
        use crate::journal::{Position, WizardOperation};
        let position = Position {
            region: 1,
            x: 2,
            y: 1,
            z: 0,
        };
        spawn_then_fork_must_not_rebind_target(
            WizardOperation::SpawnActor {
                position,
                turn_ticks: 75,
            },
            WizardOperation::SpawnActor {
                position,
                turn_ticks: 100,
            },
        );
    }

    #[test]
    fn rewind_does_not_rebind_an_abandoned_door_reference() {
        use crate::journal::{Position, WizardOperation};
        let position = Position {
            region: 1,
            x: 2,
            y: 1,
            z: 0,
        };
        spawn_then_fork_must_not_rebind_target(
            WizardOperation::PlaceDoor {
                position,
                open: false,
                height: 1,
            },
            WizardOperation::PlaceDoor {
                position,
                open: true,
                height: 1,
            },
        );
    }

    #[test]
    fn foreign_observer_and_save_references_do_not_resolve_disclosed_entities() {
        let actor = ActorId(1);
        let engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let other_save = Engine::memory(Scenario::two_room(42)).unwrap();
        let native = &engine.revision_view(actor).unwrap().observation;
        let item = native.ground_items[0].id.0;
        let door = native
            .visible_cells
            .iter()
            .find_map(|cell| cell.door.as_ref())
            .unwrap()
            .id;
        let before = engine.state(actor).unwrap();
        for action in [
            Action::Take {
                item,
                quantity: None,
            },
            Action::SetDoor { door, open: false },
            Action::Attack {
                target: tor_simulation::ActorId(actor.0),
            },
        ] {
            let disclosed = engine.encode_action(actor, &action);
            assert_eq!(engine.decode_action(actor, &disclosed).unwrap(), action);
            for foreign_scope in [
                engine.target_scope(ActorId(2)),
                other_save.target_scope(actor),
            ] {
                let foreign = wire_adapter::encode_action(&action, &foreign_scope);
                assert_ne!(foreign, disclosed);
                let rejected = engine.decode_action(actor, &foreign).unwrap_err();
                assert_eq!(rejected.code, ErrorCode::InvalidAction);
                assert_eq!(engine.state(actor).unwrap(), before);
            }
        }
    }

    #[test]
    fn durable_wire_retry_resolves_before_a_taken_item_or_revision_is_checked() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("opaque-retry.db");
        let actor = ActorId(1);
        let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        let before = engine.state(actor).unwrap();
        let item = before.observation.ground_items[0].item.id;
        let branch = engine.branch().clone();
        let wire = WireCommand::Act {
            expected_revision: before.revision,
            action: WireAction::Take {
                item,
                quantity: None,
            },
        };
        let decoded = wire_adapter::decode_command(&wire).unwrap();
        let command = engine
            .resolve_command(actor, &branch, decoded.clone())
            .unwrap();
        let accepted = engine
            .command("player", "test", actor, "take", &branch, command)
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let taken = engine.state(actor).unwrap();
        assert!(taken
            .observation
            .ground_items
            .iter()
            .all(|ground| ground.item.id != item));
        assert!(taken
            .observation
            .inventory
            .iter()
            .any(|held| held.id == item));
        engine.flush().unwrap();
        drop(engine);

        let engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        assert_eq!(engine.state(actor).unwrap(), taken);
        let retry = engine
            .retry_decoded("player", "other-client", actor, "take", &branch, &decoded)
            .unwrap()
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, accepted.entry);
        assert_eq!(engine.state(actor).unwrap(), taken);

        let fresh = WireCommand::Act {
            expected_revision: taken.revision,
            action: WireAction::Take {
                item,
                quantity: None,
            },
        };
        assert_eq!(
            engine
                .resolve_command(
                    actor,
                    &branch,
                    wire_adapter::decode_command(&fresh).unwrap()
                )
                .unwrap_err()
                .code,
            ErrorCode::InvalidAction
        );
        assert_eq!(
            engine
                .resolve_command(actor, &branch, decoded)
                .unwrap_err()
                .code,
            ErrorCode::StaleRevision
        );
    }

    #[test]
    fn dropped_target_receipt_survives_rewind_and_restart_without_inventory_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("opaque-rewind.db");
        let actor = ActorId(1);
        let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let branch = engine.branch().clone();
        let item = engine.state(actor).unwrap().observation.ground_items[0]
            .item
            .id;
        let mut accepted_drop = None;
        for (request, action) in [
            (
                "take",
                WireAction::Take {
                    item,
                    quantity: None,
                },
            ),
            (
                "drop",
                WireAction::Drop {
                    item,
                    quantity: None,
                },
            ),
        ] {
            let decoded = wire_adapter::decode_command(&WireCommand::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action,
            })
            .unwrap();
            let command = engine
                .resolve_command(actor, &branch, decoded.clone())
                .unwrap();
            let accepted = engine
                .command("player", "test", actor, request, &branch, command)
                .unwrap();
            engine.execute_next_intention().unwrap().unwrap();
            if request == "drop" {
                accepted_drop = Some((decoded, accepted));
            }
        }
        engine
            .command(
                "player",
                "test",
                actor,
                "rewind",
                &branch,
                Command::Wizard {
                    expected_revision: engine.revision(actor).unwrap(),
                    operation: crate::journal::WizardOperation::Rewind { target: None },
                },
            )
            .unwrap();
        assert_ne!(engine.branch(), &branch);
        engine.flush().unwrap();
        drop(engine);

        let engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        let restored = engine.state(actor).unwrap();
        assert!(restored
            .observation
            .inventory
            .iter()
            .all(|held| held.id != item));
        let (decoded, accepted) = accepted_drop.unwrap();
        let retry = engine
            .retry_decoded("player", "reconnected", actor, "drop", &branch, &decoded)
            .unwrap()
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, accepted.entry);
        let fresh = wire_adapter::decode_command(&WireCommand::Act {
            expected_revision: restored.revision,
            action: WireAction::Drop {
                item,
                quantity: None,
            },
        })
        .unwrap();
        assert_eq!(
            engine
                .resolve_command(actor, engine.branch(), fresh)
                .unwrap_err()
                .code,
            ErrorCode::InvalidAction
        );
        assert_eq!(engine.state(actor).unwrap(), restored);
    }

    #[test]
    fn unavailable_wire_targets_conflict_with_existing_receipts_before_resolution() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let branch = engine.branch().clone();
        let revision = engine.revision(actor).unwrap();
        engine
            .command(
                "player",
                "test",
                actor,
                "accepted",
                &branch,
                Command::AdmitIntention {
                    expected_revision: revision,
                    action: Action::Wait,
                },
            )
            .unwrap();
        let unavailable = wire_adapter::decode_command(&WireCommand::Act {
            expected_revision: revision,
            action: WireAction::Take {
                item: engine
                    .target_scope(actor)
                    .item(tor_simulation::ItemId(u64::MAX)),
                quantity: None,
            },
        })
        .unwrap();
        assert_eq!(
            engine
                .retry_decoded("player", "test", actor, "accepted", &branch, &unavailable)
                .unwrap_err()
                .code,
            ErrorCode::RequestConflict
        );
        assert!(engine
            .retry_decoded("player", "test", actor, "fresh", &branch, &unavailable)
            .unwrap()
            .is_none());
        assert_eq!(
            engine
                .retry_decoded("player", "", actor, "accepted", &branch, &unavailable)
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
}
