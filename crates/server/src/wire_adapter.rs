//! Explicit transport-to-backend command boundary.
//! Decode preserves requested facts; it grants no authority and resolves no
//! live targets. The session checks authority and durable receipts before fresh
//! input validation. Gameplay decodes to admission; execution stays simulation-owned.
//! Journal command/action types do not implement conversion methods or developer parsing.
use crate::actions as a;
use crate::journal::Command;
use tor_protocol::{self as p, EntryId};
use tor_simulation as s;

/// Preserve the requested target and quantity without resolving or authorizing them.
pub fn decode_action(action: &p::Action) -> a::Action {
    match action {
        p::Action::Attack { target } => a::Action::Attack {
            target: s::ActorId(target.0),
        },
        p::Action::SetDoor { door, open } => a::Action::SetDoor {
            door: *door,
            open: *open,
        },
        p::Action::Move { direction } => a::Action::Move {
            direction: crate::adapt::requested_direction(*direction),
        },
        p::Action::Take { item, quantity } => a::Action::Take {
            item: *item,
            quantity: *quantity,
        },
        p::Action::Drop { item, quantity } => a::Action::Drop {
            item: *item,
            quantity: *quantity,
        },
        p::Action::Wait => a::Action::Wait,
    }
}

/// Encode backend action facts explicitly for the current wire schema.
pub fn encode_action(action: &a::Action) -> p::Action {
    match action {
        a::Action::Attack { target } => p::Action::Attack {
            target: p::ActorId(target.0),
        },
        a::Action::SetDoor { door, open } => p::Action::SetDoor {
            door: *door,
            open: *open,
        },
        a::Action::Move { direction } => p::Action::Move {
            direction: crate::adapt::wire_direction(*direction),
        },
        a::Action::Take { item, quantity } => p::Action::Take {
            item: *item,
            quantity: *quantity,
        },
        a::Action::Drop { item, quantity } => p::Action::Drop {
            item: *item,
            quantity: *quantity,
        },
        a::Action::Wait => p::Action::Wait,
    }
}

/// Normalize transport input into backend receipt facts; gameplay admits work.
pub fn decode_command(command: &tor_protocol::Command) -> Result<Command, crate::Failure> {
    // Keep the transport and journal schemas independent: extending either
    // enum must require an explicit decision at this boundary.
    Ok(match command {
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
        tor_protocol::Command::Act {
            expected_revision,
            action,
        } => Command::AdmitIntention {
            expected_revision: *expected_revision,
            action: decode_action(action),
        },
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
    })
}

/// Encode supported backend requests and refuse operations with no wire authority.
pub fn encode_command(command: Command) -> Result<tor_protocol::Command, &'static str> {
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
            action: encode_action(&action),
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
