//! Translate player input using the shared client's current admission authority.
use tor_client_common::{
    items::{item_action, ItemOperation},
    ClientState,
};
use tor_protocol::*;

/// What the window asks of the worker.
pub enum Cmd {
    Move(Direction),
    Attack(ActorTarget),
    Wait,
    Control,
    Take(ItemTarget),
    Drop(ItemTarget),
    Use(ItemTarget, ItemOperation),
    Door(DoorTarget, bool),
    /// Journey to a known cell, named by its key.
    Travel(String),
}

/// The immutable disclosure boundary used to interpret this input.
pub struct Input {
    pub shown: ReplyContext,
    pub command: Cmd,
}

pub fn request_from(
    state: &ClientState,
    shown: &ReplyContext,
    command: Cmd,
) -> Result<Request, String> {
    state
        .validate_reply_context(shown)
        .map_err(|_| "State changed; request was not sent. Review the new state.".to_string())?;
    request(state, command)
}

/// The request a command makes from the state as it stands, as the ASCII
/// client builds it: a step cancels a finished strike sequence, a step into a
/// hostile creature attacks it, and waiting mid-combat continues.
pub fn request(state: &ClientState, command: Cmd) -> Result<Request, String> {
    if matches!(command, Cmd::Control) {
        return Ok(Request::AcquireControl);
    }
    if !state.has_control() {
        return Err("You are observing. Press Tab to take control.".into());
    }
    if let Cmd::Move(direction) = command {
        if let Some(request) = state.cancel_intention_with_step_request(direction) {
            return Ok(request);
        }
    }
    let view = &state.state().observation;
    if !state.can_admit_intention() {
        if !view.ready
            && matches!(command, Cmd::Wait)
            && view.combat.as_ref().is_some_and(|c| !c.terminal)
        {
            return Ok(Request::Continue);
        }
        return Err("You can't act right now.".into());
    }
    let action = match command {
        Cmd::Wait => Action::Wait,
        Cmd::Attack(target) => tor_client_common::abilities::basic_action(view, target)?,
        Cmd::Take(item) => Action::Take {
            item,
            quantity: None,
        },
        Cmd::Drop(item) => Action::Drop {
            item,
            quantity: None,
        },
        Cmd::Use(item, operation) => item_action(view, item, operation)?,
        Cmd::Door(door, open) => Action::SetDoor { door, open },
        Cmd::Travel(destination) => {
            return Ok(state.command_request(Command::Travel {
                expected_revision: state.state().revision,
                destination,
            }))
        }
        Cmd::Move(direction) => {
            let (dx, dy, dz) = offset(direction);
            let target = Position {
                x: view.position.x + dx,
                y: view.position.y + dy,
                z: view.position.z + dz,
            };
            let hostile = view.visible_actors.iter().find(|a| {
                a.id != view.self_target
                    && a.position == target
                    && view
                        .combat
                        .as_ref()
                        .is_some_and(|c| c.actors.iter().any(|o| o.actor == a.id && o.hostile))
            });
            match hostile {
                Some(foe) => tor_client_common::abilities::basic_action(view, foe.id)?,
                None => Action::Move { direction },
            }
        }
        Cmd::Control => unreachable!("handled before acting"),
    };
    Ok(state.command_request(Command::Act {
        expected_revision: state.state().revision,
        action,
    }))
}

pub fn offset(direction: Direction) -> (i32, i32, i32) {
    match direction {
        Direction::North => (0, -1, 0),
        Direction::South => (0, 1, 0),
        Direction::East => (1, 0, 0),
        Direction::West => (-1, 0, 0),
        Direction::NorthEast => (1, -1, 0),
        Direction::SouthEast => (1, 1, 0),
        Direction::SouthWest => (-1, 1, 0),
        Direction::NorthWest => (-1, -1, 0),
        Direction::Up => (0, 0, 1),
        Direction::Down => (0, 0, -1),
    }
}
