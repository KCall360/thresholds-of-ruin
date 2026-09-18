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
    messages: mpsc::Sender<ServerMessage>,
    close: watch::Sender<bool>,
}

pub(crate) struct Connection {
    pub id: u64,
    pub messages: mpsc::Receiver<ServerMessage>,
    pub close: watch::Receiver<bool>,
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
    owner: u64,
    steps: VecDeque<tor_simulation::TravelStep>,
    hazards: BTreeSet<ActorId>,
}

/// Serialized session operations keep snapshots and streamed updates consistent.
pub struct Service {
    travels: BTreeMap<ActorId, TravelJob>,
    travel_status: BTreeMap<ActorId, TravelStatus>,
    resetting_streams: bool,
    broadcasting_travel: bool,
    engine: Engine,
    clients: BTreeMap<u64, Client>,
    controllers: BTreeMap<ActorId, u64>,
    next_client: u64,
}

impl Service {
    pub fn new(engine: Engine) -> Self {
        Self {
            travels: BTreeMap::new(),
            travel_status: BTreeMap::new(),
            resetting_streams: false,
            broadcasting_travel: false,
            engine,
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
        let (messages, receiver) = mpsc::channel(64);
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
                messages,
                close,
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
            return self.snapshot(id, request_id);
        }
        let client = &self.clients[&id];
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let user = client.user.clone();
        let frontend = client.frontend.clone();
        match request {
            Request::CancelTravel { branch, travel_id } => {
                if self.controllers.get(&actor) != Some(&id) {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before cancelling travel",
                    ));
                }
                if &branch != self.engine.branch() {
                    return Err(Failure::new(
                        ErrorCode::WrongBranch,
                        "Travel belongs to another branch",
                    ));
                }
                if self.travel_status.get(&actor).map(|s| &s.id) != Some(&travel_id) {
                    return Err(Failure::new(
                        ErrorCode::InvalidRequest,
                        "Travel is unavailable",
                    ));
                }
                self.stop_travel(actor, TravelPhase::Cancelled);
                self.ack(id, request_id, None);
            }
            Request::AcquireControl => {
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
                    self.control_update(actor);
                }
                self.ack(id, request_id, None);
            }
            Request::Snapshot => return self.snapshot(id, request_id),
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
                    crate::journal::Command::Act { .. } | crate::journal::Command::Travel { .. }
                ) && self.controllers.get(&actor) != Some(&id)
                {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before acting",
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
                match visible_entry.content {
                    HistoryContent::Travel { ref destination } => {
                        self.stop_travel(actor, TravelPhase::Cancelled);
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
                                    owner: id,
                                    steps: steps.into(),
                                    hazards: potential_hazards(&observation),
                                },
                            );
                        }
                        self.travel_update(actor, Some(visible_entry.clone()));
                    }
                    HistoryContent::Wizard { rewind, .. } => {
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
                    HistoryContent::Action { .. } => {
                        self.stop_travel(actor, TravelPhase::Cancelled);
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
            let state = self.engine.state(observer)?;
            if revisions.get(&observer) != Some(&state.revision) {
                self.update(
                    recipient,
                    UpdateBody::Observation {
                        state: Box::new(state),
                        event: (observer == entry.actor).then(|| Box::new(entry.clone())),
                    },
                );
            }
        }
        Ok(())
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

    /// At most one ordinary action per actor per pump; never hold the service lock
    /// for a whole route. Network delivery/cancellation runs between boundaries.
    pub(crate) fn advance_travel(&mut self) {
        for actor in self.travels.keys().copied().collect::<Vec<_>>() {
            let Some(mut job) = self.travels.remove(&actor) else {
                continue;
            };
            if self.controllers.get(&actor) != Some(&job.owner) {
                self.stop_travel(actor, TravelPhase::ControlLost);
                continue;
            }
            let Ok(state) = self.engine.state(actor) else {
                self.stop_travel(actor, TravelPhase::Failed);
                continue;
            };
            if !potential_hazards(&state.observation).is_subset(&job.hazards) {
                self.stop_travel(actor, TravelPhase::Hazard);
                continue;
            }
            if !state.observation.ready {
                self.stop_travel(actor, TravelPhase::DecisionRequired);
                continue;
            }
            let step = job.steps.pop_front().expect("active route");
            let direction = match step.direction {
                tor_world::Direction::North => Direction::North,
                tor_world::Direction::East => Direction::East,
                tor_world::Direction::South => Direction::South,
                tor_world::Direction::West => Direction::West,
                tor_world::Direction::Up => Direction::Up,
                tor_world::Direction::Down => Direction::Down,
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
                    continue;
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
                continue;
            }
            if self.controllers.get(&actor) != Some(&job.owner) {
                self.stop_travel(actor, TravelPhase::ControlLost);
                continue;
            }
            let observation = self.engine.observation(actor).expect("surviving actor");
            let hazard = !potential_hazards(&observation).is_subset(&job.hazards);
            if job.steps.is_empty() {
                self.stop_travel(actor, TravelPhase::Arrived);
            } else if hazard {
                self.stop_travel(actor, TravelPhase::Hazard);
            } else if !observation.ready {
                self.stop_travel(actor, TravelPhase::DecisionRequired);
            } else {
                self.travels.insert(actor, job);
                self.travel_update(actor, None);
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
        self.clients
            .get_mut(&id)
            .expect("connected client")
            .observation_tick = snapshot.state.observation.tick;
        self.send(
            id,
            ServerMessage::Snapshot {
                request_id: request_id.into(),
                snapshot: Box::new(snapshot),
            },
        );
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

    fn update(&mut self, id: u64, body: UpdateBody) {
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
        if let UpdateBody::Observation { state, .. } = &body {
            client.observation_tick = state.observation.tick;
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
        if self
            .clients
            .get(&id)
            .is_some_and(|client| client.messages.try_send(message).is_err())
        {
            // A slow client must reconnect for a snapshot, never silently miss updates.
            self.disconnect(id);
        }
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
                if !self.resetting_streams {
                    self.control_update(actor);
                }
            }
        }
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
        for i in 0..64 {
            service.handle(controller.id, format!("snapshot-{i}"), Request::Snapshot);
        }
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
        for i in 0..64 {
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
        assert!(matches!(update.body, UpdateBody::Observation { .. }));
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
        for i in 0..65 {
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
}

#[cfg(test)]
#[path = "travel_tests.rs"]
mod travel_tests;
