//! Backend-only journal and developer setup types. Never sent to a frontend.
use serde::{Deserialize, Serialize};
use tor_protocol::{
    Action, ActorId, Anchor, AnnotationCategory, Audience, Author, BranchId, ClientSource,
    Direction, EntryId,
};

impl Command {
    pub fn from_wire(command: &tor_protocol::Command) -> Result<Self, crate::Failure> {
        // Keep the transport and journal schemas independent: extending either
        // enum must require an explicit decision at this boundary.
        Ok(match command {
            tor_protocol::Command::ResumeIntention {
                expected_revision,
                intention,
            } => Self::ResumeIntention {
                expected_revision: *expected_revision,
                admission: EntryId(intention.0.clone()),
            },
            tor_protocol::Command::CancelIntention {
                expected_revision,
                intention,
            } => Self::CancelIntention {
                expected_revision: *expected_revision,
                admission: EntryId(intention.0.clone()),
            },
            tor_protocol::Command::RenamePlace {
                expected_revision,
                key,
                name,
            } => Self::RenamePlace {
                expected_revision: *expected_revision,
                key: key.clone(),
                name: name.clone(),
            },
            tor_protocol::Command::Travel {
                expected_revision,
                destination,
            } => Self::Travel {
                expected_revision: *expected_revision,
                destination: destination.clone(),
            },
            tor_protocol::Command::Wizard {
                expected_revision,
                operation,
            } => Self::Wizard {
                expected_revision: *expected_revision,
                operation: crate::developer::parse_wizard(operation).map_err(|_| {
                    crate::Failure::new(
                        tor_protocol::ErrorCode::InvalidRequest,
                        "Invalid developer command",
                    )
                })?,
            },
            tor_protocol::Command::Act {
                expected_revision,
                action,
            } => Self::AdmitIntention {
                expected_revision: *expected_revision,
                action: action.clone(),
            },
            tor_protocol::Command::Annotate {
                anchor,
                text,
                source,
                audience,
                category,
            } => Self::Annotate {
                anchor: anchor.clone(),
                text: text.clone(),
                source: *source,
                audience: *audience,
                category: *category,
            },
        })
    }
}

impl TryFrom<Command> for tor_protocol::Command {
    type Error = &'static str;
    fn try_from(command: Command) -> Result<Self, Self::Error> {
        match command {
            Command::ResumeIntention {
                expected_revision,
                admission,
            } => Ok(Self::ResumeIntention {
                expected_revision,
                intention: tor_protocol::IntentionId(admission.0),
            }),
            Command::CancelIntention {
                expected_revision,
                admission,
            } => Ok(Self::CancelIntention {
                expected_revision,
                intention: tor_protocol::IntentionId(admission.0),
            }),
            Command::PausePreparation => Err("Preparation suspension is backend-only"),
            Command::Wizard {
                expected_revision,
                operation,
            } => Ok(Self::Wizard {
                expected_revision,
                operation: serde_json::to_string(&operation).expect("developer command serializes"),
            }),
            Command::RenamePlace {
                expected_revision,
                key,
                name,
            } => Ok(Self::RenamePlace {
                expected_revision,
                key,
                name,
            }),
            Command::Travel {
                expected_revision,
                destination,
            } => Ok(Self::Travel {
                expected_revision,
                destination,
            }),
            Command::AdmitIntention {
                expected_revision,
                action,
            }
            | Command::Act {
                expected_revision,
                action,
            } => Ok(Self::Act {
                expected_revision,
                action,
            }),
            Command::Annotate {
                anchor,
                text,
                source,
                audience,
                category,
            } => Ok(Self::Annotate {
                anchor,
                text,
                source,
                audience,
                category,
            }),
        }
    }
}

impl JournalEntry {
    /// Only this projection crosses the network; journal topology stays private.
    pub fn disclosed(&self) -> Option<tor_protocol::HistoryEntry> {
        use tor_protocol::{Event as VisibleEvent, HistoryContent as Content};
        let content = match &self.content {
            JournalContent::IntentionAdmitted { .. }
            | JournalContent::IntentionFailed { .. }
            | JournalContent::IntentionContinuationFailed { .. }
            | JournalContent::IntentionChanged { .. } => return None,
            JournalContent::PlaceRenamed { key, name } => Content::PlaceRenamed {
                key: key.clone(),
                name: name.clone(),
            },
            JournalContent::Travel { destination } => Content::Travel {
                destination: destination.clone(),
            },
            JournalContent::Wizard { result, .. } => Content::Wizard {
                summary: match result {
                    WizardResult::Rewound { .. } => "Timeline rewound.",
                    _ => "Developer setup completed.",
                }
                .into(),
                rewind: matches!(result, WizardResult::Rewound { .. }),
            },
            JournalContent::Action { action, event }
            | JournalContent::IntentionStarted { action, event, .. }
            | JournalContent::IntentionContinued { action, event, .. } => Content::Action {
                action: action.clone(),
                event: match event {
                    Event::PreparationPaused => VisibleEvent::PreparationPaused,
                    Event::AttackStarted { target } => {
                        VisibleEvent::AttackStarted { target: *target }
                    }
                    Event::DoorChanged { door, open } => VisibleEvent::DoorChanged {
                        door: *door,
                        open: *open,
                    },
                    Event::Moved { .. } => VisibleEvent::Moved {
                        direction: match action {
                            Action::Move { direction } => *direction,
                            _ => unreachable!("movement has a move action"),
                        },
                    },
                    Event::Taken {
                        item,
                        result,
                        quantity,
                    } => VisibleEvent::Taken {
                        item: *item,
                        result: *result,
                        quantity: *quantity,
                    },
                    Event::Dropped {
                        item,
                        result,
                        quantity,
                    } => VisibleEvent::Dropped {
                        item: *item,
                        result: *result,
                        quantity: *quantity,
                    },
                    Event::Waited => VisibleEvent::Waited,
                },
            },
            JournalContent::Annotation {
                anchor,
                category,
                text,
            } => Content::Annotation {
                anchor: anchor.clone(),
                category: *category,
                text: text.clone(),
            },
        };
        Some(tor_protocol::HistoryEntry {
            id: self.id.clone(),
            branch: self.branch.clone(),
            actor: self.actor,
            tick: self.tick,
            author: self.author.clone(),
            audience: self.audience,
            content,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub region: u64,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionView {
    pub id: u64,
    pub name: String,
    pub width: i32,
    pub depth: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ResumeIntention {
        expected_revision: u64,
        admission: EntryId,
    },
    CancelIntention {
        expected_revision: u64,
        admission: EntryId,
    },
    AdmitIntention {
        expected_revision: u64,
        action: Action,
    },
    PausePreparation,
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
        operation: WizardOperation,
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
        from: Position,
        to: Position,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WizardItem {
    Token,
    Tablet,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WizardOperation {
    SetGravity {
        region: u64,
        vector: [i32; 3],
    },
    SetCellGravity {
        position: Position,
        vector: [i32; 3],
    },
    SetBody {
        actor: ActorId,
        cells: Vec<[i32; 3]>,
        eye: [i32; 3],
        mass: u32,
    },
    SetVelocity {
        actor: ActorId,
        velocity: [i64; 3],
    },
    ConnectPortal {
        from: Position,
        direction: Direction,
        to: Position,
        rotation: u8,
        width: u16,
        height: u16,
    },
    IdentifyItem {
        actor: ActorId,
        item: u64,
    },
    PlaceChamber {
        region: RegionView,
    },
    PlaceDoor {
        position: Position,
        open: bool,
        /// Cells tall; a door must exactly fill its opening.
        height: u8,
    },
    ConnectArea {
        from: Position,
        direction: Direction,
        to: Position,
        quarter_turns: u8,
        width: u16,
        height: u16,
    },
    PlaceRoom {
        region: RegionView,
    },
    Connect {
        from: Position,
        direction: Direction,
        to: Position,
        quarter_turns: u8,
    },
    SetPlaceHint {
        position: Position,
        present: bool,
    },
    SetWall {
        position: Position,
        wall: bool,
    },
    PlaceItem {
        kind: WizardItem,
        position: Position,
    },
    SpawnActor {
        position: Position,
        turn_ticks: u64,
    },
    Teleport {
        actor: ActorId,
        position: Position,
    },
    /// Restore the state after this retained action/setup entry; None is the initial state.
    Rewind {
        target: Option<EntryId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WizardResult {
    PhysicsSet,
    ItemIdentified,
    DoorPlaced {
        door: u64,
    },
    RoomPlaced {
        region: u64,
    },
    Connected,
    WallSet,
    PlaceHintSet,
    ItemPlaced {
        item: u64,
    },
    ActorSpawned {
        actor: ActorId,
    },
    Teleported {
        actor: ActorId,
    },
    Rewound {
        from_branch: BranchId,
        branch: BranchId,
        tick: u64,
        next_actor: ActorId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionChange {
    Suspended,
    Resumed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalContent {
    /// Resumed execution of existing attack progress, under its original admission.
    IntentionContinued {
        admission: EntryId,
        intention: tor_simulation::IntentionId,
        action: Action,
        event: Event,
    },
    IntentionContinuationFailed {
        admission: EntryId,
        intention: tor_simulation::IntentionId,
    },
    IntentionChanged {
        admission: EntryId,
        intention: tor_simulation::IntentionId,
        change: IntentionChange,
    },
    /// The scheduler started this action. Attack impacts may resolve later.
    IntentionStarted {
        admission: EntryId,
        intention: tor_simulation::IntentionId,
        action: Action,
        event: Event,
    },
    /// Execution revalidation failed; no substitute action was selected.
    IntentionFailed {
        admission: EntryId,
        intention: tor_simulation::IntentionId,
    },
    /// Receipt of accepted work, before its simulation effect. Not a history event.
    IntentionAdmitted {
        intention: tor_simulation::IntentionId,
        action: Action,
    },
    PlaceRenamed {
        key: String,
        name: String,
    },
    Travel {
        destination: String,
    },
    Wizard {
        operation: WizardOperation,
        validation: Option<bool>,
        result: WizardResult,
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

impl JournalContent {
    pub(crate) fn rewindable(&self) -> bool {
        match self {
            Self::IntentionAdmitted { .. }
            | Self::IntentionFailed { .. }
            | Self::IntentionContinuationFailed { .. }
            | Self::IntentionChanged { .. }
            | Self::Annotation { .. } => false,
            Self::IntentionStarted { .. }
            | Self::IntentionContinued { .. }
            | Self::Action { .. }
            | Self::Wizard { .. }
            | Self::Travel { .. }
            | Self::PlaceRenamed { .. } => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionEndKind {
    Resolved,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentionEnd {
    pub actor: ActorId,
    pub intention: tor_simulation::IntentionId,
    pub kind: IntentionEndKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentionSuspension {
    pub actor: ActorId,
    pub intention: tor_simulation::IntentionId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Progress suspended by this action, admitted with its authoritative effects.
    pub intention_suspensions: Vec<IntentionSuspension>,
    /// Terminal work facts admitted atomically with this record's state effects.
    pub intention_ends: Vec<IntentionEnd>,
    pub id: EntryId,
    pub branch: BranchId,
    pub actor: ActorId,
    pub tick: u64,
    pub author: Author,
    pub audience: Audience,
    pub content: JournalContent,
}

impl JournalEntry {
    pub fn visible_to(&self, actor: ActorId, user: &str) -> bool {
        !matches!(
            self.content,
            JournalContent::IntentionAdmitted { .. }
                | JournalContent::IntentionFailed { .. }
                | JournalContent::IntentionContinuationFailed { .. }
                | JournalContent::IntentionChanged { .. }
        ) && self.actor == actor
            && (self.audience == Audience::Actor
                || match &self.author {
                    Author::User { user: owner } | Author::Frontend { user: owner, .. } => {
                        owner == user
                    }
                    Author::Backend { .. } => false,
                })
    }
}
