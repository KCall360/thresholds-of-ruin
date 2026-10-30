//! Resolve saved requests before consulting current target availability.
use super::{valid_label, ActorId, BranchId, Command, CommandResult, Engine, Failure};
use crate::wire_adapter::{self, DecodedCommand, DisplayCommand, TargetScope};
use tor_protocol::ErrorCode;

impl Engine {
    pub(crate) fn ability_weapon_preview_at(
        &self,
        boundary: &super::PresentationBoundary,
        actor: ActorId,
        ability: &str,
        invocation: tor_protocol::AbilityInvocation,
        supplies: &tor_protocol::SupplySelection,
    ) -> Result<tor_protocol::AbilityWeaponPreview, Failure> {
        let game = self.presentation_game(boundary)?;
        let view = super::revision_view(game, actor)?;
        let action = wire_adapter::decode_action(
            &tor_protocol::Action::UseAbility {
                id: ability.into(),
                invocation,
                supplies: supplies.clone(),
            },
            &self.target_scope(actor),
            &view.observation,
        )?;
        let tor_simulation::Action::UseAbility {
            id,
            invocation: selected,
            supplies: selected_supplies,
        } = crate::adapt::action(&action)
        else {
            unreachable!("ability decoder preserves action kind")
        };
        let value = game
            .preview_ability_weapon(
                tor_simulation::ActorId(actor.0),
                &id,
                selected,
                selected_supplies,
            )
            .map_err(|_| {
                Failure::new(
                    ErrorCode::InvalidAction,
                    "Ability weapon preview is unavailable",
                )
            })?;
        let phase = match value.phase {
            tor_simulation::abilities::AbilityWeaponPreviewPhase::Fresh => {
                tor_protocol::AbilityWeaponPreviewPhase::Fresh
            }
            tor_simulation::abilities::AbilityWeaponPreviewPhase::Resuming => {
                tor_protocol::AbilityWeaponPreviewPhase::Resuming
            }
            tor_simulation::abilities::AbilityWeaponPreviewPhase::PaidPrefix => {
                tor_protocol::AbilityWeaponPreviewPhase::PaidPrefix
            }
        };
        Ok(tor_protocol::AbilityWeaponPreview {
            ability: ability.into(),
            invocation,
            supplies: supplies.clone(),
            phase,
            cost: value.cost.map(crate::creature_view::cost_bundle),
            preparation: value.preparation,
            recovery: value.recovery,
        })
    }

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

    #[cfg(test)]
    pub(crate) fn creature_catalog_query(
        &self,
        observer: ActorId,
        branch: &BranchId,
        expected_revision: u64,
        depth: Option<u32>,
        after: Option<&str>,
    ) -> Result<tor_protocol::CreatureCatalogPage, Failure> {
        if !self.wizard_enabled() {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard authority is required",
            ));
        }
        self.check_metadata(
            observer,
            branch,
            Some((
                expected_revision,
                "Refresh the observation before inspecting",
            )),
        )?;
        self.creature_catalog_query_at(&self.presentation_boundary(), observer, depth, after)
    }

    pub(crate) fn creature_catalog_query_at(
        &self,
        boundary: &super::PresentationBoundary,
        observer: ActorId,
        depth: Option<u32>,
        after: Option<&str>,
    ) -> Result<tor_protocol::CreatureCatalogPage, Failure> {
        self.require_wizard_authority()?;
        let game = self.presentation_game(boundary)?;
        let depth = depth.or_else(|| {
            self.regions.as_ref().and_then(|regions| {
                game.known_actor_region(tor_simulation::ActorId(observer.0))
                    .and_then(|at| regions.authored_depth(at.0))
            })
        });
        crate::wizard_creatures::catalog(game, self.regions.as_ref(), depth, after)
    }

    #[cfg(test)]
    pub(crate) fn inspect_creature_query(
        &self,
        observer: ActorId,
        branch: &BranchId,
        expected_revision: u64,
        target: ActorId,
    ) -> Result<tor_protocol::CreatureInspectionView, Failure> {
        if !self.wizard_enabled() {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard authority is required",
            ));
        }
        self.check_metadata(
            observer,
            branch,
            Some((
                expected_revision,
                "Refresh the observation before inspecting",
            )),
        )?;
        self.inspect_creature_query_at(&self.presentation_boundary(), target, expected_revision)
    }

    pub(crate) fn inspect_creature_query_at(
        &self,
        boundary: &super::PresentationBoundary,
        target: ActorId,
        display_revision: u64,
    ) -> Result<tor_protocol::CreatureInspectionView, Failure> {
        self.inspect_creature_at(boundary, target, display_revision)
    }

    /// Existing immutable run/package identities bind ephemeral inspection handles.
    /// No pointer or mutable cache identity grants authority.
    pub(crate) fn strike_catalog_identity(&self) -> (String, BranchId, String, Option<String>) {
        (
            self.archive.view_salt.clone(),
            self.archive.branch.clone(),
            self.archive.ruleset_hash.clone(),
            self.archive
                .scenario
                .package
                .as_ref()
                .map(|package| package.certificate.content_hash.clone()),
        )
    }

    pub(crate) fn strike_page_query_at(
        &self,
        boundary: &super::PresentationBoundary,
        display_revision: u64,
        (target, target_revision): (ActorId, u64),
        (source, start): (
            &tor_protocol::StrikeSourceSelector,
            Option<&tor_protocol::StrikePagePosition>,
        ),
        budget: &crate::creature_view::PageBudget<'_>,
    ) -> Result<crate::creature_view::ProjectedPage, Failure> {
        self.require_wizard_authority()?;
        if display_revision != target_revision {
            return Err(Failure::new(
                ErrorCode::StaleContext,
                "Creature changed; reopen its inspection",
            ));
        }
        crate::creature_view::strike_page(
            self.presentation_game(boundary)?,
            tor_simulation::ActorId(target.0),
            source,
            start,
            Some(budget),
        )
    }

    /// Runtime diagnostic controls share query authorization and metadata checks.
    pub(crate) fn combat_diagnostics_query_at(
        &mut self,
        boundary: &super::PresentationBoundary,
        operation: crate::wire_adapter::CombatDiagnosticsOperation,
    ) -> Result<tor_protocol::CombatDiagnosticsView, Failure> {
        self.require_wizard_authority()?;
        match operation {
            crate::wire_adapter::CombatDiagnosticsOperation::Inspect { through } => {
                // Capture is runtime tooling, not a journaled gameplay change.
                // A frozen picture therefore cannot own its current switch or
                // newly captured records. Validate the capability, then read
                // the same runtime owner used by Capture.
                self.presentation_game(boundary)?;
                self.inspect_combat_diagnostics(through)
            }
            crate::wire_adapter::CombatDiagnosticsOperation::Capture { enabled } => {
                self.configure_combat_diagnostics(enabled)
            }
        }
    }

    /// Fresh resolution follows request metadata checks and reads the cached native disclosure.
    pub fn resolve_command(
        &self,
        actor: ActorId,
        branch: &BranchId,
        command: DecodedCommand,
    ) -> Result<Command, Failure> {
        self.resolve_command_with_display(actor, branch, command, None)
    }

    /// Only Service may supply a display generation after capability validation.
    pub(crate) fn resolve_command_with_display(
        &self,
        actor: ActorId,
        branch: &BranchId,
        command: DecodedCommand,
        validated_display: Option<(&super::PresentationBoundary, u64)>,
    ) -> Result<Command, Failure> {
        if command.display_basis() != validated_display.map(|(_, revision)| revision) {
            return Err(Failure::new(
                ErrorCode::StaleContext,
                "Refresh the observation before acting",
            ));
        }
        if let Some((boundary, _)) = validated_display {
            if boundary.branch() != branch {
                return Err(Failure::new(
                    ErrorCode::StaleContext,
                    "Snapshot branch changed",
                ));
            }
            let game = self.presentation_game(boundary)?;
            let unavailable =
                || Failure::new(ErrorCode::InvalidAction, "Disclosed operand is unavailable");
            let operand_available = match &command {
                DecodedCommand::Display {
                    operation: DisplayCommand::RenamePlace { key, .. },
                    ..
                } => game
                    .remembered_places(tor_simulation::ActorId(actor.0))
                    .any(|(location, ..)| {
                        crate::adapt::cell_key(&self.archive.view_salt, actor.0, location) == *key
                    }),
                DecodedCommand::Display {
                    operation: DisplayCommand::Travel { destination },
                    ..
                } => game
                    .known_cells(tor_simulation::ActorId(actor.0))
                    .any(|location| {
                        crate::adapt::cell_key(&self.archive.view_salt, actor.0, location)
                            == *destination
                    }),
                _ => true,
            };
            if !operand_available {
                return Err(unavailable());
            }
        }
        let requirement = if validated_display.is_some() {
            None
        } else {
            command.revision_requirement()
        };
        let current_boundary = self.check_metadata(actor, branch, requirement)?;
        match command {
            DecodedCommand::Backend(command) => Ok(command),
            DecodedCommand::Display {
                expected_revision,
                operation,
            } => Ok(operation.resolve(expected_revision, current_boundary)),
            DecodedCommand::AbilityWeaponPreview { .. }
            | DecodedCommand::ProjectileChoices { .. }
            | DecodedCommand::Advancement { .. }
            | DecodedCommand::CreatureCatalog { .. }
            | DecodedCommand::CreatureInspection { .. }
            | DecodedCommand::StrikeInspection { .. }
            | DecodedCommand::CombatDiagnostics { .. } => Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Read-only queries cannot become journal commands",
            )),
            DecodedCommand::Gameplay { action } => Ok(Command::AdmitIntention {
                expected_boundary: current_boundary,
                action: self.decode_action(actor, &action)?,
            }),
            DecodedCommand::ResumeIntention { admission } => Ok(Command::ResumeIntention {
                expected_boundary: current_boundary,
                admission,
            }),
            DecodedCommand::CancelIntention { admission, step } => Ok(Command::CancelIntention {
                expected_boundary: current_boundary,
                admission,
                step: step.map(crate::adapt::requested_direction),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{journal::Action, Scenario};
    use tor_protocol::{Action as WireAction, Command as WireCommand};

    #[test]
    fn escape_resolves_only_observer_owned_live_hold_offers_without_actor_authority() {
        use tor_protocol as wire;
        use tor_simulation as sim;
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let actor = wire::ActorId(1);
        let scope = engine.target_scope(actor);
        let mut observation = engine.game.observe(sim::ActorId(1)).unwrap();
        let first = sim::HoldId(7);
        let second = sim::HoldId(8);
        observation.linked_holds = vec![
            sim::linked_hold::LinkedHoldObservation {
                id: first,
                holder: sim::ActorId(99),
            },
            sim::linked_hold::LinkedHoldObservation {
                id: second,
                holder: sim::ActorId(100),
            },
        ];
        let action = wire::Action::Escape {
            hold: scope.hold(second),
        };
        assert_eq!(
            crate::wire_adapter::decode_action(&action, &scope, &observation).unwrap(),
            Action::Escape { hold: second }
        );
        let absent = wire::Action::Escape {
            hold: scope.hold(sim::HoldId(9)),
        };
        let foreign = wire::Action::Escape {
            hold: engine.target_scope(wire::ActorId(2)).hold(second),
        };
        assert_eq!(
            crate::wire_adapter::decode_action(&absent, &scope, &observation).unwrap_err(),
            crate::wire_adapter::decode_action(&foreign, &scope, &observation).unwrap_err()
        );
        observation.linked_holds.clear();
        assert!(crate::wire_adapter::decode_action(&action, &scope, &observation).is_err());
        assert_eq!(
            crate::wire_adapter::encode_action(&Action::Escape { hold: second }, &scope),
            action,
            "historical receipt encoding survives current offer removal"
        );
        let actor_action = wire::Action::UseAbility {
            id: "basic_melee".into(),
            invocation: wire::AbilityInvocation {
                aim: wire::AbilityAim::ActorTarget {
                    target: scope.actor(sim::ActorId(99)),
                },
                weapon: wire::WeaponSelection::Default,
            },
            supplies: Default::default(),
        };
        assert!(crate::wire_adapter::decode_action(&actor_action, &scope, &observation).is_err());
    }

    #[test]
    fn linked_hold_projection_only_names_currently_visible_holders_without_rebinding() {
        use tor_simulation as sim;
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let scope = engine.target_scope(ActorId(1));
        let (mut observation, scene) = engine.game.observe_scene(sim::ActorId(1)).unwrap();
        observation.visible_actors.clear();
        observation.linked_holds = vec![sim::linked_hold::LinkedHoldObservation {
            id: sim::HoldId(7),
            holder: sim::ActorId(99),
        }];
        let project = |view| {
            crate::adapt::observation(
                view,
                &scene,
                "fixture",
                true,
                &|_| [None, None, None],
                &scope,
            )
        };
        let hidden = project(observation.clone());
        assert_eq!(hidden.linked_holds[0].holder, None);
        assert_eq!(
            serde_json::to_value(&hidden.linked_holds).unwrap(),
            serde_json::json!([{"target": scope.hold(sim::HoldId(7)), "holder": null}]),
            "hidden offers disclose only an opaque relationship token, never holder identity"
        );
        observation.visible_actors.push(sim::ActorView {
            presentation: None,
            disclosure: sim::perception::ActorDisclosure::Creature,
            id: sim::ActorId(99),
            name: "Ordinary disclosed holder".into(),
            location: observation.location,
            asset: None,
            worn: Vec::new(),
        });
        let visible = project(observation.clone());
        assert_eq!(
            visible.linked_holds[0].target,
            hidden.linked_holds[0].target
        );
        assert_eq!(
            visible.linked_holds[0].holder,
            Some(scope.actor(sim::ActorId(99)))
        );
        observation.visible_actors.clear();
        assert_eq!(project(observation).linked_holds, hidden.linked_holds);
    }

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
                display_basis: None,
                expected_boundary: engine.causal_boundary(actor).unwrap(),
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
                WizardResult::ActorSpawned { actor } => Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: tor_simulation::ActorId(actor.0),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
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
            Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::ActorTarget {
                        target: tor_simulation::ActorId(actor.0),
                    },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
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
            ErrorCode::InvalidAction
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
            let decoded = wire_adapter::decode_command(&WireCommand::Act { action }).unwrap();
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
                    display_basis: None,
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
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
        let revision = engine.causal_boundary(actor).unwrap();
        engine
            .command(
                "player",
                "test",
                actor,
                "accepted",
                &branch,
                Command::AdmitIntention {
                    expected_boundary: revision,
                    action: Action::Wait,
                },
            )
            .unwrap();
        let unavailable = wire_adapter::decode_command(&WireCommand::Act {
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

#[cfg(test)]
mod inspection_query_tests {
    use super::*;
    #[test]
    fn frozen_inspection_uses_issued_state_not_later_live_damage() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mob-arena");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
        engine.enable_wizard().unwrap();
        let actor = ActorId(1);
        let boundary = engine.presentation_boundary();
        let original = engine
            .inspect_creature_query_at(&boundary, actor, 37)
            .unwrap();
        engine
            .game
            .apply(tor_simulation::abilities::Outcome {
                actor: tor_simulation::ActorId(2),
                target: tor_simulation::ActorId(1),
                ability: None,
                intention: None,
                result: tor_simulation::abilities::OutcomeStatus::Applied,
                check: None,
                resistance: None,
                participation: tor_simulation::abilities::AttemptedParticipation::target(),
                target_rider_participated: false,
                effects: vec![tor_simulation::abilities::ResolvedEffect::Damage { amount: 1 }],
            })
            .unwrap();
        let live =
            crate::creature_view::inspect(&engine.game, tor_simulation::ActorId(1), 37).unwrap();
        assert_ne!(
            live, original,
            "fixture must actually change disclosed health"
        );
        let unchanged = engine.game.clone();
        assert_eq!(
            engine
                .inspect_creature_query_at(&boundary, actor, 37)
                .unwrap(),
            original
        );
        assert_eq!(
            engine.game, unchanged,
            "frozen inspection must remain read-only"
        );
        let foreign = Engine::memory(crate::Scenario::two_room(43))
            .unwrap()
            .presentation_boundary();
        assert_eq!(
            engine
                .inspect_creature_query_at(&foreign, actor, 37)
                .unwrap_err()
                .code,
            ErrorCode::Unauthorized
        );
    }

    #[test]
    fn query_inspection_checks_authority_before_metadata_and_target() {
        let engine = Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let result =
            engine.inspect_creature_query(ActorId(1), engine.branch(), u64::MAX, ActorId(999));
        assert_eq!(result.unwrap_err().code, ErrorCode::Unauthorized);
    }
}
