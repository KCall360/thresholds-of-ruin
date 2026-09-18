//! Backend-only journal and developer setup types. Never sent to a frontend.
use serde::{Deserialize, Serialize};
use tor_protocol::{
    Action, ActorId, Anchor, AnnotationCategory, Audience, Author, BranchId, ClientSource,
    Direction, EntryId,
};

impl Command {
    pub fn from_wire(command: &tor_protocol::Command) -> Result<Self, crate::Failure> {
        if let tor_protocol::Command::Wizard {
            expected_revision,
            operation,
        } = command
        {
            let operation = crate::developer::parse_wizard(operation).map_err(|_| {
                crate::Failure::new(
                    tor_protocol::ErrorCode::InvalidRequest,
                    "Invalid developer command",
                )
            })?;
            return Ok(Self::Wizard {
                expected_revision: *expected_revision,
                operation,
            });
        }
        serde_json::from_value(serde_json::to_value(command).expect("wire command serializes"))
            .map_err(|_| {
                crate::Failure::new(tor_protocol::ErrorCode::InvalidRequest, "Invalid command")
            })
    }
}

impl From<Command> for tor_protocol::Command {
    fn from(command: Command) -> Self {
        match command {
            Command::Wizard {
                expected_revision,
                operation,
            } => Self::Wizard {
                expected_revision,
                operation: serde_json::to_string(&operation).expect("developer command serializes"),
            },
            other => {
                serde_json::from_value(serde_json::to_value(other).expect("command serializes"))
                    .expect("ordinary command has the same schema")
            }
        }
    }
}

impl HistoryEntry {
    /// Only this projection crosses the network; journal topology stays private.
    pub fn disclosed(&self) -> tor_protocol::HistoryEntry {
        use tor_protocol::{Event as VisibleEvent, HistoryContent as Content};
        let content = match &self.content {
            HistoryContent::Travel { destination } => Content::Travel {
                destination: destination.clone(),
            },
            HistoryContent::Wizard { result, .. } => Content::Wizard {
                summary: match result {
                    WizardResult::Rewound { .. } => "Timeline rewound.",
                    _ => "Developer setup completed.",
                }
                .into(),
                rewind: matches!(result, WizardResult::Rewound { .. }),
            },
            HistoryContent::Action { action, event } => Content::Action {
                action: action.clone(),
                event: match event {
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
                    Event::Taken { item } => VisibleEvent::Taken { item: *item },
                    Event::Waited => VisibleEvent::Waited,
                },
            },
            HistoryContent::Annotation {
                anchor,
                category,
                text,
            } => Content::Annotation {
                anchor: anchor.clone(),
                category: *category,
                text: text.clone(),
            },
        };
        tor_protocol::HistoryEntry {
            id: self.id.clone(),
            branch: self.branch.clone(),
            actor: self.actor,
            tick: self.tick,
            author: self.author.clone(),
            audience: self.audience,
            content,
        }
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
    DoorChanged { door: u64, open: bool },
    Moved { from: Position, to: Position },
    Taken { item: u64 },
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
    PlaceChamber {
        region: RegionView,
    },
    PlaceDoor {
        position: Position,
        open: bool,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryContent {
    Travel {
        destination: String,
    },
    Wizard {
        operation: WizardOperation,
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
