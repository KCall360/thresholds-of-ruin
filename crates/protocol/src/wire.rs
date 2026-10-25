use crate::{
    ActorId, ActorTarget, DoorTarget, HoldTarget, ItemTarget, StreamContext, StreamCursor,
    TrapTarget,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 62;
/// The maximum disclosed primary strike ordinal plus one.
pub const MAX_SEQUENCE_STRIKES: u16 = 16;

// `Option` fields normally accept omission. Current-only envelopes require an
// explicit null instead, so old payloads cannot silently acquire new semantics.
pub(crate) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
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
                    | Request::Map { .. }
                    | Request::Chart { .. }
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

/// Opaque admission or paid-choice identity. Scheduling counters never cross the wire.
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

/// Permitted public input mode, not the hidden device effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceActivation {
    Direction,
    SelfTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceAim {
    Direction { direction: Direction },
    SelfTarget {},
}
impl<'de> Deserialize<'de> for DeviceAim {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Direction {
                #[serde(deserialize_with = "read_direction_token")]
                direction: Direction,
            },
            SelfTarget {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Direction { direction } => Self::Direction { direction },
            Fields::SelfTarget {} => Self::SelfTarget {},
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Escape {
        hold: HoldTarget,
    },
    ChooseIntellectExpertise {
        basis: Box<crate::AdvancementBasis>,
        choice: crate::IntellectExpertiseOperation,
    },
    ChooseGrowthSource {
        basis: Box<crate::AdvancementBasis>,
        source: crate::GrowthSource,
    },
    Purchase {
        basis: Box<crate::AdvancementBasis>,
        purchase: crate::PurchaseOperation,
    },
    Respec {
        basis: Box<crate::AdvancementBasis>,
        replacement: crate::ReplacementOperation,
        supplies: crate::SupplySelection,
    },
    UseAbility {
        id: String,
        invocation: AbilityInvocation,
        supplies: crate::SupplySelection,
    },
    Equip {
        item: ItemTarget,
        slot: u16,
    },
    Unequip {
        item: ItemTarget,
    },
    Drink {
        item: ItemTarget,
    },
    Eat {
        item: ItemTarget,
    },
    Read {
        item: ItemTarget,
    },
    Ignite {
        item: ItemTarget,
    },
    Extinguish {
        item: ItemTarget,
    },
    Zap {
        item: ItemTarget,
        aim: DeviceAim,
    },
    Identify {
        choice: IntentionId,
        item: Option<ItemTarget>,
    },
    ExamineTrap {
        trap: TrapTarget,
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
impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Escape {
                hold: HoldTarget,
            },
            ChooseIntellectExpertise {
                basis: Box<crate::AdvancementBasis>,
                choice: crate::IntellectExpertiseOperation,
            },
            ChooseGrowthSource {
                basis: Box<crate::AdvancementBasis>,
                source: crate::GrowthSource,
            },
            Purchase {
                basis: Box<crate::AdvancementBasis>,
                purchase: crate::PurchaseOperation,
            },
            Respec {
                basis: Box<crate::AdvancementBasis>,
                replacement: crate::ReplacementOperation,
                supplies: crate::SupplySelection,
            },
            UseAbility {
                id: String,
                invocation: AbilityInvocation,
                supplies: crate::SupplySelection,
            },
            Equip {
                item: ItemTarget,
                slot: u16,
            },
            Unequip {
                item: ItemTarget,
            },
            Drink {
                item: ItemTarget,
            },
            Eat {
                item: ItemTarget,
            },
            Read {
                item: ItemTarget,
            },
            Ignite {
                item: ItemTarget,
            },
            Extinguish {
                item: ItemTarget,
            },
            Zap {
                item: ItemTarget,
                aim: DeviceAim,
            },
            Identify {
                choice: IntentionId,
                #[serde(deserialize_with = "required_nullable")]
                item: Option<ItemTarget>,
            },
            ExamineTrap {
                trap: TrapTarget,
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
        Ok(match crate::bounded::named_map(d)? {
            Fields::Escape { hold } => Self::Escape { hold },
            Fields::ChooseIntellectExpertise { basis, choice } => {
                Self::ChooseIntellectExpertise { basis, choice }
            }
            Fields::ChooseGrowthSource { basis, source } => {
                Self::ChooseGrowthSource { basis, source }
            }
            Fields::Purchase { basis, purchase } => Self::Purchase { basis, purchase },
            Fields::Respec {
                basis,
                replacement,
                supplies,
            } => {
                if !crate::advancement::valid_respec_selection(&supplies) {
                    return Err(serde::de::Error::custom(
                        "respec needs one ordinal-zero ration source",
                    ));
                }
                Self::Respec {
                    basis,
                    replacement,
                    supplies,
                }
            }
            Fields::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id,
                invocation,
                supplies,
            },
            Fields::Equip { item, slot } => Self::Equip { item, slot },
            Fields::Unequip { item } => Self::Unequip { item },
            Fields::Drink { item } => Self::Drink { item },
            Fields::Eat { item } => Self::Eat { item },
            Fields::Read { item } => Self::Read { item },
            Fields::Ignite { item } => Self::Ignite { item },
            Fields::Extinguish { item } => Self::Extinguish { item },
            Fields::Zap { item, aim } => Self::Zap { item, aim },
            Fields::Identify { choice, item } => Self::Identify { choice, item },
            Fields::ExamineTrap { trap } => Self::ExamineTrap { trap },
            Fields::SetDoor { door, open } => Self::SetDoor { door, open },
            Fields::Move { direction } => Self::Move { direction },
            Fields::Take { item, quantity } => Self::Take { item, quantity },
            Fields::Drop { item, quantity } => Self::Drop { item, quantity },
            Fields::Wait => Self::Wait,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityOutcome {
    Applied,
    Unaffected,
    Miss,
}

/// Offset in the backend-resolved observer frame, never a world coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Disclosed cosmetic identity, shared by the creature and its remains.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureAppearance {
    pub seed: crate::AppearanceSeed,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

/// Disclosed currency identity; stack quantity counts units of this currency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrencyKind {
    Gold,
}
impl<'de> Deserialize<'de> for CurrencyKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "gold" => Ok(Self::Gold),
            _ => Err(serde::de::Error::custom("unknown currency kind")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corpse: Option<CreatureAppearance>,
    pub class: ItemClass,
    #[serde(with = "crate::integers::unsigned")]
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub description: String,
    pub id: ItemTarget,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Capability is known only after item identity is known. Holds remain private.
    pub known_supply: Option<crate::SupplyView>,
    /// Currency capability is disclosed only for identified coins.
    pub known_currency: Option<CurrencyKind>,
}
impl ItemView {
    pub(crate) fn valid_disclosure(&self) -> bool {
        self.known_supply
            .as_ref()
            .is_none_or(|supply| self.identified && supply.valid())
            && self
                .known_currency
                .is_none_or(|_| self.identified && self.class == ItemClass::Coin)
            && (self.corpse.is_none() || (self.identified && self.class == ItemClass::Corpse))
    }
}
impl<'de> Deserialize<'de> for ItemView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            class: ItemClass,
            #[serde(default)]
            corpse: Option<CreatureAppearance>,
            #[serde(with = "crate::integers::unsigned")]
            quantity: u64,
            appearance: String,
            identified: bool,
            #[serde(default)]
            description: String,
            id: ItemTarget,
            name: String,
            #[serde(default)]
            asset: Option<String>,
            #[serde(deserialize_with = "required_option")]
            known_supply: Option<crate::SupplyView>,
            #[serde(deserialize_with = "required_option")]
            known_currency: Option<CurrencyKind>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            class: f.class,
            corpse: f.corpse,
            quantity: f.quantity,
            appearance: f.appearance,
            identified: f.identified,
            description: f.description,
            id: f.id,
            name: f.name,
            asset: f.asset,
            known_supply: f.known_supply,
            known_currency: f.known_currency,
        };
        if value.valid_disclosure() {
            Ok(value)
        } else {
            Err(serde::de::Error::custom("invalid item disclosure"))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GroundItemView {
    /// Cosmetic only; never changes the enclosed item's capabilities.
    pub presentation: Option<crate::ObjectPresentation>,
    pub reachable: bool,
    /// Server-owned known-supply touch affordance, separate from pickup reach.
    pub supply_reachable: bool,
    pub item: ItemView,
    pub position: Position,
}
impl GroundItemView {
    pub(crate) fn valid_presentation(&self) -> bool {
        self.presentation.as_ref().is_none_or(|p| {
            p.validate().is_ok()
                && self.item.name == "object"
                && self.item.appearance == "object"
                && self.item.description.is_empty()
                && self.item.asset.is_none()
                && self.item.corpse.is_none()
        })
    }
}
impl<'de> Deserialize<'de> for GroundItemView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            reachable: bool,
            supply_reachable: bool,
            #[serde(deserialize_with = "required_option")]
            presentation: Option<crate::ObjectPresentation>,
            item: ItemView,
            position: Position,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.supply_reachable && f.item.known_supply.is_none() {
            return Err(serde::de::Error::custom(
                "ground supply reach without a known supply",
            ));
        }
        let value = Self {
            reachable: f.reachable,
            supply_reachable: f.supply_reachable,
            presentation: f.presentation,
            item: f.item,
            position: f.position,
        };
        if !value.valid_presentation() {
            return Err(serde::de::Error::custom(
                "invalid ground object presentation",
            ));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance_seed: Option<crate::AppearanceSeed>,
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
    /// Read-only chart catalog; never sight, ordinary map memory or action offers.
    #[serde(deserialize_with = "crate::chart_pages::read_summaries")]
    pub charts: Vec<crate::chart_pages::ChartSummary>,
    /// Own appearance is disclosed even when the body is outside the eye scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_appearance: Option<CreatureAppearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions: Option<InteractionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionView>,
    pub places: Vec<PlaceView>,
    pub actor: ActorId,
    /// Own-body entity reference; attachment actor IDs are routing identities.
    pub self_target: ActorTarget,
    #[serde(deserialize_with = "read_linked_holds")]
    pub linked_holds: Vec<LinkedHold>,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub position: Position,
    pub visible_cells: Vec<CellView>,
    pub ground_items: Vec<GroundItemView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub ready: bool,
}

pub const MAX_LINKED_HOLDS: usize = 128;

/// A personal escape offer, separate from timed standalone conditions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedHold {
    pub target: HoldTarget,
    /// Present only when ordinary sight already discloses this holder.
    pub holder: Option<ActorTarget>,
}
impl<'de> Deserialize<'de> for LinkedHold {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            target: HoldTarget,
            #[serde(deserialize_with = "required_option")]
            holder: Option<ActorTarget>,
        }
        let Fields { target, holder } = crate::bounded::named_map(d)?;
        Ok(Self { target, holder })
    }
}
pub(crate) fn read_linked_holds<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<LinkedHold>, D::Error> {
    crate::bounded::bounded::<D, LinkedHold, MAX_LINKED_HOLDS>(d)
}

pub const MAX_EQUIPMENT_SLOTS: usize = 64;

/// Disclosed equipment socket kind from the active ruleset. Socket positions
/// remain numeric, while compatibility and presentation use this content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSlot {
    pub id: String,
    pub name: String,
    pub equipped_phrase: String,
    pub requires_attack: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
    Psychic,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentView {
    pub attack: Option<crate::AttackView>,
    pub modifiers: Vec<crate::CombatTraceStep>,
}
/// Owner-remembered instance bonuses; not derived projectile damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectileBonusView {
    pub hit: i32,
    pub damage: i32,
}
impl ProjectileBonusView {
    pub(crate) fn valid(&self) -> bool {
        [self.hit, self.damage]
            .into_iter()
            .all(|n| (-1_000_000..=1_000_000).contains(&n))
    }
}
impl<'de> Deserialize<'de> for ProjectileBonusView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            hit: i32,
            damage: i32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            hit: fields.hit,
            damage: fields.damage,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom(
                "invalid remembered projectile bonus",
            ));
        }
        Ok(value)
    }
}
/// Disclosed physical fuel state and current authoritative action eligibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableLightView {
    #[serde(with = "crate::integers::unsigned")]
    pub remaining: u64,
    pub lit: bool,
    pub ignitable: bool,
    pub extinguishable: bool,
}
impl PortableLightView {
    pub fn valid(&self) -> bool {
        self.remaining <= 1_000_000
            && (!self.lit || self.remaining > 0)
            && (!self.ignitable || (!self.lit && self.remaining > 0))
            && (!self.extinguishable || self.lit)
    }
}
impl<'de> Deserialize<'de> for PortableLightView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "crate::integers::unsigned")]
            remaining: u64,
            lit: bool,
            ignitable: bool,
            extinguishable: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            remaining: f.remaining,
            lit: f.lit,
            ignitable: f.ignitable,
            extinguishable: f.extinguishable,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid portable light"));
        }
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInteractionView {
    pub item: ItemTarget,
    pub slot: Option<EquipmentSlot>,
    pub equipped_slot: Option<u16>,
    pub known_equipment: Option<EquipmentView>,
    pub known_projectile_bonus: Option<ProjectileBonusView>,
    pub known_cursed: Option<bool>,
    pub known_protected: Option<bool>,
    pub drinkable: bool,
    pub edible: bool,
    pub readable: bool,
    pub zappable: bool,
    pub device_activation: Option<DeviceActivation>,
    pub portable_light: Option<PortableLightView>,
    pub charges: Option<u16>,
}
impl<'de> Deserialize<'de> for ItemInteractionView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemTarget,
            slot: Option<EquipmentSlot>,
            equipped_slot: Option<u16>,
            known_equipment: Option<EquipmentView>,
            #[serde(deserialize_with = "required_nullable")]
            known_projectile_bonus: Option<ProjectileBonusView>,
            #[serde(deserialize_with = "required_nullable")]
            known_cursed: Option<bool>,
            #[serde(deserialize_with = "required_nullable")]
            known_protected: Option<bool>,
            drinkable: bool,
            edible: bool,
            readable: bool,
            zappable: bool,
            #[serde(deserialize_with = "required_nullable")]
            device_activation: Option<DeviceActivation>,
            #[serde(deserialize_with = "required_nullable")]
            portable_light: Option<PortableLightView>,
            #[serde(deserialize_with = "required_nullable")]
            charges: Option<u16>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            item: fields.item,
            slot: fields.slot,
            equipped_slot: fields.equipped_slot,
            known_equipment: fields.known_equipment,
            known_projectile_bonus: fields.known_projectile_bonus,
            known_cursed: fields.known_cursed,
            known_protected: fields.known_protected,
            drinkable: fields.drinkable,
            edible: fields.edible,
            readable: fields.readable,
            zappable: fields.zappable,
            device_activation: fields.device_activation,
            portable_light: fields.portable_light,
            charges: fields.charges,
        })
    }
}

/// An owner-only paid selection. Items are current carried opaque references.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentificationChoiceView {
    pub choice: IntentionId,
    pub classes: Vec<ItemClass>,
    pub items: Vec<ItemTarget>,
}
impl<'de> Deserialize<'de> for IdentificationChoiceView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            choice: IntentionId,
            #[serde(deserialize_with = "read_item_classes")]
            classes: Vec<ItemClass>,
            #[serde(deserialize_with = "crate::bounded::bounded::<_, _, 64>")]
            items: Vec<ItemTarget>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let mut seen = std::collections::BTreeSet::new();
        if fields.choice.0.is_empty()
            || fields.choice.0.len() > 256
            || !valid_identification_classes(&fields.classes)
            || !fields.items.iter().all(|item| seen.insert(*item))
        {
            return Err(serde::de::Error::custom("invalid identification choice"));
        }
        Ok(Self {
            choice: fields.choice,
            classes: fields.classes,
            items: fields.items,
        })
    }
}

pub(crate) fn valid_identification_classes(classes: &[ItemClass]) -> bool {
    !classes.is_empty() && classes.len() <= 14 && classes.windows(2).all(|pair| pair[0] < pair[1])
}
fn read_item_classes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ItemClass>, D::Error> {
    #[derive(Deserialize)]
    struct Token(#[serde(deserialize_with = "read_item_class_token")] ItemClass);
    let values: Vec<Token> = crate::bounded::bounded::<_, _, 14>(d)?;
    let classes: Vec<_> = values.into_iter().map(|value| value.0).collect();
    if !valid_identification_classes(&classes) {
        return Err(serde::de::Error::custom("invalid identification class set"));
    }
    Ok(classes)
}

fn read_direction_token<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Direction, D::Error> {
    let token = String::deserialize(d)?;
    Direction::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(token))
}

fn read_item_class_token<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ItemClass, D::Error> {
    let token = String::deserialize(d)?;
    ItemClass::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(token))
}

fn required_nullable<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationView {
    pub action: Action,
    #[serde(with = "crate::integers::unsigned")]
    pub remaining: u64,
    pub active: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemRefusalReason {
    TooWeak,
}
impl<'de> Deserialize<'de> for ItemRefusalReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "too_weak" => Ok(Self::TooWeak),
            _ => Err(serde::de::Error::custom("unknown item refusal reason")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemRefusalView {
    pub action: Action,
    pub reason: ItemRefusalReason,
}
impl<'de> Deserialize<'de> for ItemRefusalView {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            action: Action,
            reason: ItemRefusalReason,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if !matches!(fields.action, Action::Zap { .. }) {
            return Err(serde::de::Error::custom("refusal requires a zap action"));
        }
        Ok(Self {
            action: fields.action,
            reason: fields.reason,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionView {
    #[serde(deserialize_with = "required_nullable")]
    pub identification: Option<IdentificationChoiceView>,
    /// This observer's completed item actions, including consumed final units.
    pub completed: Vec<Action>,
    /// Paid completed attempts that refused their effect, never successful completion.
    pub refused: Vec<ItemRefusalView>,
    pub curse_blocked: Vec<ItemTarget>,
    pub slots: Vec<EquipmentSlot>,
    pub preparation: Option<PreparationView>,
    pub inventory: Vec<ItemInteractionView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceView {
    pub completed: u16,
    pub total: u16,
    #[serde(with = "crate::integers::unsigned")]
    pub next_strike_remaining: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub cancel_recovery_remaining: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatView {
    /// Exact information about the attached actor, never another actor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_stats: Option<crate::OwnStats>,
    #[serde(with = "crate::integers::unsigned")]
    pub hp: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub max_hp: u64,
    #[serde(default, with = "crate::integers::optional_unsigned")]
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    /// Exact progress for the attached actor's multi-strike preparation only.
    #[serde(deserialize_with = "required_option")]
    pub sequence: Option<SequenceView>,
    /// One historical paid result for this observer; never a live target view.
    #[serde(deserialize_with = "required_option")]
    pub public_plan: Option<crate::PublicPlanObservationView>,
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
impl<'de> Deserialize<'de> for CombatView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            own_stats: Option<crate::OwnStats>,
            #[serde(with = "crate::integers::unsigned")]
            hp: u64,
            #[serde(with = "crate::integers::unsigned")]
            max_hp: u64,
            #[serde(default, with = "crate::integers::optional_unsigned")]
            preparation_remaining: Option<u64>,
            preparation_active: bool,
            #[serde(deserialize_with = "required_option")]
            sequence: Option<SequenceView>,
            #[serde(deserialize_with = "required_option")]
            public_plan: Option<crate::PublicPlanObservationView>,
            #[serde(with = "crate::integers::unsigned")]
            recovery_remaining: u64,
            actors: Vec<CombatActorView>,
            events: Vec<CombatEventView>,
            objective: Option<ObjectiveKind>,
            #[serde(default)]
            exit: Option<String>,
            victory: bool,
            dead: bool,
            terminal: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            own_stats: f.own_stats,
            hp: f.hp,
            max_hp: f.max_hp,
            preparation_remaining: f.preparation_remaining,
            preparation_active: f.preparation_active,
            sequence: f.sequence,
            public_plan: f.public_plan,
            recovery_remaining: f.recovery_remaining,
            actors: f.actors,
            events: f.events,
            objective: f.objective,
            exit: f.exit,
            victory: f.victory,
            dead: f.dead,
            terminal: f.terminal,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatActorView {
    pub actor: ActorTarget,
    pub hostile: bool,
    /// Server-authoritative same-faction, bidirectionally nonhostile eligibility.
    pub allied: bool,
    pub injury: Injury,
}
impl<'de> Deserialize<'de> for CombatActorView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorTarget,
            hostile: bool,
            allied: bool,
            injury: Injury,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            actor: f.actor,
            hostile: f.hostile,
            allied: f.allied,
            injury: f.injury,
        })
    }
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
    /// Carry the unique objective item through an intentional surface exit.
    RetrieveAndExit,
    ReachExit,
}

/// A combat event as the observer knows it. An absent participant is one
/// it couldn't see; the observer itself is `Observation::self_target`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatEventView {
    /// A personal sensation. The source may be a local anonymous contact;
    /// no ability identity, species, strike ordinal or private plan is disclosed.
    UnseenAbility {
        caster: Option<ActorTarget>,
        target: ActorTarget,
        outcome: AbilityOutcome,
    },
    /// Observable activity with no licensed ability or item identity.
    #[serde(deserialize_with = "read_anonymous_ability")]
    AnonymousAbility {
        caster: Option<ActorTarget>,
        target: Option<ActorTarget>,
        outcome: AbilityOutcome,
    },
    /// The observer's own Escape result, with no hidden holder details.
    EscapeResolved {
        hold: HoldTarget,
        success: bool,
    },
    Ability {
        caster: Option<ActorTarget>,
        target: Option<ActorTarget>,
        ability: crate::ContentRef,
        outcome: AbilityOutcome,
        /// Actual zero-based primary ordinal; never an opponent's future total.
        #[serde(deserialize_with = "required_option")]
        strike: Option<u16>,
    },
    /// The observer's own standalone status; no hidden causer is disclosed.
    Condition {
        condition: crate::ContentRef,
    },
    /// The observer's own attack preparation was interrupted.
    Interrupted {
        actor: ActorTarget,
    },
    Died {
        actor: ActorTarget,
    },
}

fn read_anonymous_ability<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(Option<ActorTarget>, Option<ActorTarget>, AbilityOutcome), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(deserialize_with = "required_option")]
        caster: Option<ActorTarget>,
        #[serde(deserialize_with = "required_option")]
        target: Option<ActorTarget>,
        outcome: String,
    }
    let fields: Fields = crate::bounded::named_map(deserializer)?;
    let outcome = match fields.outcome.as_str() {
        "applied" => AbilityOutcome::Applied,
        "unaffected" => AbilityOutcome::Unaffected,
        "miss" => AbilityOutcome::Miss,
        _ => {
            return Err(serde::de::Error::custom(
                "invalid anonymous ability outcome",
            ))
        }
    };
    Ok((fields.caster, fields.target, outcome))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CellView {
    /// Sampled object art; stair flags remain authoritative.
    pub object_presentation: Option<crate::ObjectPresentation>,
    /// Current server illumination, independent of sight or body awareness.
    pub lit: bool,
    pub door: Option<DoorView>,
    /// Discovered physical presence only; family stays absent until examined or witnessed.
    pub trap: Option<TrapView>,
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

impl CellView {
    pub(crate) fn valid_presentation(&self) -> bool {
        self.object_presentation.as_ref().is_none_or(|p| {
            p.validate().is_ok() && !self.wall && (self.stairs_up || self.stairs_down)
        })
    }
}
impl<'de> Deserialize<'de> for CellView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "required_option")]
            object_presentation: Option<crate::ObjectPresentation>,
            lit: bool,
            door: Option<DoorView>,
            #[serde(deserialize_with = "required_option")]
            trap: Option<TrapView>,
            #[serde(default)]
            material: String,
            key: String,
            stairs_up: bool,
            stairs_down: bool,
            position: Position,
            wall: bool,
            place_hint: bool,
            #[serde(default)]
            asset: Option<String>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            object_presentation: f.object_presentation,
            lit: f.lit,
            door: f.door,
            trap: f.trap,
            material: f.material,
            key: f.key,
            stairs_up: f.stairs_up,
            stairs_down: f.stairs_down,
            position: f.position,
            wall: f.wall,
            place_hint: f.place_hint,
            asset: f.asset,
        };
        if !value.valid_presentation() {
            return Err(serde::de::Error::custom("invalid cell object presentation"));
        }
        Ok(value)
    }
}

/// A disclosed trap family; no effect operands or discovery difficulty cross the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrapFamily {
    Rust,
    SleepingGas,
    TrapDoor,
    Arrow,
    BearTrap,
    Teleport,
    PoisonDart,
    Mysterious,
}

impl<'de> Deserialize<'de> for TrapFamily {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = TrapFamily;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a canonical trap family string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "rust" => Ok(TrapFamily::Rust),
                    "sleeping_gas" => Ok(TrapFamily::SleepingGas),
                    "trap_door" => Ok(TrapFamily::TrapDoor),
                    "arrow" => Ok(TrapFamily::Arrow),
                    "bear_trap" => Ok(TrapFamily::BearTrap),
                    "teleport" => Ok(TrapFamily::Teleport),
                    "poison_dart" => Ok(TrapFamily::PoisonDart),
                    "mysterious" => Ok(TrapFamily::Mysterious),
                    _ => Err(E::unknown_variant(
                        value,
                        &[
                            "rust",
                            "sleeping_gas",
                            "trap_door",
                            "arrow",
                            "bear_trap",
                            "teleport",
                            "poison_dart",
                            "mysterious",
                        ],
                    )),
                }
            }
        }
        d.deserialize_str(Visitor)
    }
}

impl TrapFamily {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust trap",
            Self::SleepingGas => "sleeping gas trap",
            Self::TrapDoor => "trapdoor",
            Self::Arrow => "arrow trap",
            Self::BearTrap => "bear trap",
            Self::Teleport => "teleport trap",
            Self::PoisonDart => "poison dart trap",
            Self::Mysterious => "mysterious trap",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrapView {
    pub id: TrapTarget,
    #[serde(deserialize_with = "required_option")]
    pub family: Option<TrapFamily>,
    pub reachable: bool,
}
impl<'de> Deserialize<'de> for TrapView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: TrapTarget,
            #[serde(deserialize_with = "required_option")]
            family: Option<TrapFamily>,
            reachable: bool,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            id: fields.id,
            family: fields.family,
            reachable: fields.reachable,
        })
    }
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
    #[serde(deserialize_with = "crate::projectile_choices::read_command")]
    ProjectileChoices {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        query: crate::ProjectileChoiceQuery,
    },
    #[serde(deserialize_with = "crate::weapon_preview::read_command")]
    PreviewAbilityWeapon {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        ability: String,
        invocation: crate::AbilityInvocation,
        supplies: crate::SupplySelection,
    },
    #[serde(deserialize_with = "crate::advancement::read_query_command")]
    Advancement {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        query: crate::AdvancementQuery,
    },
    /// Private read-only authored source detail; authority is checked by the host.
    InspectStrikeSource {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        actor: ActorId,
        #[serde(with = "crate::integers::unsigned")]
        target_revision: u64,
        source: crate::StrikeSourceSelector,
        #[serde(deserialize_with = "required_option")]
        cursor: Option<crate::StrikePageCursor>,
    },
    ResumeIntention {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        intention: IntentionId,
    },
    CancelIntention {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        intention: IntentionId,
        #[serde(deserialize_with = "required_option")]
        step: Option<Direction>,
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
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    EscapeStarted {
        hold: HoldTarget,
    },
    AbilityStarted {
        ability: crate::ContentRef,
        target: ActorTarget,
    },
    PreparationPaused,
    ItemStarted {
        action: Action,
    },
    DoorChanged {
        door: DoorTarget,
        open: bool,
    },
    Moved {
        direction: Direction,
    },
    /// The actor deliberately left through its terminal objective endpoint.
    Escaped {
        item: ItemTarget,
    },
    Crumbled {
        item: ItemTarget,
        #[serde(with = "crate::integers::unsigned")]
        quantity: u64,
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
        event: Box<Event>,
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
        request: Box<Request>,
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
    /// Read-only appearances owned by the attached actor.
    Map {
        query: crate::MapQuery,
    },
    /// Read-only immutable chart pages owned by the attached view, never actions.
    Chart {
        query: crate::chart_pages::ChartQuery,
    },
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyContext {
    pub input: InputContext,
    pub actor: ActorId,
    pub branch: BranchId,
    pub cursor: StreamCursor,
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
}
impl<'de> Deserialize<'de> for ReplyContext {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            input: InputContext,
            actor: ActorId,
            branch: BranchId,
            cursor: StreamCursor,
            #[serde(with = "crate::integers::unsigned")]
            revision: u64,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            input: fields.input,
            actor: fields.actor,
            branch: fields.branch,
            cursor: fields.cursor,
            revision: fields.revision,
        })
    }
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
    /// Capacity is exhausted; the affected admission or execution has not committed.
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
    #[serde(deserialize_with = "crate::projectile_choices::read_reply")]
    ProjectileChoices {
        context: ReplyContext,
        request_id: String,
        page: Box<crate::ProjectileChoicePage>,
    },
    #[serde(deserialize_with = "crate::weapon_preview::read_reply")]
    AbilityWeaponPreview {
        context: ReplyContext,
        request_id: String,
        preview: Box<crate::AbilityWeaponPreview>,
    },
    #[serde(deserialize_with = "crate::map_pages::read_page_reply")]
    MapPage {
        context: ReplyContext,
        request_id: String,
        page: Box<crate::MapPage>,
    },
    #[serde(deserialize_with = "crate::chart_pages::read_page_reply")]
    ChartPage {
        context: ReplyContext,
        request_id: String,
        page: Box<crate::chart_pages::ChartPage>,
    },
    #[serde(deserialize_with = "crate::creature_catalog::read_reply")]
    CreatureCatalog {
        context: ReplyContext,
        request_id: String,
        page: Box<crate::CreatureCatalogPage>,
    },
    #[serde(deserialize_with = "crate::advancement::read_page_reply")]
    AdvancementPage {
        context: ReplyContext,
        request_id: String,
        page: Box<crate::AdvancementPage>,
    },
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
    CombatDiagnostics {
        context: ReplyContext,
        request_id: String,
        report: Box<crate::CombatDiagnosticsView>,
    },
    CreatureStrikePage {
        context: ReplyContext,
        request_id: String,
        page: Box<crate::CreatureStrikePage>,
    },
    CreatureInspection {
        context: ReplyContext,
        request_id: String,
        report: Box<crate::CreatureInspectionView>,
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
            Self::ProjectileChoices { context, .. }
            | Self::AbilityWeaponPreview { context, .. }
            | Self::MapPage { context, .. }
            | Self::ChartPage { context, .. }
            | Self::CreatureCatalog { context, .. }
            | Self::AdvancementPage { context, .. }
            | Self::Ack { context, .. }
            | Self::History { context, .. }
            | Self::CreatureInspection { context, .. }
            | Self::CreatureStrikePage { context, .. }
            | Self::CombatDiagnostics { context, .. }
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

/// Disclosed physical category; independent of true identity and item effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    #[default]
    Misc,
    Weapon,
    Armor,
    Potion,
    Food,
    Corpse,
    Tool,
    Amulet,
    Ring,
    Scroll,
    Spellbook,
    Wand,
    Coin,
    Gem,
}
/// Explicit ability operands. Physical projectile delivery is its sole weapon selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityInvocation {
    pub aim: AbilityAim,
    pub weapon: WeaponSelection,
}
impl AbilityInvocation {
    pub fn actor_target(target: ActorTarget) -> Self {
        Self {
            aim: AbilityAim::ActorTarget { target },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn self_step(direction: Direction) -> Self {
        Self {
            aim: AbilityAim::SelfStep { direction },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn directional_projectile(
        direction: Direction,
        delivery: crate::ProjectileDeliverySelection,
    ) -> Self {
        Self {
            aim: AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn directional_ray(direction: Direction) -> Self {
        Self {
            aim: AbilityAim::DirectionalRay { direction },
            weapon: WeaponSelection::Default,
        }
    }
}
impl From<ActorTarget> for AbilityInvocation {
    fn from(target: ActorTarget) -> Self {
        Self::actor_target(target)
    }
}
impl<'de> Deserialize<'de> for AbilityInvocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            aim: AbilityAim,
            weapon: WeaponSelection,
        }
        let Fields { aim, weapon } = crate::bounded::named_map(deserializer)?;
        if matches!(aim, AbilityAim::DirectionalProjectile { .. })
            && weapon != WeaponSelection::Default
        {
            return Err(serde::de::Error::custom(
                "projectile delivery is the sole weapon selector",
            ));
        }
        Ok(Self { aim, weapon })
    }
}
/// Exact equipped identity and socket, resolved only in the owner's disclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WeaponSelection {
    Default,
    Equipped { item: ItemTarget, socket: u16 },
}
impl<'de> Deserialize<'de> for WeaponSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Default {},
            Equipped { item: ItemTarget, socket: u16 },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Default {} => Self::Default,
            Fields::Equipped { item, socket } => {
                if usize::from(socket) >= MAX_EQUIPMENT_SLOTS {
                    return Err(serde::de::Error::custom(
                        "invalid equipped weapon selection",
                    ));
                }
                Self::Equipped { item, socket }
            }
        })
    }
}
/// Selected public aim; the backend resolves movement and ray collision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AbilityAim {
    ActorTarget {
        target: ActorTarget,
    },
    SelfStep {
        direction: Direction,
    },
    DirectionalRay {
        direction: Direction,
    },
    DirectionalProjectile {
        direction: Direction,
        delivery: crate::ProjectileDeliverySelection,
    },
}
impl<'de> Deserialize<'de> for AbilityAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Unit-enum deserialization alone also accepts an externally tagged
        // object. Selected directions are the documented string token only.
        fn direction_token<'de, D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Direction, D::Error> {
            let token = String::deserialize(deserializer)?;
            Direction::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(token))
        }
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ActorTarget {
                target: ActorTarget,
            },
            SelfStep {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
            DirectionalRay {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
            DirectionalProjectile {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
                delivery: crate::ProjectileDeliverySelection,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::ActorTarget { target } => Self::ActorTarget { target },
            Fields::SelfStep { direction } => Self::SelfStep { direction },
            Fields::DirectionalRay { direction } => Self::DirectionalRay { direction },
            Fields::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction,
                delivery,
            },
        })
    }
}
