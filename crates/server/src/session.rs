use crate::engine::valid_label;
use crate::{Engine, Failure};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tokio::sync::{mpsc, watch};
use tor_protocol::*;

/// Trusted startup configuration. Tokens are never put into game history.
#[derive(Clone)]
pub struct Account {
    pub user: String,
    pub token: String,
    pub role: AccessRole,
    pub actors: BTreeSet<ActorId>,
}

struct Client {
    role: AccessRole,
    user: String,
    frontend: String,
    allowed: BTreeSet<ActorId>,
    actor: Option<ActorId>,
    sequence: u64,
    observation_tick: u64,
    /// Last full state disclosed on this stream; the base for the next delta.
    last_state: Option<StateView>,
    /// The palette last sent and its revision: the base for the next delta.
    palette: Option<(u64, BTreeSet<String>)>,
    /// The region that palette was forecast from.
    palette_region: Option<u64>,
    messages: mpsc::Sender<ServerMessage>,
    close: watch::Sender<bool>,
    /// What this client was last told play waits for, since its last update
    /// or snapshot; `None` once anything has changed.
    waiting: Option<Waiting>,
}

pub(crate) struct Connection {
    pub id: u64,
    pub messages: mpsc::Receiver<ServerMessage>,
    pub close: watch::Receiver<bool>,
}

/// Each client's outgoing queue. A client whose queue overflows is
/// disconnected; it never silently misses an update.
pub(crate) const QUEUE: usize = 256;

/// A run pauses while an attached client's queue has less room than this. One
/// action sends each client only a few messages, so a run never overflows a
/// client that keeps reading.
pub(crate) const HEADROOM: usize = 16;

/// What [`Service::step`] did.
pub(crate) enum Step {
    /// It took an action or ended a journey; call it again.
    Progress,
    /// It needs input from a client, or nobody is playing.
    Blocked,
    /// These clients' queues are nearly full; wait for them to read.
    Full(Vec<(u64, mpsc::Sender<ServerMessage>)>),
}

/// Until hostility and environmental danger are modeled, another perceived
/// actor is conservatively a potential hazard. Never consult hidden actors,
/// and do not classify another portal view of the observer as a threat.
fn potential_hazards(observation: &Observation) -> BTreeSet<ActorId> {
    observation
        .visible_actors
        .iter()
        .filter(|other| other.id != observation.actor)
        .map(|other| other.id)
        .collect()
}

struct TravelJob {
    hp: Option<u32>,
    owner: u64,
    steps: VecDeque<tor_simulation::TravelStep>,
    hazards: BTreeSet<ActorId>,
}

/// Serialized session operations keep snapshots and streamed updates consistent.
pub struct Service {
    pending_pauses: BTreeSet<ActorId>,
    autonomous_enabled: bool,
    travels: BTreeMap<ActorId, TravelJob>,
    travel_status: BTreeMap<ActorId, TravelStatus>,
    resetting_streams: bool,
    broadcasting_travel: bool,
    engine: Engine,
    pending_saves: Vec<(u64, String, u64)>,
    save_warning: Option<String>,
    clients: BTreeMap<u64, Client>,
    controllers: BTreeMap<ActorId, u64>,
    next_client: u64,
}

impl Service {
    pub fn new(mut engine: Engine) -> Self {
        let mut save_warning = None;
        for actor in engine.actors() {
            if let Err(error) = engine.pause_preparation(actor) {
                save_warning = Some(error.to_string());
            }
        }
        Self {
            pending_pauses: BTreeSet::new(),
            autonomous_enabled: false,
            travels: BTreeMap::new(),
            travel_status: BTreeMap::new(),
            resetting_streams: false,
            broadcasting_travel: false,
            engine,
            pending_saves: Vec::new(),
            save_warning,
            clients: BTreeMap::new(),
            controllers: BTreeMap::new(),
            next_client: 1,
        }
    }

    pub(crate) fn connect(
        &mut self,
        account: &Account,
        frontend: String,
    ) -> Result<Connection, Failure> {
        if !valid_label(&frontend) || self.clients.len() >= 128 {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Connection is unavailable",
            ));
        }
        let next = self.next_client.checked_add(1).ok_or_else(|| {
            Failure::new(ErrorCode::InvalidRequest, "Connection identity exhausted")
        })?;
        let id = self.next_client;
        self.next_client = next;
        let (messages, receiver) = mpsc::channel(QUEUE);
        let (close, closing) = watch::channel(false);
        let actors: Vec<_> = self
            .engine
            .actors()
            .into_iter()
            .filter(|actor| account.role == AccessRole::Wizard || account.actors.contains(actor))
            .collect();
        self.clients.insert(
            id,
            Client {
                role: account.role,
                user: account.user.clone(),
                frontend,
                allowed: actors.iter().copied().collect(),
                actor: None,
                sequence: 0,
                observation_tick: 0,
                last_state: None,
                palette: None,
                palette_region: None,
                messages,
                close,
                waiting: None,
            },
        );
        self.send(
            id,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                user: account.user.clone(),
                actors,
                role: account.role,
            },
        );
        Ok(Connection {
            id,
            messages: receiver,
            close: closing,
        })
    }

    pub(crate) fn handle(&mut self, id: u64, request_id: String, request: Request) {
        if !self.clients.contains_key(&id) {
            return;
        }
        let result = if valid_label(&request_id) {
            self.process(id, &request_id, request)
        } else {
            Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid request ID",
            ))
        };
        if let Err(error) = result {
            self.send(
                id,
                ServerMessage::Error {
                    request_id: Some(request_id),
                    code: error.code,
                    message: error.message,
                },
            );
        }
    }

    fn process(&mut self, id: u64, request_id: &str, request: Request) -> Result<(), Failure> {
        if matches!(
            &request,
            Request::Command {
                command: Command::Wizard { .. },
                ..
            }
        ) && (self.clients[&id].role != AccessRole::Wizard || !self.engine.wizard_enabled())
        {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard authority is required",
            ));
        }
        // Check authority before attachment, receipt lookup, or any mutation.
        if !self.clients[&id].role.permits(&request) {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Spectator access is read-only",
            ));
        }
        if let Request::Attach { actor } = request {
            let client = self.clients.get_mut(&id).expect("connected client");
            if client.actor.is_some() {
                return Err(Failure::new(
                    ErrorCode::AlreadyAttached,
                    "Reconnect to attach another actor",
                ));
            }
            if !client.allowed.contains(&actor) {
                return Err(Failure::new(
                    ErrorCode::Unauthorized,
                    "Actor is unavailable",
                ));
            }
            client.actor = Some(actor);
            self.snapshot(id, request_id)?;
            if let Some(message) = self.save_warning.clone() {
                self.send(
                    id,
                    ServerMessage::Error {
                        request_id: None,
                        code: ErrorCode::StorageFailure,
                        message,
                    },
                );
            }
            return Ok(());
        }
        let client = &self.clients[&id];
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let user = client.user.clone();
        let frontend = client.frontend.clone();
        match request {
            Request::Continue => {
                if self.controllers.get(&actor) != Some(&id) {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before continuing",
                    ));
                }
                self.autonomous_enabled = true;
                self.ack(id, request_id, None);
            }
            Request::Save => {
                if self
                    .pending_saves
                    .iter()
                    .any(|(client, _, _)| *client == id)
                {
                    return Err(Failure::new(
                        ErrorCode::InvalidRequest,
                        "A save is already pending",
                    ));
                }
                let target = self.engine.request_save();
                self.pending_saves.push((id, request_id.into(), target));
                self.poll_saves();
            }
            Request::AcquireControl => {
                if self.engine.is_ai(actor) {
                    return Err(Failure::new(
                        ErrorCode::Unauthorized,
                        "This actor is controlled by scenario AI",
                    ));
                }
                match self.controllers.get(&actor) {
                    Some(owner) if *owner != id => {
                        return Err(Failure::new(
                            ErrorCode::ControlTaken,
                            "Another client controls this actor",
                        ))
                    }
                    Some(_) => {}
                    None => {
                        self.controllers.insert(actor, id);
                        self.control_update(actor);
                    }
                }
                self.ack(id, request_id, None);
            }
            Request::ReleaseControl => {
                if self
                    .controllers
                    .get(&actor)
                    .is_some_and(|owner| *owner != id)
                {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "This client does not control the actor",
                    ));
                }
                self.stop_travel(actor, TravelPhase::ControlLost);
                if self.controllers.remove(&actor).is_some() {
                    self.pending_pauses.insert(actor);
                    self.autonomous_enabled = false;
                    self.control_update(actor);
                }
                self.ack(id, request_id, None);
            }
            Request::Snapshot => return self.snapshot(id, request_id),
            Request::Palette => self.palette_update(id, Some(request_id), true),
            Request::HistoryBranch {
                branch,
                before,
                limit,
            } => {
                let page = self.engine.history_branch(
                    actor,
                    &user,
                    &branch,
                    before.as_ref(),
                    usize::from(limit),
                )?;
                self.send(
                    id,
                    ServerMessage::History {
                        request_id: request_id.into(),
                        page,
                    },
                );
            }
            Request::History { before, limit } => {
                let page =
                    self.engine
                        .history(actor, &user, before.as_ref(), usize::from(limit))?;
                self.send(
                    id,
                    ServerMessage::History {
                        request_id: request_id.into(),
                        page,
                    },
                );
            }
            Request::Command { branch, command } => {
                let command = crate::journal::Command::from_wire(&command)?;
                if let Some(previous) = self
                    .engine
                    .retry(&user, actor, request_id, &branch, &command)?
                {
                    self.ack(id, request_id, Some(previous.entry.id));
                    return Ok(());
                }
                if matches!(
                    command,
                    crate::journal::Command::RenamePlace { .. }
                        | crate::journal::Command::Act { .. }
                        | crate::journal::Command::Travel { .. }
                ) && self.controllers.get(&actor) != Some(&id)
                {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before acting",
                    ));
                }
                // Only the server ends a journey; the player waits for it.
                if matches!(
                    command,
                    crate::journal::Command::Act { .. } | crate::journal::Command::Travel { .. }
                ) && self.travels.contains_key(&actor)
                {
                    return Err(Failure::new(
                        ErrorCode::ActorBusy,
                        "Your character is still travelling",
                    ));
                }
                let revisions: BTreeMap<_, _> = self
                    .engine
                    .actors()
                    .into_iter()
                    .map(|actor| Ok((actor, self.engine.revision(actor)?)))
                    .collect::<Result<_, Failure>>()?;
                let result = self
                    .engine
                    .command(&user, &frontend, actor, request_id, &branch, command)?;
                let visible_entry = result.entry.disclosed();
                if matches!(
                    visible_entry.content,
                    HistoryContent::Action { .. } | HistoryContent::Travel { .. }
                ) {
                    self.autonomous_enabled = true;
                }
                match visible_entry.content {
                    HistoryContent::Travel { ref destination } => {
                        let steps = self.engine.travel_route(actor, destination)?;
                        let observation = self.engine.observation(actor)?;
                        let phase = if steps.is_empty() {
                            TravelPhase::Arrived
                        } else {
                            TravelPhase::Active
                        };
                        self.travel_status.insert(
                            actor,
                            TravelStatus {
                                id: result.entry.id.clone(),
                                destination: destination.clone(),
                                completed_steps: 0,
                                phase,
                            },
                        );
                        if phase == TravelPhase::Active {
                            self.travels.insert(
                                actor,
                                TravelJob {
                                    hp: observation.combat.as_ref().map(|c| c.hp),
                                    owner: id,
                                    steps: steps.into(),
                                    hazards: potential_hazards(&observation),
                                },
                            );
                        }
                        self.travel_update(actor, Some(visible_entry.clone()));
                    }
                    HistoryContent::Wizard { rewind, .. } => {
                        self.autonomous_enabled = false;
                        self.pending_pauses.clear();
                        for actor in self.engine.actors() {
                            if let Err(error) = self.engine.pause_preparation(actor) {
                                self.save_warning = Some(error.to_string());
                            }
                        }
                        if rewind {
                            // The committed rewind already changed the branch. Publish
                            // only the explicit snapshot boundary into the old stream.
                            self.travels.clear();
                            self.travel_status.clear();
                        } else {
                            for travelling in self.travels.keys().copied().collect::<Vec<_>>() {
                                self.stop_travel(travelling, TravelPhase::WorldChanged);
                            }
                        }
                        // A committed setup/rewind establishes an explicit stream boundary.
                        // Clients attached to actors removed by rewind must reattach.
                        let recipients: Vec<_> = self
                            .clients
                            .iter()
                            .filter_map(|(&id, c)| c.actor.map(|a| (id, a)))
                            .collect();
                        let previous_controllers = self.controllers.clone();
                        let actors = self.engine.actors();
                        self.controllers.retain(|actor, _| actors.contains(actor));
                        self.resetting_streams = true;
                        for (recipient, observer) in recipients {
                            if !self.clients.contains_key(&recipient) {
                                continue;
                            }
                            let disconnect = match self.engine.revision(observer) {
                                Err(_) => true,
                                Ok(revision) => {
                                    (rewind || revisions.get(&observer) != Some(&revision))
                                        && self.snapshot(recipient, "").is_err()
                                }
                            };
                            if disconnect {
                                self.disconnect(recipient);
                            }
                        }
                        self.resetting_streams = false;
                        for (actor, owner) in previous_controllers {
                            if self.controllers.get(&actor) != Some(&owner)
                                && actors.contains(&actor)
                            {
                                self.control_update(actor);
                            }
                        }
                    }
                    HistoryContent::Annotation { .. } => self.annotation_update(&visible_entry),
                    HistoryContent::PlaceRenamed { .. } | HistoryContent::Action { .. } => {
                        self.action_update(&revisions, &visible_entry)?;
                    }
                }
                self.ack(id, request_id, Some(result.entry.id));
            }
            Request::Attach { .. } => unreachable!("handled before attachment lookup"),
        }
        Ok(())
    }

    fn action_update(
        &mut self,
        revisions: &BTreeMap<ActorId, u64>,
        entry: &HistoryEntry,
    ) -> Result<(), Failure> {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter_map(|(&id, c)| c.actor.map(|a| (id, a)))
            .collect();
        for (recipient, observer) in recipients {
            // Region streaming can unload an actor nobody keeps in play (an
            // AI actor a spectator watches); its clients must reattach.
            let Ok(state) = self.engine.state(observer) else {
                self.detach_unloaded(recipient);
                continue;
            };
            if revisions.get(&observer) != Some(&state.revision) {
                self.update(
                    recipient,
                    UpdateBody::Observation {
                        state: Box::new(state),
                        event: (observer == entry.actor).then(|| Box::new(entry.clone())),
                    },
                );
            }
            self.palette_update(recipient, None, false);
        }
        Ok(())
    }

    /// Send a client its actor's palette: the whole palette when asked, or
    /// the first time; otherwise the changes, if any, as a delta. Nothing is
    /// acknowledged. Scenarios that name no assets send no palettes, but a
    /// palette request still gets an (empty) answer.
    fn palette_update(&mut self, id: u64, request_id: Option<&str>, full: bool) {
        let Some(client) = self.clients.get(&id) else {
            return;
        };
        let Some(actor) = client.actor else {
            return;
        };
        // A palette depends only on the actor's region: skip recomputing it
        // while that stays the same.
        let region = self.engine.palette_region(actor);
        if !full && client.palette.is_some() && client.palette_region == region {
            return;
        }
        let assets = match self.engine.palette(actor) {
            Some(assets) => assets,
            None if request_id.is_some() => BTreeSet::new(),
            None => return,
        };
        let client = self.clients.get_mut(&id).expect("connected client");
        if !full
            && client
                .palette
                .as_ref()
                .is_some_and(|(_, previous)| *previous == assets)
        {
            client.palette_region = region;
            return;
        }
        let body = match &client.palette {
            Some((base, previous)) if !full => PaletteBody::Delta {
                base: *base,
                added: assets.difference(previous).cloned().collect(),
                removed: previous.difference(&assets).cloned().collect(),
            },
            _ => PaletteBody::Full {
                assets: assets.clone(),
            },
        };
        let revision = client.palette.as_ref().map_or(1, |(r, _)| r + 1);
        client.palette = Some((revision, assets));
        client.palette_region = region;
        self.send(
            id,
            ServerMessage::Palette {
                request_id: request_id.map(Into::into),
                palette: PaletteUpdate { revision, body },
            },
        );
    }

    fn travel_update(&mut self, actor: ActorId, entry: Option<HistoryEntry>) {
        let Some(initial) = self.travel_status.get(&actor).cloned() else {
            return;
        };
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, c)| c.actor == Some(actor))
            .map(|(&id, _)| id)
            .collect();
        self.broadcasting_travel = true;
        for id in recipients {
            // Sending can disconnect a slow controller and stop its job. Never
            // follow that terminal status with an obsolete active status.
            let status = self.travel_status[&actor].clone();
            self.update(
                id,
                UpdateBody::Travel {
                    status,
                    entry: entry.clone().map(Box::new),
                },
            );
        }
        self.broadcasting_travel = false;
        if self.travel_status.get(&actor) != Some(&initial) {
            self.travel_update(actor, None);
        }
    }

    fn stop_travel(&mut self, actor: ActorId, phase: TravelPhase) {
        self.travels.remove(&actor);
        if let Some(status) = self.travel_status.get_mut(&actor) {
            if status.phase == TravelPhase::Active {
                status.phase = phase;
                if !self.broadcasting_travel {
                    self.travel_update(actor, None);
                }
            }
        }
    }

    /// Apply the pauses a released or disconnected controller left behind.
    fn apply_pending_pauses(&mut self) {
        for actor in std::mem::take(&mut self.pending_pauses) {
            let revisions = self
                .engine
                .actors()
                .into_iter()
                .map(|id| (id, self.engine.revision(id).unwrap()))
                .collect();
            match self.engine.pause_preparation(actor) {
                Ok(Some(result)) => {
                    let _ = self.action_update(&revisions, &result.entry.disclosed());
                }
                Ok(None) => {}
                Err(error) => {
                    self.save_warning = Some(error.to_string());
                }
            }
        }
    }

    /// Take one action, or say why the simulation can't. The runner calls this
    /// until it blocks and handles waiting mail between calls, so a request
    /// waits for at most one action. What happens depends only on the game and
    /// the commands it received, never on wall-clock time.
    pub(crate) fn step(&mut self) -> Step {
        self.apply_pending_pauses();
        let full: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| client.actor.is_some() && client.messages.capacity() < HEADROOM)
            .map(|(&id, client)| (id, client.messages.clone()))
            .collect();
        if !full.is_empty() {
            return Step::Full(full);
        }
        let Some(next) = self.engine.next_actor() else {
            self.announce_waiting();
            return Step::Blocked;
        };
        if self.travels.contains_key(&next) {
            self.travel_step(next);
            return Step::Progress;
        }
        // Scenario AI plays only while someone is playing: a run never
        // continues with no controlled actor left alive.
        if !self.engine.is_ai(next)
            || !self.autonomous_enabled
            || !self
                .controllers
                .keys()
                .any(|&actor| self.engine.alive(actor))
        {
            self.announce_waiting();
            return Step::Blocked;
        }
        self.advance_ai(next);
        Step::Progress
    }

    /// Tell each attached client what stopped play waits for, once per stop.
    fn announce_waiting(&mut self) {
        let next = self.engine.next_actor();
        let anyone = self
            .controllers
            .keys()
            .any(|&actor| self.engine.alive(actor));
        let ids: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, c)| c.actor.is_some())
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            let on = match next {
                None => Waiting::Stopped,
                // AI plays only while someone does.
                Some(actor) if self.engine.is_ai(actor) && !anyone => Waiting::Stopped,
                Some(actor) if self.engine.is_ai(actor) => Waiting::Paused,
                Some(actor) => match self.controllers.get(&actor) {
                    Some(&owner) if owner == id => Waiting::You,
                    Some(_) => Waiting::Others,
                    None => Waiting::Unclaimed,
                },
            };
            if self.clients[&id].waiting != Some(on) {
                self.send(id, ServerMessage::Waiting { on });
                if let Some(client) = self.clients.get_mut(&id) {
                    client.waiting = Some(on);
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn run_until_blocked(&mut self) {
        while matches!(self.step(), Step::Progress) {}
    }

    /// One step of a journey whose actor is next. The checks before the step
    /// catch anything other actors did while the journey waited for its turn.
    fn travel_step(&mut self, actor: ActorId) {
        let Some(mut job) = self.travels.remove(&actor) else {
            return;
        };
        if self.controllers.get(&actor) != Some(&job.owner) {
            self.stop_travel(actor, TravelPhase::ControlLost);
            return;
        }
        let Ok(state) = self.engine.state(actor) else {
            self.stop_travel(actor, TravelPhase::Failed);
            return;
        };
        if job
            .hp
            .zip(state.observation.combat.as_ref().map(|c| c.hp))
            .is_some_and(|(before, after)| after < before)
        {
            self.stop_travel(actor, TravelPhase::Hazard);
            return;
        }
        if !potential_hazards(&state.observation).is_subset(&job.hazards) {
            self.stop_travel(actor, TravelPhase::Hazard);
            return;
        }
        let step = job.steps.pop_front().expect("active route");
        let direction = match step.direction {
            tor_world::Direction::North => Direction::North,
            tor_world::Direction::East => Direction::East,
            tor_world::Direction::South => Direction::South,
            tor_world::Direction::West => Direction::West,
            tor_world::Direction::NorthEast => Direction::NorthEast,
            tor_world::Direction::SouthEast => Direction::SouthEast,
            tor_world::Direction::SouthWest => Direction::SouthWest,
            tor_world::Direction::NorthWest => Direction::NorthWest,

            tor_world::Direction::Up => Direction::Up,
            tor_world::Direction::Down => Direction::Down,
            _ => unreachable!("travel directions are observer-relative"),
        };
        let revisions = self
            .engine
            .actors()
            .into_iter()
            .map(|a| (a, self.engine.revision(a).expect("existing actor")))
            .collect();
        let client = &self.clients[&job.owner];
        let result = self.engine.command(
            &client.user,
            &client.frontend,
            actor,
            &uuid::Uuid::new_v4().to_string(),
            &self.engine.branch().clone(),
            crate::journal::Command::Act {
                expected_revision: state.revision,
                action: Action::Move { direction },
            },
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.stop_travel(
                    actor,
                    if error.code == ErrorCode::InvalidAction {
                        TravelPhase::Blocked
                    } else {
                        TravelPhase::Failed
                    },
                );
                return;
            }
        };
        self.travel_status
            .get_mut(&actor)
            .expect("active status")
            .completed_steps += 1;
        if self
            .action_update(&revisions, &result.entry.disclosed())
            .is_err()
        {
            self.stop_travel(actor, TravelPhase::Failed);
            return;
        }
        if self.controllers.get(&actor) != Some(&job.owner) {
            self.stop_travel(actor, TravelPhase::ControlLost);
            return;
        }
        let Ok(observation) = self.engine.observation(actor) else {
            self.stop_travel(actor, TravelPhase::Failed);
            return;
        };
        let hazard = !potential_hazards(&observation).is_subset(&job.hazards)
            || job
                .hp
                .zip(observation.combat.as_ref().map(|c| c.hp))
                .is_some_and(|(before, after)| after < before);
        if observation
            .motion
            .as_ref()
            .is_some_and(|m| m.displaced || m.impacted)
        {
            self.stop_travel(actor, TravelPhase::DecisionRequired);
        } else if job.steps.is_empty() {
            self.stop_travel(actor, TravelPhase::Arrived);
        } else if hazard {
            self.stop_travel(actor, TravelPhase::Hazard);
        } else {
            // Another actor may be next; the journey waits for its turn.
            self.travels.insert(actor, job);
            self.travel_update(actor, None);
        }
    }

    fn advance_ai(&mut self, actor: ActorId) {
        let action = self
            .engine
            .next_ai_action()
            .filter(|(chosen, _)| *chosen == actor)
            .map(|(_, action)| action);
        let revisions = self
            .engine
            .actors()
            .into_iter()
            .map(|id| (id, self.engine.revision(id).unwrap()))
            .collect();
        let result = match action {
            Some(action) => {
                let command = crate::journal::Command::Act {
                    expected_revision: self.engine.revision(actor).unwrap(),
                    action,
                };
                self.engine.command(
                    "scenario-ai",
                    "server-ai",
                    actor,
                    &uuid::Uuid::new_v4().to_string(),
                    &self.engine.branch().clone(),
                    command,
                )
            }
            None => Err(Failure::new(
                ErrorCode::InvalidAction,
                "Scenario AI has no action",
            )),
        };
        match result {
            Ok(result) => {
                let _ = self.action_update(&revisions, &result.entry.disclosed());
            }
            Err(error) => {
                self.autonomous_enabled = false;
                for client in self.controllers.values().copied().collect::<Vec<_>>() {
                    self.send(
                        client,
                        ServerMessage::Error {
                            request_id: None,
                            code: error.code,
                            message: "Autonomous action failed; simulation paused.".into(),
                        },
                    );
                }
            }
        }
    }

    fn snapshot(&mut self, id: u64, request_id: &str) -> Result<(), Failure> {
        let client = &self.clients[&id];
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let state = self.engine.state(actor)?;
        let snapshot = Snapshot {
            travel: self.travel_status.get(&actor).cloned(),
            actor,
            branch: self.engine.branch().clone(),
            cursor: StreamCursor {
                sequence: client.sequence,
                tick: state.observation.tick,
            },
            state,
            has_control: self.controllers.get(&actor) == Some(&id),
            history: self
                .engine
                .history(actor, &client.user, None, MAX_HISTORY_PAGE)?,
        };
        let client = self.clients.get_mut(&id).expect("connected client");
        client.observation_tick = snapshot.state.observation.tick;
        client.last_state = Some(snapshot.state.clone());
        self.send(
            id,
            ServerMessage::Snapshot {
                request_id: request_id.into(),
                snapshot: Box::new(snapshot),
            },
        );
        // Attaching sends the whole palette; a later snapshot, what changed.
        self.palette_update(id, None, false);
        Ok(())
    }

    /// Trusted backend entry point; not exposed as a client request.
    pub fn annotate_backend(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
    ) -> Result<HistoryEntry, Failure> {
        let entry = self
            .engine
            .annotate_backend(actor, component, anchor, category, text)?;
        let entry = entry.disclosed();
        self.annotation_update(&entry);
        Ok(entry)
    }

    fn annotation_update(&mut self, entry: &HistoryEntry) {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| {
                client
                    .actor
                    .is_some_and(|actor| entry.visible_to(actor, &client.user))
            })
            .map(|(&id, _)| id)
            .collect();
        for id in recipients {
            self.update(
                id,
                UpdateBody::Annotation {
                    entry: Box::new(entry.clone()),
                },
            );
        }
    }

    fn control_update(&mut self, actor: ActorId) {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| client.actor == Some(actor))
            .map(|(&id, _)| id)
            .collect();
        for id in recipients {
            self.update(
                id,
                UpdateBody::Control {
                    has_control: self.controllers.get(&actor) == Some(&id),
                },
            );
        }
    }

    fn update(&mut self, id: u64, mut body: UpdateBody) {
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        let Some(sequence) = client.sequence.checked_add(1) else {
            self.disconnect(id);
            return;
        };
        let actor = client.actor.expect("only attached clients receive updates");
        client.sequence = sequence;
        // Disconnecting a slow controller can publish control changes in the
        // middle of an action broadcast. Keep those changes at each recipient's
        // last disclosed tick until its new observation has been queued.
        if let UpdateBody::Observation { state, event } = body {
            client.observation_tick = state.observation.tick;
            let delta = client
                .last_state
                .as_ref()
                .and_then(|base| StateDelta::between(base, &state));
            client.last_state = Some((*state).clone());
            body = match delta {
                Some(delta) => UpdateBody::ObservationDelta {
                    state: Box::new(delta),
                    event,
                },
                None => UpdateBody::Observation { state, event },
            };
        }
        let tick = client.observation_tick;
        self.send(
            id,
            ServerMessage::Update {
                update: Box::new(StreamUpdate {
                    actor,
                    branch: self.engine.branch().clone(),
                    cursor: StreamCursor { sequence, tick },
                    body,
                }),
            },
        );
    }

    fn ack(&mut self, id: u64, request_id: &str, entry_id: Option<EntryId>) {
        self.send(
            id,
            ServerMessage::Ack {
                request_id: request_id.into(),
                entry_id,
            },
        );
    }

    fn send(&mut self, id: u64, message: ServerMessage) {
        // Anything that changes what a client knows, and every answer to a
        // request, needs a fresh word on whose move it is.
        if matches!(
            message,
            ServerMessage::Update { .. }
                | ServerMessage::Snapshot { .. }
                | ServerMessage::Ack { .. }
        ) {
            if let Some(client) = self.clients.get_mut(&id) {
                client.waiting = None;
            }
        }
        if self
            .clients
            .get(&id)
            .is_some_and(|client| client.messages.try_send(message).is_err())
        {
            // A slow client must reconnect for a snapshot, never silently miss updates.
            self.disconnect(id);
        }
    }

    /// Tell a client its actor left the loaded world, then disconnect it.
    fn detach_unloaded(&mut self, id: u64) {
        self.send(
            id,
            ServerMessage::Error {
                request_id: None,
                code: ErrorCode::NotAttached,
                message: "Your actor left the loaded world; attach again once it's back in play"
                    .into(),
            },
        );
        self.disconnect(id);
    }

    pub(crate) fn disconnect(&mut self, id: u64) {
        let Some(client) = self.clients.remove(&id) else {
            return;
        };
        let _ = client.close.send(true);
        if let Some(actor) = client.actor {
            if self.controllers.get(&actor) == Some(&id) {
                self.stop_travel(actor, TravelPhase::ControlLost);
            }
            if self.controllers.get(&actor) == Some(&id) {
                self.controllers.remove(&actor);
                self.pending_pauses.insert(actor);
                self.autonomous_enabled = false;
                if !self.resetting_streams {
                    self.control_update(actor);
                }
            }
        }
    }

    pub(crate) fn poll_saves(&mut self) {
        let status = self.engine.save_status();
        let warning = status.error.clone().or_else(|| {
            status.overdue.then(|| {
                "Saving is behind schedule; recent play may be lost if the server stops.".into()
            })
        });
        if warning != self.save_warning {
            if let Some(message) = &warning {
                eprintln!("{message}");
                // Never interleave a warning with the welcome/attach handshake.
                for id in self
                    .clients
                    .iter()
                    .filter_map(|(id, client)| client.actor.map(|_| *id))
                    .collect::<Vec<_>>()
                {
                    self.send(
                        id,
                        ServerMessage::Error {
                            request_id: None,
                            code: ErrorCode::StorageFailure,
                            message: message.clone(),
                        },
                    );
                }
            }
            self.save_warning = warning;
        }
        let pending = std::mem::take(&mut self.pending_saves);
        for (id, request_id, target) in pending {
            if !self.clients.contains_key(&id) {
                continue;
            }
            if status.durable_sequence >= target {
                self.ack(id, &request_id, None);
            } else if let Some(message) = &status.error {
                self.send(
                    id,
                    ServerMessage::Error {
                        request_id: Some(request_id),
                        code: ErrorCode::StorageFailure,
                        message: message.clone(),
                    },
                );
            } else {
                self.pending_saves.push((id, request_id, target));
            }
        }
    }
    pub(crate) fn flush_handle(&self) -> Option<crate::storage::Store> {
        self.engine.flush_handle()
    }
    pub(crate) fn shutdown(&mut self) {
        for id in self.clients.keys().copied().collect::<Vec<_>>() {
            self.disconnect(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scenario;

    #[test]
    fn slow_controller_during_rewind_cannot_send_control_into_the_old_branch() {
        let mut engine = Engine::memory(Scenario::two_room(0)).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Wizard,
            user: "wizard".into(),
            token: "test".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut controller = service.connect(&account, "text".into()).unwrap();
        let mut observer = service.connect(&account, "ascii".into()).unwrap();
        for c in [&mut controller, &mut observer] {
            c.messages.try_recv().unwrap();
            service.handle(c.id, "attach".into(), Request::Attach { actor: ActorId(1) });
            c.messages.try_recv().unwrap();
        }
        service.handle(controller.id, "control".into(), Request::AcquireControl);
        controller.messages.try_recv().unwrap();
        controller.messages.try_recv().unwrap();
        observer.messages.try_recv().unwrap();
        for i in 0..QUEUE {
            service.handle(controller.id, format!("snapshot-{i}"), Request::Snapshot);
        }
        service.autonomous_enabled = true;
        let branch = service.engine.branch().clone();
        service.handle(
            observer.id,
            "rewind".into(),
            Request::Command {
                branch: branch.clone(),
                command: Command::Wizard {
                    expected_revision: 0,
                    operation: "rewind initial".into(),
                },
            },
        );
        assert!(!service.autonomous_enabled);
        assert!(*controller.close.borrow());
        let ServerMessage::Snapshot { snapshot, .. } = observer.messages.try_recv().unwrap() else {
            panic!("snapshot must precede control transition")
        };
        assert_ne!(snapshot.branch, branch);
        let ServerMessage::Update { update } = observer.messages.try_recv().unwrap() else {
            panic!("control")
        };
        assert_eq!(update.branch, snapshot.branch);
        assert!(matches!(
            update.body,
            UpdateBody::Control { has_control: false }
        ));
    }

    #[test]
    fn disconnect_during_action_broadcast_keeps_control_tick_at_last_disclosed_state() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let account = Account {
            role: tor_protocol::AccessRole::Player,
            user: "alice".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut controller = service.connect(&account, "text".into()).unwrap();
        let mut observer = service.connect(&account, "ascii".into()).unwrap();
        for client in [&mut controller, &mut observer] {
            client.messages.try_recv().unwrap();
            service.handle(
                client.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            client.messages.try_recv().unwrap();
        }
        service.handle(controller.id, "control".into(), Request::AcquireControl);
        controller.messages.try_recv().unwrap();
        controller.messages.try_recv().unwrap();
        observer.messages.try_recv().unwrap();
        for i in 0..QUEUE {
            service.handle(controller.id, format!("snapshot-{i}"), Request::Snapshot);
        }
        service.handle(
            controller.id,
            "wait".into(),
            Request::Command {
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            },
        );
        assert!(*controller.close.borrow());
        let ServerMessage::Update { update } = observer.messages.try_recv().unwrap() else {
            panic!("control update")
        };
        assert!(matches!(
            update.body,
            UpdateBody::Control { has_control: false }
        ));
        assert_eq!(update.cursor.tick, 0);
        let ServerMessage::Update { update } = observer.messages.try_recv().unwrap() else {
            panic!("observation update")
        };
        assert!(matches!(
            update.body,
            UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
        ));
        assert_eq!(update.cursor.tick, 100);
    }

    #[test]
    fn slow_client_is_disconnected_and_releases_control_instead_of_losing_updates() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let account = Account {
            role: tor_protocol::AccessRole::Player,
            user: "alice".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut connection = service.connect(&account, "text".into()).unwrap();
        connection.messages.try_recv().unwrap();
        service.handle(
            connection.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        connection.messages.try_recv().unwrap();
        service.handle(connection.id, "control".into(), Request::AcquireControl);
        connection.messages.try_recv().unwrap();
        connection.messages.try_recv().unwrap();
        for i in 0..QUEUE + 1 {
            service.handle(connection.id, format!("snapshot-{i}"), Request::Snapshot);
        }
        assert!(*connection.close.borrow());
        assert!(!service.clients.contains_key(&connection.id));
        assert!(!service.controllers.contains_key(&ActorId(1)));
        let mut replacement = service.connect(&account, "ascii".into()).unwrap();
        replacement.messages.try_recv().unwrap();
        service.handle(
            replacement.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        let ServerMessage::Snapshot { snapshot, .. } = replacement.messages.try_recv().unwrap()
        else {
            panic!("snapshot")
        };
        assert_eq!(snapshot.cursor.sequence, 0);
        service.handle(replacement.id, "control".into(), Request::AcquireControl);
        assert_eq!(service.controllers.get(&ActorId(1)), Some(&replacement.id));
    }

    /// A dungeon where, after the character waits, the guard is the next actor.
    fn character_waiting_on_a_guard() -> (Service, Connection) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&root, 1, None, false).unwrap()).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Wizard,
            user: "wizard".into(),
            token: "test".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        client.messages.try_recv().unwrap();
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        let revision = service.engine.revision(ActorId(1)).unwrap();
        service.handle(
            client.id,
            "wait".into(),
            Request::Command {
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    expected_revision: revision,
                    action: Action::Wait,
                },
            },
        );
        while client.messages.try_recv().is_ok() {}
        assert!(
            service.engine.next_ai_action().is_some(),
            "the guard must be next"
        );
        (service, client)
    }

    #[test]
    fn ai_turns_run_until_a_controlled_actor_is_due() {
        let (mut service, _client) = character_waiting_on_a_guard();
        let before = service.engine.revision(ActorId(1)).unwrap();
        service.run_until_blocked();
        assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
        assert_ne!(service.engine.revision(ActorId(1)).unwrap(), before);
        assert!(matches!(service.step(), Step::Blocked));
    }

    #[test]
    fn ai_does_not_play_when_nobody_controls_an_actor() {
        let (mut service, client) = character_waiting_on_a_guard();
        service.handle(client.id, "release".into(), Request::ReleaseControl);
        service.run_until_blocked();
        assert!(
            service.engine.next_ai_action().is_some(),
            "the guard must still be waiting for its turn"
        );
    }

    #[test]
    fn a_full_client_queue_pauses_the_run_until_the_client_reads() {
        let (mut service, mut client) = character_waiting_on_a_guard();
        for i in 0..QUEUE - HEADROOM + 1 {
            service.handle(client.id, format!("fill-{i}"), Request::Snapshot);
        }
        let before = service.engine.revision(ActorId(1)).unwrap();
        let Step::Full(full) = service.step() else {
            panic!("the run must wait for the client");
        };
        assert_eq!(
            full.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [client.id]
        );
        assert_eq!(service.engine.revision(ActorId(1)).unwrap(), before);
        assert!(!*client.close.borrow());
        while client.messages.try_recv().is_ok() {}
        service.run_until_blocked();
        assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
        assert_ne!(service.engine.revision(ActorId(1)).unwrap(), before);
    }

    /// Mail that is already waiting, here a rewind against the current
    /// revision, is handled before the next action can change that revision.
    #[tokio::test]
    async fn mail_waiting_in_the_mailbox_is_handled_before_the_next_action() {
        let (service, mut client) = character_waiting_on_a_guard();
        let revision = service.engine.revision(ActorId(1)).unwrap();
        let branch = service.engine.branch().clone();
        let (mail, mailbox) = mpsc::channel(8);
        mail.send(crate::runner::Mail::Request {
            client: client.id,
            request_id: "rewind".into(),
            request: Request::Command {
                branch: branch.clone(),
                command: Command::Wizard {
                    expected_revision: revision,
                    operation: "rewind initial".into(),
                },
            },
            started: None,
        })
        .await
        .unwrap();
        let (reply, stopped) = tokio::sync::oneshot::channel();
        mail.send(crate::runner::Mail::Shutdown(reply))
            .await
            .unwrap();
        crate::runner::run(service, mailbox, crate::runner::STALL).await;
        stopped.await.unwrap();
        let mut messages = Vec::new();
        while let Ok(message) = client.messages.try_recv() {
            messages.push(message);
        }
        assert!(
            messages.iter().all(|message| !matches!(
                message,
                ServerMessage::Error {
                    code: ErrorCode::StaleRevision,
                    ..
                }
            )),
            "waiting command was rejected: {messages:?}"
        );
        assert!(
            messages.iter().any(|message| matches!(
                message,
                ServerMessage::Snapshot { snapshot, .. } if snapshot.branch != branch
            )),
            "rewind did not fork: {messages:?}"
        );
    }
}

/// Running until blocked: journeys, outgoing queues and slow clients, and the
/// runner over its mailbox (see docs/run-until-blocked.md).
#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;
