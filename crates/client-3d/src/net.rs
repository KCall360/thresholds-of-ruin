//! The network worker: one thread owns the connection and the shared client
//! state, and hands the window plain views of what the character knows.
use std::{
    net::SocketAddr,
    sync::mpsc::{self, Receiver, Sender},
    thread::JoinHandle,
    time::Duration,
};
use tokio::{sync::mpsc as async_mpsc, time::timeout};
use tor_client_common::{
    items::{item_action, ItemOperation},
    ClientState, Connection, PendingRequest, RequestCompletion,
};
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// What the window asks of the worker.
pub enum Cmd {
    Move(Direction),
    Wait,
    Control,
    Take(ItemTarget),
    Drop(ItemTarget),
    Use(ItemTarget, ItemOperation),
    Door(DoorTarget, bool),
    /// Journey to a known cell, named by its key.
    Travel(String),
}

/// One creature in view.
#[derive(Clone)]
pub struct Actor {
    pub id: ActorTarget,
    pub name: String,
    pub description: String,
    pub position: Position,
    pub hostile: bool,
    pub is_self: bool,
}

/// Everything the window draws, in the observer frame of this moment.
pub struct View {
    pub feet: Position,
    pub visible: Vec<CellView>,
    /// Remembered cells aligned to this frame that are not currently seen.
    pub memory: Vec<CellView>,
    pub actors: Vec<Actor>,
    pub items: Vec<GroundItem>,
    pub inventory: Vec<Carried>,
    pub narration: Vec<String>,
    pub hp: Option<(u64, u64)>,
    pub has_control: bool,
    /// Whether a new action would be admitted now (no journey or work under way).
    pub can_act: bool,
    pub tick: u64,
    /// Changes whenever the server's state does; events belong to one revision.
    pub revision: u64,
    /// What the action this view follows did.
    pub events: Vec<Happening>,
}

/// Something seen to happen, from the combat events the server disclosed.
#[derive(Clone, Debug)]
pub enum Happening {
    Ability {
        caster: Option<ActorTarget>,
        target: Option<ActorTarget>,
        name: String,
        missed: bool,
        /// Ordinal of the blow within a strike sequence.
        strike: Option<u16>,
    },
    Died(ActorTarget),
}

/// An item lying in view.
#[derive(Clone)]
pub struct GroundItem {
    pub id: ItemTarget,
    pub name: String,
    pub class: ItemClass,
    pub quantity: u64,
    pub description: String,
    pub position: Position,
    pub reachable: bool,
}

/// An item the character carries, with what can be done with it now.
#[derive(Clone)]
pub struct Carried {
    pub id: ItemTarget,
    pub name: String,
    pub class: ItemClass,
    pub quantity: u64,
    pub description: String,
    /// How it is worn or held, when equipped.
    pub equipped: Option<String>,
    pub can_equip: bool,
    pub drinkable: bool,
    pub edible: bool,
}

pub enum Event {
    View(Box<View>),
    Status(String),
    /// The worker can take another command.
    Ready,
    Fatal(String),
}

pub struct Network {
    pub commands: async_mpsc::UnboundedSender<Cmd>,
    pub events: Receiver<Event>,
    worker: Option<JoinHandle<()>>,
}

impl Network {
    pub fn start(address: SocketAddr, token: String, actor: ActorId, observe: bool) -> Self {
        let (commands, rx) = async_mpsc::unbounded_channel();
        let (tx, events) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(Error::from)
                .and_then(|rt| rt.block_on(run(address, token, actor, observe, rx, &tx)));
            if let Err(error) = result {
                let _ = tx.send(Event::Fatal(format!("Connection ended: {error}")));
            }
        });
        Self {
            commands,
            events,
            worker: Some(worker),
        }
    }

    /// Close the connection cleanly so the server keeps what was played.
    pub fn shutdown(&mut self) {
        let (closed, _) = async_mpsc::unbounded_channel();
        drop(std::mem::replace(&mut self.commands, closed));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn view(state: &ClientState) -> View {
    let o = &state.state().observation;
    let hostile = |id| {
        o.combat
            .as_ref()
            .is_some_and(|c| c.actors.iter().any(|a| a.actor == id && a.hostile))
    };
    let mut actors: Vec<Actor> = o
        .visible_actors
        .iter()
        .map(|a| Actor {
            id: a.id,
            name: a.name.clone(),
            description: a.description.clone(),
            position: a.position,
            hostile: hostile(a.id),
            is_self: a.id == o.self_target,
        })
        .collect();
    // The observer's own body need not be listed among what it sees.
    // A dead observer's body becomes its corpse, so it is no longer added.
    let dead = o.combat.as_ref().is_some_and(|c| c.dead);
    if !dead && !actors.iter().any(|a| a.is_self) {
        actors.push(Actor {
            id: o.self_target,
            name: String::new(),
            description: String::new(),
            position: o.position,
            hostile: false,
            is_self: true,
        });
    }
    let seen: std::collections::HashSet<Position> =
        o.visible_cells.iter().map(|c| c.position).collect();
    View {
        feet: o.position,
        visible: o.visible_cells.clone(),
        memory: state
            .map_memory()
            .filter(|c| !seen.contains(&c.position))
            .map(|c| c.cell_view())
            .collect(),
        actors,
        items: o
            .ground_items
            .iter()
            .map(|g| GroundItem {
                id: g.item.id,
                name: g.item.name.clone(),
                class: g.item.class,
                quantity: g.item.quantity,
                description: g.item.description.clone(),
                position: g.position,
                reachable: g.reachable,
            })
            .collect(),
        inventory: o
            .inventory
            .iter()
            .map(|item| {
                let interactions = o.interactions.as_ref();
                let affordance = interactions
                    .and_then(|i| i.inventory.iter().find(|a| a.item == item.id));
                let equipped = affordance.and_then(|a| a.equipped_slot).map(|index| {
                    interactions
                        .and_then(|i| i.slots.get(index as usize))
                        .map_or_else(|| "in use".to_string(), |slot| slot.equipped_phrase.clone())
                });
                Carried {
                    id: item.id,
                    name: item.name.clone(),
                    class: item.class,
                    quantity: item.quantity,
                    description: item.description.clone(),
                    can_equip: affordance.is_some_and(|a| a.slot.is_some() && a.equipped_slot.is_none()),
                    equipped,
                    drinkable: affordance.is_some_and(|a| a.drinkable),
                    edible: affordance.is_some_and(|a| a.edible),
                }
            })
            .collect(),
        narration: state.narration().to_vec(),
        hp: o.combat.as_ref().map(|c| (c.hp, c.max_hp)),
        has_control: state.has_control(),
        can_act: state.can_admit_intention(),
        tick: o.tick,
        revision: state.state().revision,
        events: o
            .combat
            .iter()
            .flat_map(|c| &c.events)
            .filter_map(|e| match e {
                CombatEventView::Ability {
                    caster,
                    target,
                    ability,
                    outcome,
                    strike,
                } => Some(Happening::Ability {
                    caster: *caster,
                    target: *target,
                    name: ability.name.clone(),
                    missed: *outcome == AbilityOutcome::Miss,
                    strike: *strike,
                }),
                CombatEventView::Died { actor } => Some(Happening::Died(*actor)),
                _ => None,
            })
            .collect(),
    }
}

fn publish(tx: &Sender<Event>, event: Event) -> Result<(), Error> {
    tx.send(event).map_err(|_| "Window closed".into())
}

fn present(connection: &Connection, message: &ServerMessage, tx: &Sender<Event>) -> Result<(), Error> {
    match message {
        ServerMessage::Update { .. } | ServerMessage::Snapshot { .. } => {
            publish(tx, Event::View(Box::new(view(&connection.state))))
        }
        ServerMessage::Error { message, .. } => publish(tx, Event::Status(message.clone())),
        _ => Ok(()),
    }
}

async fn run(
    address: SocketAddr,
    token: String,
    actor: ActorId,
    observe: bool,
    mut rx: async_mpsc::UnboundedReceiver<Cmd>,
    tx: &Sender<Event>,
) -> Result<(), Error> {
    let mut connection = Connection::connect(address, token, actor, "3d").await?;
    connection.use_external_advancement_presentation();
    connection.set_pace(Duration::from_millis(90));
    publish(tx, Event::View(Box::new(view(&connection.state))))?;
    if !observe && connection.role() != AccessRole::Spectator {
        transact(&mut connection, Request::AcquireControl, tx).await?;
    }
    publish(tx, Event::Ready)?;
    loop {
        tokio::select! {
            biased;
            message = connection.next() => {
                let message = message?;
                present(&connection, &message, tx)?;
            }
            command = rx.recv() => match command {
                None => {
                    connection.close().await?;
                    return Ok(());
                }
                Some(command) => {
                    match request(&connection.state, command) {
                        Ok(request) if connection.is_synchronized() => {
                            transact(&mut connection, request, tx).await?;
                        }
                        Ok(_) => publish(tx, Event::Status("Resynchronizing; try again.".into()))?,
                        Err(reason) => publish(tx, Event::Status(reason))?,
                    }
                    publish(tx, Event::Ready)?;
                }
            },
        }
    }
}

/// The request a command makes from the state as it stands, as the ASCII
/// client builds it: a step cancels a finished strike sequence, a step into a
/// hostile creature attacks it, and waiting mid-combat continues.
fn request(state: &ClientState, command: Cmd) -> Result<Request, String> {
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
        Cmd::Take(item) => Action::Take { item, quantity: None },
        Cmd::Drop(item) => Action::Drop { item, quantity: None },
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
                    && view.combat.as_ref().is_some_and(|c| {
                        c.actors.iter().any(|o| o.actor == a.id && o.hostile)
                    })
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

async fn transact(connection: &mut Connection, request: Request, tx: &Sender<Event>) -> Result<(), Error> {
    let id = connection.request(request).await?;
    let mut pending = PendingRequest::new(id);
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            let completion = pending.observe(connection, &message);
            let complete = completion.is_some();
            let unknown = matches!(completion, Some(RequestCompletion::Unknown));
            present(connection, &message, tx)?;
            if unknown {
                publish(tx, Event::Status("Resynchronized; the last request may have completed.".into()))?;
            }
            if complete {
                return Ok::<(), Error>(());
            }
        }
    })
    .await
    .map_err(|_| "Response timed out")??;
    Ok(())
}
