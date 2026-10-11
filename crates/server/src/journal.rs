//! Backend-only journal and developer setup types. Never sent to a frontend.
pub use crate::actions::{Ability, Action, Direction};
use serde::{Deserialize, Serialize};
use tor_protocol::{
    ActorId, Anchor, AnnotationCategory, Audience, Author, BranchId, ClientSource, EntryId,
};

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
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
    },
    CancelIntention {
        expected_revision: u64,
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
    },
    AdmitIntention {
        expected_revision: u64,
        #[serde(with = "crate::storage::schema::Action")]
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
        #[serde(with = "crate::storage::schema::Action")]
        action: Action,
    },
    Annotate {
        #[serde(with = "crate::storage::schema::Anchor")]
        anchor: Anchor,
        text: String,
        #[serde(default)]
        #[serde(with = "crate::storage::schema::ClientSource")]
        source: ClientSource,
        #[serde(default)]
        #[serde(with = "crate::storage::schema::Audience")]
        audience: Audience,
        #[serde(default)]
        #[serde(with = "crate::storage::schema::AnnotationCategory")]
        category: AnnotationCategory,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    AbilityStarted {
        #[serde(with = "crate::storage::schema::Ability")]
        ability: crate::actions::Ability,
        #[serde(with = "crate::storage::schema::ActorId")]
        target: ActorId,
    },
    PreparationPaused,
    ItemStarted {
        #[serde(with = "crate::storage::schema::Action")]
        action: Action,
    },
    AttackStarted {
        #[serde(with = "crate::storage::schema::ActorId")]
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

/// Backend-only advancement choices. Public owner ordinals are one-based.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatureAdvancement {
    AddHitDie {
        source: crate::creature_authoring::HitDieSource,
    },
    Train {
        owner: u16,
        skill: crate::creature_authoring::Skill,
    },
    IncreaseAttribute {
        owner: u16,
        attribute: crate::creature_authoring::Attribute,
    },
    SelectTalent {
        owner: u16,
        talent: crate::creature_authoring::Talent,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WizardOperation {
    AdvanceCreature {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        advancement: CreatureAdvancement,
    },
    SetCreatureTemplate {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        template: String,
        enabled: bool,
    },
    RemoveCreatureHitDie {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
    },
    ArenaControl {
        paused: bool,
        advance: u64,
    },
    SetGravity {
        region: u64,
        vector: [i32; 3],
    },
    SetCellGravity {
        position: Position,
        vector: [i32; 3],
    },
    SetBody {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        cells: Vec<[i32; 3]>,
        eye: [i32; 3],
        mass: u32,
    },
    SetVelocity {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        velocity: [i64; 3],
    },
    ConnectPortal {
        from: Position,
        #[serde(with = "crate::storage::schema::Direction")]
        direction: Direction,
        to: Position,
        rotation: u8,
        width: u16,
        height: u16,
    },
    IdentifyItem {
        #[serde(with = "crate::storage::schema::ActorId")]
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
        #[serde(with = "crate::storage::schema::Direction")]
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
        #[serde(with = "crate::storage::schema::Direction")]
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
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        position: Position,
    },
    /// Restore the state after this retained action/setup entry; None is the initial state.
    Rewind {
        #[serde(with = "crate::storage::schema::optional_entry")]
        target: Option<EntryId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WizardResult {
    CreatureAdvanced {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        advancement: CreatureAdvancement,
    },
    CreatureTemplateSet {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
        template: String,
        enabled: bool,
    },
    CreatureHitDieRemoved {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
    },
    ArenaControlled {
        paused: bool,
        advance: u64,
    },
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
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
    },
    Teleported {
        #[serde(with = "crate::storage::schema::ActorId")]
        actor: ActorId,
    },
    Rewound {
        #[serde(with = "crate::storage::schema::BranchId")]
        from_branch: BranchId,
        #[serde(with = "crate::storage::schema::BranchId")]
        branch: BranchId,
        tick: u64,
        #[serde(with = "crate::storage::schema::ActorId")]
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
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
        intention: tor_simulation::IntentionId,
        #[serde(with = "crate::storage::schema::Action")]
        action: Action,
        event: Event,
    },
    IntentionContinuationFailed {
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
        intention: tor_simulation::IntentionId,
    },
    IntentionChanged {
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
        intention: tor_simulation::IntentionId,
        change: IntentionChange,
    },
    /// The scheduler started this action. Attack impacts may resolve later.
    IntentionStarted {
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
        intention: tor_simulation::IntentionId,
        #[serde(with = "crate::storage::schema::Action")]
        action: Action,
        event: Event,
    },
    /// Execution revalidation failed; no substitute action was selected.
    IntentionFailed {
        #[serde(with = "crate::storage::schema::EntryId")]
        admission: EntryId,
        intention: tor_simulation::IntentionId,
    },
    /// A backend decision admitted without an authenticated RPC or chosen action.
    AutonomousIntentionAdmitted {
        intention: tor_simulation::IntentionId,
    },
    /// Native movement linked to an accepted journey, without a fabricated RPC.
    TravelIntentionAdmitted {
        intention: tor_simulation::IntentionId,
        #[serde(with = "crate::storage::schema::EntryId")]
        journey: EntryId,
        step: u64,
        #[serde(with = "crate::storage::schema::Action")]
        action: Action,
        destination: tor_world::Location,
    },
    /// Receipt of accepted work, before its simulation effect. Not a history event.
    IntentionAdmitted {
        intention: tor_simulation::IntentionId,
        #[serde(with = "crate::storage::schema::Action")]
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
        #[serde(with = "crate::storage::schema::Action")]
        action: Action,
        event: Event,
    },
    Annotation {
        #[serde(with = "crate::storage::schema::Anchor")]
        anchor: Anchor,
        #[serde(with = "crate::storage::schema::AnnotationCategory")]
        category: AnnotationCategory,
        text: String,
    },
}

/// Admission ownership remains explicit independently of its later chosen action.
#[derive(Clone, Copy)]
pub(crate) enum AdmittedWork<'a> {
    Human(&'a Action),
    AutonomousDecision,
    Travel(&'a Action, tor_world::Location),
}

impl AdmittedWork<'_> {
    pub(crate) fn origin(self) -> tor_simulation::IntentionOrigin {
        match self {
            Self::Human(_) => tor_simulation::IntentionOrigin::Human,
            Self::AutonomousDecision => tor_simulation::IntentionOrigin::Autonomous,
            Self::Travel(_, _) => tor_simulation::IntentionOrigin::Travel,
        }
    }

    pub(crate) fn matches_intention(self, queued: &tor_simulation::QueuedIntention) -> bool {
        queued.origin == self.origin()
            && self.matches(queued.work)
            && match self {
                Self::Travel(_, destination) => queued
                    .movement_context
                    .is_some_and(|context| context.destination() == destination),
                _ => true,
            }
    }

    pub(crate) fn matches(self, work: tor_simulation::IntentionWork) -> bool {
        match self {
            Self::Human(action) => {
                let original = crate::adapt::action(action);
                work == tor_simulation::IntentionWork::Action(original)
                    || matches!(work, tor_simulation::IntentionWork::ResumePreparation { work } if work.action() == original)
            }
            Self::AutonomousDecision => work == tor_simulation::IntentionWork::AiDecision,
            Self::Travel(action, _) => {
                work == tor_simulation::IntentionWork::Action(crate::adapt::action(action))
            }
        }
    }
}

impl JournalContent {
    pub(crate) fn admission(&self) -> Option<(tor_simulation::IntentionId, AdmittedWork<'_>)> {
        match self {
            Self::IntentionAdmitted { intention, action } => {
                Some((*intention, AdmittedWork::Human(action)))
            }
            Self::TravelIntentionAdmitted {
                intention,
                action,
                destination,
                ..
            } => Some((*intention, AdmittedWork::Travel(action, *destination))),
            Self::AutonomousIntentionAdmitted { intention } => {
                Some((*intention, AdmittedWork::AutonomousDecision))
            }
            _ => None,
        }
    }
    pub(crate) fn rewindable(&self) -> bool {
        match self {
            Self::IntentionAdmitted { .. }
            | Self::AutonomousIntentionAdmitted { .. }
            | Self::TravelIntentionAdmitted { .. }
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
    #[serde(with = "crate::storage::schema::ActorId")]
    pub actor: ActorId,
    pub intention: tor_simulation::IntentionId,
    pub kind: IntentionEndKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentionSuspension {
    #[serde(with = "crate::storage::schema::ActorId")]
    pub actor: ActorId,
    pub intention: tor_simulation::IntentionId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Progress suspended by this action, admitted with its authoritative effects.
    pub intention_suspensions: Vec<IntentionSuspension>,
    /// Terminal work facts admitted atomically with this record's state effects.
    pub intention_ends: Vec<IntentionEnd>,
    #[serde(with = "crate::storage::schema::EntryId")]
    pub id: EntryId,
    #[serde(with = "crate::storage::schema::BranchId")]
    pub branch: BranchId,
    #[serde(with = "crate::storage::schema::ActorId")]
    pub actor: ActorId,
    pub tick: u64,
    #[serde(with = "crate::storage::schema::Author")]
    pub author: Author,
    #[serde(with = "crate::storage::schema::Audience")]
    pub audience: Audience,
    pub content: JournalContent,
}

impl JournalEntry {
    pub fn visible_to(&self, actor: ActorId, user: &str) -> bool {
        !matches!(
            self.content,
            JournalContent::IntentionAdmitted { .. }
                | JournalContent::AutonomousIntentionAdmitted { .. }
                | JournalContent::TravelIntentionAdmitted { .. }
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
