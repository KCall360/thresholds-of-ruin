use crate::{ActorId, StreamCursor};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 10;
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
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SetDoor { door: u64, open: bool },
    Move { direction: Direction },
    Take { item: u64 },
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
    /// Perceived appearance only; never hidden properties.
    #[serde(default)]
    pub description: String,
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundItemView {
    pub reachable: bool,
    pub item: ItemView,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorView {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub id: ActorId,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
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
pub struct CellView {
    pub floor: Option<SurfaceView>,
    pub ceiling: Option<SurfaceView>,
    pub door: Option<DoorView>,
    /// Cosmetic surface material, not a physical interaction rule.
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
}

/// Perceived solid surface along a vertical column, not a movement affordance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceView {
    pub material: String,
    /// Offset from this empty cell to the solid cell, in five-foot cubes.
    pub distance: u32,
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
    DoorChanged { door: u64, open: bool },
    Moved { direction: Direction },
    Taken { item: u64 },
    Waited,
}

/// Validated development inputs, never arbitrary world-state edits.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryContent {
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
    CancelTravel {
        branch: BranchId,
        travel_id: EntryId,
    },
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
    Cancelled,
    Blocked,
    Hazard,
    DecisionRequired,
    ControlLost,
    WorldChanged,
    Failed,
}
