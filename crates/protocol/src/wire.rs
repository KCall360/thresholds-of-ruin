use crate::{ActorId, ActorTarget, DoorTarget, ItemTarget, StreamContext, StreamCursor};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 30;
/// Static limits of this authenticated server. Available capacity is not
/// advertised: it can change between welcome and the next request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerCapabilities {
    pub max_request_bytes: u32,
    pub max_response_bytes: u32,
    pub max_retained_state_bytes: u32,
    pub max_snapshot_bytes: u32,
    pub max_connections: u32,
    pub max_history_page_entries: u32,
}

impl ServerCapabilities {
    /// Advertise native protocol bounds with the host's response and connection limits.
    pub fn new(max_response_bytes: u32, max_connections: u32) -> Self {
        Self {
            max_request_bytes: crate::MAX_REQUEST_BYTES as u32,
            max_response_bytes,
            max_retained_state_bytes: crate::MAX_STATE_BYTES as u32,
            max_snapshot_bytes: crate::MAX_SNAPSHOT_BYTES as u32,
            max_connections,
            max_history_page_entries: MAX_HISTORY_PAGE as u32,
        }
    }

    /// Reject unsupported or nonsensical limits before attaching an actor.
    pub fn is_valid(self) -> bool {
        self.max_request_bytes > 0
            && self.max_request_bytes <= crate::MAX_REQUEST_BYTES as u32
            && self.max_response_bytes > 0
            && self.max_response_bytes <= crate::MAX_RESPONSE_BYTES as u32
            && self.max_retained_state_bytes > 0
            && self.max_retained_state_bytes <= crate::MAX_STATE_BYTES as u32
            && self.max_snapshot_bytes >= self.max_retained_state_bytes
            && self.max_snapshot_bytes <= crate::MAX_SNAPSHOT_BYTES as u32
            && self.max_connections > 0
            && self.max_history_page_entries > 0
            && self.max_history_page_entries <= MAX_HISTORY_PAGE as u32
    }
}

/// Server-granted session authority; never selected by the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessRole {
    Player,
    Spectator,
    Wizard,
}

impl AccessRole {
    /// Only explicitly enumerated reads are available to spectators. New requests
    /// stay denied until their disclosure and side effects have been reviewed.
    pub fn permits(self, request: &Request) -> bool {
        matches!(self, Self::Player | Self::Wizard)
            || matches!(
                request,
                Request::Attach { .. }
                    | Request::Snapshot
                    | Request::Palette
                    | Request::History { .. }
                    | Request::HistoryBranch { .. }
            )
    }
}

pub const MAX_NOTE_BYTES: usize = 4096;
pub const MAX_HISTORY_PAGE: usize = 100;

/// Opaque history identities use server entropy, never simulation randomness.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntryId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BranchId(pub String);

/// Opaque admission identity. Backend scheduling counters never cross the wire.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentionId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionPhase {
    Queued,
    Suspended,
    /// Existing attack preparation is paused; the queue slot remains available.
    Paused,
    Started,
    Resolved,
    Failed,
    Cancelled,
}

impl IntentionPhase {
    pub fn pending(self) -> bool {
        matches!(self, Self::Queued | Self::Suspended)
    }

    pub fn active(self) -> bool {
        self.pending() || matches!(self, Self::Started | Self::Paused)
    }

    pub fn can_follow(self, previous: Self) -> bool {
        match previous {
            Self::Queued => true,
            Self::Suspended => matches!(
                self,
                Self::Suspended | Self::Queued | Self::Failed | Self::Cancelled
            ),
            Self::Paused => matches!(
                self,
                Self::Paused | Self::Queued | Self::Resolved | Self::Failed | Self::Cancelled
            ),
            Self::Started => matches!(
                self,
                Self::Started | Self::Paused | Self::Resolved | Self::Failed | Self::Cancelled
            ),
            Self::Resolved | Self::Failed | Self::Cancelled => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentionStatus {
    pub actor: ActorId,
    pub branch: BranchId,
    pub intention: IntentionId,
    pub entry_id: EntryId,
    pub phase: IntentionPhase,
}

impl IntentionStatus {
    pub fn valid_context(&self, actor: ActorId, branch: &BranchId) -> bool {
        self.actor == actor
            && &self.branch == branch
            && !self.intention.0.is_empty()
            && self.intention.0.len() <= 64
            && !self.entry_id.0.is_empty()
            && self.entry_id.0.len() <= 64
    }
}

/// Immediate operations finish at acknowledgement; gameplay is admitted first.
/// Admission context remains stable even when a retry follows a branch change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RequestReceipt {
    Immediate {
        actor: ActorId,
        branch: BranchId,
        entry_id: Option<EntryId>,
    },
    Admitted {
        actor: ActorId,
        branch: BranchId,
        intention: IntentionId,
        entry_id: EntryId,
        phase: IntentionPhase,
    },
}

impl RequestReceipt {
    /// The operation's original actor, including a receipt resolved by retry.
    pub fn actor(&self) -> ActorId {
        match self {
            Self::Immediate { actor, .. } | Self::Admitted { actor, .. } => *actor,
        }
    }

    /// Committed operations retain their original branch. A session response
    /// with no journal entry names the branch at its immediate completion.
    pub fn branch(&self) -> &BranchId {
        match self {
            Self::Immediate { branch, .. } | Self::Admitted { branch, .. } => branch,
        }
    }

    pub fn entry_id(&self) -> Option<&EntryId> {
        match self {
            Self::Immediate { entry_id, .. } => entry_id.as_ref(),
            Self::Admitted { entry_id, .. } => Some(entry_id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Attack {
        target: ActorTarget,
    },
    SetDoor {
        door: DoorTarget,
        open: bool,
    },
    Move {
        direction: Direction,
    },
    Take {
        item: ItemTarget,
        #[serde(default, with = "crate::integers::optional_unsigned")]
        quantity: Option<u64>,
    },
    Drop {
        item: ItemTarget,
        #[serde(default, with = "crate::integers::optional_unsigned")]
        quantity: Option<u64>,
    },
    Wait,
}

/// Offset in the backend-resolved observer frame, never a world coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    #[serde(with = "crate::integers::unsigned")]
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    /// Perceived appearance only; never hidden properties.
    #[serde(default)]
    pub description: String,
    pub id: ItemTarget,
    pub name: String,
    /// The asset a client draws it with, from its palette. Absent when the
    /// scenario names none; clients then fall back to their own look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundItemView {
    pub reachable: bool,
    pub item: ItemView,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorView {
    /// Empty when nothing names it. The observer's own body is the actor
    /// whose id is `Observation::self_target`; clients say who that is.
    #[serde(default)]
    pub name: String,
    /// Authored appearance; empty when none is authored.
    #[serde(default)]
    pub description: String,
    pub id: ActorTarget,
    pub position: Position,
    /// The asset a client draws it with, from its palette. Absent when the
    /// scenario names none; clients then fall back to their own look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceView {
    pub key: String,
    /// The name the character knows the place by.
    pub name: String,
    /// Where that name came from: clients may leave invented ones unsaid.
    pub origin: PlaceNameOrigin,
}

/// Where a remembered place's name came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceNameOrigin {
    /// A mnemonic the game made up when the place was first seen.
    Invented,
    /// The scenario's name for the place, learned on seeing it.
    Authored,
    /// The player's own name for it.
    Player,
}

/// Own-body sensations only; never includes hidden field geometry or other bodies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionView {
    #[serde(with = "crate::integers::signed_triple")]
    pub velocity: [i64; 3],
    pub units_per_cell: u32,
    pub displaced: bool,
    pub impacted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionView>,
    pub places: Vec<PlaceView>,
    pub actor: ActorId,
    /// Own-body entity reference; attachment actor IDs are routing identities.
    pub self_target: ActorTarget,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub position: Position,
    pub visible_cells: Vec<CellView>,
    pub ground_items: Vec<GroundItemView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub ready: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatView {
    pub hp: u32,
    pub max_hp: u32,
    #[serde(default, with = "crate::integers::optional_unsigned")]
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery_remaining: u64,
    pub actors: Vec<CombatActorView>,
    /// What the action this view follows did, as far as the observer knows.
    /// Clients write their own prose from these.
    pub events: Vec<CombatEventView>,
    pub objective: Option<ObjectiveKind>,
    /// The key of the cell where the objective is met, sent with the
    /// objective. The cell itself is disclosed only when it's seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<String>,
    pub victory: bool,
    pub dead: bool,
    pub terminal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatActorView {
    pub actor: ActorTarget,
    pub hostile: bool,
    pub injury: Injury,
}

/// How hurt a visible actor looks; never its numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Injury {
    Healthy,
    Wounded,
    BadlyWounded,
    NearDeath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveKind {
    /// Bring the objective item back to the exit.
    RetrieveAndReturn,
    ReachExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackOutcome {
    Miss,
    /// Struck, but every damage component was resisted.
    NoInjury,
    Hit,
}

/// A combat event as the observer knows it. An absent participant is one
/// it couldn't see; the observer itself is `Observation::self_target`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatEventView {
    Attack {
        attacker: Option<ActorTarget>,
        target: Option<ActorTarget>,
        outcome: AttackOutcome,
    },
    /// The observer's own attack preparation was interrupted.
    Interrupted {
        actor: ActorTarget,
    },
    Died {
        actor: ActorTarget,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellView {
    pub door: Option<DoorView>,
    /// Cosmetic material of a solid cell, not a physical interaction rule.
    /// Floors and ceilings are ordinary seen solid cells; clients derive them.
    #[serde(default)]
    pub material: String,
    /// Stable opaque identity for remembering a disclosed cell.
    pub key: String,
    pub stairs_up: bool,
    pub stairs_down: bool,
    pub position: Position,
    pub wall: bool,
    /// Unnamed anchor hint, disclosed only with this cell; no area membership.
    pub place_hint: bool,
    /// The asset a client draws it with, from its palette. Absent when the
    /// scenario names none; clients then fall back to their own look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoorView {
    pub id: DoorTarget,
    pub name: String,
    pub description: String,
    pub open: bool,
    pub reachable: bool,
    /// Currently perceived standing cells from which this door can be reached.
    /// These disclose no unobserved geometry and are not a planned route.
    pub approaches: Vec<String>,
    /// The asset a client draws it with, from its palette. Absent when the
    /// scenario names none; clients then fall back to their own look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateView {
    pub wizard_game: bool,
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
    pub observation: Observation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    #[default]
    Private,
    Actor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationCategory {
    #[default]
    Note,
    Bookmark,
    Explanation,
}

/// This input type intentionally has no Backend variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSource {
    #[default]
    User,
    Frontend,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Author {
    User { user: String },
    Frontend { user: String, component: String },
    Backend { component: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Anchor {
    /// A disclosed observation/decision state on this entry's branch.
    State {
        #[serde(with = "crate::integers::unsigned")]
        revision: u64,
    },
    /// An accessible action/event pair or an earlier annotation.
    Entry { id: EntryId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ResumeIntention {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        intention: IntentionId,
    },
    CancelIntention {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        intention: IntentionId,
    },
    RenamePlace {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        key: String,
        name: String,
    },
    Travel {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        destination: String,
    },
    Wizard {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        operation: String,
    },
    Act {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        action: Action,
    },
    Annotate {
        anchor: Anchor,
        text: String,
        #[serde(default)]
        source: ClientSource,
        #[serde(default)]
        audience: Audience,
        #[serde(default)]
        category: AnnotationCategory,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PreparationPaused,
    AttackStarted {
        target: ActorTarget,
    },
    DoorChanged {
        door: DoorTarget,
        open: bool,
    },
    Moved {
        direction: Direction,
    },
    Taken {
        item: ItemTarget,
        result: ItemTarget,
        #[serde(with = "crate::integers::unsigned")]
        quantity: u64,
    },
    Dropped {
        item: ItemTarget,
        result: ItemTarget,
        #[serde(with = "crate::integers::unsigned")]
        quantity: u64,
    },
    Waited,
}

/// Validated development inputs, never arbitrary world-state edits.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryContent {
    PlaceRenamed {
        key: String,
        name: String,
    },
    Travel {
        destination: String,
    },
    Wizard {
        summary: String,
        rewind: bool,
    },
    Action {
        action: Action,
        event: Event,
    },
    Annotation {
        anchor: Anchor,
        category: AnnotationCategory,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: EntryId,
    pub branch: BranchId,
    pub actor: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub author: Author,
    pub audience: Audience,
    pub content: HistoryContent,
}

impl HistoryEntry {
    pub fn visible_to(&self, actor: ActorId, user: &str) -> bool {
        self.actor == actor
            && (self.audience == Audience::Actor
                || match &self.author {
                    Author::User { user: owner } | Author::Frontend { user: owner, .. } => {
                        owner == user
                    }
                    Author::Backend { .. } => false,
                })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryPage {
    /// Oldest to newest, with private entries filtered before pagination.
    pub entries: Vec<HistoryEntry>,
    pub older_before: Option<EntryId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Hello {
        protocol: u32,
        token: String,
        frontend: String,
    },
    Request {
        request_id: String,
        request: Request,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// Resume autonomous recovery after fresh controller input; never starts a new action.
    Continue,
    /// Acknowledged only after all earlier accepted records are durable.
    Save,
    HistoryBranch {
        branch: BranchId,
        before: Option<EntryId>,
        limit: u16,
    },
    Attach {
        actor: ActorId,
    },
    AcquireControl,
    ReleaseControl,
    Snapshot,
    /// The whole current palette, as after attaching.
    Palette,
    Command {
        context: InputContext,
        branch: BranchId,
        command: Command,
    },
    History {
        before: Option<EntryId>,
        limit: u16,
    },
}

/// The disclosed attachment and authority generation used to build a command.
/// A retry still names its original context; receipt lookup precedes freshness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputContext {
    pub stream: StreamContext,
    #[serde(with = "crate::integers::unsigned")]
    pub readiness_revision: u64,
}

/// Disclosed attachment state when a reply was published. Receipt identity can
/// belong to an earlier branch; it must never substitute for this current context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyContext {
    pub input: InputContext,
    pub actor: ActorId,
    pub branch: BranchId,
    pub cursor: StreamCursor,
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
}

/// Transport failures have no host ordering boundary. Host errors distinguish
/// an attachment not yet established from an existing disclosed attachment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ErrorScope {
    Transport {},
    Unattached {},
    Attached { context: ReplyContext },
}

/// Authoritative input permissions for one attachment. The revision is
/// independent of observation revisions and simulation time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readiness {
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
    pub admission: bool,
    pub resume: Vec<IntentionId>,
    pub cancel: Vec<IntentionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub context: StreamContext,
    pub readiness: Readiness,
    pub intentions: Vec<IntentionStatus>,
    pub travel: Option<TravelStatus>,
    pub actor: ActorId,
    pub branch: BranchId,
    pub cursor: StreamCursor,
    /// Immutable disclosed state; shared ownership does not change its wire shape.
    pub state: Arc<StateView>,
    pub has_control: bool,
    pub history: HistoryPage,
}

impl Snapshot {
    pub fn reply_context(&self) -> ReplyContext {
        ReplyContext {
            input: InputContext {
                stream: self.context.clone(),
                readiness_revision: self.readiness.revision,
            },
            actor: self.actor,
            branch: self.branch.clone(),
            cursor: self.cursor,
            revision: self.state.revision,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateBody {
    Readiness {
        readiness: Readiness,
    },
    Intention {
        status: IntentionStatus,
    },
    Travel {
        status: TravelStatus,
        entry: Option<Box<HistoryEntry>>,
    },
    Observation {
        /// One immutable observation may be retained by multiple readers.
        state: Arc<StateView>,
        event: Option<Box<HistoryEntry>>,
    },
    /// The next observation as changes to the previous one on this stream.
    ObservationDelta {
        base: crate::ObservationBase,
        state: Box<crate::StateDelta>,
        event: Option<Box<HistoryEntry>>,
    },
    Annotation {
        entry: Box<HistoryEntry>,
    },
    Control {
        has_control: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamUpdate {
    pub context: StreamContext,
    pub actor: ActorId,
    pub branch: BranchId,
    pub cursor: StreamCursor,
    pub body: UpdateBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    VersionMismatch,
    InvalidRequest,
    /// Host capacity is exhausted; the request has not entered the simulation.
    ResourceLimit,
    NotAttached,
    AlreadyAttached,
    ControlTaken,
    NotController,
    StaleRevision,
    StaleContext,
    WrongBranch,
    RequestConflict,
    InvalidAnchor,
    InvalidAnnotation,
    InvalidAction,
    StorageFailure,
    InvalidArchive,
    /// The actor is still on a journey; only the server ends one.
    ActorBusy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        protocol: u32,
        capabilities: ServerCapabilities,
        user: String,
        actors: Vec<ActorId>,
        role: AccessRole,
    },
    Snapshot {
        request_id: String,
        snapshot: Box<Snapshot>,
    },
    /// One ordered part of a bounded logical snapshot. No part grants authority.
    SnapshotPart {
        part: Box<crate::SnapshotPart>,
    },
    Update {
        update: Box<StreamUpdate>,
    },
    Ack {
        context: ReplyContext,
        request_id: String,
        receipt: RequestReceipt,
    },
    History {
        context: ReplyContext,
        request_id: String,
        page: HistoryPage,
    },
    Error {
        scope: ErrorScope,
        request_id: Option<String>,
        code: ErrorCode,
        message: String,
    },
    /// The assets a client may need soon, revisioned independently of
    /// observations. `request_id` answers a palette request.
    Palette {
        context: ReplyContext,
        request_id: Option<String>,
        palette: PaletteUpdate,
    },
    /// Play has stopped and needs input; says whose. Sent each time a run
    /// stops, after the updates it sent, and again after a snapshot.
    Waiting {
        on: Waiting,
    },
}

impl ServerMessage {
    pub fn reply_context(&self) -> Option<&ReplyContext> {
        match self {
            Self::Ack { context, .. }
            | Self::History { context, .. }
            | Self::Palette { context, .. }
            | Self::Error {
                scope: ErrorScope::Attached { context },
                ..
            } => Some(context),
            _ => None,
        }
    }
}

/// What stopped play is waiting for, as seen by one client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Waiting {
    /// This client's actor is next, and this client controls it: its move.
    You,
    /// An actor another client controls is next.
    Others,
    /// A character no client controls is next, until someone takes control.
    Unclaimed,
    /// AI play is paused until a controller acts or sends `continue`.
    Paused,
    /// Nothing can act: no actor is next, or none a client controls is alive.
    Stopped,
}

/// Asset identifiers the attached actor may soon see, forecast from the
/// themes of the regions near it, never from what's in them. Attaching
/// (and a palette request) sends the whole palette; later changes are
/// deltas against the previous revision. Nothing is acknowledged, and it's
/// recomputed after a restart rather than saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteUpdate {
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
    pub body: PaletteBody,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PaletteBody {
    Full {
        assets: std::collections::BTreeSet<String>,
    },
    /// Changes from revision `base`.
    Delta {
        #[serde(with = "crate::integers::unsigned")]
        base: u64,
        added: std::collections::BTreeSet<String>,
        removed: std::collections::BTreeSet<String>,
    },
}

/// Session travel state contains no planned route or undisclosed geometry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TravelStatus {
    pub id: EntryId,
    pub destination: String,
    #[serde(with = "crate::integers::unsigned")]
    pub completed_steps: u64,
    pub phase: TravelPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelPhase {
    Active,
    Arrived,
    Blocked,
    Hazard,
    DecisionRequired,
    ControlLost,
    WorldChanged,
    Failed,
}
