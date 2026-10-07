use crate::engine::valid_label;
use crate::{Engine, Failure};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use tokio::sync::watch;
use tor_protocol::*;

/// Trusted startup configuration. Tokens are never put into game history.
#[derive(Clone)]
pub struct Account {
    pub user: String,
    pub token: String,
    pub role: AccessRole,
    pub actors: BTreeSet<ActorId>,
}

struct DisclosedObservation {
    branch: BranchId,
    base: ObservationBase,
    state: Arc<StateView>,
}

/// Ownership is part of the generation even when permission vectors stay empty.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DisclosedReadiness {
    has_control: bool,
    permissions: Readiness,
}

/// Payloads prepared by a request and published after its ordered effects.
enum RequestReply {
    Receipt(RequestReceipt),
    History(HistoryPage),
    Palette(PaletteUpdate),
}

impl RequestReply {
    fn message(self, request_id: String, context: ReplyContext) -> ServerMessage {
        match self {
            Self::Receipt(receipt) => ServerMessage::Ack {
                context,
                request_id,
                receipt,
            },
            Self::History(page) => ServerMessage::History {
                context,
                request_id,
                page,
            },
            Self::Palette(palette) => ServerMessage::Palette {
                context,
                request_id: Some(request_id),
                palette,
            },
        }
    }
}

enum ProcessedRequest {
    Reply(RequestReply),
    /// Snapshot publication owns the stream reset; no additional reply is due.
    Published,
    /// Persistence owns completion; poll it at the common publication boundary.
    SaveQueued,
}

struct Client {
    role: AccessRole,
    user: String,
    frontend: String,
    allowed: BTreeSet<ActorId>,
    actor: Option<ActorId>,
    context: Option<StreamContext>,
    readiness: Option<DisclosedReadiness>,
    sequence: u64,
    observation_tick: u64,
    /// Last full state disclosed on this stream; the base for the next delta.
    last_observation: Option<DisclosedObservation>,
    /// The palette last sent and its revision: the base for the next delta.
    palette: Option<(u64, BTreeSet<String>)>,
    /// The region that palette was forecast from.
    palette_region: Option<u64>,
    messages: crate::outbound::Sender,
    close: watch::Sender<bool>,
    /// What this client was last told play waits for, since its last update
    /// or snapshot; `None` once anything has changed.
    waiting: Option<Waiting>,
}

pub(crate) struct Connection {
    pub id: u64,
    pub messages: crate::outbound::Receiver,
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
    Full(Vec<(u64, crate::outbound::Sender)>),
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
    pending: Option<EntryId>,
}

impl TravelJob {
    fn hazard_in(&self, observation: &Observation) -> bool {
        !potential_hazards(observation).is_subset(&self.hazards)
            || self
                .hp
                .zip(observation.combat.as_ref().map(|combat| combat.hp))
                .is_some_and(|(before, after)| after < before)
    }
}

#[derive(Clone, Copy)]
enum TravelExecution {
    Moved,
    Blocked,
}

/// Serialized session operations keep snapshots and streamed updates consistent.
pub struct Service {
    outbound: crate::outbound::Pool,
    diagnostics: Option<crate::diagnostics::Diagnostics>,
    pending_pauses: BTreeSet<ActorId>,
    pending_travel_cancellations: BTreeSet<ActorId>,
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
    pub fn new(engine: Engine) -> Self {
        Self::with_outbound_limits(engine, crate::OutboundLimits::default())
            .expect("valid default outbound limits")
    }

    /// Configure host output bounds before any client connects. Limits are not
    /// persisted and are validated before startup.
    pub fn with_outbound_limits(
        mut engine: Engine,
        limits: crate::OutboundLimits,
    ) -> Result<Self, Failure> {
        if !limits.is_valid() {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid outbound byte limits",
            ));
        }
        let mut save_warning = None;
        // Journeys are session-owned and never resume implicitly after restart.
        // Their admitted steps must be terminal before clients can acquire control.
        for actor in engine.queued_travel_actors() {
            engine.cancel_travel(actor)?;
        }
        for actor in engine.queued_human_actors() {
            engine.suspend_queued_intention(actor)?;
        }
        for actor in engine.actors() {
            if let Err(error) = engine.pause_preparation(actor) {
                save_warning = Some(error.to_string());
            }
        }
        Ok(Self {
            outbound: crate::outbound::Pool::new(limits),
            diagnostics: None,
            pending_pauses: BTreeSet::new(),
            pending_travel_cancellations: BTreeSet::new(),
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
        })
    }

    pub(crate) fn set_diagnostics(&mut self, diagnostics: Option<crate::diagnostics::Diagnostics>) {
        self.diagnostics = diagnostics;
    }

    pub(crate) fn connect(
        &mut self,
        account: &Account,
        frontend: String,
    ) -> Result<Connection, Failure> {
        if !valid_label(&frontend) || self.clients.len() >= self.outbound.connection_limit() {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Connection is unavailable",
            ));
        }
        let next = self.next_client.checked_add(1).ok_or_else(|| {
            Failure::new(ErrorCode::InvalidRequest, "Connection identity exhausted")
        })?;
        let id = self.next_client;
        let (messages, receiver) = self
            .outbound
            .channel(QUEUE)
            .map_err(|_| Failure::new(ErrorCode::InvalidRequest, "Connection is unavailable"))?;
        self.next_client = next;
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
                context: None,
                readiness: None,
                sequence: 0,
                observation_tick: 0,
                last_observation: None,
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
        if matches!(result, Ok(ProcessedRequest::SaveQueued)) {
            self.poll_saves();
            return;
        }
        // Publish causing effects and permissions before completing the request.
        // Receipt identity comes from the original operation, never this stream.
        self.refresh_readiness();
        let clients = self.clients.len();
        match result {
            Ok(ProcessedRequest::Reply(reply)) => self.publish_reply(id, request_id, reply),
            Ok(ProcessedRequest::Published) => {}
            Ok(ProcessedRequest::SaveQueued) => unreachable!("handled by persistence"),
            Err(error) => self.send(
                id,
                ServerMessage::Error {
                    scope: self.error_scope(id),
                    request_id: Some(request_id),
                    code: error.code,
                    message: error.message,
                },
            ),
        }
        // Rejected output can remove a controller and change other clients' input.
        if self.clients.len() != clients {
            self.refresh_readiness();
        }
    }

    fn process(
        &mut self,
        id: u64,
        request_id: &str,
        request: Request,
    ) -> Result<ProcessedRequest, Failure> {
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
            client.context = Some(StreamContext {
                stream: StreamId(uuid::Uuid::new_v4().to_string()),
                epoch: 0,
            });
            self.snapshot(id, request_id)?;
            if let Some(message) = self.save_warning.clone() {
                self.send(
                    id,
                    ServerMessage::Error {
                        scope: self.error_scope(id),
                        request_id: None,
                        code: ErrorCode::StorageFailure,
                        message,
                    },
                );
            }
            return Ok(ProcessedRequest::Published);
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
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
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
                Ok(ProcessedRequest::SaveQueued)
            }
            Request::AcquireControl => {
                self.apply_pending_pauses();
                if self.pending_pauses.contains(&actor) {
                    return Err(Failure::new(
                        ErrorCode::StorageFailure,
                        "Pending gameplay suspension could not be saved",
                    ));
                }
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
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
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
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
            }
            Request::Snapshot => self
                .snapshot(id, request_id)
                .map(|()| ProcessedRequest::Published),
            Request::Palette => Ok(self
                .prepare_palette(id, true)
                .map(|palette| ProcessedRequest::Reply(RequestReply::Palette(palette)))
                .unwrap_or(ProcessedRequest::Published)),
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
                Ok(ProcessedRequest::Reply(RequestReply::History(page)))
            }
            Request::History { before, limit } => {
                let page =
                    self.engine
                        .history(actor, &user, before.as_ref(), usize::from(limit))?;
                Ok(ProcessedRequest::Reply(RequestReply::History(page)))
            }
            Request::Command {
                context,
                branch,
                command,
            } => {
                let command = crate::journal::Command::from_wire(&command)?;
                if let Some(previous) = self
                    .engine
                    .retry(&user, actor, request_id, &branch, &command)?
                {
                    return Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                        self.engine.request_receipt(&previous),
                    )));
                }
                // Ownership and availability can change without an observation
                // revision. Resolve receipts first, then reject fresh input built
                // for another attachment, reset or authority generation.
                let current = self.current_readiness(id).ok_or_else(|| {
                    Failure::new(
                        ErrorCode::StaleContext,
                        "Input context exhausted; reconnect",
                    )
                })?;
                let expected = self
                    .input_context(id)
                    .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
                if context != expected
                    || current.permissions.revision != expected.readiness_revision
                {
                    return Err(Failure::new(
                        ErrorCode::StaleContext,
                        "Input context changed; use the current disclosed state",
                    ));
                }
                if matches!(
                    command,
                    crate::journal::Command::RenamePlace { .. }
                        | crate::journal::Command::ResumeIntention { .. }
                        | crate::journal::Command::CancelIntention { .. }
                        | crate::journal::Command::Act { .. }
                        | crate::journal::Command::AdmitIntention { .. }
                        | crate::journal::Command::Travel { .. }
                ) && self.controllers.get(&actor) != Some(&id)
                {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before acting",
                    ));
                }
                // Fresh gameplay must honor the same derived permissions that
                // were published. Simulation still validates the action itself;
                // session policy cannot be bypassed by a correctly stamped request.
                match &command {
                    crate::journal::Command::Act { .. }
                    | crate::journal::Command::AdmitIntention { .. }
                    | crate::journal::Command::Travel { .. }
                        if !current.permissions.admission =>
                    {
                        return Err(Failure::new(
                            ErrorCode::ActorBusy,
                            "New gameplay is unavailable in the current input state",
                        ));
                    }
                    crate::journal::Command::ResumeIntention { admission, .. }
                        if !current
                            .permissions
                            .resume
                            .iter()
                            .any(|id| id.0 == admission.0) =>
                    {
                        return Err(Failure::new(
                            ErrorCode::InvalidAction,
                            "Intention cannot be resumed in the current input state",
                        ));
                    }
                    crate::journal::Command::CancelIntention { admission, .. }
                        if !current
                            .permissions
                            .cancel
                            .iter()
                            .any(|id| id.0 == admission.0) =>
                    {
                        return Err(Failure::new(
                            ErrorCode::InvalidAction,
                            "Intention cannot be cancelled in the current input state",
                        ));
                    }
                    _ => {}
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
                let Some(visible_entry) = result.entry.disclosed() else {
                    if matches!(
                        result.entry.content,
                        crate::journal::JournalContent::IntentionAdmitted { .. }
                            | crate::journal::JournalContent::IntentionChanged {
                                change: crate::journal::IntentionChange::Resumed,
                                ..
                            }
                    ) {
                        self.autonomous_enabled = true;
                    }
                    self.action_result_update(&revisions, &result)?;
                    return Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                        self.engine.request_receipt(&result),
                    )));
                };
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
                                    pending: None,
                                },
                            );
                        }
                        self.travel_update(actor, Some(visible_entry.clone()));
                    }
                    HistoryContent::Wizard { rewind, .. } => {
                        self.autonomous_enabled = false;
                        if rewind {
                            self.pending_pauses.clear();
                            self.pending_travel_cancellations.clear();
                            for actor in self.engine.queued_travel_actors() {
                                self.cancel_travel_work(actor);
                            }
                            for actor in self.engine.queued_human_actors() {
                                if let Err(error) = self.engine.suspend_queued_intention(actor) {
                                    self.pending_pauses.insert(actor);
                                    self.save_warning = Some(error.to_string());
                                }
                            }
                        }
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
                        self.action_result_update(&revisions, &result)?;
                    }
                }
                Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                    self.engine.request_receipt(&result),
                )))
            }
            Request::Attach { .. } => unreachable!("handled before attachment lookup"),
        }
    }

    fn action_result_update(
        &mut self,
        revisions: &BTreeMap<ActorId, u64>,
        result: &crate::CommandResult,
    ) -> Result<(), Failure> {
        let entry = result.entry.disclosed();
        self.action_update(revisions, entry.as_ref())?;
        self.intention_update(result);
        Ok(())
    }

    fn intention_update(&mut self, result: &crate::CommandResult) {
        for status in self.engine.intention_updates(result) {
            let recipients: Vec<_> = self
                .clients
                .iter()
                .filter(|(_, client)| client.actor == Some(status.actor))
                .map(|(&id, _)| id)
                .collect();
            for recipient in recipients {
                self.update(
                    recipient,
                    UpdateBody::Intention {
                        status: status.clone(),
                    },
                );
            }
        }
    }

    /// Reuse only an equal disclosure for the same actor and branch. Stream
    /// cursors and reset contexts remain owned by each connection.
    fn share_observation(&self, state: StateView) -> Arc<StateView> {
        self.clients
            .values()
            .filter(|client| client.actor == Some(state.observation.actor))
            .filter_map(|client| client.last_observation.as_ref())
            .find(|disclosed| {
                &disclosed.branch == self.engine.branch() && disclosed.state.as_ref() == &state
            })
            .map(|disclosed| Arc::clone(&disclosed.state))
            .unwrap_or_else(|| Arc::new(state))
    }

    fn action_update(
        &mut self,
        revisions: &BTreeMap<ActorId, u64>,
        entry: Option<&HistoryEntry>,
    ) -> Result<(), Failure> {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter_map(|(&id, c)| c.actor.map(|a| (id, a)))
            .filter(|(_, actor)| self.engine.revision(*actor).ok() != revisions.get(actor).copied())
            .collect();
        // Disclosed state belongs to an actor at this boundary, not to a
        // connection. Keep stream bases and sequencing per client, but resolve
        // perception only once for everyone watching the same actor.
        let states: BTreeMap<_, _> = recipients
            .iter()
            .map(|(_, actor)| *actor)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|actor| {
                (
                    actor,
                    self.engine
                        .state(actor)
                        .ok()
                        .map(|state| self.share_observation(state)),
                )
            })
            .collect();
        for (recipient, observer) in recipients {
            // Region streaming can unload an actor nobody keeps in play (an
            // AI actor a spectator watches); its clients must reattach.
            let Some(state) = states[&observer].as_ref() else {
                self.detach_unloaded(recipient);
                continue;
            };
            if revisions.get(&observer) != Some(&state.revision) {
                self.update(
                    recipient,
                    UpdateBody::Observation {
                        state: Arc::clone(state),
                        event: entry
                            .filter(|entry| observer == entry.actor)
                            .map(|entry| Box::new(entry.clone())),
                    },
                );
            }
            self.palette_update(recipient);
        }
        Ok(())
    }

    /// Send a client its actor's palette: the whole palette when asked, or
    /// the first time; otherwise the changes, if any, as a delta. Nothing is
    /// acknowledged. Scenarios that name no assets send no palettes, but a
    /// palette request still gets an (empty) answer.
    fn prepare_palette(&mut self, id: u64, full: bool) -> Option<PaletteUpdate> {
        let client = self.clients.get(&id)?;
        let actor = client.actor?;
        // A palette depends only on the actor's region: skip recomputing it
        // while that stays the same.
        let region = self.engine.palette_region(actor);
        if !full && client.palette.is_some() && client.palette_region == region {
            return None;
        }
        let assets = match self.engine.palette(actor) {
            Some(assets) => assets,
            None if full => BTreeSet::new(),
            None => return None,
        };
        let client = self.clients.get_mut(&id).expect("connected client");
        if !full
            && client
                .palette
                .as_ref()
                .is_some_and(|(_, previous)| *previous == assets)
        {
            client.palette_region = region;
            return None;
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
        Some(PaletteUpdate { revision, body })
    }

    fn palette_update(&mut self, id: u64) {
        if let Some(palette) = self.prepare_palette(id, false) {
            if let Some(context) = self.reply_context(id) {
                self.send(
                    id,
                    ServerMessage::Palette {
                        context,
                        request_id: None,
                        palette,
                    },
                );
            }
        }
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
        self.cancel_travel_work(actor);
        if let Some(status) = self.travel_status.get_mut(&actor) {
            if status.phase == TravelPhase::Active {
                status.phase = phase;
                if !self.broadcasting_travel {
                    self.travel_update(actor, None);
                }
            }
        }
    }

    fn cancel_travel_work(&mut self, actor: ActorId) {
        match self.engine.cancel_travel(actor) {
            Ok(_) => {
                self.pending_travel_cancellations.remove(&actor);
            }
            Err(error) => {
                self.pending_travel_cancellations.insert(actor);
                self.save_warning = Some(error.to_string());
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
            match self.engine.suspend_queued_intention(actor) {
                Ok(Some(result)) => {
                    self.intention_update(&result);
                }
                Ok(None) => {}
                Err(error) => {
                    self.pending_pauses.insert(actor);
                    self.save_warning = Some(error.to_string());
                    continue;
                }
            }
            match self.engine.pause_preparation(actor) {
                Ok(Some(result)) => {
                    let _ = self.action_result_update(&revisions, &result);
                }
                Ok(None) => {}
                Err(error) => {
                    self.pending_pauses.insert(actor);
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
        let step = self.step_inner();
        self.refresh_readiness();
        step
    }

    fn step_inner(&mut self) -> Step {
        for actor in std::mem::take(&mut self.pending_travel_cancellations) {
            self.cancel_travel_work(actor);
        }
        self.apply_pending_pauses();
        if !self.pending_pauses.is_empty() || !self.pending_travel_cancellations.is_empty() {
            return Step::Blocked;
        }
        let full: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| client.actor.is_some() && !client.messages.has_headroom(HEADROOM))
            .map(|(&id, client)| (id, client.messages.clone()))
            .collect();
        if !full.is_empty() {
            return Step::Full(full);
        }
        if let Some(next) = self.engine.next_intention_actor() {
            return self.execute_queued_intention(next);
        }
        let Some(next) = self.engine.next_actor() else {
            self.announce_waiting();
            return Step::Blocked;
        };
        if self.travels.contains_key(&next) {
            return self.travel_step(next);
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
        self.admit_ai(next);
        match self.engine.next_intention_actor() {
            Some(next) => self.execute_queued_intention(next),
            None => Step::Blocked,
        }
    }

    /// All due work uses this path, including an AI decision just admitted above.
    fn execute_queued_intention(&mut self, next: ActorId) -> Step {
        let allowed = match self.engine.next_intention_origin() {
            Some(tor_simulation::IntentionOrigin::Human) => {
                !self.engine.alive(next) || self.controllers.contains_key(&next)
            }
            Some(tor_simulation::IntentionOrigin::Autonomous) => {
                self.autonomous_enabled
                    && self
                        .controllers
                        .keys()
                        .any(|&actor| self.engine.alive(actor))
            }
            Some(tor_simulation::IntentionOrigin::Travel) => {
                let linked = self
                    .travels
                    .get(&next)
                    .and_then(|job| job.pending.as_ref())
                    .is_some_and(|pending| {
                        self.engine.queued_travel_admission(next) == Some(pending)
                    });
                if !linked {
                    self.stop_travel(next, TravelPhase::Failed);
                    return Step::Progress;
                }
                if !self.travel_ready(next) {
                    return if self.pending_travel_cancellations.is_empty() {
                        Step::Progress
                    } else {
                        Step::Blocked
                    };
                }
                true
            }
            None => false,
        };
        if !allowed {
            self.announce_waiting();
            return Step::Blocked;
        }
        let revisions = self
            .engine
            .actors()
            .into_iter()
            .map(|actor| Ok((actor, self.engine.revision(actor)?)))
            .collect::<Result<BTreeMap<_, _>, Failure>>();
        match revisions.and_then(|revisions| {
            self.engine
                .execute_next_intention()
                .map(|result| (revisions, result))
        }) {
            Ok((revisions, Some(result))) => {
                // Account committed movement before output can disconnect its owner.
                let travel = self.commit_travel_progress(next, &result);
                if let Err(error) = self.action_result_update(&revisions, &result) {
                    self.save_warning = Some(error.to_string());
                }
                if let Some(outcome) = travel {
                    self.finish_travel_execution(next, outcome);
                }
                return Step::Progress;
            }
            Ok((_, None)) => {}
            Err(error) => self.save_warning = Some(error.to_string()),
        }
        self.announce_waiting();
        Step::Blocked
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

    fn travel_observation(&self, actor: ActorId) -> Result<Observation, TravelPhase> {
        let job = self.travels.get(&actor).ok_or(TravelPhase::Failed)?;
        if self.controllers.get(&actor) != Some(&job.owner) {
            return Err(TravelPhase::ControlLost);
        }
        self.engine
            .observation(actor)
            .map_err(|_| TravelPhase::Failed)
    }

    /// Revalidate session authority and disclosed hazards after each mailbox pass.
    fn travel_ready(&mut self, actor: ActorId) -> bool {
        let phase = match self.travel_observation(actor) {
            Err(phase) => Some(phase),
            Ok(observation) => self
                .travels
                .get(&actor)
                .filter(|job| job.hazard_in(&observation))
                .map(|_| TravelPhase::Hazard),
        };
        if let Some(phase) = phase {
            self.stop_travel(actor, phase);
            return false;
        }
        true
    }

    /// Admit only; the next scheduler boundary executes this original step.
    fn travel_step(&mut self, actor: ActorId) -> Step {
        if !self.travel_ready(actor) {
            return Step::Progress;
        }
        let job = &self.travels[&actor];
        let Some(step) = job.steps.front().copied() else {
            self.stop_travel(actor, TravelPhase::Arrived);
            return Step::Progress;
        };
        let status = &self.travel_status[&actor];
        let Some(ordinal) = status.completed_steps.checked_add(1) else {
            self.stop_travel(actor, TravelPhase::Failed);
            return Step::Progress;
        };
        match self
            .engine
            .admit_travel(actor, &status.id.clone(), ordinal, step)
        {
            Ok(result) => {
                self.travels
                    .get_mut(&actor)
                    .expect("admitted journey")
                    .pending = Some(result.entry.id);
                Step::Progress
            }
            Err(error) if error.code == ErrorCode::StorageFailure => {
                self.save_warning = Some(error.to_string());
                Step::Blocked
            }
            Err(error) => {
                self.stop_travel(
                    actor,
                    if error.code == ErrorCode::InvalidAction {
                        TravelPhase::Blocked
                    } else {
                        TravelPhase::Failed
                    },
                );
                Step::Progress
            }
        }
    }

    /// The route advances only when its exact admitted step has committed.
    fn commit_travel_progress(
        &mut self,
        actor: ActorId,
        result: &crate::CommandResult,
    ) -> Option<TravelExecution> {
        let job = self.travels.get_mut(&actor)?;
        let (admission, outcome) = match &result.entry.content {
            crate::journal::JournalContent::IntentionStarted { admission, .. } => {
                (admission, TravelExecution::Moved)
            }
            crate::journal::JournalContent::IntentionFailed { admission, .. } => {
                (admission, TravelExecution::Blocked)
            }
            _ => return None,
        };
        if job.pending.as_ref() != Some(admission) {
            return None;
        }
        job.pending = None;
        if matches!(outcome, TravelExecution::Moved) {
            job.steps.pop_front().expect("admitted route step");
            self.travel_status
                .get_mut(&actor)
                .expect("active journey status")
                .completed_steps += 1;
        }
        Some(outcome)
    }

    fn finish_travel_execution(&mut self, actor: ActorId, outcome: TravelExecution) {
        // Output may already have disconnected the controller and ended its job.
        let Some(job) = self.travels.get(&actor) else {
            return;
        };
        if matches!(outcome, TravelExecution::Blocked) {
            self.stop_travel(actor, TravelPhase::Blocked);
            return;
        }
        let observation = match self.travel_observation(actor) {
            Ok(observation) => observation,
            Err(phase) => {
                self.stop_travel(actor, phase);
                return;
            }
        };
        let phase = if observation
            .motion
            .as_ref()
            .is_some_and(|motion| motion.displaced || motion.impacted)
        {
            Some(TravelPhase::DecisionRequired)
        } else if job.steps.is_empty() {
            Some(TravelPhase::Arrived)
        } else if job.hazard_in(&observation) {
            Some(TravelPhase::Hazard)
        } else {
            None
        };
        match phase {
            Some(phase) => self.stop_travel(actor, phase),
            None => self.travel_update(actor, None),
        }
    }

    fn admit_ai(&mut self, actor: ActorId) {
        let revisions = self
            .engine
            .actors()
            .into_iter()
            .map(|id| (id, self.engine.revision(id).unwrap()))
            .collect();
        let result = self.engine.admit_ai(actor);
        match result {
            Ok(result) => {
                let _ = self.action_result_update(&revisions, &result);
            }
            Err(error) => {
                self.autonomous_enabled = false;
                for client in self.controllers.values().copied().collect::<Vec<_>>() {
                    self.send(
                        client,
                        ServerMessage::Error {
                            scope: self.error_scope(client),
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
        let context = match client.context.as_ref().and_then(StreamContext::next_reset) {
            Some(context) => context,
            None => {
                self.disconnect(id);
                return Err(Failure::new(
                    ErrorCode::InvalidRequest,
                    "Stream exhausted; reconnect",
                ));
            }
        };
        let state = self.engine.state(actor)?;
        let readiness = match self.current_readiness(id) {
            Some(readiness) => readiness,
            None => {
                self.disconnect(id);
                return Err(Failure::new(
                    ErrorCode::InvalidRequest,
                    "Readiness exhausted; reconnect",
                ));
            }
        };
        let snapshot = Snapshot {
            context: context.clone(),
            readiness: readiness.permissions.clone(),
            intentions: self.engine.pending_intentions(actor),
            travel: self.travel_status.get(&actor).cloned(),
            actor,
            branch: self.engine.branch().clone(),
            cursor: StreamCursor {
                sequence: client.sequence,
                tick: state.observation.tick,
            },
            state: self.share_observation(state),
            has_control: readiness.has_control,
            history: self
                .engine
                .history(actor, &client.user, None, MAX_HISTORY_PAGE)?,
        };
        let message = ServerMessage::Snapshot {
            request_id: request_id.into(),
            snapshot: Box::new(snapshot),
        };
        if !self.admit_output(id, &message) {
            // Output rejection closes the connection; it is not a request error.
            return Ok(());
        }
        let ServerMessage::Snapshot { snapshot, .. } = message else {
            unreachable!()
        };
        let client = self.clients.get_mut(&id).expect("admitted client");
        client.context = Some(context);
        client.readiness = Some(readiness);
        client.waiting = None;
        client.observation_tick = snapshot.state.observation.tick;
        client.last_observation = Some(DisclosedObservation {
            branch: snapshot.branch,
            base: ObservationBase {
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            },
            state: snapshot.state,
        });
        // Attaching sends the whole palette; a later snapshot, what changed.
        self.palette_update(id);
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
        let entry = entry.disclosed().ok_or_else(|| {
            Failure::new(
                ErrorCode::InvalidRequest,
                "Backend note has no disclosed event",
            )
        })?;
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

    fn input_context(&self, id: u64) -> Option<InputContext> {
        let client = self.clients.get(&id)?;
        Some(InputContext {
            stream: client.context.clone()?,
            readiness_revision: client.readiness.as_ref()?.permissions.revision,
        })
    }

    fn current_readiness(&self, id: u64) -> Option<DisclosedReadiness> {
        let actor = self.clients.get(&id)?.actor?;
        self.readiness_for_client(id, self.engine.input_readiness(actor))
    }

    fn readiness_for_client(
        &self,
        id: u64,
        mut permissions: Readiness,
    ) -> Option<DisclosedReadiness> {
        let client = self.clients.get(&id)?;
        let actor = client.actor?;
        let has_control =
            client.role != AccessRole::Spectator && self.controllers.get(&actor) == Some(&id);
        let enabled = has_control
            && self.pending_pauses.is_empty()
            && self.pending_travel_cancellations.is_empty();
        if !enabled {
            permissions.admission = false;
            permissions.resume.clear();
            permissions.cancel.clear();
        } else if self.travels.contains_key(&actor) {
            permissions.admission = false;
        }
        let mut readiness = DisclosedReadiness {
            has_control,
            permissions,
        };
        if let Some(previous) = &client.readiness {
            readiness.permissions.revision = previous.permissions.revision;
            if &readiness != previous {
                readiness.permissions.revision = previous.permissions.revision.checked_add(1)?;
            }
        }
        Some(readiness)
    }

    /// Query each actor once per publication pass. Output rejection can remove a
    /// controller and affect clients visited earlier; only such removals require
    /// another pass. The client count strictly decreases on every repeated pass.
    fn refresh_readiness(&mut self) {
        loop {
            let count = self.clients.len();
            let recipients: Vec<_> = self
                .clients
                .iter()
                .filter_map(|(&id, client)| {
                    client.readiness.as_ref()?;
                    Some((id, client.actor?))
                })
                .collect();
            let mut inputs = BTreeMap::new();
            for (id, actor) in recipients {
                if !self.clients.contains_key(&id) {
                    continue;
                }
                let input = inputs
                    .entry(actor)
                    .or_insert_with(|| self.engine.input_readiness(actor))
                    .clone();
                let Some(readiness) = self.readiness_for_client(id, input) else {
                    self.disconnect(id);
                    continue;
                };
                let client = self.clients.get_mut(&id).expect("connected recipient");
                if client.readiness.as_ref() == Some(&readiness) {
                    continue;
                }
                client.readiness = Some(readiness.clone());
                self.update(
                    id,
                    UpdateBody::Readiness {
                        readiness: readiness.permissions,
                    },
                );
            }
            if self.clients.len() == count {
                break;
            }
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
        let branch = self.engine.branch().clone();
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        let Some(sequence) = client.sequence.checked_add(1) else {
            self.disconnect(id);
            return;
        };
        let actor = client.actor.expect("only attached clients receive updates");
        // Control changes during a broadcast retain the last admitted tick.
        let tick = match &body {
            UpdateBody::Observation { state, .. } => state.observation.tick,
            _ => client.observation_tick,
        };
        let previous = client
            .last_observation
            .as_ref()
            .filter(|previous| previous.branch == branch)
            .map(|previous| (previous.base, previous.state.as_ref()));
        let message = ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: client.context.clone().expect("attached stream context"),
                actor,
                branch: branch.clone(),
                cursor: StreamCursor { sequence, tick },
                body,
            }),
        };
        if client
            .messages
            .try_send_with(&message, |message, limit| {
                encode_response(message, previous, limit).map(|encoded| encoded.text)
            })
            .is_err()
        {
            self.disconnect(id);
            return;
        }
        // Only an admitted output establishes the next disclosure base.
        client.sequence = sequence;
        client.observation_tick = tick;
        client.waiting = None;
        let ServerMessage::Update { update } = message else {
            unreachable!()
        };
        if let UpdateBody::Observation { state, .. } = update.body {
            client.last_observation = Some(DisclosedObservation {
                branch,
                base: ObservationBase {
                    cursor: StreamCursor { sequence, tick },
                    revision: state.revision,
                },
                state,
            });
        }
    }

    fn reply_context(&self, id: u64) -> Option<ReplyContext> {
        let client = self.clients.get(&id)?;
        let observed = client.last_observation.as_ref()?;
        Some(ReplyContext {
            input: self.input_context(id)?,
            actor: client.actor?,
            branch: observed.branch.clone(),
            cursor: StreamCursor {
                sequence: client.sequence,
                tick: client.observation_tick,
            },
            revision: observed.state.revision,
        })
    }

    fn publish_reply(&mut self, id: u64, request_id: String, reply: RequestReply) {
        if let Some(context) = self.reply_context(id) {
            self.send(id, reply.message(request_id, context));
        }
    }

    fn error_scope(&self, id: u64) -> ErrorScope {
        match self.reply_context(id) {
            Some(context) => ErrorScope::Attached { context },
            None => ErrorScope::Unattached {},
        }
    }

    fn session_receipt(&self, id: u64) -> Option<RequestReceipt> {
        let actor = self.clients.get(&id)?.actor?;
        Some(RequestReceipt::Immediate {
            actor,
            branch: self.engine.branch().clone(),
            entry_id: None,
        })
    }

    /// Admit an ordinary response before the caller commits disclosure state.
    /// Output failure ends this stream; it must never silently skip a response.
    fn admit_output(&mut self, id: u64, message: &ServerMessage) -> bool {
        let Some(client) = self.clients.get(&id) else {
            return false;
        };
        if client
            .messages
            .try_send_with(message, encode_bounded_json)
            .is_err()
        {
            self.disconnect(id);
            return false;
        }
        true
    }

    fn send(&mut self, id: u64, message: ServerMessage) {
        if !self.admit_output(id, &message) {
            return;
        }
        // A successfully admitted answer or disclosure needs a fresh waiting word.
        if matches!(
            message,
            ServerMessage::Update { .. }
                | ServerMessage::Snapshot { .. }
                | ServerMessage::Ack { .. }
        ) {
            self.clients.get_mut(&id).expect("admitted client").waiting = None;
        }
    }

    /// Tell a client its actor left the loaded world, then disconnect it.
    fn detach_unloaded(&mut self, id: u64) {
        self.send(
            id,
            ServerMessage::Error {
                scope: self.error_scope(id),
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
        let clients_before_warnings = self.clients.len();
        let status = self.engine.save_status();
        let warning = status.error.clone().or_else(|| {
            status.overdue.then(|| {
                "Saving is behind schedule; recent play may be lost if the server stops.".into()
            })
        });
        if warning != self.save_warning {
            if let Some(message) = &warning {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.warning(message);
                }
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
                            scope: self.error_scope(id),
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
        if pending.is_empty() {
            if self.clients.len() != clients_before_warnings {
                self.refresh_readiness();
            }
            return;
        }
        self.refresh_readiness();
        let clients = self.clients.len();
        for (id, request_id, target) in pending {
            if !self.clients.contains_key(&id) {
                continue;
            }
            if status.durable_sequence >= target {
                if let Some(receipt) = self.session_receipt(id) {
                    self.publish_reply(id, request_id, RequestReply::Receipt(receipt));
                }
            } else if let Some(message) = &status.error {
                self.send(
                    id,
                    ServerMessage::Error {
                        scope: self.error_scope(id),
                        request_id: Some(request_id),
                        code: ErrorCode::StorageFailure,
                        message: message.clone(),
                    },
                );
            } else {
                self.pending_saves.push((id, request_id, target));
            }
        }
        if self.clients.len() != clients {
            self.refresh_readiness();
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
    use tokio::sync::mpsc;

    #[test]
    fn narrowing_a_view_chooses_the_smaller_complete_encoded_update() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "encoding-observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv_frame().is_ok() {}
        let mut base = service.engine.state(actor).unwrap();
        base.revision += 1;
        base.observation.tick += 1;
        base.observation.visible_cells.truncate(5);
        assert_eq!(base.observation.visible_cells.len(), 5);
        base.observation.visible_cells[0].door = Some(DoorView {
            asset: None,
            id: 77,
            name: "visible door".into(),
            description: "An already disclosed elaborate carving. ".repeat(128),
            open: false,
            reachable: false,
            approaches: Vec::new(),
        });
        base.validate().unwrap();
        service.update(
            client.id,
            UpdateBody::Observation {
                state: base.clone().into(),
                event: None,
            },
        );
        while client.messages.try_recv_frame().is_ok() {}
        let disclosed = service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap();
        let previous = disclosed.base;
        assert_eq!(*disclosed.state, base);
        let mut next = base.clone();
        next.revision += 1;
        next.observation.tick += 1;
        next.observation.visible_cells.truncate(1);
        next.validate().unwrap();
        // A concrete valid delta is a witness that counting removed cells is
        // insufficient: the retained cell holds most of the full payload.
        let o = &next.observation;
        let witness = StateDelta {
            base_revision: base.revision,
            wizard_game: next.wizard_game,
            revision: next.revision,
            combat: o.combat.clone(),
            motion: o.motion.clone(),
            places: Vec::new(),
            actor: o.actor,
            tick: o.tick,
            position: o.position,
            cells: CellChanges {
                shift: Position { x: 0, y: 0, z: 0 },
                removed: base.observation.visible_cells[1..]
                    .iter()
                    .map(|cell| cell.position)
                    .collect(),
                changed: Vec::new(),
            },
            ground_items: Vec::new(),
            inventory: Vec::new(),
            visible_actors: Vec::new(),
            ready: o.ready,
        };
        assert_eq!(witness.clone().apply(&base).unwrap(), next);
        let client_state = &service.clients[&client.id];
        let envelope = |body| ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: client_state.context.clone().unwrap(),
                actor,
                branch: service.engine.branch().clone(),
                cursor: StreamCursor {
                    sequence: client_state.sequence + 1,
                    tick: next.observation.tick,
                },
                body,
            }),
        };
        let full_bytes = serde_json::to_vec(&envelope(UpdateBody::Observation {
            state: next.clone().into(),
            event: None,
        }))
        .unwrap()
        .len();
        let delta_bytes = serde_json::to_vec(&envelope(UpdateBody::ObservationDelta {
            base: previous,
            state: Box::new(witness),
            event: None,
        }))
        .unwrap()
        .len();
        assert!(
            delta_bytes < full_bytes,
            "the witness must actually save bytes"
        );
        service.update(
            client.id,
            UpdateBody::Observation {
                state: next.clone().into(),
                event: None,
            },
        );
        let frame = client.messages.try_recv_frame().unwrap();
        let ServerMessage::Update { update } = decode_response(&frame.text).unwrap() else {
            panic!("expected actual encoded observation");
        };
        let restored = match update.body {
            UpdateBody::Observation { state, .. } => Arc::unwrap_or_clone(state),
            UpdateBody::ObservationDelta {
                base: actual_base,
                state,
                ..
            } => {
                assert_eq!(actual_base, previous);
                state.apply(&base).unwrap()
            }
            _ => panic!("expected observation update"),
        };
        assert_eq!(restored, next);
        assert!(frame.text.len() <= delta_bytes,
            "actual server chose {} bytes, but a complete equivalent delta needs {delta_bytes} (full {full_bytes})",
            frame.text.len());
    }

    #[test]
    fn published_disabled_admission_rejects_fresh_gameplay_without_mutation() {
        for pause in [false, true] {
            for command in [
                Command::Act {
                    expected_revision: 0,
                    action: Action::Wait,
                },
                Command::Travel {
                    expected_revision: 0,
                    destination: "other-room".into(),
                },
            ] {
                let actor = ActorId(1);
                let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
                let account = Account {
                    role: AccessRole::Player,
                    user: "p".into(),
                    token: "test".into(),
                    actors: BTreeSet::from([actor]),
                };
                let mut client = service.connect(&account, "headless".into()).unwrap();
                service.handle(client.id, "attach".into(), Request::Attach { actor });
                service.handle(client.id, "control".into(), Request::AcquireControl);
                // Another actor's unsettled host work masks gameplay globally.
                if pause {
                    service.pending_pauses.insert(ActorId(2));
                } else {
                    service.pending_travel_cancellations.insert(ActorId(2));
                }
                service.refresh_readiness();
                while client.messages.try_recv().is_ok() {}
                assert!(
                    !service.clients[&client.id]
                        .readiness
                        .as_ref()
                        .unwrap()
                        .permissions
                        .admission
                );
                let before = service.engine.state(actor).unwrap();
                let branch = service.engine.branch().clone();
                service.handle(
                    client.id,
                    "disabled".into(),
                    Request::Command {
                        context: service.input_context(client.id).unwrap(),
                        branch: branch.clone(),
                        command,
                    },
                );
                let messages: Vec<_> =
                    std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
                assert!(
                    messages.iter().any(|message| matches!(message,
                    ServerMessage::Error { request_id: Some(id), code: ErrorCode::ActorBusy, .. }
                    if id == "disabled")),
                    "disabled admission must reject: {messages:?}"
                );
                assert!(!messages
                    .iter()
                    .any(|message| matches!(message, ServerMessage::Ack { .. })));
                assert!(!service.engine.has_pending_intention(actor));
                assert!(!service.travels.contains_key(&actor));
                assert_eq!(service.engine.state(actor).unwrap(), before);
                assert_eq!(service.engine.branch(), &branch);
            }
        }
    }

    #[test]
    fn published_disabled_recovery_rejects_resume_and_cancel_without_mutation() {
        for resume in [false, true] {
            let actor = ActorId(1);
            let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
            let admitted = engine
                .command(
                    "p",
                    "test",
                    actor,
                    "original",
                    &engine.branch().clone(),
                    crate::journal::Command::AdmitIntention {
                        expected_revision: 0,
                        action: Action::Wait,
                    },
                )
                .unwrap();
            let mut service = Service::new(engine);
            let account = Account {
                role: AccessRole::Player,
                user: "p".into(),
                token: "test".into(),
                actors: BTreeSet::from([actor]),
            };
            let mut client = service.connect(&account, "headless".into()).unwrap();
            service.handle(client.id, "attach".into(), Request::Attach { actor });
            service.handle(client.id, "control".into(), Request::AcquireControl);
            let intention = IntentionId(admitted.entry.id.0.clone());
            let permissions = &service.clients[&client.id]
                .readiness
                .as_ref()
                .unwrap()
                .permissions;
            assert!(permissions.resume.contains(&intention));
            assert!(permissions.cancel.contains(&intention));
            service.pending_pauses.insert(ActorId(2));
            service.refresh_readiness();
            while client.messages.try_recv().is_ok() {}
            let before = service.engine.state(actor).unwrap();
            let pending = service.engine.pending_intentions(actor);
            let command = if resume {
                Command::ResumeIntention {
                    expected_revision: 0,
                    intention,
                }
            } else {
                Command::CancelIntention {
                    expected_revision: 0,
                    intention,
                }
            };
            service.handle(
                client.id,
                "disabled".into(),
                Request::Command {
                    context: service.input_context(client.id).unwrap(),
                    branch: service.engine.branch().clone(),
                    command: command.clone(),
                },
            );
            let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            assert!(
                messages.iter().any(|message| matches!(message,
                ServerMessage::Error { request_id: Some(id), code: ErrorCode::InvalidAction, .. }
                if id == "disabled")),
                "disabled recovery must reject: {messages:?}"
            );
            assert!(!messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Ack { .. })));
            assert_eq!(service.engine.state(actor).unwrap(), before);
            assert_eq!(service.engine.pending_intentions(actor), pending);
            // The rejection leaves the same work usable after policy settles.
            service.pending_pauses.clear();
            service.refresh_readiness();
            while client.messages.try_recv().is_ok() {}
            service.handle(
                client.id,
                "enabled".into(),
                Request::Command {
                    context: service.input_context(client.id).unwrap(),
                    branch: service.engine.branch().clone(),
                    command,
                },
            );
            let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            assert!(
                messages.iter().any(|message| matches!(message,
                ServerMessage::Ack { request_id, .. } if request_id == "enabled")),
                "enabled recovery must succeed: {messages:?}"
            );
            assert_eq!(service.engine.state(actor).unwrap(), before);
        }
    }

    #[test]
    fn query_replies_follow_changed_permissions_without_running_simulation() {
        for request in [
            Request::History {
                before: None,
                limit: 8,
            },
            Request::Palette,
        ] {
            let actor = ActorId(1);
            let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
            let account = Account {
                role: AccessRole::Player,
                user: "p".into(),
                token: "test".into(),
                actors: BTreeSet::from([actor]),
            };
            let mut client = service.connect(&account, "headless".into()).unwrap();
            while client.messages.try_recv().is_ok() {}
            service.handle(client.id, "attach".into(), Request::Attach { actor });
            while client.messages.try_recv().is_ok() {}
            service.handle(client.id, "control".into(), Request::AcquireControl);
            while client.messages.try_recv().is_ok() {}
            let before = service.engine.state(actor).unwrap();
            // A suspension awaiting persistence disables new input globally.
            service.pending_pauses.insert(actor);
            service.handle(client.id, "query".into(), request);
            let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            let readiness = messages
                .iter()
                .position(|message| {
                    matches!(message,
                ServerMessage::Update { update } if matches!(&update.body,
                    UpdateBody::Readiness { readiness } if !readiness.admission))
                })
                .unwrap();
            let reply = messages.iter().position(|message| matches!(message,
                ServerMessage::History { request_id, .. } if request_id == "query") || matches!(message,
                ServerMessage::Palette { request_id: Some(request_id), .. } if request_id == "query")).unwrap();
            assert!(
                readiness < reply,
                "query completion must follow current permissions: {messages:?}"
            );
            let ServerMessage::Update { update } = &messages[readiness] else {
                unreachable!()
            };
            let context = match &messages[reply] {
                ServerMessage::History { context, .. } | ServerMessage::Palette { context, .. } => {
                    context
                }
                _ => unreachable!(),
            };
            let UpdateBody::Readiness { readiness } = &update.body else {
                unreachable!()
            };
            assert_eq!(context.input.stream, update.context);
            assert_eq!(context.input.readiness_revision, readiness.revision);
            assert_eq!(context.actor, actor);
            assert_eq!(context.branch, update.branch);
            assert_eq!(context.cursor, update.cursor);
            assert_eq!(context.revision, before.revision);
            assert_eq!(service.engine.state(actor).unwrap(), before);
        }
    }

    #[test]
    fn rejected_reply_disconnection_publishes_permissions_to_surviving_controllers() {
        let mut service =
            Service::new(Engine::memory(Scenario::performance(42, 4, 2).unwrap()).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([ActorId(1), ActorId(2)]),
        };
        let mut clients = Vec::new();
        for actor in [ActorId(1), ActorId(2)] {
            let mut client = service.connect(&account, "headless".into()).unwrap();
            while client.messages.try_recv().is_ok() {}
            service.handle(client.id, "attach".into(), Request::Attach { actor });
            while client.messages.try_recv().is_ok() {}
            service.handle(client.id, "control".into(), Request::AcquireControl);
            while client.messages.try_recv().is_ok() {}
            clients.push(client);
        }
        let slow = clients[0].id;
        let survivor = clients[1].id;
        assert!(
            service.clients[&survivor]
                .readiness
                .as_ref()
                .unwrap()
                .permissions
                .admission
        );
        for _ in 0..QUEUE {
            service.send(slow, ServerMessage::Waiting { on: Waiting::You });
        }
        assert!(service.clients.contains_key(&slow));
        service.handle(slow, String::new(), Request::Continue);
        assert!(!service.clients.contains_key(&slow));
        let messages: Vec<_> = std::iter::from_fn(|| clients[1].messages.try_recv().ok()).collect();
        assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Update { update } if matches!(&update.body,
                UpdateBody::Readiness { readiness } if !readiness.admission))),
            "controller removal must publish disabled input before the next simulation step: {messages:?}");
    }

    #[test]
    fn readiness_tracks_control_and_queue_lifecycle_without_changing_observation_bases() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        let snapshot = loop {
            if let ServerMessage::Snapshot { snapshot, .. } = client.messages.try_recv().unwrap() {
                break *snapshot;
            }
        };
        assert_eq!(snapshot.readiness.revision, 0);
        assert!(!snapshot.readiness.admission);
        let mut model = tor_client_common::ClientState::from_snapshot(snapshot).unwrap();
        while client.messages.try_recv().is_ok() {}
        let initial_base = model.observation_base();
        for (request_id, request, expected_revision, admission) in [
            ("acquire", Request::AcquireControl, 1, true),
            ("release", Request::ReleaseControl, 2, false),
            ("reacquire", Request::AcquireControl, 3, true),
        ] {
            service.handle(client.id, request_id.into(), request);
            let mut readiness_seen = false;
            while let Ok(message) = client.messages.try_recv() {
                match message {
                    ServerMessage::Update { update } => {
                        readiness_seen |= matches!(update.body, UpdateBody::Readiness { .. });
                        model.apply(*update).unwrap();
                    }
                    ServerMessage::Ack {
                        context,
                        request_id: answered,
                        ..
                    } => {
                        model.validate_reply_context(&context).unwrap();
                        assert_eq!(answered, request_id);
                        assert!(readiness_seen, "permissions must precede acknowledgement");
                    }
                    _ => {}
                }
            }
            assert_eq!(model.readiness().revision, expected_revision);
            assert_eq!(model.readiness().admission, admission);
            assert_eq!(model.observation_base(), initial_base);
        }
        service.handle(
            client.id,
            "act".into(),
            Request::Command {
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    expected_revision: 0,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            },
        );
        while let Ok(message) = client.messages.try_recv() {
            if let ServerMessage::Update { update } = message {
                model.apply(*update).unwrap();
            }
        }
        assert_eq!(model.readiness().revision, 4);
        assert!(!model.readiness().admission);
        assert_eq!(
            model.readiness().cancel,
            vec![model.intentions()[0].intention.clone()]
        );
        assert_eq!(model.observation_base(), initial_base);
        assert!(matches!(service.step(), Step::Progress));
        while let Ok(message) = client.messages.try_recv() {
            if let ServerMessage::Update { update } = message {
                model.apply(*update).unwrap();
            }
        }
        assert_eq!(model.readiness().revision, 5);
        assert!(model.readiness().admission);
        assert!(model.readiness().cancel.is_empty());
        assert!(model.observation_base().cursor.sequence > initial_base.cursor.sequence);
    }

    #[test]
    fn a_new_command_cannot_reuse_authority_from_before_control_loss_or_snapshot_reset() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        service.handle(client.id, "acquire".into(), Request::AcquireControl);
        let before = service.engine.state(actor).unwrap();
        let old_authority = service.input_context(client.id).unwrap();
        service.handle(client.id, "release".into(), Request::ReleaseControl);
        service.handle(client.id, "reacquire".into(), Request::AcquireControl);
        let old_epoch = service.input_context(client.id).unwrap();
        service.handle(client.id, "reset".into(), Request::Snapshot);
        let current = service.input_context(client.id).unwrap();
        let mut foreign_stream = current.clone();
        foreign_stream.stream.stream = StreamId("another-attachment".into());
        while client.messages.try_recv().is_ok() {}
        for (index, context) in [old_authority, old_epoch, foreign_stream]
            .into_iter()
            .enumerate()
        {
            let request_id = format!("stale-{index}");
            service.handle(
                client.id,
                request_id.clone(),
                Request::Command {
                    context,
                    branch: service.engine.branch().clone(),
                    command: Command::Act {
                        expected_revision: 0,
                        action: Action::Wait,
                    },
                },
            );
            let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            assert!(messages.iter().any(|message| matches!(message,
                ServerMessage::Error { request_id: Some(id), code: ErrorCode::StaleContext, .. } if id == &request_id)),
                "stale transport authority must reject before admission: {messages:?}");
            assert!(!messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Ack { .. })));
            assert_eq!(service.engine.state(actor).unwrap(), before);
            assert!(!service.engine.has_pending_intention(actor));
        }
        let request = Request::Command {
            context: current,
            branch: service.engine.branch().clone(),
            command: Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        };
        service.handle(client.id, "original".into(), request.clone());
        assert!(service.engine.has_pending_intention(actor));
        while client.messages.try_recv().is_ok() {}
        assert!(matches!(service.step(), Step::Progress));
        let after = service.engine.state(actor).unwrap();
        service.handle(client.id, "release-final".into(), Request::ReleaseControl);
        service.handle(client.id, "reset-final".into(), Request::Snapshot);
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "original".into(), request);
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Ack { request_id, receipt: RequestReceipt::Admitted { phase: IntentionPhase::Resolved, .. }, .. }
                if request_id == "original")), "original receipt must win over abandoned input context: {messages:?}");
        assert_eq!(service.engine.state(actor).unwrap(), after);
        assert!(!service.engine.has_pending_intention(actor));
    }

    #[test]
    fn a_command_cannot_predict_an_unpublished_readiness_generation() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        service.handle(client.id, "acquire".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        let before = service.engine.state(actor).unwrap();
        let mut guessed = service.input_context(client.id).unwrap();
        guessed.readiness_revision += 1;
        // Pending host work changed permissions but has not published the new
        // generation. Neither the old generation nor a predicted one is fresh.
        service.pending_travel_cancellations.insert(actor);
        service.handle(
            client.id,
            "predicted".into(),
            Request::Command {
                context: guessed,
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            },
        );
        let messages: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            messages.iter().any(|message| matches!(
                message,
                ServerMessage::Error {
                    code: ErrorCode::StaleContext,
                    ..
                }
            )),
            "unpublished generation must reject: {messages:?}"
        );
        assert!(!messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Ack { .. })));
        assert!(!service.engine.has_pending_intention(actor));
        assert_eq!(service.engine.state(actor).unwrap(), before);
    }

    #[test]
    fn control_changes_invalidate_readiness_even_when_all_permissions_stay_disabled() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv().is_ok() {}
        let before = service.engine.state(actor).unwrap();
        // Unsettled cancellation prevents gameplay permission publication. Control
        // can still transfer while the disclosed permission vectors remain empty.
        service.pending_travel_cancellations.insert(actor);
        for (request, revision) in [(Request::AcquireControl, 1), (Request::ReleaseControl, 2)] {
            service.handle(client.id, format!("control-{revision}"), request);
            let mut published = None;
            while let Ok(message) = client.messages.try_recv() {
                if let ServerMessage::Update { update } = message {
                    if let UpdateBody::Readiness { readiness } = update.body {
                        assert!(published.replace(readiness).is_none());
                    }
                }
            }
            let readiness = published.expect(
                "ownership changes need a fresh generation even with identical permissions",
            );
            assert_eq!(
                readiness,
                Readiness {
                    revision,
                    admission: false,
                    resume: vec![],
                    cancel: vec![]
                }
            );
            assert_eq!(service.engine.state(actor).unwrap(), before);
        }
    }

    #[test]
    fn snapshot_admission_establishes_the_exact_reset_base() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "snapshot-observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut connection = service.connect(&account, "headless".into()).unwrap();
        service.handle(connection.id, "attach".into(), Request::Attach { actor });
        while connection.messages.try_recv().is_ok() {}
        let previous = service.clients[&connection.id].context.clone().unwrap();
        service.snapshot(connection.id, "reset").unwrap();
        let ServerMessage::Snapshot {
            request_id,
            snapshot,
        } = connection.messages.try_recv().unwrap()
        else {
            panic!("reset must publish a snapshot first");
        };
        assert_eq!(request_id, "reset");
        assert_eq!(snapshot.context, previous.next_reset().unwrap());
        snapshot.state.validate().unwrap();
        let client = &service.clients[&connection.id];
        let disclosed = client.last_observation.as_ref().unwrap();
        assert_eq!(client.context.as_ref(), Some(&snapshot.context));
        assert_eq!(client.observation_tick, snapshot.cursor.tick);
        assert_eq!(disclosed.branch, snapshot.branch);
        assert_eq!(
            disclosed.base,
            ObservationBase {
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            }
        );
        assert_eq!(disclosed.state, snapshot.state);
        assert_eq!(client.waiting, None);
    }

    #[test]
    fn rejected_snapshot_output_disconnects_without_affecting_a_healthy_peer() {
        let actor = ActorId(1);
        for closed in [false, true] {
            let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
            let account = Account {
                role: AccessRole::Spectator,
                user: "snapshot-observer".into(),
                token: "test-only".into(),
                actors: BTreeSet::from([actor]),
            };
            let mut connection = service.connect(&account, "headless".into()).unwrap();
            let mut healthy = service.connect(&account, "headless".into()).unwrap();
            for peer in [&mut connection, &mut healthy] {
                service.handle(peer.id, "attach".into(), Request::Attach { actor });
                while peer.messages.try_recv().is_ok() {}
            }
            let before = service.engine.state(actor).unwrap();
            let id = connection.id;
            if closed {
                drop(connection.messages);
            } else {
                for _ in 0..QUEUE {
                    service.clients[&id]
                        .messages
                        .try_send(ServerMessage::Waiting { on: Waiting::You })
                        .unwrap();
                }
            }
            service.snapshot(id, "reset").unwrap();
            assert!(!service.clients.contains_key(&id));
            assert!(*connection.close.borrow());
            assert_eq!(service.engine.state(actor).unwrap(), before);
            service.snapshot(healthy.id, "healthy-reset").unwrap();
            let ServerMessage::Snapshot {
                request_id,
                snapshot,
            } = healthy.messages.try_recv().unwrap()
            else {
                panic!("healthy peer must receive its snapshot");
            };
            assert_eq!(request_id, "healthy-reset");
            assert_eq!(*snapshot.state, before);
        }
    }

    #[test]
    fn a_snapshot_with_exhausted_readiness_disconnects_instead_of_reusing_authority() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv().is_ok() {}
        let previous_context = service.clients[&client.id].context.clone();
        let readiness = service
            .clients
            .get_mut(&client.id)
            .unwrap()
            .readiness
            .as_mut()
            .unwrap();
        readiness.permissions.revision = u64::MAX;
        // A stale cache requires a fresh authority generation, which cannot wrap.
        readiness.permissions.admission = true;
        assert!(service.snapshot(client.id, "reset").is_err());
        assert!(
            !service.clients.contains_key(&client.id),
            "exhausted authority must disconnect"
        );
        assert!(previous_context.is_some());
        assert!(
            client.messages.try_recv().is_err(),
            "no invalid snapshot may be published"
        );
    }

    #[test]
    fn recovered_queued_gameplay_is_suspended_before_control_can_restart_it() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "queued",
                &engine.branch().clone(),
                crate::journal::Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        let state = engine.state(actor).unwrap();
        let mut service = Service::new(engine);
        let status = service.engine.pending_intentions(actor);
        assert_eq!(status[0].phase, IntentionPhase::Suspended);
        assert_eq!(status[0].entry_id, admitted.entry.id);
        assert_eq!(service.engine.state(actor).unwrap(), state);
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut client = service.connect(&account, "test".into()).unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        assert!(matches!(service.step(), Step::Blocked));
        assert_eq!(service.engine.state(actor).unwrap(), state);
        service.handle(
            client.id,
            "cancel".into(),
            Request::Command {
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command: Command::CancelIntention {
                    expected_revision: 0,
                    intention: IntentionId(admitted.entry.id.0.clone()),
                },
            },
        );
        assert!(
            !service.autonomous_enabled,
            "cancellation must not restart unrelated simulation work"
        );
        assert!(service.engine.pending_intentions(actor).is_empty());
        assert_eq!(service.engine.state(actor).unwrap(), state);
    }

    #[test]
    fn client_action_is_admitted_before_simulation_applies_its_effect() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "p".into(),
            token: "test".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        while client.messages.try_recv().is_ok() {}
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        let before = service.engine.state(ActorId(1)).unwrap();
        let request = Request::Command {
            context: service.input_context(client.id).unwrap(),
            branch: service.engine.branch().clone(),
            command: Command::Act {
                expected_revision: 0,
                action: Action::Move {
                    direction: Direction::East,
                },
            },
        };
        service.handle(client.id, "queued".into(), request.clone());
        assert!(
            service.engine.state(ActorId(1)).unwrap() == before,
            "handling a gameplay request admits work without executing it"
        );
        let mut accepted = false;
        let mut identity = None;
        while let Ok(message) = client.messages.try_recv() {
            if let ServerMessage::Ack {
                request_id,
                receipt,
                ..
            } = message
            {
                assert_eq!(request_id, "queued");
                assert!(matches!(
                    &receipt,
                    RequestReceipt::Admitted {
                        actor: ActorId(1),
                        phase: IntentionPhase::Queued,
                        ..
                    }
                ));
                if let RequestReceipt::Admitted {
                    intention,
                    entry_id,
                    ..
                } = receipt
                {
                    identity = Some((intention, entry_id));
                }
                accepted = true;
            }
        }
        assert!(accepted);
        assert!(matches!(service.step(), Step::Progress));
        let after = service.engine.state(ActorId(1)).unwrap();
        assert!(
            after != before,
            "simulation step applies the admitted action"
        );
        while client.messages.try_recv().is_ok() {}
        service.handle(client.id, "queued".into(), request);
        let mut retried = false;
        while let Ok(message) = client.messages.try_recv() {
            if let ServerMessage::Ack {
                request_id,
                receipt,
                ..
            } = message
            {
                assert_eq!(request_id, "queued");
                assert!(
                    matches!(
                        &receipt,
                        RequestReceipt::Admitted {
                            phase: IntentionPhase::Resolved,
                            ..
                        }
                    ),
                    "a retry must resolve the original admitted intention"
                );
                if let RequestReceipt::Admitted {
                    intention,
                    entry_id,
                    ..
                } = receipt
                {
                    assert_eq!(Some((intention, entry_id)), identity);
                }
                retried = true;
            }
        }
        assert!(retried);
        assert!(
            service.engine.state(ActorId(1)).unwrap() == after,
            "retry must not execute again"
        );
    }

    #[test]
    fn byte_backlog_disconnects_before_message_slots_fill_and_other_clients_continue() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "watcher".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut slow = service.connect(&account, "ascii".into()).unwrap();
        slow.messages.try_recv().unwrap();
        let mut healthy = service.connect(&account, "headless".into()).unwrap();
        healthy.messages.try_recv().unwrap();
        let branch = service.engine.branch().clone();
        let revision = service.engine.revision(ActorId(1)).unwrap();
        // These frames fit the existing native client's frame ceiling. Their
        // backlog exceeds 64 MiB while still far below the 256 message slots.
        for _ in 0..65 {
            service.send(
                slow.id,
                ServerMessage::Error {
                    scope: service.error_scope(slow.id),
                    request_id: None,
                    code: ErrorCode::InvalidRequest,
                    message: "x".repeat(1024 * 1024),
                },
            );
        }
        assert!(
            *slow.close.borrow(),
            "byte pressure must disconnect a slow stream"
        );
        assert!(!*healthy.close.borrow());
        service.handle(
            healthy.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        assert!(matches!(
            healthy.messages.try_recv().unwrap(),
            ServerMessage::Snapshot { .. }
        ));
        assert_eq!(service.engine.branch(), &branch);
        assert_eq!(service.engine.revision(ActorId(1)).unwrap(), revision);
    }

    #[test]
    fn observation_sharing_requires_equal_state_actor_and_branch() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut connection = service.connect(&account, "observer".into()).unwrap();
        connection.messages.try_recv().unwrap();
        service.handle(
            connection.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        connection.messages.try_recv().unwrap();
        let original = Arc::clone(
            &service.clients[&connection.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state,
        );
        assert!(Arc::ptr_eq(
            &original,
            &service.share_observation((*original).clone())
        ));

        // Metadata can change without changing the revision or simulation tick.
        let mut changed = (*original).clone();
        changed.wizard_game = !changed.wizard_game;
        let replacement = service.share_observation(changed.clone());
        assert!(!Arc::ptr_eq(&original, &replacement));
        assert_eq!(replacement.as_ref(), &changed);
        assert_ne!(original.wizard_game, replacement.wizard_game);
        assert_eq!(original.revision, replacement.revision);
        assert_eq!(original.observation.tick, replacement.observation.tick);

        let client = service.clients.get_mut(&connection.id).unwrap();
        client.actor = Some(ActorId(2));
        assert!(!Arc::ptr_eq(
            &original,
            &service.share_observation((*original).clone())
        ));
        let client = service.clients.get_mut(&connection.id).unwrap();
        client.actor = Some(ActorId(1));
        client.last_observation.as_mut().unwrap().branch = BranchId("other-branch".into());
        assert!(!Arc::ptr_eq(
            &original,
            &service.share_observation((*original).clone())
        ));
    }

    #[test]
    fn rejected_connection_preserves_identity_and_waits_for_transport_release() {
        let frame = tor_protocol::MAX_RESPONSE_BYTES;
        let mut service = Service::with_outbound_limits(
            Engine::memory(Scenario::two_room(0)).unwrap(),
            crate::OutboundLimits {
                frame_bytes: frame,
                client_bytes: frame,
                total_bytes: frame,
            },
        )
        .unwrap();
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let connection = service.connect(&account, "first".into()).unwrap();
        service.disconnect(connection.id);
        let next = service.next_client;
        assert!(service.connect(&account, "rejected".into()).is_err());
        assert_eq!(service.next_client, next);
        drop(connection);
        let replacement = service.connect(&account, "replacement".into()).unwrap();
        assert_eq!(replacement.id, next);
    }

    #[test]
    fn action_broadcast_observes_once_per_actor_with_independent_client_streams() {
        for watchers in [1, 8, 32] {
            let mut limits = crate::OutboundLimits::default();
            limits.total_bytes = limits.total_bytes.max(watchers * limits.frame_bytes);
            let mut service = Service::with_outbound_limits(
                Engine::memory(Scenario::two_room(0)).unwrap(),
                limits,
            )
            .unwrap();
            let account = Account {
                role: AccessRole::Spectator,
                user: "observer".into(),
                token: "test-only".into(),
                actors: BTreeSet::from([ActorId(1)]),
            };
            let mut connections = Vec::new();
            for index in 0..watchers {
                let mut client = service
                    .connect(&account, format!("observer-{index}"))
                    .unwrap();
                client.messages.try_recv().unwrap();
                service.handle(
                    client.id,
                    "attach".into(),
                    Request::Attach { actor: ActorId(1) },
                );
                let ServerMessage::Snapshot { snapshot, .. } = client.messages.try_recv().unwrap()
                else {
                    panic!("initial snapshot");
                };
                connections.push((client, snapshot));
            }
            let first = &service.clients[&connections[0].0.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state;
            for (client, _) in &connections {
                let disclosed = &service.clients[&client.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state;
                assert!(
                    Arc::ptr_eq(first, disclosed),
                    "equal initial snapshots must share their retained observation"
                );
            }
            assert_eq!(
                connections
                    .iter()
                    .map(|(_, snapshot)| snapshot.context.stream.0.clone())
                    .collect::<BTreeSet<_>>()
                    .len(),
                watchers
            );
            let revisions = BTreeMap::from([(ActorId(1), 0)]);
            let result = service
                .engine
                .command(
                    "test",
                    "test",
                    ActorId(1),
                    "wait",
                    &service.engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_revision: 0,
                        action: Action::Wait,
                    },
                )
                .unwrap();
            let before = tor_simulation::diagnostics::work_counts();
            service.action_result_update(&revisions, &result).unwrap();
            let after = tor_simulation::diagnostics::work_counts();
            assert_eq!(
                after.observations - before.observations,
                1,
                "watchers: {watchers}"
            );
            let expected = service.engine.state(ActorId(1)).unwrap();
            let first: &StateView = &service.clients[&connections[0].0.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state;
            for (client, _) in &connections {
                let disclosed: &StateView = &service.clients[&client.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state;
                assert!(
                    std::ptr::eq(first, disclosed),
                    "one actor observation must be shared across its readers"
                );
            }
            let retained = Arc::clone(
                &service.clients[&connections[0].0.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state,
            );
            for (client, snapshot) in &mut connections {
                let ServerMessage::Update { update } = client.messages.try_recv().unwrap() else {
                    panic!("observation update");
                };
                assert_eq!(update.cursor.sequence, snapshot.cursor.sequence + 1);
                assert_eq!(update.branch, snapshot.branch);
                let (state, event) = match update.body {
                    UpdateBody::Observation { state, event } => {
                        (Arc::unwrap_or_clone(state), event)
                    }
                    UpdateBody::ObservationDelta { state, event, .. } => {
                        (state.apply(&snapshot.state).unwrap(), event)
                    }
                    _ => panic!("observation body"),
                };
                assert_eq!(state, expected);
                assert_eq!(event.unwrap().id, result.entry.id);
            }
            let next_result = service
                .engine
                .command(
                    "test",
                    "test",
                    ActorId(1),
                    "wait-again",
                    &service.engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_revision: expected.revision,
                        action: Action::Wait,
                    },
                )
                .unwrap();
            service
                .action_result_update(
                    &BTreeMap::from([(ActorId(1), expected.revision)]),
                    &next_result,
                )
                .unwrap();
            assert_eq!(*retained, expected, "an older boundary stays immutable");
            let current = &service.clients[&connections[0].0.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state;
            assert!(!Arc::ptr_eq(&retained, current));
            for (client, _) in &connections {
                let next = &service.clients[&client.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state;
                assert!(Arc::ptr_eq(current, next));
            }
        }
    }

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
        assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(update.body, UpdateBody::Control { has_control: true })));
        assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission)));
        assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Ack { request_id, .. } if request_id == "control"));
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
                context: service.input_context(observer.id).unwrap(),
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
    fn disconnect_during_admission_broadcast_does_not_execute_queued_work() {
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
        assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(update.body, UpdateBody::Control { has_control: true })));
        assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Update { update } if matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission)));
        assert!(matches!(controller.messages.try_recv().unwrap(),
            ServerMessage::Ack { request_id, .. } if request_id == "control"));
        observer.messages.try_recv().unwrap();
        for i in 0..QUEUE {
            service.handle(controller.id, format!("snapshot-{i}"), Request::Snapshot);
        }
        service.handle(
            controller.id,
            "wait".into(),
            Request::Command {
                context: service.input_context(controller.id).unwrap(),
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
            panic!("intention update")
        };
        assert!(matches!(
            update.body,
            UpdateBody::Intention {
                status: IntentionStatus {
                    phase: IntentionPhase::Queued,
                    ..
                }
            }
        ));
        assert_eq!(update.cursor.tick, 0);
        assert!(service.engine.has_pending_intention(ActorId(1)));
        assert!(matches!(service.step(), Step::Blocked));
        assert_eq!(
            service.engine.state(ActorId(1)).unwrap().observation.tick,
            0
        );
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
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    expected_revision: revision,
                    action: Action::Wait,
                },
            },
        );
        while client.messages.try_recv().is_ok() {}
        assert!(service.engine.has_pending_intention(ActorId(1)));
        assert!(matches!(service.step(), Step::Progress));
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

    #[tokio::test]
    async fn replenished_mailbox_cannot_starve_a_due_simulation_action() {
        use crate::runner::Mail;
        use std::sync::{Arc, Mutex};
        use tokio::sync::oneshot;

        fn next_mail(
            sender: mpsc::Sender<Mail>,
            remaining: usize,
            observed: Arc<Mutex<Vec<Option<ActorId>>>>,
            stopped: oneshot::Sender<Option<crate::storage::Store>>,
        ) -> Mail {
            Mail::Call(Box::new(move |service| {
                observed.lock().unwrap().push(service.engine.next_actor());
                let next = if remaining == 0 {
                    Mail::Shutdown(stopped)
                } else {
                    next_mail(sender.clone(), remaining - 1, observed, stopped)
                };
                // One replacement always fits, so this reproduces a mailbox
                // that never empties without relying on threads or wall time.
                assert!(sender.try_send(next).is_ok());
            }))
        }

        for capacity in [4, 16, 256] {
            let (service, _client) = character_waiting_on_a_guard();
            let (mail, mailbox) = mpsc::channel(capacity);
            let observed = Arc::new(Mutex::new(Vec::new()));
            let (reply, stopped) = oneshot::channel();
            assert!(mail
                .try_send(next_mail(
                    mail.clone(),
                    capacity * 3,
                    observed.clone(),
                    reply
                ))
                .is_ok());
            crate::runner::run(service, mailbox, crate::runner::STALL).await;
            stopped.await.unwrap();
            let observed = observed.lock().unwrap();
            assert_ne!(observed[0], Some(ActorId(1)));
            assert_eq!(
                observed[capacity],
                Some(ActorId(1)),
                "due AI was starved by replenished mail at capacity {capacity}"
            );
        }
    }

    #[tokio::test]
    async fn a_closed_full_mailbox_stops_before_the_next_simulation_action() {
        let (service, _client) = character_waiting_on_a_guard();
        let before = service.engine.next_actor();
        let (mail, mailbox) = mpsc::channel(4);
        for _ in 0..4 {
            assert!(mail
                .try_send(crate::runner::Mail::Call(Box::new(|_| {})))
                .is_ok());
        }
        drop(mail);
        let service = crate::runner::run(service, mailbox, crate::runner::STALL).await;
        assert_eq!(service.engine.next_actor(), before);
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
                context: service.input_context(client.id).unwrap(),
                branch: branch.clone(),
                command: Command::Wizard {
                    expected_revision: revision,
                    operation: "rewind initial".into(),
                },
            },
            timing: None,
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
