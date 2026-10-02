use crate::{ActorId, StreamCursor};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 21;
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
    Attack { target: ActorId },
    SetDoor { door: u64, open: bool },
    Move { direction: Direction },
    Take { item: u64, quantity: Option<u64> },
    Drop { item: u64, quantity: Option<u64> },
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
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    /// Perceived appearance only; never hidden properties.
    #[serde(default)]
    pub description: String,
    pub id: u64,
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
    /// whose id is `Observation::actor`; clients say who that is.
    #[serde(default)]
    pub name: String,
    /// Authored appearance; empty when none is authored.
    #[serde(default)]
    pub description: String,
    pub id: ActorId,
    pub position: Position,
    /// The asset a client draws it with, from its palette. Absent when the
    /// scenario names none; clients then fall back to their own look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceView {
    pub key: String,
    /// Character-owned mnemonic, not an authored region name.
    pub name: String,
}

/// Own-body sensations only; never includes hidden field geometry or other bodies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionView {
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
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    pub recovery_remaining: u64,
    pub actors: Vec<CombatActorView>,
    /// What the action this view follows did, as far as the observer knows.
    /// Clients write their own prose from these.
    pub events: Vec<CombatEventView>,
    pub objective: Option<ObjectiveKind>,
    pub victory: bool,
    pub dead: bool,
    pub terminal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatActorView {
    pub actor: ActorId,
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
/// it couldn't see; the observer itself is `Observation::actor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatEventView {
    Attack {
        attacker: Option<ActorId>,
        target: Option<ActorId>,
        outcome: AttackOutcome,
    },
    /// The observer's own attack preparation was interrupted.
    Interrupted {
        actor: ActorId,
    },
    Died {
        actor: ActorId,
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
    pub id: u64,
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
    State { revision: u64 },
    /// An accessible action/event pair or an earlier annotation.
    Entry { id: EntryId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    RenamePlace {
        expected_revision: u64,
        key: String,
        name: String,
    },
    Travel {
        expected_revision: u64,
        destination: String,
    },
    Wizard {
        expected_revision: u64,
        operation: String,
    },
    Act {
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
        target: ActorId,
    },
    DoorChanged {
        door: u64,
        open: bool,
    },
    Moved {
        direction: Direction,
    },
    Taken {
        item: u64,
        result: u64,
        quantity: u64,
    },
    Dropped {
        item: u64,
        result: u64,
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
        branch: BranchId,
        command: Command,
    },
    History {
        before: Option<EntryId>,
        limit: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub travel: Option<TravelStatus>,
    pub actor: ActorId,
    pub branch: BranchId,
    pub cursor: StreamCursor,
    pub state: StateView,
    pub has_control: bool,
    pub history: HistoryPage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateBody {
    Travel {
        status: TravelStatus,
        entry: Option<Box<HistoryEntry>>,
    },
    Observation {
        state: Box<StateView>,
        event: Option<Box<HistoryEntry>>,
    },
    /// The next observation as changes to the previous one on this stream.
    ObservationDelta {
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
pub struct StreamUpdate {
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
    NotAttached,
    AlreadyAttached,
    ControlTaken,
    NotController,
    StaleRevision,
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
        user: String,
        actors: Vec<ActorId>,
        role: AccessRole,
    },
    Snapshot {
        request_id: String,
        snapshot: Box<Snapshot>,
    },
    Update {
        update: Box<StreamUpdate>,
    },
    Ack {
        request_id: String,
        entry_id: Option<EntryId>,
    },
    History {
        request_id: String,
        page: HistoryPage,
    },
    Error {
        request_id: Option<String>,
        code: ErrorCode,
        message: String,
    },
    /// The assets a client may need soon, revisioned independently of
    /// observations. `request_id` answers a palette request.
    Palette {
        request_id: Option<String>,
        palette: PaletteUpdate,
    },
}

/// Asset identifiers the attached actor may soon see, forecast from the
/// themes of the regions near it, never from what's in them. Attaching
/// (and a palette request) sends the whole palette; later changes are
/// deltas against the previous revision. Nothing is acknowledged, and it's
/// recomputed after a restart rather than saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteUpdate {
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
