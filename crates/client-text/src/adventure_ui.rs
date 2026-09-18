use std::{
    collections::BTreeSet,
    io::{self, BufRead, Write},
    time::Duration,
};
use tokio::{sync::mpsc, time::timeout};
use tor_client_common::Connection;
use tor_client_text::{
    adventure::{self, Dialogue, Intent},
    history, safe, Input,
};
use tor_protocol::*;

use super::Error;

struct Journey {
    branch: BranchId,
    receipt: Option<EntryId>,
    item: Option<u64>,
    label: String,
    direction: Option<Direction>,
    hazards: BTreeSet<ActorId>,
    cancelled: bool,
}

impl Journey {
    fn walking(&self) -> String {
        self.direction.map_or_else(
            || format!("toward {}", self.label),
            |d| adventure::direction_name(d).into(),
        )
    }
    fn interrupted(&self, reason: &str) -> String {
        format!("You start walking {}, but {}", self.walking(), reason)
    }
}

#[derive(Default)]
struct Session {
    dialogue: Dialogue,
    journey: Option<Journey>,
    summarizing_pickup: bool,
    quiet_control: bool,
    epoch: u64,
}

fn hazards(state: &StateView) -> BTreeSet<ActorId> {
    state
        .observation
        .visible_actors
        .iter()
        .map(|a| a.id)
        .filter(|id| *id != state.observation.actor)
        .collect()
}

pub async fn run(mut connection: Connection, observe: bool) -> Result<(), Error> {
    let mut session = Session {
        quiet_control: true,
        ..Session::default()
    };
    println!("Thresholds of Ruin\nType help for things you can try.\n");
    if connection.role() == AccessRole::Spectator {
        println!("Spectator access is read-only.");
    } else if !observe {
        transact(&mut connection, &mut session, Request::AcquireControl).await?;
    }
    session.quiet_control = false;
    println!("{}", adventure::describe(connection.state.state()));
    prompt()?;
    let (tx, mut rx) = mpsc::channel(16);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let failed = line.is_err();
            if tx.blocking_send(line).is_err() || failed {
                break;
            }
        }
    });
    loop {
        tokio::select! {
            message = connection.next() => {
                let message = message?;
                let terminal = present(&connection, &mut session, &message);
                finish_journey(&mut connection, &mut session).await?;
                if terminal && session.journey.is_none() { prompt()?; } else { io::stdout().flush()?; }
            }
            line = rx.recv() => {
                let Some(line) = line else { break; };
                let line = line?;
                if line.trim().is_empty() { if session.journey.is_none() { prompt()?; } continue; }
                let intent = session.dialogue.interpret(&line, connection.state.state());
                if matches!(intent, Intent::Tools(Input::Quit)) { break; }
                dispatch(&mut connection, &mut session, intent).await?;
                finish_journey(&mut connection, &mut session).await?;
                if session.journey.is_none() { prompt()?; } else { io::stdout().flush()?; }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    connection.close().await;
    println!("Goodbye.");
    Ok(())
}

fn prompt() -> io::Result<()> {
    print!("> ");
    io::stdout().flush()
}

fn can_act(connection: &Connection) -> bool {
    if connection.role() == AccessRole::Spectator {
        println!("Spectator access is read-only.");
        false
    } else if !connection.state.has_control() {
        println!("You are observing. Use control to take over when it is available.");
        false
    } else {
        true
    }
}

async fn stop(connection: &mut Connection, session: &mut Session) -> Result<(), Error> {
    // Discard the follow-up before waiting for cancellation: arrival can race
    // with the request, but the player's stop must still prevent pickup.
    if let Some(journey) = &mut session.journey {
        journey.cancelled = true;
    }
    if let Some(status) = connection
        .state
        .travel()
        .filter(|t| t.phase == TravelPhase::Active)
    {
        let request = Request::CancelTravel {
            branch: connection.state.branch().clone(),
            travel_id: status.id.clone(),
        };
        transact(connection, session, request).await?;
    }
    finish_journey(connection, session).await?;
    Ok(())
}

async fn dispatch(
    connection: &mut Connection,
    session: &mut Session,
    intent: Intent,
) -> Result<(), Error> {
    match intent {
        Intent::Look => println!("{}", adventure::describe(connection.state.state())),
        Intent::Say(text) => println!("{text}"),
        Intent::Stop => {
            if can_act(connection) {
                let moving = session.journey.is_some();
                stop(connection, session).await?;
                if !moving {
                    println!("You are not walking anywhere.");
                }
            }
        }
        Intent::Action(action) => {
            if can_act(connection) {
                stop(connection, session).await?;
                act(connection, session, action).await?;
            }
        }
        Intent::Travel {
            destination,
            take,
            label,
            direction,
        } => {
            if can_act(connection) {
                stop(connection, session).await?;
                let branch = connection.state.branch().clone();
                let epoch = session.epoch;
                let hazards = hazards(connection.state.state());
                session.journey = Some(Journey {
                    branch: branch.clone(),
                    receipt: None,
                    item: take,
                    label,
                    direction,
                    hazards,
                    cancelled: false,
                });
                let request = Request::Command {
                    branch: branch.clone(),
                    command: Command::Travel {
                        expected_revision: connection.state.state().revision,
                        destination,
                    },
                };
                match transact(connection, session, request).await?.flatten() {
                    Some(id) if session.epoch == epoch && connection.state.branch() == &branch => {
                        if let Some(journey) = &mut session.journey {
                            journey.receipt = Some(id);
                        }
                    }
                    _ => session.journey = None,
                }
            }
        }
        Intent::Tools(input) => {
            let request = match input {
                Input::Request(request) => request,
                Input::Command(command) => Request::Command {
                    branch: connection.state.branch().clone(),
                    command,
                },
                Input::Look => {
                    println!("{}", adventure::describe(connection.state.state()));
                    return Ok(());
                }
                Input::Inventory => {
                    println!("{}", adventure::inventory(connection.state.state()));
                    return Ok(());
                }
                Input::Help => {
                    println!("{}", adventure::HELP);
                    return Ok(());
                }
                Input::Quit => return Ok(()),
            };
            if !connection.role().permits(&request) {
                println!("Spectator access is read-only.");
            } else {
                if matches!(
                    request,
                    Request::ReleaseControl
                        | Request::Command {
                            command: Command::Wizard { .. },
                            ..
                        }
                ) {
                    if let Some(journey) = &mut session.journey {
                        journey.cancelled = true;
                    }
                }
                transact(connection, session, request).await?;
            }
        }
    }
    Ok(())
}

async fn act(
    connection: &mut Connection,
    session: &mut Session,
    action: Action,
) -> Result<bool, Error> {
    let request = Request::Command {
        branch: connection.state.branch().clone(),
        command: Command::Act {
            expected_revision: connection.state.state().revision,
            action,
        },
    };
    Ok(transact(connection, session, request).await?.is_some())
}

async fn finish_journey(connection: &mut Connection, session: &mut Session) -> Result<(), Error> {
    let Some(journey) = session.journey.as_ref() else {
        return Ok(());
    };
    let Some(status) = connection.state.travel() else {
        return Ok(());
    };
    if journey.receipt.as_ref() != Some(&status.id) {
        return Ok(());
    }
    if status.phase == TravelPhase::Active {
        return Ok(());
    }
    let phase = status.phase;
    let journey = session.journey.take().expect("active intention");
    if journey.cancelled {
        println!("{}", journey.interrupted("stop before going any farther."));
        return Ok(());
    }
    if phase != TravelPhase::Arrived {
        println!(
            "{}",
            journey.interrupted(&interruption(
                phase,
                connection.state.state(),
                &journey.hazards
            ))
        );
        return Ok(());
    }
    let state = connection.state.state();
    // Arrival wins over hazards in backend travel, but never authorizes another
    // automatic action. Keep this check even while combining the narration.
    if !hazards(state).is_subset(&journey.hazards) {
        println!(
            "{}",
            journey.interrupted(&interruption(TravelPhase::Hazard, state, &journey.hazards))
        );
    } else if let Some(item) = journey.item {
        if journey.branch != *connection.state.branch() || !connection.state.has_control() {
            println!("You stop before picking anything up.");
        } else if !state.observation.ready {
            println!(
                "{}",
                journey.interrupted("must wait before picking anything up.")
            );
        } else if state
            .observation
            .ground_items
            .iter()
            .any(|i| i.item.id == item && i.reachable)
        {
            session.summarizing_pickup = true;
            let accepted = act(connection, session, Action::Take { item }).await?;
            session.summarizing_pickup = false;
            if accepted {
                println!("You walk over to {} and pick it up.", journey.label);
            }
        } else {
            println!(
                "You walk over to {}, but it is no longer within reach.",
                journey.label
            );
        }
    } else {
        println!("You walk {}.", journey.walking());
        if journey.direction.is_some() {
            println!("{}", adventure::describe(connection.state.state()));
        }
    }
    Ok(())
}

fn interruption(phase: TravelPhase, state: &StateView, previous: &BTreeSet<ActorId>) -> String {
    match phase {
        TravelPhase::Hazard => {
            let name = state
                .observation
                .visible_actors
                .iter()
                .find(|a| a.id != state.observation.actor && !previous.contains(&a.id))
                .map(|a| a.name.as_str())
                .filter(|n| !n.is_empty())
                .unwrap_or("figure");
            let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            format!("stop when {article} {} comes into view.", safe(name))
        }
        TravelPhase::Cancelled => "stop before going any farther.".into(),
        TravelPhase::Blocked => "find the way blocked and stop.".into(),
        TravelPhase::DecisionRequired => "have to stop and wait.".into(),
        TravelPhase::ControlLost => "stop as control changes.".into(),
        TravelPhase::WorldChanged => "stop as your surroundings change.".into(),
        _ => "cannot continue.".into(),
    }
}

/// The inner option is an optional durable receipt; the outer option is success.
async fn transact(
    connection: &mut Connection,
    session: &mut Session,
    request: Request,
) -> Result<Option<Option<EntryId>>, Error> {
    let invalid = match &request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => "You can't find a way there.",
        Request::Command {
            command:
                Command::Act {
                    action: Action::Take { .. },
                    ..
                },
            ..
        } => "You can't pick that up from here.",
        Request::Command {
            command:
                Command::Act {
                    action: Action::Move { .. },
                    ..
                },
            ..
        } => "You can't go that way.",
        _ => "You can't do that here.",
    };
    let id = connection.request(request).await?;
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            present(connection, session, &message);
            match message {
                ServerMessage::Ack { request_id, entry_id } if request_id == id => return Ok(Some(entry_id)),
                ServerMessage::Snapshot { request_id, .. } | ServerMessage::History { request_id, .. } if request_id == id => return Ok(Some(None)),
                ServerMessage::Error {request_id, code, ..} if request_id.as_ref() == Some(&id) => {
                    println!("{}", match code {
                        ErrorCode::InvalidAction => invalid,
                        ErrorCode::NotController | ErrorCode::ControlTaken => "Another player has control. You are observing.",
                        ErrorCode::Unauthorized => "You do not have permission to do that.",
                        ErrorCode::StaleRevision | ErrorCode::WrongBranch => "Things have changed. Look around and try again.",
                        ErrorCode::StorageFailure => "The game could not save that action. It was not completed.",
                        _ => "That request could not be completed.",
                    });
                    return Ok(None);
                },
                _ => {},
            }
        }
    }).await.map_err(|_| "The server did not answer. The action may have completed; reconnect and check history before trying again.")?
}

fn present(connection: &Connection, session: &mut Session, message: &ServerMessage) -> bool {
    match message {
        ServerMessage::Update { update } => match &update.body {
            UpdateBody::Observation { event, .. } => {
                if session.journey.is_none()
                    && !session.summarizing_pickup
                    && connection
                        .state
                        .travel()
                        .is_none_or(|t| t.phase != TravelPhase::Active)
                {
                    if let Some(entry) = event {
                        println!("{}", adventure::event(entry, connection.state.state()));
                    } else {
                        println!("{}", adventure::describe(connection.state.state()));
                    }
                }
            }
            UpdateBody::Travel { status, .. } if status.phase != TravelPhase::Active => {
                if session.journey.is_none() {
                    if status.phase == TravelPhase::Arrived {
                        println!("You finish walking.");
                        println!("{}", adventure::describe(connection.state.state()));
                    } else {
                        println!(
                            "You {}",
                            interruption(status.phase, connection.state.state(), &BTreeSet::new())
                        );
                    }
                }
                return true;
            }
            UpdateBody::Travel { .. } => {}
            UpdateBody::Annotation { entry } => {
                println!("{}", adventure::event(entry, connection.state.state()))
            }
            UpdateBody::Control { has_control } => {
                if !has_control {
                    session.epoch += 1;
                }
                if !session.quiet_control {
                    println!(
                        "{}",
                        if *has_control {
                            "You are in control."
                        } else {
                            "You are now observing."
                        }
                    );
                }
            }
        },
        ServerMessage::Snapshot { .. } => {
            if let Some(journey) = session.journey.take() {
                println!(
                    "{}",
                    journey.interrupted("stop as your surroundings change.")
                );
            }
            session.epoch += 1;
            session.dialogue.reset();
            println!("{}", adventure::describe(connection.state.state()));
            return true;
        }
        ServerMessage::History { page, .. } => {
            // Detailed anchors are available only through this explicit session tool.
            if page.entries.is_empty() {
                println!("There is no history to show.");
            }
            for entry in &page.entries {
                println!("{}", history(entry));
            }
            if let Some(before) = &page.older_before {
                println!("Older entries: history {}", safe(&before.0));
            }
        }
        ServerMessage::Error { .. } | ServerMessage::Ack { .. } | ServerMessage::Welcome { .. } => {
        }
    }
    false
}
