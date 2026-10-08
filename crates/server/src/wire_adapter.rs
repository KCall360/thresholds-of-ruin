//! Explicit transport-to-backend command boundary.
//! Decode preserves requested facts; it grants no authority and resolves no
//! live targets. The session checks authority and durable receipts before fresh
//! input validation. Fresh resolution produces admission; execution stays simulation-owned.
//! Journal command/action types do not implement conversion methods or developer parsing.
use crate::actions as a;
use crate::journal::Command;
use tor_protocol::{self as p, EntryId};
use tor_simulation as s;
#[path = "wire_targets.rs"]
mod targets;
pub use targets::TargetScope;

/// Resolve only entities present in the observer's immutable disclosure.
pub fn decode_action(
    action: &p::Action,
    scope: &TargetScope,
    observation: &s::Observation,
) -> Result<a::Action, crate::Failure> {
    let unavailable = || crate::Failure::new(p::ErrorCode::InvalidAction, "Target is unavailable");
    if scope.observer() != observation.actor {
        return Err(unavailable());
    }
    Ok(match action {
        p::Action::Attack { target } => {
            let actor = std::iter::once(observation.actor)
                .chain(observation.visible_actors.iter().map(|actor| actor.id))
                .find(|actor| scope.actor(*actor) == *target)
                .ok_or_else(unavailable)?;
            a::Action::Attack { target: actor }
        }
        p::Action::SetDoor { door, open } => {
            let identity = observation
                .visible_cells
                .iter()
                .filter_map(|cell| cell.door.as_ref())
                .find(|candidate| scope.door(candidate.id) == *door)
                .ok_or_else(unavailable)?
                .id;
            a::Action::SetDoor {
                door: identity,
                open: *open,
            }
        }
        p::Action::Move { direction } => a::Action::Move {
            direction: crate::adapt::requested_direction(*direction),
        },
        p::Action::Take { item, quantity } => {
            let identity = observation
                .ground_items
                .iter()
                .find(|candidate| scope.item(candidate.id) == *item)
                .ok_or_else(unavailable)?
                .id
                .0;
            a::Action::Take {
                item: identity,
                quantity: *quantity,
            }
        }
        p::Action::Drop { item, quantity } => {
            let identity = observation
                .inventory
                .iter()
                .find(|candidate| scope.item(candidate.id) == *item)
                .ok_or_else(unavailable)?
                .id
                .0;
            a::Action::Drop {
                item: identity,
                quantity: *quantity,
            }
        }
        p::Action::Equip { item, .. } | p::Action::Unequip { item } | p::Action::Drink { item } => {
            let identity = observation
                .inventory
                .iter()
                .find(|candidate| scope.item(candidate.id) == *item)
                .ok_or_else(unavailable)?
                .id
                .0;
            match action {
                p::Action::Equip { slot, .. } => a::Action::Equip {
                    item: identity,
                    slot: *slot,
                },
                p::Action::Unequip { .. } => a::Action::Unequip { item: identity },
                p::Action::Drink { .. } => a::Action::Drink { item: identity },
                _ => unreachable!(),
            }
        }
        p::Action::Wait => a::Action::Wait,
    })
}

/// Encode backend action facts explicitly for the current wire schema.
pub fn encode_action(action: &a::Action, scope: &TargetScope) -> p::Action {
    match action {
        a::Action::Attack { target } => p::Action::Attack {
            target: scope.actor(*target),
        },
        a::Action::SetDoor { door, open } => p::Action::SetDoor {
            door: scope.door(*door),
            open: *open,
        },
        a::Action::Move { direction } => p::Action::Move {
            direction: crate::adapt::wire_direction(*direction),
        },
        a::Action::Take { item, quantity } => p::Action::Take {
            item: scope.item(s::ItemId(*item)),
            quantity: *quantity,
        },
        a::Action::Drop { item, quantity } => p::Action::Drop {
            item: scope.item(s::ItemId(*item)),
            quantity: *quantity,
        },
        a::Action::Equip { item, slot } => p::Action::Equip {
            item: scope.item(s::ItemId(*item)),
            slot: *slot,
        },
        a::Action::Unequip { item } => p::Action::Unequip {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Drink { item } => p::Action::Drink {
            item: scope.item(s::ItemId(*item)),
        },
        a::Action::Wait => p::Action::Wait,
    }
}

/// Normalized request facts before any live target resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedCommand {
    Backend(Command),
    Gameplay {
        expected_revision: u64,
        action: p::Action,
    },
}

impl DecodedCommand {
    pub(crate) fn requires_control(&self) -> bool {
        match self {
            Self::Gameplay { .. } => true,
            Self::Backend(command) => matches!(
                command,
                Command::RenamePlace { .. }
                    | Command::ResumeIntention { .. }
                    | Command::CancelIntention { .. }
                    | Command::Act { .. }
                    | Command::AdmitIntention { .. }
                    | Command::Travel { .. }
            ),
        }
    }
    /// Compare with the original saved request without consulting live targets.
    pub fn matches(&self, command: &Command, scope: &TargetScope) -> bool {
        match (self, command) {
            (Self::Backend(request), original) => request == original,
            (
                Self::Gameplay {
                    expected_revision,
                    action,
                },
                Command::AdmitIntention {
                    expected_revision: original_revision,
                    action: original,
                },
            ) => {
                expected_revision == original_revision && *action == encode_action(original, scope)
            }
            _ => false,
        }
    }

    pub(crate) fn revision_requirement(&self) -> Option<(u64, &'static str)> {
        match self {
            Self::Backend(command) => command.revision_requirement(),
            Self::Gameplay {
                expected_revision, ..
            } => Some((*expected_revision, "Refresh the observation before acting")),
        }
    }
}

/// Normalize transport input; preserve gameplay references until fresh resolution.
pub fn decode_command(command: &tor_protocol::Command) -> Result<DecodedCommand, crate::Failure> {
    // Keep the transport and journal schemas independent: extending either
    // enum must require an explicit decision at this boundary.
    if let p::Command::Act {
        expected_revision,
        action,
    } = command
    {
        return Ok(DecodedCommand::Gameplay {
            expected_revision: *expected_revision,
            action: action.clone(),
        });
    }
    Ok(DecodedCommand::Backend(match command {
        tor_protocol::Command::ResumeIntention {
            expected_revision,
            intention,
        } => Command::ResumeIntention {
            expected_revision: *expected_revision,
            admission: EntryId(intention.0.clone()),
        },
        tor_protocol::Command::CancelIntention {
            expected_revision,
            intention,
        } => Command::CancelIntention {
            expected_revision: *expected_revision,
            admission: EntryId(intention.0.clone()),
        },
        tor_protocol::Command::RenamePlace {
            expected_revision,
            key,
            name,
        } => Command::RenamePlace {
            expected_revision: *expected_revision,
            key: key.clone(),
            name: name.clone(),
        },
        tor_protocol::Command::Travel {
            expected_revision,
            destination,
        } => Command::Travel {
            expected_revision: *expected_revision,
            destination: destination.clone(),
        },
        tor_protocol::Command::Wizard {
            expected_revision,
            operation,
        } => Command::Wizard {
            expected_revision: *expected_revision,
            operation: crate::developer::parse_wizard(operation).map_err(|_| {
                crate::Failure::new(
                    tor_protocol::ErrorCode::InvalidRequest,
                    "Invalid developer command",
                )
            })?,
        },
        p::Command::Act { .. } => unreachable!("gameplay decoded above"),
        tor_protocol::Command::Annotate {
            anchor,
            text,
            source,
            audience,
            category,
        } => Command::Annotate {
            anchor: anchor.clone(),
            text: text.clone(),
            source: *source,
            audience: *audience,
            category: *category,
        },
    }))
}

/// Encode supported backend requests and refuse operations with no wire authority.
pub fn encode_command(
    command: Command,
    scope: &TargetScope,
) -> Result<tor_protocol::Command, &'static str> {
    match command {
        Command::ResumeIntention {
            expected_revision,
            admission,
        } => Ok(tor_protocol::Command::ResumeIntention {
            expected_revision,
            intention: tor_protocol::IntentionId(admission.0),
        }),
        Command::CancelIntention {
            expected_revision,
            admission,
        } => Ok(tor_protocol::Command::CancelIntention {
            expected_revision,
            intention: tor_protocol::IntentionId(admission.0),
        }),
        Command::PausePreparation => Err("Preparation suspension is backend-only"),
        Command::Wizard {
            expected_revision,
            operation,
        } => Ok(tor_protocol::Command::Wizard {
            expected_revision,
            operation: serde_json::to_string(&operation).expect("developer command serializes"),
        }),
        Command::RenamePlace {
            expected_revision,
            key,
            name,
        } => Ok(tor_protocol::Command::RenamePlace {
            expected_revision,
            key,
            name,
        }),
        Command::Travel {
            expected_revision,
            destination,
        } => Ok(tor_protocol::Command::Travel {
            expected_revision,
            destination,
        }),
        Command::AdmitIntention {
            expected_revision,
            action,
        }
        | Command::Act {
            expected_revision,
            action,
        } => Ok(tor_protocol::Command::Act {
            expected_revision,
            action: encode_action(&action, scope),
        }),
        Command::Annotate {
            anchor,
            text,
            source,
            audience,
            category,
        } => Ok(tor_protocol::Command::Annotate {
            anchor,
            text,
            source,
            audience,
            category,
        }),
    }
}

/// Only this projection crosses the network; journal topology stays private.
pub fn disclose_entry(
    entry: &crate::journal::JournalEntry,
    scope: &TargetScope,
) -> Option<tor_protocol::HistoryEntry> {
    use crate::journal::{Action, Event, JournalContent, WizardResult};
    use tor_protocol::{Event as VisibleEvent, HistoryContent as Content};
    let content = match &entry.content {
        JournalContent::IntentionAdmitted { .. }
        | JournalContent::AutonomousIntentionAdmitted { .. }
        | JournalContent::TravelIntentionAdmitted { .. }
        | JournalContent::IntentionFailed { .. }
        | JournalContent::IntentionContinuationFailed { .. }
        | JournalContent::IntentionChanged { .. } => return None,
        JournalContent::PlaceRenamed { key, name } => Content::PlaceRenamed {
            key: key.clone(),
            name: name.clone(),
        },
        JournalContent::Travel { destination } => Content::Travel {
            destination: destination.clone(),
        },
        JournalContent::Wizard { result, .. } => Content::Wizard {
            summary: match result {
                WizardResult::Rewound { .. } => "Timeline rewound.",
                _ => "Developer setup completed.",
            }
            .into(),
            rewind: matches!(result, WizardResult::Rewound { .. }),
        },
        JournalContent::Action { action, event }
        | JournalContent::IntentionStarted { action, event, .. }
        | JournalContent::IntentionContinued { action, event, .. } => Content::Action {
            action: encode_action(action, scope),
            event: match event {
                Event::ItemStarted { action } => VisibleEvent::ItemStarted {
                    action: encode_action(action, scope),
                },
                Event::PreparationPaused => VisibleEvent::PreparationPaused,
                Event::AttackStarted { target } => VisibleEvent::AttackStarted {
                    target: scope.actor(s::ActorId(target.0)),
                },
                Event::DoorChanged { door, open } => VisibleEvent::DoorChanged {
                    door: scope.door(*door),
                    open: *open,
                },
                Event::Moved { .. } => VisibleEvent::Moved {
                    direction: match action {
                        Action::Move { direction } => crate::adapt::wire_direction(*direction),
                        _ => unreachable!("movement has a move action"),
                    },
                },
                Event::Taken {
                    item,
                    result,
                    quantity,
                } => VisibleEvent::Taken {
                    item: scope.item(s::ItemId(*item)),
                    result: scope.item(s::ItemId(*result)),
                    quantity: *quantity,
                },
                Event::Dropped {
                    item,
                    result,
                    quantity,
                } => VisibleEvent::Dropped {
                    item: scope.item(s::ItemId(*item)),
                    result: scope.item(s::ItemId(*result)),
                    quantity: *quantity,
                },
                Event::Waited => VisibleEvent::Waited,
            },
        },
        JournalContent::Annotation {
            anchor,
            category,
            text,
        } => Content::Annotation {
            anchor: anchor.clone(),
            category: *category,
            text: text.clone(),
        },
    };
    Some(tor_protocol::HistoryEntry {
        id: entry.id.clone(),
        branch: entry.branch.clone(),
        actor: entry.actor,
        tick: entry.tick,
        author: entry.author.clone(),
        audience: entry.audience,
        content,
    })
}
