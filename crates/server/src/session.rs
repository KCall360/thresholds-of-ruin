use crate::engine::valid_label;
use crate::{Engine, Failure};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use tokio::sync::watch;
use tor_protocol::*;

#[path = "chart_page_session.rs"]
mod chart_pages;
#[path = "map_page_session.rs"]
mod map_pages;
#[path = "presentation_history.rs"]
mod presentation_history;
use presentation_history::{IssuedPresentation, PresentationHistory};

#[cfg(feature = "service-contract-host")]
#[path = "service_contract_host.rs"]
pub(crate) mod contract_host;

/// Trusted startup configuration. Tokens are never put into game history.
#[derive(Clone)]
pub struct Account {
    pub user: String,
    pub token: String,
    pub role: AccessRole,
    pub actors: BTreeSet<ActorId>,
}

/// Keep the sharing fast path beside the restriction it must exactly match.
/// Presence, including a known false curse/protection fact, is private knowledge.
fn has_private_item_disclosure(state: &StateView) -> bool {
    state.observation.interactions.as_ref().is_some_and(|view| {
        view.identification.is_some()
            || !view.curse_blocked.is_empty()
            || view.inventory.iter().any(|item| {
                item.charges.is_some()
                    || item.known_equipment.is_some()
                    || item.known_projectile_bonus.is_some()
                    || item.known_cursed.is_some()
                    || item.known_protected.is_some()
            })
    })
}

/// Ordinary preparation remains visible; paid selection, exact-instance counts,
/// and learned equipment facts belong only to account-authorized owner attachments.
fn restrict_item_disclosure(state: &mut StateView, role: AccessRole) {
    if role == AccessRole::Spectator {
        if let Some(interaction) = &mut state.observation.interactions {
            interaction.identification = None;
            interaction.curse_blocked.clear();
            for item in &mut interaction.inventory {
                item.charges = None;
                item.known_equipment = None;
                item.known_projectile_bonus = None;
                item.known_cursed = None;
                item.known_protected = None;
            }
        }
    }
}

fn semantic_item_facts(
    capture: &crate::engine::SemanticObservation,
    role: AccessRole,
) -> std::borrow::Cow<'_, OutcomeObservation> {
    let mut facts = std::borrow::Cow::Borrowed(&capture.facts);
    if role == AccessRole::Spectator
        && facts
            .interactions
            .as_ref()
            .is_some_and(|view| !view.curse_blocked.is_empty())
    {
        facts
            .to_mut()
            .interactions
            .as_mut()
            .expect("restricted interaction")
            .curse_blocked
            .clear();
    }
    facts
}

/// Ephemeral semantic capture shared by all attachments watching each actor.
/// Receipt provenance is private and never encoded into an outcome or snapshot.
struct SemanticStates {
    views: BTreeMap<ActorId, Arc<crate::engine::SemanticObservation>>,
    events: EventProvenance,
}

/// Canonical fallback ownership belongs to the observer, not each connection.
/// These two pictures live only through one action's publication.
struct SemanticPicturePair {
    before: Arc<StateView>,
    after: Arc<StateView>,
}

struct DisclosedObservation {
    branch: BranchId,
    base: ObservationBase,
    state: Arc<StateView>,
}

/// Ownership is part of the generation even when permission vectors stay empty.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DisclosedReadiness {
    // Private monotonic facts contribute to the already public authority revision.
    engine_input_generation: u64,
    control_generation: u64,
    has_control: bool,
    permissions: Readiness,
}

/// Payloads prepared by a request and published after its ordered effects.
enum RequestReply {
    ProjectileChoices(Box<ProjectileChoicePage>),
    AbilityWeaponPreview(Box<tor_protocol::AbilityWeaponPreview>),
    Receipt(RequestReceipt),
    History(HistoryPage),
    Palette(PaletteUpdate),
    CreatureInspection(Box<CreatureInspectionView>),
    CreatureCatalog(Box<CreatureCatalogPage>),
    StrikePage(Box<CreatureStrikePage>),
    AdvancementPage(Box<AdvancementPage>),
    CombatDiagnostics(Box<tor_protocol::CombatDiagnosticsView>),
}

impl RequestReply {
    fn message(self, request_id: String, context: ReplyContext) -> ServerMessage {
        match self {
            Self::ProjectileChoices(page) => ServerMessage::ProjectileChoices {
                context,
                request_id,
                page,
            },
            Self::AbilityWeaponPreview(preview) => ServerMessage::AbilityWeaponPreview {
                context,
                request_id,
                preview,
            },
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
            Self::CombatDiagnostics(report) => ServerMessage::CombatDiagnostics {
                context,
                request_id,
                report,
            },
            Self::AdvancementPage(page) => ServerMessage::AdvancementPage {
                context,
                request_id,
                page,
            },
            Self::StrikePage(page) => ServerMessage::CreatureStrikePage {
                context,
                request_id,
                page,
            },
            Self::CreatureCatalog(page) => ServerMessage::CreatureCatalog {
                context,
                request_id,
                page,
            },
            Self::CreatureInspection(report) => ServerMessage::CreatureInspection {
                context,
                request_id,
                report,
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
    /// Map cursors advance only after their correlated page is admitted.
    Map(Box<map_pages::MapTransfer>),
    Chart(Box<chart_pages::ChartTransfer>),
    /// Snapshot publication owns the stream reset; no additional reply is due.
    Published,
    /// Persistence owns completion; poll it at the common publication boundary.
    SaveQueued,
}

/// One ephemeral read capability, discarded on reset/disconnect or replacement.
/// This is not saved, an access grant, a journal receipt, or simulation randomness.
struct StrikeInspectionHandle {
    id: StrikeInspectionId,
    context: InputContext,
    branch: BranchId,
    observer_revision: u64,
    target: ActorId,
    target_revision: u64,
    source: StrikeSourceSelector,
    resolved: StrikeSourceAttribution,
    catalog: (String, BranchId, String, Option<String>),
}

struct Client {
    strike_inspection: Option<StrikeInspectionHandle>,
    map_transfer: Option<map_pages::MapTransfer>,
    chart_transfer: Option<chart_pages::ChartTransfer>,
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
    presentations: PresentationHistory,
    /// The palette last sent and its revision: the base for the next delta.
    palette: Option<(u64, BTreeSet<String>)>,
    /// The region that palette was forecast from.
    palette_region: Option<u64>,
    messages: crate::outbound::Sender,
    close: watch::Sender<Option<DisconnectMode>>,
    /// What this client was last told play waits for, since its last update
    /// or snapshot; `None` once anything has changed.
    waiting: Option<Waiting>,
}

/// A resource failure discards stale output; an intentional detach may deliver
/// its final explanation before closing. Neither changes simulation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisconnectMode {
    Abort,
    Drain,
}

pub(crate) struct Connection {
    pub id: u64,
    pub messages: crate::outbound::Receiver,
    pub close: watch::Receiver<Option<DisconnectMode>>,
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
fn potential_hazards(observation: &Observation) -> BTreeSet<tor_protocol::ActorTarget> {
    observation
        .visible_actors
        .iter()
        .filter(|other| other.id != observation.self_target)
        .map(|other| other.id)
        .collect()
}

struct TravelJob {
    hp: Option<u64>,
    owner: u64,
    steps: VecDeque<tor_simulation::TravelStep>,
    hazards: BTreeSet<tor_protocol::ActorTarget>,
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
    Redirected,
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
    // One ephemeral delivery witness, never simulation or saved authority.
    reported_execution_failure: Option<(u64, StreamContext, BranchId, EntryId, ErrorCode)>,
    clients: BTreeMap<u64, Client>,
    controllers: BTreeMap<ActorId, u64>,
    control_generations: BTreeMap<ActorId, Option<u64>>,
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
            // Arena AI has permanent autonomous authority; preserve its paid
            // preparation through restart without admitting replacement work.
            if engine.unattended_arena() && engine.is_ai(actor) {
                continue;
            }
            if let Err(error) = engine.pause_preparation(actor) {
                save_warning = Some(error.to_string());
            }
        }
        Ok(Self {
            outbound: crate::outbound::Pool::new(limits),
            diagnostics: None,
            pending_pauses: BTreeSet::new(),
            pending_travel_cancellations: BTreeSet::new(),
            autonomous_enabled: engine.unattended_arena() && engine.arena_execution_enabled(),
            travels: BTreeMap::new(),
            travel_status: BTreeMap::new(),
            resetting_streams: false,
            broadcasting_travel: false,
            engine,
            pending_saves: Vec::new(),
            save_warning,
            reported_execution_failure: None,
            clients: BTreeMap::new(),
            controllers: BTreeMap::new(),
            control_generations: BTreeMap::new(),
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
        if !valid_label(&frontend) {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Connection is unavailable",
            ));
        }
        if self.clients.len() >= self.outbound.connection_limit() {
            return Err(Failure::new(
                ErrorCode::ResourceLimit,
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
            .map_err(|_| Failure::new(ErrorCode::ResourceLimit, "Connection is unavailable"))?;
        self.next_client = next;
        let (close, closing) = watch::channel(None);
        let actors: Vec<_> = self
            .engine
            .actors()
            .into_iter()
            .filter(|actor| account.role == AccessRole::Wizard || account.actors.contains(actor))
            .collect();
        self.clients.insert(
            id,
            Client {
                strike_inspection: None,
                map_transfer: None,
                chart_transfer: None,
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
                presentations: PresentationHistory::default(),
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
                capabilities: self.outbound.capabilities(),
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
            Ok(ProcessedRequest::Map(transfer)) => {
                if let Err(error) = self.publish_map_page(id, &request_id, *transfer) {
                    self.send(
                        id,
                        ServerMessage::Error {
                            scope: self.error_scope(id),
                            request_id: Some(request_id),
                            code: error.code,
                            message: error.message,
                        },
                    );
                }
            }
            Ok(ProcessedRequest::Chart(transfer)) => {
                if let Err(error) = self.publish_chart_page(id, &request_id, *transfer) {
                    self.send(
                        id,
                        ServerMessage::Error {
                            scope: self.error_scope(id),
                            request_id: Some(request_id),
                            code: error.code,
                            message: error.message,
                        },
                    );
                }
            }
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
                command: Command::Wizard { .. } | Command::InspectStrikeSource { .. },
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
            self.engine.register_input_observer(actor)?;
            let client = self.clients.get_mut(&id).expect("connected client");
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
            Request::Chart { query } => self
                .prepare_chart_page(id, query)
                .map(|transfer| ProcessedRequest::Chart(Box::new(transfer))),
            Request::Map { query } => self
                .prepare_map_page(id, query)
                .map(|transfer| ProcessedRequest::Map(Box::new(transfer))),
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
                        self.set_controller(actor, Some(id));
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
                if self.set_controller(actor, None) {
                    self.pending_pauses.insert(actor);
                    if !self.engine.unattended_arena() {
                        self.autonomous_enabled = false;
                    }
                    self.control_update(actor);
                }
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
            }
            Request::Observe => self.observe(id).map(|()| ProcessedRequest::Published),
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
                let command = crate::wire_adapter::decode_command(&command)?;
                if let Some(previous) = self
                    .engine
                    .retry_decoded(&user, &frontend, actor, request_id, &branch, &command)?
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
                if command.requires_control() && self.controllers.get(&actor) != Some(&id) {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before acting",
                    ));
                }
                if let crate::wire_adapter::DecodedCommand::Gameplay { action, .. } = &command {
                    let basis = match action {
                        Action::ChooseGrowthSource { basis, .. }
                        | Action::ChooseIntellectExpertise { basis, .. }
                        | Action::Purchase { basis, .. }
                        | Action::Respec { basis, .. } => Some(basis.as_ref()),
                        _ => None,
                    };
                    if let Some(basis) = basis {
                        let reply = self.reply_context(id).ok_or_else(|| {
                            Failure::new(ErrorCode::StaleContext, "Input context changed")
                        })?;
                        self.engine.check_advancement_basis(&reply, basis)?;
                    }
                }
                if let crate::wire_adapter::DecodedCommand::ProjectileChoices {
                    expected_revision,
                    query,
                } = &command
                {
                    let context = self.reply_context(id).ok_or_else(|| {
                        Failure::new(ErrorCode::StaleContext, "Input context changed")
                    })?;
                    let budget = crate::engine::ProjectileChoicePageBudget {
                        context: &context,
                        request_id,
                        limit: self.outbound.capabilities().max_response_bytes as usize,
                    };
                    let boundary = self
                        .query_presentation(id, *expected_revision)?
                        .boundary
                        .clone();
                    let page = self.engine.projectile_choices_at(
                        &boundary,
                        actor,
                        *expected_revision,
                        query,
                        &budget,
                    )?;
                    return Ok(ProcessedRequest::Reply(RequestReply::ProjectileChoices(
                        Box::new(page),
                    )));
                }
                if let crate::wire_adapter::DecodedCommand::AbilityWeaponPreview {
                    expected_revision,
                    ability,
                    invocation,
                    supplies,
                } = &command
                {
                    let boundary = self
                        .query_presentation(id, *expected_revision)?
                        .boundary
                        .clone();
                    let preview = self.engine.ability_weapon_preview_at(
                        &boundary,
                        actor,
                        ability,
                        *invocation,
                        supplies,
                    )?;
                    let context = self.reply_context(id).ok_or_else(|| {
                        Failure::new(ErrorCode::StaleContext, "Input context changed")
                    })?;
                    let message = ServerMessage::AbilityWeaponPreview {
                        context,
                        request_id: request_id.into(),
                        preview: Box::new(preview.clone()),
                    };
                    if tor_protocol::encoded_length(
                        &message,
                        self.outbound.capabilities().max_response_bytes as usize,
                    )
                    .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Invalid preview"))?
                    .is_none()
                    {
                        return Err(Failure::new(
                            ErrorCode::ResourceLimit,
                            "Preview exceeds response limit",
                        ));
                    }
                    return Ok(ProcessedRequest::Reply(RequestReply::AbilityWeaponPreview(
                        Box::new(preview),
                    )));
                }
                if let crate::wire_adapter::DecodedCommand::Advancement {
                    expected_revision,
                    query,
                } = &command
                {
                    let reply = self.reply_context(id).ok_or_else(|| {
                        Failure::new(ErrorCode::StaleContext, "Input context changed")
                    })?;
                    let budget = crate::advancement_page::PageBudget {
                        context: &reply,
                        request_id,
                        limit: self.outbound.capabilities().max_response_bytes as usize,
                    };
                    let boundary = self
                        .query_presentation(id, *expected_revision)?
                        .boundary
                        .clone();
                    let page = self.engine.advancement_query_at(
                        &boundary,
                        actor,
                        *expected_revision,
                        query,
                        &budget,
                    )?;
                    return Ok(ProcessedRequest::Reply(RequestReply::AdvancementPage(
                        Box::new(page),
                    )));
                }
                // Fresh gameplay must honor the same derived permissions that
                // were published. Simulation still validates the action itself;
                // session policy cannot be bypassed by a correctly stamped request.
                match &command {
                    crate::wire_adapter::DecodedCommand::Gameplay { .. }
                    | crate::wire_adapter::DecodedCommand::Display {
                        operation: crate::wire_adapter::DisplayCommand::Travel { .. },
                        ..
                    } if !current.permissions.admission => {
                        return Err(Failure::new(
                            ErrorCode::ActorBusy,
                            "New gameplay is unavailable in the current input state",
                        ));
                    }
                    crate::wire_adapter::DecodedCommand::ResumeIntention { admission }
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
                    crate::wire_adapter::DecodedCommand::CancelIntention { admission, .. }
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
                if let crate::wire_adapter::DecodedCommand::StrikeInspection {
                    expected_revision,
                    target,
                    target_revision,
                    source,
                    cursor,
                } = command
                {
                    let catalog = self.engine.strike_catalog_identity();
                    let stale = || {
                        Failure::new(
                            ErrorCode::StaleContext,
                            "Strike inspection changed; reopen the source",
                        )
                    };
                    if let Some(cursor) = &cursor {
                        let handle = self.clients[&id]
                            .strike_inspection
                            .as_ref()
                            .ok_or_else(stale)?;
                        if handle.id != cursor.inspection
                            || handle.context != context
                            || handle.branch != branch
                            || handle.observer_revision != expected_revision
                            || handle.target != target
                            || handle.target_revision != target_revision
                            || handle.source != source
                            || handle.catalog != catalog
                        {
                            return Err(stale());
                        }
                    }
                    let inspection = cursor
                        .as_ref()
                        .map(|cursor| cursor.inspection.clone())
                        .unwrap_or_else(|| StrikeInspectionId(uuid::Uuid::new_v4().to_string()));
                    let reply_context = self.reply_context(id).ok_or_else(stale)?;
                    let budget = crate::creature_view::PageBudget {
                        context: &reply_context,
                        request_id,
                        inspection: &inspection,
                        actor: target,
                        target_revision,
                        source: &source,
                        limit: self.outbound.capabilities().max_response_bytes as usize,
                    };
                    let boundary = self
                        .query_presentation(id, expected_revision)?
                        .boundary
                        .clone();
                    let projected = self.engine.strike_page_query_at(
                        &boundary,
                        expected_revision,
                        (target, target_revision),
                        (&source, cursor.as_ref().map(|cursor| &cursor.position)),
                        &budget,
                    )?;
                    if cursor.is_some()
                        && self.clients[&id]
                            .strike_inspection
                            .as_ref()
                            .is_some_and(|handle| handle.resolved != projected.resolved)
                    {
                        return Err(stale());
                    }
                    let page = CreatureStrikePage {
                        inspection: inspection.clone(),
                        actor: target,
                        target_revision,
                        source: source.clone(),
                        resolved: projected.resolved.clone(),
                        strike_count: projected.strike_count,
                        start: projected.start,
                        entries: projected.entries,
                        next: projected.next.map(|position| StrikePageCursor {
                            inspection: inspection.clone(),
                            position,
                        }),
                    };
                    page.validate().map_err(|_| {
                        Failure::new(ErrorCode::InvalidArchive, "Invalid strike inspection page")
                    })?;
                    // Defense in depth on the actual complete correlated reply, without
                    // cloning the projected page or allocating its encoded JSON.
                    budget.check(&page)?;
                    self.clients
                        .get_mut(&id)
                        .expect("connected client")
                        .strike_inspection = Some(StrikeInspectionHandle {
                        id: inspection,
                        context,
                        branch,
                        observer_revision: expected_revision,
                        target,
                        target_revision,
                        source,
                        resolved: projected.resolved,
                        catalog,
                    });
                    return Ok(ProcessedRequest::Reply(RequestReply::StrikePage(Box::new(
                        page,
                    ))));
                }
                if let crate::wire_adapter::DecodedCommand::CombatDiagnostics {
                    expected_revision,
                    operation,
                } = command
                {
                    let boundary = self
                        .query_presentation(id, expected_revision)?
                        .boundary
                        .clone();
                    let report = self
                        .engine
                        .combat_diagnostics_query_at(&boundary, operation)?;
                    return Ok(ProcessedRequest::Reply(RequestReply::CombatDiagnostics(
                        Box::new(report),
                    )));
                }
                if let crate::wire_adapter::DecodedCommand::CreatureCatalog {
                    expected_revision,
                    depth,
                    after,
                } = command
                {
                    let boundary = self
                        .query_presentation(id, expected_revision)?
                        .boundary
                        .clone();
                    let page = self.engine.creature_catalog_query_at(
                        &boundary,
                        actor,
                        depth,
                        after.as_deref(),
                    )?;
                    let context = self.reply_context(id).ok_or_else(|| {
                        Failure::new(ErrorCode::StaleContext, "Input context changed")
                    })?;
                    let page = crate::wizard_creatures::fit_reply(
                        page,
                        &context,
                        request_id,
                        self.outbound.capabilities().max_response_bytes as usize,
                    )?;
                    return Ok(ProcessedRequest::Reply(RequestReply::CreatureCatalog(
                        Box::new(page),
                    )));
                }
                if let crate::wire_adapter::DecodedCommand::CreatureInspection {
                    expected_revision,
                    target,
                } = command
                {
                    let boundary = self
                        .query_presentation(id, expected_revision)?
                        .boundary
                        .clone();
                    let report = self.engine.inspect_creature_query_at(
                        &boundary,
                        target,
                        expected_revision,
                    )?;
                    return Ok(ProcessedRequest::Reply(RequestReply::CreatureInspection(
                        Box::new(report),
                    )));
                }
                let display_boundary = match command.display_basis() {
                    Some(revision) => Some((
                        self.query_presentation(id, revision)?.boundary.clone(),
                        revision,
                    )),
                    None => None,
                };
                let command = self.engine.resolve_command_with_display(
                    actor,
                    &branch,
                    command,
                    display_boundary
                        .as_ref()
                        .map(|(boundary, revision)| (boundary, *revision)),
                )?;
                let before_states = self.semantic_states()?;
                let normalized_annotation = match &command {
                    crate::journal::Command::Annotate {
                        anchor: Anchor::State { snapshot },
                        ..
                    } => {
                        let issued = self.presentation_by_snapshot(id, snapshot)?;
                        Some(crate::journal::RecordedAnchor::State {
                            snapshot: snapshot.clone(),
                            boundary: issued.boundary.entry().cloned(),
                        })
                    }
                    _ => None,
                };
                let result = if let Some(anchor) = normalized_annotation {
                    self.engine.command_with_annotation(
                        &crate::engine::Receipt {
                            user: user.clone(),
                            frontend: frontend.clone(),
                            request_id: request_id.into(),
                            actor,
                            branch: branch.clone(),
                            command,
                        },
                        anchor,
                    )?
                } else {
                    self.engine
                        .command(&user, &frontend, actor, request_id, &branch, command)?
                };
                if matches!(
                    &result.entry.content,
                    crate::journal::JournalContent::Wizard {
                        operation: crate::journal::WizardOperation::ArenaControl { .. },
                        ..
                    }
                ) {
                    self.autonomous_enabled = self.engine.arena_execution_enabled();
                    // Scheduler controls change ordinary readiness, not world
                    // topology. Preserve streams, preparations and private history.
                    self.action_update(&before_states, None)?;
                    return Ok(ProcessedRequest::Reply(RequestReply::Receipt(
                        self.engine.request_receipt(&result),
                    )));
                }
                let Some(visible_entry) = self.engine.disclose_entry(&result.entry) else {
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
                    self.action_result_update(&before_states, &result)?;
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
                            let disconnect = self.engine.causal_boundary(observer).is_err()
                                || self.snapshot(recipient, "").is_err();
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
                        self.action_result_update(&before_states, &result)?;
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
        before: &SemanticStates,
        result: &crate::CommandResult,
    ) -> Result<(), Failure> {
        let entry = self.engine.disclose_entry(&result.entry);
        self.action_update(before, entry.as_ref())?;
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

    /// Project a role only when it has private item details to remove. Equal
    /// projections retain the same allocation and per-connection stream bases.
    fn item_observation(&self, state: Arc<StateView>, role: AccessRole) -> Arc<StateView> {
        if role != AccessRole::Spectator {
            return state;
        }
        if has_private_item_disclosure(&state) {
            let mut projected = state.as_ref().clone();
            restrict_item_disclosure(&mut projected, role);
            Arc::new(projected)
        } else {
            state
        }
    }

    /// Capture only attached observers. These transient values are narration
    /// inputs, not public display snapshots and not persisted revision state.
    fn semantic_states(&self) -> Result<SemanticStates, Failure> {
        let boundary = self.engine.presentation_boundary();
        let events = self.engine.event_history_boundary_at(&boundary)?;
        let views = self
            .clients
            .values()
            .filter_map(|client| client.actor)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|actor| {
                self.engine
                    .semantic_observation(actor)
                    .map(|state| (actor, Arc::new(state)))
            })
            .collect::<Result<_, _>>()?;
        Ok(SemanticStates { views, events })
    }

    fn action_update(
        &mut self,
        before: &SemanticStates,
        entry: Option<&HistoryEntry>,
    ) -> Result<(), Failure> {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter_map(|(&id, client)| client.actor.map(|actor| (id, actor, client.role)))
            .collect();
        let events = match self
            .engine
            .event_history_boundary_at(&self.engine.presentation_boundary())
        {
            Ok(events) => events,
            Err(error) => {
                for (id, _, _) in recipients {
                    self.disconnect_failed_projection(id, &error.to_string());
                }
                return Ok(());
            }
        };
        let after: BTreeMap<_, _> = recipients
            .iter()
            .map(|(_, actor, _)| *actor)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|actor| (actor, self.engine.semantic_observation(actor).map(Arc::new)))
            .collect();
        let pictures: BTreeMap<_, _> = after
            .iter()
            .filter_map(|(actor, current)| {
                let (Ok(current), Some(previous)) = (current, before.views.get(actor)) else {
                    return None;
                };
                let comparison = (|| {
                    if self.engine.semantic_picture_unchanged(previous, current)? {
                        return Ok(None);
                    }
                    Ok::<_, Failure>(Some(SemanticPicturePair {
                        before: Arc::new(self.engine.semantic_picture(previous)?),
                        after: Arc::new(self.engine.semantic_picture(current)?),
                    }))
                })();
                Some((*actor, comparison))
            })
            .collect();
        for (id, actor, role) in recipients {
            if !self.clients.contains_key(&id) {
                continue;
            }
            let state = match &after[&actor] {
                Ok(state) => state,
                Err(error) => {
                    if self.engine.actors().contains(&actor) {
                        self.disconnect_failed_projection(id, &error.to_string());
                    } else {
                        self.detach_unloaded(id);
                    }
                    continue;
                }
            };
            let Some(previous) = before.views.get(&actor) else {
                self.disconnect_failed_projection(id, "Missing pre-commit observer capture");
                continue;
            };
            let previous_facts = semantic_item_facts(previous, role);
            let current_facts = semantic_item_facts(state, role);
            let own_entry = entry.filter(|entry| entry.actor == actor);
            let own_event = own_entry.and_then(|entry| match &entry.content {
                HistoryContent::Action { event, .. } => Some(event.as_ref()),
                _ => None,
            });
            let facts = OutcomeFacts::capture_compact_causal(
                &previous_facts,
                &current_facts,
                own_event,
                before.events,
                events,
            )
            .and_then(|facts| {
                facts.validate_at(current_facts.tick)?;
                Ok(facts)
            });
            let facts = match facts {
                Ok(facts) => facts,
                Err(error) => {
                    // The action is already committed. Close this stream rather
                    // than skip its outcome; healthy peers still receive theirs.
                    self.disconnect_failed_projection(id, &format!("{error:?}"));
                    continue;
                }
            };
            let visual_changed = match &pictures[&actor] {
                Ok(None) => false,
                Ok(Some(pictures)) => {
                    let old = self.item_observation(Arc::clone(&pictures.before), role);
                    let new = self.item_observation(Arc::clone(&pictures.after), role);
                    OutcomeFacts::picture_changed(&old.observation, &new.observation)
                }
                Err(error) => {
                    self.disconnect_failed_projection(id, &error.to_string());
                    continue;
                }
            };
            if visual_changed || !facts.facts.is_empty() || own_entry.is_some() {
                self.update_at(
                    id,
                    UpdateBody::Outcome {
                        visual_changed,
                        facts,
                        event: own_entry.map(|entry| Box::new(entry.clone())),
                    },
                    Some(current_facts.tick),
                );
            }
            self.palette_update(id);
        }
        Ok(())
    }

    /// Outcome delivery failures are not persistence failures and must not
    /// overwrite the save worker's warning/recovery state.
    fn disconnect_failed_projection(&mut self, id: u64, reason: &str) {
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.warning(&format!(
                "Committed outcome projection failed for connection {id}; disconnected: {reason}"
            ));
        }
        self.disconnect(id);
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
        // Retain ownership until both operations succeed. In particular, a
        // fallible observer capture must not discard this actor or the rest of
        // the batch and accidentally reopen admission/simulation.
        let pending: Vec<_> = self.pending_pauses.iter().copied().collect();
        for actor in pending {
            let before = match self.semantic_states() {
                Ok(states) => states,
                Err(error) => {
                    if let Some(diagnostics) = &self.diagnostics {
                        diagnostics
                            .warning(&format!("Pending pause observer capture failed: {error}"));
                    }
                    return;
                }
            };
            match self.engine.suspend_queued_intention(actor) {
                Ok(Some(result)) => self.intention_update(&result),
                Ok(None) => {}
                Err(error) => {
                    self.save_warning = Some(error.to_string());
                    continue;
                }
            }
            match self.engine.pause_preparation(actor) {
                Ok(Some(result)) => {
                    let _ = self.action_result_update(&before, &result);
                    self.pending_pauses.remove(&actor);
                }
                Ok(None) => {
                    self.pending_pauses.remove(&actor);
                }
                Err(error) => self.save_warning = Some(error.to_string()),
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

    fn ai_has_driver(&self) -> bool {
        self.engine.unattended_arena()
            || self
                .controllers
                .keys()
                .any(|&actor| self.engine.alive(actor))
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
        if !self.engine.arena_execution_enabled() {
            self.announce_waiting();
            return Step::Blocked;
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
        // Adventures need a living controller; arenas have bounded unattended
        // execution, including after selected death.
        if !self.engine.is_ai(next) || !self.autonomous_enabled || !self.ai_has_driver() {
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
                self.autonomous_enabled && self.ai_has_driver()
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
        let before_states = self.semantic_states();
        match before_states.and_then(|before_states| {
            self.engine
                .execute_next_intention()
                .map(|result| (before_states, result))
        }) {
            Ok((before_states, Some(result))) => {
                self.reported_execution_failure = None;
                // Account committed movement before output can disconnect its owner.
                let travel = self.commit_travel_progress(next, &result);
                if let Err(error) = self.action_result_update(&before_states, &result) {
                    self.save_warning = Some(error.to_string());
                }
                if let Some(outcome) = travel {
                    self.finish_travel_execution(next, outcome);
                }
                return Step::Progress;
            }
            Ok((_, None)) => {}
            // Ordinary unavailable actions already return a committed
            // IntentionFailed above. Returned simulation refusals retain their
            // queue and need a controller notice, not a storage warning.
            Err(error)
                if matches!(
                    error.code,
                    ErrorCode::ResourceLimit | ErrorCode::InvalidAction
                ) =>
            {
                self.report_execution_failure(next, error);
            }
            Err(error) => self.save_warning = Some(error.to_string()),
        }
        self.announce_waiting();
        Step::Blocked
    }

    /// A refused admitted action stays retryable. Report its typed simulation
    /// failure to its current controller once, independently of storage polling.
    fn report_execution_failure(&mut self, actor: ActorId, failure: Failure) {
        let Some(&client) = self.controllers.get(&actor) else {
            return;
        };
        let Some(admission) = self.engine.queued_intention_admission(actor).cloned() else {
            return;
        };
        let Some(stream) = self
            .clients
            .get(&client)
            .and_then(|client| client.context.clone())
        else {
            return;
        };
        let key = (
            client,
            stream,
            self.engine.branch().clone(),
            admission,
            failure.code,
        );
        if self.reported_execution_failure.as_ref() == Some(&key) {
            return;
        }
        let message = ServerMessage::Error {
            scope: self.error_scope(client),
            request_id: None,
            code: failure.code,
            message: failure.message,
        };
        if self.admit_output(client, &message) {
            self.reported_execution_failure = Some(key);
        }
    }

    /// Tell each attached client what stopped play waits for, once per stop.
    fn announce_waiting(&mut self) {
        let next = self.engine.next_actor();
        let anyone = self.ai_has_driver();
        let ids: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, c)| c.actor.is_some())
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            let on = match next {
                None => Waiting::Stopped,
                // Adventure AI needs a player; arena AI has its own driver.
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
            crate::journal::JournalContent::IntentionStarted {
                admission, event, ..
            } => {
                let matched = matches!(event, crate::journal::Event::Moved { to, .. }
                    if job.steps.front().is_some_and(|step| *to == crate::adapt::position(step.destination)));
                (
                    admission,
                    if matched {
                        TravelExecution::Moved
                    } else {
                        TravelExecution::Redirected
                    },
                )
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
        if matches!(
            outcome,
            TravelExecution::Blocked | TravelExecution::Redirected
        ) {
            self.stop_travel(
                actor,
                if matches!(outcome, TravelExecution::Redirected) {
                    TravelPhase::DecisionRequired
                } else {
                    TravelPhase::Blocked
                },
            );
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
        let before_states = match self.semantic_states() {
            Ok(states) => states,
            Err(error) => {
                self.save_warning = Some(error.to_string());
                return;
            }
        };
        let result = self.engine.admit_ai(actor);
        match result {
            Ok(result) => {
                let _ = self.action_result_update(&before_states, &result);
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

    fn observe(&mut self, id: u64) -> Result<(), Failure> {
        let (boundary, state) = self.prepare_presentation(id)?;
        let identity = state.snapshot.clone();
        self.update(
            id,
            UpdateBody::Observation {
                state: Arc::new(state),
                event: None,
            },
        );
        if self
            .clients
            .get(&id)
            .and_then(|client| client.last_observation.as_ref())
            .is_some_and(|observed| observed.state.snapshot == identity)
        {
            self.commit_admitted_presentation(id, boundary, false);
        }
        Ok(())
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
        let (boundary, state) = self.prepare_presentation(id)?;
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
            state: Arc::new(state),
            has_control: readiness.has_control,
            history: self
                .engine
                .history(actor, &client.user, None, MAX_HISTORY_PAGE)?,
        };
        let message = ServerMessage::Snapshot {
            request_id: request_id.into(),
            snapshot: Box::new(snapshot),
        };
        if self.clients[&id]
            .messages
            .try_send_snapshot(&message)
            .is_err()
        {
            // Output rejection closes the connection; it is not a request error.
            self.disconnect(id);
            return Ok(());
        }
        let ServerMessage::Snapshot { snapshot, .. } = message else {
            unreachable!()
        };
        let client = self.clients.get_mut(&id).expect("admitted client");
        client.context = Some(context);
        client.strike_inspection = None;
        client.map_transfer = None;
        client.chart_transfer = None;
        client.readiness = Some(readiness);
        client.waiting = None;
        client.observation_tick = snapshot.state.observation.tick;
        client.last_observation = Some(DisclosedObservation {
            branch: snapshot.branch,
            base: ObservationBase {
                snapshot: snapshot.state.snapshot.clone(),
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            },
            state: snapshot.state,
        });
        self.commit_admitted_presentation(id, boundary, true);
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
        let entry = self.engine.disclose_entry(&entry).ok_or_else(|| {
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

    /// Record every actual ownership transition, even if refresh is batched.
    /// Exhaustion fails input closed; it never wraps or rolls simulation back.
    fn set_controller(&mut self, actor: ActorId, owner: Option<u64>) -> bool {
        if self.controllers.get(&actor).copied() == owner {
            return false;
        }
        match owner {
            Some(id) => {
                self.controllers.insert(actor, id);
            }
            None => {
                self.controllers.remove(&actor);
            }
        }
        let generation = self.control_generations.entry(actor).or_insert(Some(0));
        *generation = generation.and_then(|value| value.checked_add(1));
        true
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
        // Read-only spectators have no action authority to invalidate. Owner
        // operand changes and control ABA must not become spectator readiness
        // signals when their disabled permissions remain unchanged.
        let (engine_input_generation, control_generation) = if client.role == AccessRole::Spectator
        {
            (0, 0)
        } else {
            (
                self.engine.input_generation(actor).ok()?,
                self.control_generations
                    .get(&actor)
                    .copied()
                    .unwrap_or(Some(0))?,
            )
        };
        let mut readiness = DisclosedReadiness {
            engine_input_generation,
            control_generation,
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
        self.update_at(id, body, None);
    }

    fn update_at(&mut self, id: u64, mut body: UpdateBody, committed_tick: Option<u64>) {
        if let Some(client) = self.clients.get(&id) {
            if let UpdateBody::Observation { state, .. } = &mut body {
                *state = self.item_observation(Arc::clone(state), client.role);
            }
        }
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
            _ => committed_tick.unwrap_or(client.observation_tick),
        };
        let previous = client
            .last_observation
            .as_ref()
            .filter(|previous| previous.branch == branch)
            .map(|previous| (previous.base.clone(), previous.state.as_ref()));
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
                    snapshot: state.snapshot.clone(),
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
            snapshot: observed.state.snapshot.clone(),
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
        self.disconnect_with(id, DisconnectMode::Drain);
    }

    pub(crate) fn disconnect(&mut self, id: u64) {
        self.disconnect_with(id, DisconnectMode::Abort);
    }

    fn disconnect_with(&mut self, id: u64, mode: DisconnectMode) {
        let Some(client) = self.clients.remove(&id) else {
            return;
        };
        let _ = client.close.send(Some(mode));
        if let Some(actor) = client.actor {
            if self.controllers.get(&actor) == Some(&id) {
                self.stop_travel(actor, TravelPhase::ControlLost);
            }
            if self.controllers.get(&actor) == Some(&id) {
                self.set_controller(actor, None);
                self.pending_pauses.insert(actor);
                if !self.engine.unattended_arena() {
                    self.autonomous_enabled = false;
                }
                if !self.resetting_streams {
                    self.control_update(actor);
                }
            }
            if !self
                .clients
                .values()
                .any(|other| other.actor == Some(actor))
            {
                self.engine.unregister_input_observer(actor);
                self.control_generations.remove(&actor);
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
                self.engine.survival_save_barrier(status.durable_sequence);
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
    fn observer_capture_failure_retains_every_pending_pause_until_retry() {
        let actors = [ActorId(1), ActorId(2)];
        let mut service =
            Service::new(Engine::memory(Scenario::performance(42, 2, 2).unwrap()).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test".into(),
            actors: actors.into_iter().collect(),
        };
        let mut connection = service.connect(&account, "pause-fault".into()).unwrap();
        service.handle(
            connection.id,
            "attach".into(),
            Request::Attach { actor: actors[0] },
        );
        while connection.messages.try_recv().is_ok() {}
        for actor in actors {
            service
                .engine
                .command(
                    "p",
                    "test",
                    actor,
                    &format!("queued-{}", actor.0),
                    &service.engine.branch().clone(),
                    crate::journal::Command::AdmitIntention {
                        expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                        action: crate::journal::Action::Wait,
                    },
                )
                .unwrap();
            service.pending_pauses.insert(actor);
        }
        let before = service.engine.causal_boundary(actors[0]).unwrap();
        // A failed required observer projection must not drain pause ownership.
        service.clients.get_mut(&connection.id).unwrap().actor = Some(ActorId(u64::MAX));
        assert!(service.semantic_states().is_err());
        assert!(matches!(service.step_inner(), Step::Blocked));
        assert_eq!(service.pending_pauses, actors.into_iter().collect());
        assert_eq!(service.engine.causal_boundary(actors[0]).unwrap(), before);
        for actor in actors {
            assert_eq!(
                service.engine.pending_intentions(actor)[0].phase,
                IntentionPhase::Queued
            );
        }
        service.clients.get_mut(&connection.id).unwrap().actor = Some(actors[0]);
        service.apply_pending_pauses();
        assert!(service.pending_pauses.is_empty());
        for actor in actors {
            assert_eq!(
                service.engine.pending_intentions(actor)[0].phase,
                IntentionPhase::Suspended
            );
        }
    }

    #[test]
    fn ordered_outcomes_survive_multiple_actions_without_visual_publication() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut connection = service.connect(&account, "paced".into()).unwrap();
        service.handle(connection.id, "attach".into(), Request::Attach { actor });
        while connection.messages.try_recv().is_ok() {}
        let original = service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .clone();
        let mut expected = Vec::new();
        for index in 0..2 {
            let before = service.semantic_states().unwrap();
            let result = service
                .engine
                .command(
                    "backend",
                    "test",
                    actor,
                    &format!("ordered-{index}"),
                    &service.engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                        action: crate::journal::Action::Wait,
                    },
                )
                .unwrap();
            expected.push(result.entry.id.clone());
            service.action_result_update(&before, &result).unwrap();
        }
        let messages: Vec<_> = std::iter::from_fn(|| connection.messages.try_recv().ok()).collect();
        let outcomes: Vec<_> = messages
            .iter()
            .filter_map(|message| match message {
                ServerMessage::Update { update } => match &update.body {
                    UpdateBody::Outcome {
                        event: Some(entry),
                        facts,
                        ..
                    } => {
                        assert!(facts
                            .facts
                            .iter()
                            .any(|fact| matches!(fact, OutcomeFact::OwnAction { .. })));
                        Some((update.cursor.sequence, entry.id.clone()))
                    }
                    UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. } => {
                        panic!("unsolicited picture")
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(
            outcomes
                .iter()
                .map(|(_, id)| id.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(outcomes[0].0 < outcomes[1].0);
        assert_eq!(
            service.clients[&connection.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state
                .snapshot,
            original.snapshot
        );
        service.handle(connection.id, "picture".into(), Request::Observe);
        let current = &service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state;
        assert_ne!(current.snapshot, original.snapshot);
        assert_eq!(current.revision, original.revision + 1);
        assert!(current.observation.tick > original.observation.tick);
        let visual: Vec<_> = std::iter::from_fn(|| connection.messages.try_recv().ok()).collect();
        assert!(visual.iter().all(|message| !matches!(message, ServerMessage::Update { update } if matches!(update.body, UpdateBody::Outcome { .. }))));
    }

    #[test]
    fn invalid_committed_outcome_closes_stream_instead_of_skipping_delivery() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut connection = service.connect(&account, "bounded".into()).unwrap();
        service.handle(connection.id, "attach".into(), Request::Attach { actor });
        while connection.messages.try_recv().is_ok() {}
        let mut before = service.semantic_states().unwrap();
        let result = service
            .engine
            .command(
                "backend",
                "test",
                actor,
                "committed",
                &service.engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                    action: crate::journal::Action::Wait,
                },
            )
            .unwrap();
        before.events = service
            .engine
            .event_history_boundary_at(&service.engine.presentation_boundary())
            .unwrap();
        before.events.through += 1; // impossible provenance is rejected, never silently truncated
        service.save_warning = Some("existing storage warning".into());
        service.action_result_update(&before, &result).unwrap();
        assert!(!service.clients.contains_key(&connection.id));
        assert!(connection.close.borrow().is_some());
        assert_eq!(
            service.save_warning.as_deref(),
            Some("existing storage warning")
        );
        assert!(
            service.engine.causal_boundary(actor).unwrap() > 0,
            "committed action is not rolled back by output failure"
        );
    }

    #[test]
    fn hidden_clock_change_does_not_publish_an_outcome_or_change_display_identity() {
        let actor = ActorId(1);
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let mut connection = service.connect(&account, "private".into()).unwrap();
        service.handle(connection.id, "attach".into(), Request::Attach { actor });
        while connection.messages.try_recv().is_ok() {}
        let identity = service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .snapshot
            .clone();
        let mut before = service.semantic_states().unwrap();
        Arc::make_mut(before.views.get_mut(&actor).unwrap())
            .facts
            .tick += 1;
        service.action_update(&before, None).unwrap();
        assert!(connection.messages.try_recv().is_err());
        assert_eq!(
            service.clients[&connection.id]
                .last_observation
                .as_ref()
                .unwrap()
                .state
                .snapshot,
            identity
        );
        service.observe(connection.id).unwrap();
        let first = service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .clone();
        service.observe(connection.id).unwrap();
        let second = &service.clients[&connection.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state;
        assert_ne!(
            first.snapshot, second.snapshot,
            "identity follows demand, not hidden activity"
        );
        assert_eq!(second.revision, first.revision + 1);
        assert_eq!(first.observation, second.observation);
    }

    #[test]
    fn intentional_detach_drains_its_explanation_but_resource_disconnect_aborts() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
        let account = Account {
            role: AccessRole::Spectator,
            user: "observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut detached = service.connect(&account, "headless".into()).unwrap();
        detached.messages.try_recv().unwrap();
        service.detach_unloaded(detached.id);
        assert_eq!(*detached.close.borrow(), Some(DisconnectMode::Drain));
        assert!(matches!(
            detached.messages.try_recv().unwrap(),
            ServerMessage::Error {
                code: ErrorCode::NotAttached,
                ..
            }
        ));
        let failed = service.connect(&account, "headless".into()).unwrap();
        service.disconnect(failed.id);
        assert_eq!(*failed.close.borrow(), Some(DisconnectMode::Abort));
    }

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
            id: service.engine.target_scope(actor).door(77),
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
        let previous = disclosed.base.clone();
        assert_eq!(*disclosed.state, base);
        let mut next = base.clone();
        next.snapshot = SnapshotId("next-issued-test-picture".into());
        next.revision += 1;
        next.observation.tick += 1;
        next.observation.visible_cells.truncate(1);
        next.validate().unwrap();
        // A concrete valid delta is a witness that counting removed cells is
        // insufficient: the retained cell holds most of the full payload.
        let o = &next.observation;
        let witness = StateDelta {
            base_snapshot: base.snapshot.clone(),
            snapshot: next.snapshot.clone(),
            charts: o.charts.clone(),
            self_appearance: o.self_appearance.clone(),
            interactions: None,
            base_revision: base.revision,
            wizard_game: next.wizard_game,
            revision: next.revision,
            combat: o.combat.clone(),
            motion: o.motion.clone(),
            places: Vec::new(),
            actor: o.actor,
            self_target: o.self_target,
            linked_holds: o.linked_holds.clone(),
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
            base: previous.clone(),
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
                    action: Action::Wait,
                },
                Command::Travel {
                    expected_revision: 1,
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
                        expected_boundary: 0,
                        action: crate::journal::Action::Wait,
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
                Command::ResumeIntention { intention }
            } else {
                Command::CancelIntention {
                    step: None,
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
                snapshot: snapshot.state.snapshot.clone(),
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            }
        );
        assert_eq!(disclosed.state, snapshot.state);
        assert_eq!(client.waiting, None);
    }

    #[test]
    fn recovery_preserves_a_complete_snapshot_when_metadata_exceeds_one_frame() {
        let actor = ActorId(1);
        let account = Account {
            role: AccessRole::Spectator,
            user: "recovery-observer".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([actor]),
        };
        // Size the host limit from a real initial response, not guessed padding
        // for every possible future history or queue envelope.
        let frame_bytes = {
            let mut probe = Service::new(Engine::memory(Scenario::two_room(42)).unwrap());
            let mut peer = probe.connect(&account, "headless".into()).unwrap();
            probe.handle(peer.id, "attach".into(), Request::Attach { actor });
            let mut snapshot_bytes = None;
            while let Ok(frame) = peer.messages.try_recv_frame() {
                if matches!(
                    decode_response(&frame.text).unwrap(),
                    ServerMessage::Snapshot { .. }
                ) {
                    snapshot_bytes = Some(frame.text.len());
                }
            }
            snapshot_bytes.expect("initial snapshot") + 256
        };
        let mut service = Service::with_outbound_limits(
            Engine::memory(Scenario::two_room(42)).unwrap(),
            crate::OutboundLimits {
                frame_bytes,
                client_bytes: 4 * frame_bytes,
                total_bytes: 16 * frame_bytes,
            },
        )
        .unwrap();
        let mut peer = service.connect(&account, "headless".into()).unwrap();
        service.handle(peer.id, "attach".into(), Request::Attach { actor });
        while peer.messages.try_recv_frame().is_ok() {}
        assert!(service.clients.contains_key(&peer.id));
        for _ in 0..4 {
            service
                .annotate_backend(
                    actor,
                    "recovery-test",
                    Anchor::State {
                        snapshot: SnapshotId("trusted-recovery-note".into()),
                    },
                    AnnotationCategory::Note,
                    &"x".repeat(256),
                )
                .unwrap();
            while let Ok(frame) = peer.messages.try_recv_frame() {
                assert!(frame.text.len() <= frame_bytes);
            }
            assert!(
                service.clients.contains_key(&peer.id),
                "individual updates fit"
            );
        }
        let expected_state = service.engine.state(actor).unwrap();
        assert!(serde_json::to_vec(&expected_state).unwrap().len() < frame_bytes);
        let expected_history = service
            .engine
            .history(actor, &account.user, None, MAX_HISTORY_PAGE)
            .unwrap();
        service.snapshot(peer.id, "recover").unwrap();
        assert!(
            service.clients.contains_key(&peer.id),
            "a bounded state must remain recoverable when its complete snapshot spans frames"
        );
        let ServerMessage::Snapshot {
            request_id,
            snapshot,
        } = peer.messages.try_recv().unwrap()
        else {
            panic!("recovery must publish one complete logical snapshot");
        };
        assert_eq!(request_id, "recover");
        assert_eq!(*snapshot.state, expected_state);
        assert_eq!(
            snapshot.history, expected_history,
            "recovery cannot prune history to fit"
        );
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
            assert!(connection.close.borrow().is_some());
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
                    expected_boundary: 0,
                    action: crate::journal::Action::Move {
                        direction: crate::journal::Direction::East,
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
                    step: None,
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
        let revision = service.engine.causal_boundary(ActorId(1)).unwrap();
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
            slow.close.borrow().is_some(),
            "byte pressure must disconnect a slow stream"
        );
        assert!(healthy.close.borrow().is_none());
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
        assert_eq!(
            service.engine.causal_boundary(ActorId(1)).unwrap(),
            revision
        );
    }

    #[test]
    fn paid_identification_and_instance_counts_are_owner_only_on_encoded_updates() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let item = ItemTarget::from_digest([72; 32]);
        let mut state = service.engine.state(ActorId(1)).unwrap();
        state.observation.inventory.push(serde_json::from_value(serde_json::json!({
            "id":item,"class":"wand","quantity":"1","appearance":"ivory wand","identified":true,"name":"Known wand","known_currency":null,"known_supply":null
        })).unwrap());
        let coin = ItemTarget::from_digest([84; 32]);
        state.observation.inventory.push(
            serde_json::from_value(serde_json::json!({
                "id":coin,"class":"coin","quantity":"9007199254740993",
                "appearance":"unmarked discs","identified":true,"name":"Known tender",
                "known_currency":"gold","known_supply":null
            }))
            .unwrap(),
        );
        let public = Arc::new(state.clone());
        for role in [AccessRole::Player, AccessRole::Spectator] {
            assert!(Arc::ptr_eq(
                &service.item_observation(Arc::clone(&public), role),
                &public
            ));
        }
        state.observation.interactions = Some(InteractionView {
            curse_blocked: vec![],
            completed: vec![],
            refused: vec![],
            slots: vec![],
            preparation: Some(PreparationView {
                action: Action::Zap {
                    item,
                    aim: tor_protocol::DeviceAim::Direction {
                        direction: Direction::North,
                    },
                },
                remaining: 1,
                active: true,
            }),
            identification: Some(IdentificationChoiceView {
                choice: IntentionId("private-paid-choice".into()),
                classes: vec![ItemClass::Wand],
                items: vec![item],
            }),
            inventory: vec![ItemInteractionView {
                portable_light: None,
                item,
                slot: None,
                equipped_slot: None,
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: Some(ProjectileBonusView { hit: -2, damage: 3 }),
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: true,
                device_activation: Some(tor_protocol::DeviceActivation::Direction),
                charges: Some(7),
            }],
        });
        state.revision += 1;
        let shared = Arc::new(state.clone());
        let mut retained_projections = BTreeMap::new();
        for role in [
            AccessRole::Player,
            AccessRole::Spectator,
            AccessRole::Spectator,
            AccessRole::Player,
        ] {
            let account = Account {
                role,
                user: format!("{role:?}"),
                token: "test-only".into(),
                actors: BTreeSet::from([ActorId(1)]),
            };
            let mut connection = service.connect(&account, "headless".into()).unwrap();
            connection.messages.try_recv().unwrap();
            service.handle(
                connection.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            while connection.messages.try_recv().is_ok() {}
            service.update(
                connection.id,
                UpdateBody::Observation {
                    state: Arc::clone(&shared),
                    event: None,
                },
            );
            let frame = connection.messages.try_recv().unwrap();
            let text = serde_json::to_string(&frame).unwrap();
            assert_eq!(
                text.contains("private-paid-choice"),
                role == AccessRole::Player
            );
            let retained = Arc::clone(
                &service.clients[&connection.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state,
            );
            let owner = role == AccessRole::Player;
            assert_eq!(Arc::ptr_eq(&retained, &shared), owner);
            if let Some(previous) = retained_projections.insert(owner, Arc::clone(&retained)) {
                assert!(
                    Arc::ptr_eq(&previous, &retained),
                    "equal role projections must share storage"
                );
            }
            let disclosed = &retained.observation;
            let tender = disclosed
                .inventory
                .iter()
                .find(|row| row.id == coin)
                .unwrap();
            assert_eq!(tender.known_currency, Some(CurrencyKind::Gold));
            assert_eq!(tender.quantity, 9_007_199_254_740_993);
            assert!(text.contains("\"known_currency\":\"gold\""));
            let interaction = disclosed.interactions.as_ref().unwrap();
            assert_eq!(
                interaction.identification.is_some(),
                role == AccessRole::Player
            );
            assert_eq!(
                interaction.inventory[0].charges,
                (role == AccessRole::Player).then_some(7)
            );
            assert!(
                interaction.preparation.is_some(),
                "ordinary preparation remains visible"
            );
            assert!(interaction.inventory[0].zappable);
            assert_eq!(
                interaction.inventory[0].known_projectile_bonus,
                owner.then_some(ProjectileBonusView { hit: -2, damage: 3 })
            );
            // Bonus alone must trigger the shared-observation sanitation fast path.
            let mut bonus_only = state.clone();
            let facts = bonus_only.observation.interactions.as_mut().unwrap();
            facts.identification = None;
            facts.inventory[0].charges = None;
            let projected = service.item_observation(Arc::new(bonus_only), role);
            assert_eq!(
                projected
                    .observation
                    .interactions
                    .as_ref()
                    .unwrap()
                    .inventory[0]
                    .known_projectile_bonus,
                owner.then_some(ProjectileBonusView { hit: -2, damage: 3 })
            );
            // The same restriction owns snapshots as well as delta bases.
            let mut snapshot_state = state.clone();
            restrict_item_disclosure(&mut snapshot_state, role);
            assert_eq!(snapshot_state.observation, *disclosed);
        }
        assert_eq!(
            shared.as_ref(),
            &state,
            "spectator projection cannot mutate shared owner state"
        );
    }

    #[test]
    fn each_private_equipment_fact_is_sanitized_without_losing_public_arc_sharing() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let item = ItemTarget::from_digest([73; 32]);
        let mut public = service.engine.state(ActorId(1)).unwrap();
        public.observation.inventory.push(
            serde_json::from_value(serde_json::json!({
                "id":item,"class":"ring","quantity":"1","appearance":"silver ring",
                "identified":true,"name":"Known ring","known_currency":null,"known_supply":null
            }))
            .unwrap(),
        );
        public.observation.interactions = Some(InteractionView {
            curse_blocked: vec![],
            completed: vec![],
            refused: vec![],
            slots: vec![],
            preparation: None,
            identification: None,
            inventory: vec![ItemInteractionView {
                portable_light: None,
                item,
                slot: Some(EquipmentSlot {
                    id: "fixture_ring".into(),
                    name: "Ring".into(),
                    equipped_phrase: "worn".into(),
                    requires_attack: false,
                }),
                equipped_slot: None,
                known_equipment: None,
                known_projectile_bonus: None,
                known_cursed: None,
                known_protected: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                device_activation: None,
                charges: None,
            }],
        });
        public.revision += 1;
        assert!(public.validate().is_ok());
        let public = Arc::new(public);
        for role in [AccessRole::Player, AccessRole::Spectator] {
            assert!(
                Arc::ptr_eq(
                    &public,
                    &service.item_observation(Arc::clone(&public), role)
                ),
                "truly public updates must preserve shared storage"
            );
        }
        for fact in 0..6 {
            let mut owner = public.as_ref().clone();
            let interaction = owner.observation.interactions.as_mut().unwrap();
            match fact {
                0 => {
                    interaction.inventory[0].known_equipment = Some(EquipmentView {
                        attack: None,
                        modifiers: vec![],
                    })
                }
                1 => interaction.inventory[0].known_cursed = Some(true),
                2 => interaction.inventory[0].known_protected = Some(true),
                3 => interaction.curse_blocked.push(item),
                4 => interaction.inventory[0].known_cursed = Some(false),
                _ => interaction.inventory[0].known_protected = Some(false),
            }
            assert!(owner.validate().is_ok(), "fact {fact}");
            let owner = Arc::new(owner);
            let owner_projection = service.item_observation(Arc::clone(&owner), AccessRole::Player);
            assert!(Arc::ptr_eq(&owner, &owner_projection));
            let projected = service.item_observation(Arc::clone(&owner), AccessRole::Spectator);
            assert_eq!(
                projected.as_ref(),
                public.as_ref(),
                "private fact {fact} leaked through the fast path"
            );
            assert!(!Arc::ptr_eq(&owner, &projected));
            for role in [AccessRole::Player, AccessRole::Spectator] {
                let account = Account {
                    role,
                    user: format!("{role:?}-{fact}"),
                    token: "test-only".into(),
                    actors: BTreeSet::from([ActorId(1)]),
                };
                let mut connection = service.connect(&account, "headless".into()).unwrap();
                connection.messages.try_recv().unwrap();
                service.handle(
                    connection.id,
                    "attach".into(),
                    Request::Attach { actor: ActorId(1) },
                );
                while connection.messages.try_recv().is_ok() {}
                let base = Arc::clone(
                    &service.clients[&connection.id]
                        .last_observation
                        .as_ref()
                        .unwrap()
                        .state,
                );
                service.update(
                    connection.id,
                    UpdateBody::Observation {
                        state: Arc::clone(&owner),
                        event: None,
                    },
                );
                let frame = connection.messages.try_recv_frame().unwrap();
                let ServerMessage::Update { update } = decode_response(&frame.text).unwrap() else {
                    panic!("observation update expected")
                };
                let received = match update.body {
                    UpdateBody::Observation { state, .. } => state.as_ref().clone(),
                    UpdateBody::ObservationDelta { state, .. } => state.apply(&base).unwrap(),
                    _ => panic!("observation body expected"),
                };
                let retained = &service.clients[&connection.id]
                    .last_observation
                    .as_ref()
                    .unwrap()
                    .state;
                let expected = if role == AccessRole::Spectator {
                    &public
                } else {
                    &owner
                };
                assert_eq!(
                    retained.as_ref(),
                    expected.as_ref(),
                    "retained update role {role:?}, fact {fact}"
                );
                assert_eq!(
                    &received,
                    expected.as_ref(),
                    "decoded update role {role:?}, fact {fact}"
                );
                let mut snapshot = owner.as_ref().clone();
                restrict_item_disclosure(&mut snapshot, role);
                assert_eq!(&snapshot, expected.as_ref());
            }
        }
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
        assert_eq!(
            service
                .connect(&account, "rejected".into())
                .err()
                .unwrap()
                .code,
            ErrorCode::ResourceLimit
        );
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
                    panic!("initial snapshot")
                };
                connections.push((client, snapshot));
            }
            assert_eq!(
                connections
                    .iter()
                    .map(|(_, snapshot)| snapshot.context.stream.0.clone())
                    .collect::<BTreeSet<_>>()
                    .len(),
                watchers
            );
            assert_eq!(
                connections
                    .iter()
                    .map(|(_, snapshot)| snapshot.state.snapshot.clone())
                    .collect::<BTreeSet<_>>()
                    .len(),
                watchers,
                "capabilities belong to attachments, not shared display counters"
            );
            // The test transport decodes serialized messages, so the received
            // snapshot has an independent Arc. Preserve the server allocations
            // separately to prove that causal updates never replace them.
            let retained_pictures: BTreeMap<_, _> = connections
                .iter()
                .map(|(client, _)| {
                    (
                        client.id,
                        Arc::clone(
                            &service.clients[&client.id]
                                .last_observation
                                .as_ref()
                                .unwrap()
                                .state,
                        ),
                    )
                })
                .collect();
            for (index, action) in [
                crate::journal::Action::Wait,
                crate::journal::Action::Move {
                    direction: crate::journal::Direction::East,
                },
                crate::journal::Action::Move {
                    direction: crate::journal::Direction::West,
                },
                crate::journal::Action::Wait,
            ]
            .into_iter()
            .enumerate()
            {
                let before_states = service.semantic_states().unwrap();
                let result = service
                    .engine
                    .command(
                        "test",
                        "test",
                        ActorId(1),
                        &format!("wait-{index}"),
                        &service.engine.branch().clone(),
                        crate::journal::Command::Act {
                            expected_boundary: service.engine.causal_boundary(ActorId(1)).unwrap(),
                            action,
                        },
                    )
                    .unwrap();
                let before = tor_simulation::diagnostics::work_counts();
                let pictures_before = Engine::picture_adaptation_count();
                service
                    .action_result_update(&before_states, &result)
                    .unwrap();
                let after = tor_simulation::diagnostics::work_counts();
                assert_eq!(
                    Engine::picture_adaptation_count() - pictures_before,
                    if index == 1 || index == 2 { 2 } else { 0 },
                    "canonical fallback is once per actor, not per {watchers} watchers"
                );
                assert_eq!(
                    after.observations - before.observations,
                    1,
                    "watchers: {watchers}"
                );
                for (client, snapshot) in &mut connections {
                    let ServerMessage::Update { update } = client.messages.try_recv().unwrap()
                    else {
                        panic!("outcome update")
                    };
                    assert_eq!(
                        update.cursor.sequence,
                        snapshot.cursor.sequence + index as u64 + 1
                    );
                    assert_eq!(update.branch, snapshot.branch);
                    let UpdateBody::Outcome {
                        event: Some(event),
                        facts,
                        ..
                    } = update.body
                    else {
                        panic!("ordered outcome")
                    };
                    assert_eq!(event.id, result.entry.id);
                    assert!(facts
                        .facts
                        .iter()
                        .any(|fact| matches!(fact, OutcomeFact::OwnAction { .. })));
                    let retained = &service.clients[&client.id]
                        .last_observation
                        .as_ref()
                        .unwrap()
                        .state;
                    assert!(
                        Arc::ptr_eq(retained, &retained_pictures[&client.id]),
                        "causal updates do not mutate or replace retained pictures"
                    );
                    assert_eq!(retained.as_ref(), snapshot.state.as_ref());
                }
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
                    expected_revision: service.clients[&observer.id]
                        .last_observation
                        .as_ref()
                        .unwrap()
                        .state
                        .revision,
                    operation: "rewind initial".into(),
                },
            },
        );
        assert!(!service.autonomous_enabled);
        assert!(controller.close.borrow().is_some());
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
                    action: Action::Wait,
                },
            },
        );
        assert!(controller.close.borrow().is_some());
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
    fn action_broadcast_survives_cascaded_backpressure_disconnects() {
        let mut service = Service::new(Engine::memory(Scenario::two_room(0)).unwrap());
        let account = Account {
            role: AccessRole::Player,
            user: "owner".into(),
            token: "test-only".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut clients = Vec::new();
        for index in 0..3 {
            let mut client = service
                .connect(&account, format!("reader-{index}"))
                .unwrap();
            while client.messages.try_recv().is_ok() {}
            service.handle(
                client.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            while client.messages.try_recv().is_ok() {}
            clients.push(client);
        }
        service.handle(clients[0].id, "control".into(), Request::AcquireControl);
        for client in &mut clients {
            while client.messages.try_recv().is_ok() {}
        }
        for client in &clients[..2] {
            for _ in 0..QUEUE {
                service.send(client.id, ServerMessage::Waiting { on: Waiting::You });
            }
            assert!(service.clients.contains_key(&client.id));
        }
        let before_states = service.semantic_states().unwrap();
        let result = service
            .engine
            .command(
                "test",
                "test",
                ActorId(1),
                "wait",
                &service.engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: 0,
                    action: crate::journal::Action::Wait,
                },
            )
            .unwrap();
        service
            .action_result_update(&before_states, &result)
            .unwrap();
        for client in &clients[..2] {
            assert!(client.close.borrow().is_some());
            assert!(!service.clients.contains_key(&client.id));
        }
        assert!(!service.controllers.contains_key(&ActorId(1)));
        let survivor = &mut clients[2];
        assert!(service.clients.contains_key(&survivor.id));
        let messages: Vec<_> = std::iter::from_fn(|| survivor.messages.try_recv().ok()).collect();
        assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::Update { update } if matches!(update.body,
                UpdateBody::Outcome { .. }))));
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
        assert!(connection.close.borrow().is_some());
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
        service.handle(
            client.id,
            "wait".into(),
            Request::Command {
                context: service.input_context(client.id).unwrap(),
                branch: service.engine.branch().clone(),
                command: Command::Act {
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
        let before = service.engine.causal_boundary(ActorId(1)).unwrap();
        service.run_until_blocked();
        assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
        assert_ne!(service.engine.causal_boundary(ActorId(1)).unwrap(), before);
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
        let before = service.engine.causal_boundary(ActorId(1)).unwrap();
        let Step::Full(full) = service.step() else {
            panic!("the run must wait for the client");
        };
        assert_eq!(
            full.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [client.id]
        );
        assert_eq!(service.engine.causal_boundary(ActorId(1)).unwrap(), before);
        assert!(client.close.borrow().is_none());
        while client.messages.try_recv().is_ok() {}
        service.run_until_blocked();
        assert_eq!(service.engine.next_actor(), Some(ActorId(1)));
        assert_ne!(service.engine.causal_boundary(ActorId(1)).unwrap(), before);
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
        let revision = service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .revision;
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

    #[test]
    fn combat_diagnostics_query_is_private_and_rejects_stale_controls() {
        let mut engine = Engine::memory(crate::Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let mut clients = Vec::new();
        for (user, role) in [
            ("wizard", AccessRole::Wizard),
            ("player", AccessRole::Player),
        ] {
            let mut client = service
                .connect(
                    &Account {
                        role,
                        user: user.into(),
                        token: "test".into(),
                        actors: BTreeSet::from([ActorId(1)]),
                    },
                    "headless".into(),
                )
                .unwrap();
            while client.messages.try_recv().is_ok() {}
            service.handle(
                client.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            while client.messages.try_recv().is_ok() {}
            clients.push(client);
        }
        let before = service.engine.state(ActorId(1)).unwrap();
        let history = service
            .engine
            .history(ActorId(1), "wizard", None, 100)
            .unwrap();
        for (index, case, operation, expected) in [
            (1, 0, "combat capture on", Some(ErrorCode::Unauthorized)),
            (1, 0, "combat inspect bad", Some(ErrorCode::Unauthorized)),
            (0, 1, "combat capture on", Some(ErrorCode::StaleRevision)),
            (0, 2, "combat capture on", Some(ErrorCode::WrongBranch)),
            (0, 3, "combat capture on", Some(ErrorCode::StaleContext)),
            (0, 0, "combat capture on", None),
            (0, 1, "combat capture off", Some(ErrorCode::StaleRevision)),
            (0, 0, "combat inspect", None),
            (0, 0, "combat inspect 1", Some(ErrorCode::InvalidAction)),
            (0, 0, "combat capture off", None),
        ] {
            let enabled_before = service
                .engine
                .inspect_combat_diagnostics(None)
                .unwrap()
                .enabled;
            let id = clients[index].id;
            let mut context = service.input_context(id).unwrap();
            if case == 3 {
                context.readiness_revision += 1;
            }
            service.handle(
                id,
                "diagnostics".into(),
                Request::Command {
                    context,
                    branch: if case == 2 {
                        BranchId("absent".into())
                    } else {
                        service.engine.branch().clone()
                    },
                    command: Command::Wizard {
                        expected_revision: service.clients[&id]
                            .last_observation
                            .as_ref()
                            .unwrap()
                            .state
                            .revision
                            + u64::from(case == 1),
                        operation: operation.into(),
                    },
                },
            );
            let messages: Vec<_> =
                std::iter::from_fn(|| clients[index].messages.try_recv().ok()).collect();
            if let Some(code) = expected {
                assert!(
                    messages.iter().any(|message| matches!(message,
                    ServerMessage::Error { code: actual, .. } if *actual == code)),
                    "{messages:?}"
                );
                assert!(!messages
                    .iter()
                    .any(|message| matches!(message, ServerMessage::CombatDiagnostics { .. })));
                assert_eq!(
                    service
                        .engine
                        .inspect_combat_diagnostics(None)
                        .unwrap()
                        .enabled,
                    enabled_before
                );
            } else {
                let (context, report) = messages
                    .iter()
                    .find_map(|message| match message {
                        ServerMessage::CombatDiagnostics {
                            context,
                            request_id,
                            report,
                        } if request_id == "diagnostics" => Some((context, report)),
                        _ => None,
                    })
                    .expect("private diagnostics reply");
                assert!(report.validate().is_ok());
                assert_eq!(report.enabled, operation != "combat capture off");
                assert_eq!(context.actor, ActorId(1));
                assert_eq!(context.input, service.input_context(id).unwrap());
            }
            assert!(clients[1 - index].messages.try_recv().is_err());
            assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
            assert_eq!(
                service
                    .engine
                    .history(ActorId(1), "wizard", None, 100)
                    .unwrap(),
                history
            );
        }
    }

    #[test]
    fn creature_inspection_query_is_private_readonly_and_contextual() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mob-arena");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let mut clients = Vec::new();
        for (user, role) in [
            ("wizard", AccessRole::Wizard),
            ("player", AccessRole::Player),
        ] {
            let mut client = service
                .connect(
                    &Account {
                        role,
                        user: user.into(),
                        token: "test".into(),
                        actors: BTreeSet::from([ActorId(1)]),
                    },
                    "headless".into(),
                )
                .unwrap();
            while client.messages.try_recv().is_ok() {}
            service.handle(
                client.id,
                "attach".into(),
                Request::Attach { actor: ActorId(1) },
            );
            while client.messages.try_recv().is_ok() {}
            clients.push(client);
        }
        let before = service.engine.state(ActorId(1)).unwrap();
        let history = service
            .engine
            .history(ActorId(1), "wizard", None, 100)
            .unwrap();
        for (index, target) in [(0, 2), (1, 2), (1, 999)] {
            let id = clients[index].id;
            service.handle(
                id,
                "inspect".into(),
                Request::Command {
                    context: service.input_context(id).unwrap(),
                    branch: service.engine.branch().clone(),
                    command: Command::Wizard {
                        expected_revision: service.clients[&id]
                            .last_observation
                            .as_ref()
                            .unwrap()
                            .state
                            .revision,
                        operation: format!("creature inspect {target}"),
                    },
                },
            );
            let messages: Vec<_> =
                std::iter::from_fn(|| clients[index].messages.try_recv().ok()).collect();
            if index == 0 {
                let (context, report) = messages
                    .iter()
                    .find_map(|message| match message {
                        ServerMessage::CreatureInspection {
                            context,
                            request_id,
                            report,
                        } if request_id == "inspect" => Some((context, report)),
                        _ => None,
                    })
                    .expect("private inspection reply");
                assert_eq!(report.actor, ActorId(2));
                assert_eq!(report.name, "blue sentinel");
                assert!(report.validate().is_ok());
                assert_eq!(context.actor, ActorId(1));
                assert_eq!(context.input, service.input_context(id).unwrap());
                assert!(clients[1].messages.try_recv().is_err());
            } else {
                assert!(messages.iter().any(|message| matches!(
                    message,
                    ServerMessage::Error {
                        code: ErrorCode::Unauthorized,
                        ..
                    }
                )));
                assert!(!messages
                    .iter()
                    .any(|message| matches!(message, ServerMessage::CreatureInspection { .. })));
            }
            assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
            assert_eq!(
                service
                    .engine
                    .history(ActorId(1), "wizard", None, 100)
                    .unwrap(),
                history
            );
        }
        for (case, code) in [
            (0, ErrorCode::StaleRevision),
            (1, ErrorCode::WrongBranch),
            (2, ErrorCode::StaleContext),
            (3, ErrorCode::InvalidAction),
        ] {
            let id = clients[0].id;
            let mut context = service.input_context(id).unwrap();
            if case == 2 {
                context.readiness_revision += 1;
            }
            service.handle(
                id,
                format!("invalid-{case}"),
                Request::Command {
                    context,
                    branch: if case == 1 {
                        BranchId("absent".into())
                    } else {
                        service.engine.branch().clone()
                    },
                    command: Command::Wizard {
                        expected_revision: service.clients[&id]
                            .last_observation
                            .as_ref()
                            .unwrap()
                            .state
                            .revision
                            + u64::from(case == 0),
                        operation: format!("creature inspect {}", if case == 3 { 999 } else { 2 }),
                    },
                },
            );
            let messages: Vec<_> =
                std::iter::from_fn(|| clients[0].messages.try_recv().ok()).collect();
            assert!(messages.iter().any(|message| matches!(message, ServerMessage::Error { code: actual, .. } if *actual == code)), "{messages:?}");
            assert!(!messages
                .iter()
                .any(|message| matches!(message, ServerMessage::CreatureInspection { .. })));
            assert_eq!(service.engine.state(ActorId(1)).unwrap(), before);
        }
    }
}

/// Running until blocked: journeys, outgoing queues and slow clients, and the
/// runner over its mailbox (see docs/architecture/simulation.md).
#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;

#[cfg(test)]
#[path = "strike_page_session_tests.rs"]
mod strike_page_tests;

#[cfg(test)]
#[path = "advancement_session_tests.rs"]
mod advancement_tests;

#[cfg(test)]
#[path = "../test_support/public_plan_sessions.rs"]
mod public_plan_observation_tests;

#[cfg(test)]
#[path = "locomotion_session_tests.rs"]
mod locomotion_tests;

#[cfg(test)]
#[path = "perception_session_tests.rs"]
mod perception_tests;

#[cfg(test)]
#[path = "corrosion_session_tests.rs"]
mod corrosion_tests;

#[cfg(test)]
#[path = "ray_session_tests.rs"]
mod ray_tests;

#[cfg(test)]
#[path = "magic_item_session_tests.rs"]
mod magic_item_tests;

#[cfg(test)]
#[path = "weapon_selection_session_tests.rs"]
mod weapon_selection_tests;

#[cfg(test)]
mod input_operand_context_tests {

    #[test]
    fn spectator_readiness_ignores_owner_inventory_and_control_generations() {
        for inventory_change in [true, false] {
            let (mut service, controller) = fixture();
            let actor = ActorId(1);
            let account = Account {
                role: AccessRole::Spectator,
                user: "readiness-spectator".into(),
                token: "spectator-test".into(),
                actors: BTreeSet::from([actor]),
            };
            let mut spectator = service.connect(&account, "headless".into()).unwrap();
            service.handle(
                spectator.id,
                "attach-spectator".into(),
                Request::Attach { actor },
            );
            while spectator.messages.try_recv().is_ok() {}
            let spectator_context = service.input_context(spectator.id).unwrap();
            let spectator_readiness = service.current_readiness(spectator.id).unwrap();
            assert!(!spectator_readiness.permissions.admission);
            assert!(spectator_readiness.permissions.resume.is_empty());
            assert!(spectator_readiness.permissions.cancel.is_empty());
            let controller_readiness = service.current_readiness(controller.id).unwrap();
            let before = service.engine.state(actor).unwrap();
            if inventory_change {
                // Inventory presence is public. Its private authority generation must
                // nevertheless not turn spectator readiness into an owner-input clock.
                let item = before.observation.ground_items[0].item.id;
                let generation = service.engine.input_generation(actor).unwrap();
                let wire = Command::Act {
                    action: Action::Take {
                        item,
                        quantity: None,
                    },
                };
                let decoded = crate::wire_adapter::decode_command(&wire).unwrap();
                let branch = service.engine.branch().clone();
                let command = service
                    .engine
                    .resolve_command(actor, &branch, decoded)
                    .unwrap();
                service
                    .engine
                    .command(
                        "input-test",
                        "headless",
                        actor,
                        "take-for-authority",
                        &branch,
                        command,
                    )
                    .unwrap();
                service.engine.execute_next_intention().unwrap().unwrap();
                assert!(service
                    .engine
                    .state(actor)
                    .unwrap()
                    .observation
                    .inventory
                    .iter()
                    .any(|held| held.id == item));
                assert!(service.engine.input_generation(actor).unwrap() > generation);
            } else {
                assert!(service.set_controller(actor, None));
                assert!(service.set_controller(actor, Some(controller.id)));
                assert_eq!(
                    service.engine.state(actor).unwrap(),
                    before,
                    "control ABA changes authority, not the disclosed observation"
                );
            }
            assert_ne!(
                service.current_readiness(controller.id).unwrap(),
                controller_readiness,
                "owner authority must still invalidate"
            );
            assert_eq!(
                service.current_readiness(spectator.id).unwrap(),
                spectator_readiness,
                "spectator permissions and input context must not expose owner generation changes"
            );
            service.refresh_readiness();
            assert_eq!(
                service.input_context(spectator.id).unwrap(),
                spectator_context
            );
            let updates: Vec<_> =
                std::iter::from_fn(|| spectator.messages.try_recv().ok()).collect();
            assert!(
                !updates.iter().any(|message| matches!(message,
                    ServerMessage::Update { update, .. }
                        if matches!(&update.body, UpdateBody::Readiness { .. })
                )),
                "spurious spectator readiness update: {updates:?}"
            );
        }
    }

    #[test]
    fn input_registration_survives_one_viewer_and_ends_with_last_disconnect() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Player,
            user: "registration-test".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let first = service.connect(&account, "first-viewer".into()).unwrap();
        service.handle(first.id, "attach-first".into(), Request::Attach { actor });
        assert_eq!(service.engine.input_generation(actor).unwrap(), 0);
        assert!(
            service.controllers.is_empty(),
            "observers acquire no control"
        );
        teleport(
            &mut service,
            actor,
            "registration-move",
            Position {
                region: 2,
                x: 1,
                y: 1,
                z: 0,
            },
        );
        let generation = service.engine.input_generation(actor).unwrap();
        assert!(generation > 0, "positive noninitial registration witness");
        let second = service.connect(&account, "second-viewer".into()).unwrap();
        service.handle(second.id, "attach-second".into(), Request::Attach { actor });
        assert_eq!(service.engine.input_generation(actor).unwrap(), generation);
        assert_eq!(service.clients[&first.id].actor, Some(actor));
        assert_eq!(service.clients[&second.id].actor, Some(actor));
        service.disconnect(first.id);
        assert!(!service.clients.contains_key(&first.id));
        assert!(service.clients.contains_key(&second.id));
        assert_eq!(service.engine.input_generation(actor).unwrap(), generation);
        assert!(service.pending_pauses.is_empty());
        service.disconnect(second.id);
        assert!(service.engine.input_generation(actor).is_err());
        assert!(!service.control_generations.contains_key(&actor));
        assert!(service.pending_pauses.is_empty());
    }
    use super::*;
    use crate::journal::{Command as BackendCommand, Position, WizardOperation};
    use crate::{ActorSetup, Scenario};

    fn fixture() -> (Service, Connection) {
        let mut scenario = Scenario::two_room(42);
        scenario.actors.push(ActorSetup {
            position: Position {
                region: 1,
                x: 2,
                y: 1,
                z: 0,
            },
            turn_ticks: 100,
        });
        let mut engine = Engine::memory(scenario).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Player,
            user: "input-test".into(),
            token: "test".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        (service, client)
    }
    fn teleport(service: &mut Service, target: ActorId, request: &str, position: Position) {
        let actor = ActorId(1);
        service
            .engine
            .command(
                "test",
                "test",
                actor,
                request,
                &service.engine.branch().clone(),
                BackendCommand::Wizard {
                    display_basis: None,
                    expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                    operation: WizardOperation::Teleport {
                        actor: target,
                        position,
                    },
                },
            )
            .unwrap();
    }
    fn wait(service: &mut Service, client: &Connection, context: InputContext, request: &str) {
        service.handle(
            client.id,
            request.into(),
            Request::Command {
                context,
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    action: Action::Wait,
                },
            },
        );
    }
    #[test]
    fn unrelated_display_change_accepts_original_context_and_retry_exactly_once() {
        let (mut service, mut client) = fixture();
        let context = service.input_context(client.id).unwrap();
        let revision = service.engine.causal_boundary(ActorId(1)).unwrap();
        teleport(
            &mut service,
            ActorId(2),
            "neighbor",
            Position {
                region: 1,
                x: 3,
                y: 1,
                z: 0,
            },
        );
        assert!(service.engine.causal_boundary(ActorId(1)).unwrap() > revision);
        assert_eq!(
            service.current_readiness(client.id),
            service.clients[&client.id].readiness
        );
        wait(&mut service, &client, context.clone(), "original");
        let admitted: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            admitted
                .iter()
                .any(|m| matches!(m, ServerMessage::Ack { .. })),
            "{admitted:?}"
        );
        assert!(service.engine.has_pending_intention(ActorId(1)));
        let sequence = service.engine.save_status().accepted_sequence;
        // The admission changed input authority. Exact receipt retry still wins first.
        assert_ne!(service.input_context(client.id).unwrap(), context);
        wait(&mut service, &client, context, "original");
        let retry: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            retry.iter().any(|m| matches!(m, ServerMessage::Ack { .. })),
            "{retry:?}"
        );
        assert_eq!(service.engine.save_status().accepted_sequence, sequence);
    }
    #[test]
    fn origin_and_control_aba_reject_without_publication_or_admission() {
        for control in [false, true] {
            let (mut service, mut client) = fixture();
            let context = service.input_context(client.id).unwrap();
            if control {
                assert!(service.set_controller(ActorId(1), None));
                assert!(service.set_controller(ActorId(1), Some(client.id)));
            } else {
                teleport(
                    &mut service,
                    ActorId(1),
                    "away",
                    Position {
                        region: 2,
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                );
                teleport(
                    &mut service,
                    ActorId(1),
                    "back",
                    Position {
                        region: 1,
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                );
            }
            assert_eq!(
                service.input_context(client.id).unwrap(),
                context,
                "no intermediate publication"
            );
            let sequence = service.engine.save_status().accepted_sequence;
            wait(&mut service, &client, context, "stale");
            let replies: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            assert!(
                replies.iter().any(|m| matches!(
                    m,
                    ServerMessage::Error {
                        code: ErrorCode::StaleContext,
                        ..
                    }
                )),
                "{replies:?}"
            );
            assert!(!service.engine.has_pending_intention(ActorId(1)));
            assert_eq!(service.engine.save_status().accepted_sequence, sequence);
        }
    }
}

#[path = "presentation_queries.rs"]
mod presentation_queries;
