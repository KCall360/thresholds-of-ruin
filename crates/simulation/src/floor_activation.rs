//! Once-only authoritative population at an accepted character entry. Geometry
//! preparation registers immutable inputs only; it never rolls or spawns.
use crate::prepared_spawn::{DepthGrowth, PreparedActor};
use crate::quantity::QuantityExpr;
use crate::{ActorId, Game, GameError, ItemId, ItemLocation, ItemSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use tor_world::{Location, RegionId, Shared};

#[path = "spawn_carried_loot.rs"]
mod carried_loot;
#[path = "room_cluster.rs"]
mod room_cluster;
#[path = "wandering_population.rs"]
mod wandering_population;
use carried_loot::{read_pools, BirthStockContext};
pub use carried_loot::{
    CarriedLoot, CarriedLootDecision, CarryChance, PreparedCarriedItemPool, MAX_CARRIED_ITEM_POOLS,
};
pub use wandering_population::WanderingPolicy;
pub(crate) use wandering_population::WanderingPopulation;

pub const MAX_PROFILES: usize = 32;
pub const MAX_STAGES: usize = 32;
pub const MAX_DEPTH_BANDS: usize = 32;
pub const MAX_CHOICES: usize = 64;
pub const MAX_ATTEMPTS: u16 = 256;
pub const MAX_SCOPE_MEMBERS: usize = 64;
pub const MAX_SCOPE_CELLS: usize = 65_536;
pub const MAX_ROOM_GROUPS: usize = 64;
pub const MAX_ROOM_CELLS: usize = 1024;
pub const MAX_SCOPES: usize = 1_000_000;
pub const MAX_RETAINED_DECISIONS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCapabilities {
    pub npc_stair_traversal: bool,
}
impl<'de> Deserialize<'de> for ScenarioCapabilities {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            npc_stair_traversal: bool,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            npc_stair_traversal: fields.npc_stair_traversal,
        })
    }
}

impl Default for ScenarioCapabilities {
    fn default() -> Self {
        Self {
            npc_stair_traversal: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioPolicy {
    capabilities: ScenarioCapabilities,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    selected_character: Option<ActorId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationPlan {
    pub protagonist: ActorId,
    pub carried_item_pools: BTreeMap<String, PreparedCarriedItemPool>,
    #[serde(deserialize_with = "read_profiles")]
    pub profiles: BTreeMap<String, PopulationProfile>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationProfile {
    pub version: u32,
    #[serde(deserialize_with = "read_stages")]
    pub stages: Vec<PopulationStage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationStage {
    pub id: String,
    pub version: u32,
    pub count: [u16; 2],
    pub population: PopulationContent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DepthInterval {
    Finite { minimum: u32, maximum: u32 },
    Open { minimum: u32 },
}
impl DepthInterval {
    fn minimum(self) -> u32 {
        match self {
            Self::Finite { minimum, .. } | Self::Open { minimum } => minimum,
        }
    }
    fn maximum(self) -> Option<u32> {
        match self {
            Self::Finite { maximum, .. } => Some(maximum),
            Self::Open { .. } => None,
        }
    }
    fn valid(self) -> bool {
        self.minimum() > 0 && self.maximum().is_none_or(|end| end >= self.minimum())
    }
    pub fn contains(self, depth: u32) -> bool {
        depth >= self.minimum() && self.maximum().is_none_or(|end| depth <= end)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DepthBand<T> {
    pub depth: DepthInterval,
    pub choices: Vec<T>,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for DepthBand<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
        struct Fields<T> {
            depth: DepthInterval,
            #[serde(deserialize_with = "read_choices")]
            choices: Vec<T>,
        }
        let fields: Fields<T> = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            depth: fields.depth,
            choices: fields.choices,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationContent {
    RoomCluster {
        chance: CarryChance,
        item_count: [u16; 2],
        actor_count: [u16; 2],
        actor_depth_offset: u32,
        #[serde(deserialize_with = "read_bands")]
        item_bands: Vec<DepthBand<PreparedItemChoice>>,
        #[serde(deserialize_with = "read_bands")]
        actor_bands: Vec<DepthBand<PreparedActorChoice>>,
        avoid_protagonist_sight: bool,
    },
    RoomItems {
        #[serde(deserialize_with = "read_bands")]
        bands: Vec<DepthBand<PreparedItemChoice>>,
        chance: CarryChance,
    },
    Items {
        #[serde(deserialize_with = "read_bands")]
        bands: Vec<DepthBand<PreparedItemChoice>>,
    },
    Actors {
        #[serde(deserialize_with = "read_bands")]
        bands: Vec<DepthBand<PreparedActorChoice>>,
        avoid_protagonist_sight: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedItemChoice {
    pub content: String,
    pub weight: u32,
    pub quantity: QuantityExpr,
    pub spec: ItemSpec,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedActorChoice {
    pub carried_loot: Option<CarriedLoot>,
    pub depth_growth: DepthGrowth,
    pub advancement: Vec<crate::prepared_spawn::OrdinalAdvancement>,
    pub content: String,
    pub weight: u32,
    pub turn_ticks: u64,
    pub actor: PreparedActor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedScope {
    pub scope: String,
    pub depth: u32,
    pub profile: String,
    #[serde(deserialize_with = "read_members")]
    pub members: BTreeSet<RegionId>,
    #[serde(deserialize_with = "read_cells")]
    pub cells: Vec<Location>,
    #[serde(deserialize_with = "read_protected")]
    pub protected: BTreeSet<Location>,
    #[serde(deserialize_with = "read_rooms")]
    pub rooms: Vec<PreparedRoomGroup>,
}

/// Immutable generated room interior. IDs are original one-based slot ordinals;
/// absent slots never renumber other rooms. This metadata creates no residency pin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedRoomGroup {
    pub id: u32,
    pub cells: Vec<Location>,
}
impl<'de> Deserialize<'de> for PreparedRoomGroup {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: u32,
            #[serde(deserialize_with = "read_room_cells")]
            cells: Vec<Location>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            id: fields.id,
            cells: fields.cells,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryCause {
    Initial,
    Movement,
    Physics,
    Teleport,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FloorActivationRecord {
    pub scope: String,
    pub entrant: ActorId,
    pub tick: u64,
    pub ordinal: u64,
    pub cause: EntryCause,
    pub profile: String,
    pub profile_version: u32,
    #[serde(deserialize_with = "read_decisions")]
    pub decisions: Vec<StageDecision>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageDecision {
    pub id: String,
    pub version: u32,
    pub result: StageResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageResult {
    RoomCluster {
        selection: ClusterSelection,
    },
    RoomsEvaluated {
        #[serde(deserialize_with = "read_room_decisions")]
        rooms: Vec<RoomDecision>,
    },
    Skipped {
        reason: SkipReason,
    },
    Evaluated {
        requested: u16,
        #[serde(deserialize_with = "read_attempts")]
        attempts: Vec<PlacementDecision>,
    },
}
/// Counts retain the independent authored requests, before room-area caps.
/// Placements retain immutable births, not continued ownership or survival.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClusterSelection {
    ChanceMiss {},
    NoEligibleRoom {},
    Selected {
        room: u32,
        room_area: u16,
        actor_depth: u32,
        item_requested: u16,
        actor_requested: u16,
        #[serde(deserialize_with = "read_attempts")]
        items: Vec<PlacementDecision>,
        #[serde(deserialize_with = "read_attempts")]
        actors: Vec<PlacementDecision>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomDecision {
    pub room: u32,
    pub result: RoomResult,
}
impl<'de> Deserialize<'de> for RoomDecision {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            room: u32,
            result: RoomResult,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            room: fields.room,
            result: fields.result,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoomResult {
    ChanceMiss,
    Attempt { decision: PlacementDecision },
}
impl<'de> Deserialize<'de> for RoomResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ChanceMiss {},
            Attempt { decision: PlacementDecision },
        }
        Ok(match Fields::deserialize(d)? {
            Fields::ChanceMiss {} => Self::ChanceMiss,
            Fields::Attempt { decision } => Self::Attempt { decision },
        })
    }
}
impl StageResult {
    /// Every ordinary birth receipt, including room-scoped attempts.
    pub fn attempts(&self) -> impl Iterator<Item = &PlacementDecision> {
        let (attempts, actors, rooms): (
            &[PlacementDecision],
            &[PlacementDecision],
            &[RoomDecision],
        ) = match self {
            Self::Skipped { .. }
            | Self::RoomCluster {
                selection: ClusterSelection::ChanceMiss {} | ClusterSelection::NoEligibleRoom {},
            } => (&[], &[], &[]),
            Self::Evaluated { attempts, .. } => (attempts, &[], &[]),
            Self::RoomsEvaluated { rooms } => (&[], &[], rooms),
            Self::RoomCluster {
                selection: ClusterSelection::Selected { items, actors, .. },
            } => (items, actors, &[]),
        };
        attempts
            .iter()
            .chain(actors)
            .chain(rooms.iter().filter_map(|room| match &room.result {
                RoomResult::ChanceMiss => None,
                RoomResult::Attempt { decision } => Some(decision),
            }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    NoEligibleContent,
    NoEligibleCell,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlacementDecision {
    Placed {
        content: String,
        #[serde(deserialize_with = "crate::bounded::location")]
        location: Location,
        entity: PopulationEntity,
    },
    Skipped {
        content: String,
        reason: SkipReason,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationEntity {
    Actor {
        id: ActorId,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        carried_loot: Option<CarriedLootDecision>,
    },
    Item {
        id: ItemId,
        quantity: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivationState {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub(crate) wandering: Option<wandering_population::WanderingPopulation>,
    plan: Shared<PopulationPlan>,
    #[serde(deserialize_with = "read_scopes")]
    scopes: Shared<BTreeMap<String, Shared<PreparedScope>>>,
    #[serde(deserialize_with = "read_member_index")]
    members: Shared<BTreeMap<RegionId, String>>,
    #[serde(deserialize_with = "read_floors")]
    pub(crate) floors: Shared<BTreeMap<String, Shared<FloorActivationRecord>>>,
    #[serde(deserialize_with = "read_order")]
    order: Shared<Vec<String>>,
    decision_count: usize,
}

impl<'de> Deserialize<'de> for ScenarioPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            capabilities: ScenarioCapabilities,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            selected_character: Option<ActorId>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            capabilities: fields.capabilities,
            selected_character: fields.selected_character,
        })
    }
}
impl<'de> Deserialize<'de> for PopulationPlan {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            protagonist: ActorId,
            #[serde(deserialize_with = "read_pools")]
            carried_item_pools: BTreeMap<String, PreparedCarriedItemPool>,
            #[serde(deserialize_with = "read_profiles")]
            profiles: BTreeMap<String, PopulationProfile>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            protagonist: fields.protagonist,
            carried_item_pools: fields.carried_item_pools,
            profiles: fields.profiles,
        })
    }
}
impl<'de> Deserialize<'de> for PopulationProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            #[serde(deserialize_with = "read_stages")]
            stages: Vec<PopulationStage>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            version: fields.version,
            stages: fields.stages,
        })
    }
}
impl<'de> Deserialize<'de> for PopulationStage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: String,
            version: u32,
            count: [u16; 2],
            population: PopulationContent,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            version: fields.version,
            count: fields.count,
            population: fields.population,
        })
    }
}
impl<'de> Deserialize<'de> for PreparedItemChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            content: String,
            weight: u32,
            quantity: QuantityExpr,
            spec: ItemSpec,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            content: fields.content,
            weight: fields.weight,
            quantity: fields.quantity,
            spec: fields.spec,
        })
    }
}
impl<'de> Deserialize<'de> for PreparedActorChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            depth_growth: DepthGrowth,
            advancement: crate::bounded::Bounded<
                crate::prepared_spawn::OrdinalAdvancement,
                { crate::limits::HIT_DICE },
            >,
            content: String,
            weight: u32,
            turn_ticks: u64,
            actor: PreparedActor,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            carried_loot: Option<CarriedLoot>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            depth_growth: fields.depth_growth,
            advancement: fields.advancement.0,
            content: fields.content,
            weight: fields.weight,
            turn_ticks: fields.turn_ticks,
            actor: fields.actor,
            carried_loot: fields.carried_loot,
        })
    }
}
impl<'de> Deserialize<'de> for PreparedScope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            scope: String,
            depth: u32,
            profile: String,
            #[serde(deserialize_with = "read_members")]
            members: BTreeSet<RegionId>,
            #[serde(deserialize_with = "read_cells")]
            cells: Vec<Location>,
            #[serde(deserialize_with = "read_protected")]
            protected: BTreeSet<Location>,
            #[serde(deserialize_with = "read_rooms")]
            rooms: Vec<PreparedRoomGroup>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            scope: fields.scope,
            depth: fields.depth,
            profile: fields.profile,
            members: fields.members,
            cells: fields.cells,
            protected: fields.protected,
            rooms: fields.rooms,
        })
    }
}
impl<'de> Deserialize<'de> for FloorActivationRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            scope: String,
            entrant: ActorId,
            tick: u64,
            ordinal: u64,
            cause: EntryCause,
            profile: String,
            profile_version: u32,
            #[serde(deserialize_with = "read_decisions")]
            decisions: Vec<StageDecision>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            scope: fields.scope,
            entrant: fields.entrant,
            tick: fields.tick,
            ordinal: fields.ordinal,
            cause: fields.cause,
            profile: fields.profile,
            profile_version: fields.profile_version,
            decisions: fields.decisions,
        })
    }
}
impl<'de> Deserialize<'de> for StageDecision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: String,
            version: u32,
            result: StageResult,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            version: fields.version,
            result: fields.result,
        })
    }
}
impl<'de> Deserialize<'de> for ActivationState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            wandering: Option<wandering_population::WanderingPopulation>,
            plan: Shared<PopulationPlan>,
            #[serde(deserialize_with = "read_scopes")]
            scopes: Shared<BTreeMap<String, Shared<PreparedScope>>>,
            #[serde(deserialize_with = "read_member_index")]
            members: Shared<BTreeMap<RegionId, String>>,
            #[serde(deserialize_with = "read_floors")]
            floors: Shared<BTreeMap<String, Shared<FloorActivationRecord>>>,
            #[serde(deserialize_with = "read_order")]
            order: Shared<Vec<String>>,
            decision_count: usize,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            wandering: fields.wandering,
            plan: fields.plan,
            scopes: fields.scopes,
            members: fields.members,
            floors: fields.floors,
            order: fields.order,
            decision_count: fields.decision_count,
        })
    }
}

fn bounded_map<'de, K, V, D, const N: usize>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    struct Map<K, V, const N: usize>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>, const N: usize>
        serde::de::Visitor<'de> for Map<K, V, N>
    {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} unique map entries")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if map.size_hint().is_some_and(|size| size > N) {
                return Err(A::Error::custom("collection exceeds limit"));
            }
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                if result.len() == N || result.insert(key, value).is_some() {
                    return Err(A::Error::custom("excessive or duplicate map entry"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Map::<K, V, N>(std::marker::PhantomData))
}
fn read_stages<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<PopulationStage>, D::Error> {
    Ok(crate::bounded::Bounded::<PopulationStage, MAX_STAGES>::deserialize(d)?.0)
}
fn read_choices<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    Ok(crate::bounded::Bounded::<T, MAX_CHOICES>::deserialize(d)?.0)
}
fn read_bands<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<DepthBand<T>>, D::Error> {
    Ok(crate::bounded::Bounded::<DepthBand<T>, MAX_DEPTH_BANDS>::deserialize(d)?.0)
}
fn read_decisions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<StageDecision>, D::Error> {
    Ok(crate::bounded::Bounded::<StageDecision, MAX_STAGES>::deserialize(d)?.0)
}
fn read_attempts<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PlacementDecision>, D::Error> {
    Ok(crate::bounded::Bounded::<PlacementDecision, { MAX_ATTEMPTS as usize }>::deserialize(d)?.0)
}

fn read_profiles<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, PopulationProfile>, D::Error> {
    bounded_map::<_, _, _, MAX_PROFILES>(d)
}
fn read_scopes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<String, Shared<PreparedScope>>>, D::Error> {
    bounded_map::<_, _, _, MAX_SCOPES>(d).map(Shared::new)
}
fn read_member_index<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<RegionId, String>>, D::Error> {
    bounded_map::<_, _, _, { MAX_SCOPES * MAX_SCOPE_MEMBERS }>(d).map(Shared::new)
}
fn read_floors<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<String, Shared<FloorActivationRecord>>>, D::Error> {
    bounded_map::<_, _, _, MAX_SCOPES>(d).map(Shared::new)
}
fn read_order<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Shared<Vec<String>>, D::Error> {
    Ok(Shared::new(
        crate::bounded::Bounded::<String, MAX_SCOPES>::deserialize(d)?.0,
    ))
}
fn read_members<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeSet<RegionId>, D::Error> {
    let values = crate::bounded::Bounded::<RegionId, MAX_SCOPE_MEMBERS>::deserialize(d)?.0;
    let count = values.len();
    let set: BTreeSet<_> = values.into_iter().collect();
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate scope member"));
    }
    Ok(set)
}
fn read_cells<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Location>, D::Error> {
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Cell(#[serde(deserialize_with = "crate::bounded::location")] Location);
    Ok(
        crate::bounded::Bounded::<Cell, MAX_SCOPE_CELLS>::deserialize(d)?
            .0
            .into_iter()
            .map(|c| c.0)
            .collect(),
    )
}
fn read_room_cells<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Location>, D::Error> {
    #[derive(Deserialize)]
    struct Cell(#[serde(deserialize_with = "crate::bounded::location")] Location);
    Ok(
        crate::bounded::Bounded::<Cell, MAX_ROOM_CELLS>::deserialize(d)?
            .0
            .into_iter()
            .map(|cell| cell.0)
            .collect(),
    )
}
fn read_rooms<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<PreparedRoomGroup>, D::Error> {
    Ok(crate::bounded::Bounded::<PreparedRoomGroup, MAX_ROOM_GROUPS>::deserialize(d)?.0)
}
fn read_room_decisions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<RoomDecision>, D::Error> {
    Ok(crate::bounded::Bounded::<RoomDecision, MAX_ROOM_GROUPS>::deserialize(d)?.0)
}

fn read_protected<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeSet<Location>, D::Error> {
    let values = read_cells(d)?;
    let count = values.len();
    let set: BTreeSet<_> = values.into_iter().collect();
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate protected cell"));
    }
    Ok(set)
}

fn invalid() -> GameError {
    GameError::Rule(crate::abilities::RuleError::InvalidDefinition)
}
fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}
fn valid_bands<T>(
    bands: &[DepthBand<T>],
    header: fn(&T) -> (&str, u32),
    mut valid: impl FnMut(&T) -> bool,
) -> bool {
    !bands.is_empty()
        && bands.len() <= MAX_DEPTH_BANDS
        && bands.windows(2).all(|pair| {
            pair[0]
                .depth
                .maximum()
                .is_some_and(|end| end < pair[1].depth.minimum())
        })
        && bands.iter().all(|band| {
            let mut names = BTreeSet::new();
            let mut total = 0u32;
            band.depth.valid()
                && !band.choices.is_empty()
                && band.choices.len() <= MAX_CHOICES
                && band.choices.iter().all(|choice| {
                    let (content, weight) = header(choice);
                    label(content)
                        && names.insert(content)
                        && weight > 0
                        && total.checked_add(weight).is_some_and(|sum| {
                            total = sum;
                            true
                        })
                        && valid(choice)
                })
        })
}
fn eligible_band<T>(bands: &[DepthBand<T>], depth: u32) -> Option<&DepthBand<T>> {
    bands.iter().find(|band| band.depth.contains(depth))
}
impl PopulationContent {
    /// Actor recipe vocabulary, shared by wizard inspection and source validation.
    /// Explicit wizard birth depths already denote birth depth; no room offset applies.
    pub fn actor_bands(&self) -> Option<&[DepthBand<PreparedActorChoice>]> {
        match self {
            Self::Actors { bands, .. }
            | Self::RoomCluster {
                actor_bands: bands, ..
            } => Some(bands),
            _ => None,
        }
    }
    fn eligible(&self, depth: u32) -> bool {
        match self {
            Self::Items { bands } | Self::RoomItems { bands, .. } => {
                eligible_band(bands, depth).is_some()
            }
            Self::Actors { bands, .. } => eligible_band(bands, depth).is_some(),
            Self::RoomCluster {
                item_bands,
                actor_bands,
                actor_depth_offset,
                ..
            } => {
                eligible_band(item_bands, depth).is_some()
                    && depth
                        .checked_add(*actor_depth_offset)
                        .is_some_and(|depth| eligible_band(actor_bands, depth).is_some())
            }
        }
    }
}
impl PreparedActorChoice {
    /// Selection weights do not change a selected creature's construction.
    pub fn same_spawn_recipe(&self, other: &Self) -> bool {
        self.content == other.content
            && self.turn_ticks == other.turn_ticks
            && self.depth_growth == other.depth_growth
            && self.advancement == other.advancement
            && self.carried_loot == other.carried_loot
            && self.actor == other.actor
    }
}

/// One validation invocation, bound to its immutable content. Labels only pick a
/// candidate; exact recipe equality authorizes reuse. Limit retained successes to
/// one band's capacity, independently of the total authored choice count.
struct PopulationRecipeValidation<'a> {
    game: &'a Game,
    successful: BTreeMap<String, PreparedActorChoice>,
    #[cfg(test)]
    actor_checks: usize,
    #[cfg(test)]
    birth_checks: usize,
}
impl<'a> PopulationRecipeValidation<'a> {
    fn new(game: &'a Game) -> Self {
        Self {
            game,
            successful: BTreeMap::new(),
            #[cfg(test)]
            actor_checks: 0,
            #[cfg(test)]
            birth_checks: 0,
        }
    }
    fn valid(&mut self, choice: &PreparedActorChoice) -> bool {
        if self
            .successful
            .get(&choice.content)
            .is_some_and(|previous| previous.same_spawn_recipe(choice))
        {
            return true;
        }
        #[cfg(test)]
        {
            self.actor_checks += 1;
        }
        if !choice.actor.valid(self.game) {
            return false;
        }
        #[cfg(test)]
        {
            self.birth_checks += 1;
        }
        if choice
            .actor
            .validate_birth_advancement(
                self.game.content.clone(),
                choice.depth_growth,
                &choice.advancement,
            )
            .is_err()
        {
            return false;
        }
        if self.successful.len() < MAX_CHOICES || self.successful.contains_key(&choice.content) {
            self.successful
                .insert(choice.content.clone(), choice.clone());
        }
        true
    }
}

impl PopulationPlan {
    /// Resolve one authored recipe, never an arbitrary first depth band.
    pub fn named_actor(
        &self,
        profile_id: &str,
        stage_id: &str,
        content: &str,
        depth: Option<u32>,
    ) -> Result<(&PopulationProfile, &PopulationStage, &PreparedActorChoice), GameError> {
        if depth == Some(0) {
            return Err(invalid());
        }
        let profile = self.profiles.get(profile_id).ok_or_else(invalid)?;
        let stage = profile
            .stages
            .iter()
            .find(|stage| stage.id == stage_id)
            .ok_or_else(invalid)?;
        let bands = stage.population.actor_bands().ok_or_else(invalid)?;
        let mut choices = bands
            .iter()
            .filter(|band| depth.is_none_or(|depth| band.depth.contains(depth)))
            .flat_map(|band| &band.choices)
            .filter(|choice| choice.content == content);
        let choice = choices.next().ok_or_else(invalid)?;
        if choice.actor.build.is_none() || choices.any(|other| !choice.same_spawn_recipe(other)) {
            return Err(invalid());
        }
        Ok((profile, stage, choice))
    }
    fn valid(&self, game: &Game) -> bool {
        self.valid_with_recipes(&mut PopulationRecipeValidation::new(game))
    }
    fn valid_with_recipes(&self, recipes: &mut PopulationRecipeValidation<'_>) -> bool {
        let game = recipes.game;
        self.protagonist.0 != 0
            && self.carried_item_pools.len() <= MAX_CARRIED_ITEM_POOLS
            && self
                .carried_item_pools
                .iter()
                .all(|(id, pool)| label(id) && pool.valid(game))
            && !self.profiles.is_empty()
            && self.profiles.len() <= MAX_PROFILES
            && self.profiles.iter().all(|(id, profile)| {
                let mut names = BTreeSet::new();
                label(id)
                    && profile.version == 1
                    && profile.stages.len() <= MAX_STAGES
                    && profile
                        .stages
                        .iter()
                        .map(PopulationStage::maximum_attempts)
                        .sum::<usize>()
                        <= usize::from(MAX_ATTEMPTS)
                    && profile.stages.iter().all(|stage| {
                        label(&stage.id)
                            && names.insert(&stage.id)
                            && stage.version == 1
                            && stage.count[0] <= stage.count[1]
                            && stage.count[1] <= MAX_ATTEMPTS
                            && match &stage.population {
                                PopulationContent::RoomCluster {
                                    chance,
                                    item_count,
                                    actor_count,
                                    actor_depth_offset,
                                    item_bands,
                                    actor_bands,
                                    ..
                                } => {
                                    stage.count == [1, 1]
                                        && chance.valid()
                                        && *actor_depth_offset <= 1
                                        && [item_count, actor_count].iter().all(|count| {
                                            count[0] > 0
                                                && count[0] <= count[1]
                                                && count[1] <= MAX_ATTEMPTS
                                        })
                                        && stage.maximum_attempts() <= usize::from(MAX_ATTEMPTS)
                                        && valid_bands(
                                            item_bands,
                                            |c| (&c.content, c.weight),
                                            |c| c.valid(game),
                                        )
                                        && valid_bands(
                                            actor_bands,
                                            |c| (&c.content, c.weight),
                                            |c| {
                                                c.turn_ticks > 0
                                                    && c.carried_loot.as_ref().is_none_or(
                                                        |policy| {
                                                            policy.valid(&self.carried_item_pools)
                                                        },
                                                    )
                                                    && recipes.valid(c)
                                            },
                                        )
                                }
                                PopulationContent::Items { bands } => valid_bands(
                                    bands,
                                    |c| (&c.content, c.weight),
                                    |c| c.valid(game),
                                ),
                                PopulationContent::RoomItems { bands, chance } => {
                                    stage.count == [1, 1]
                                        && chance.valid()
                                        && valid_bands(
                                            bands,
                                            |c| (&c.content, c.weight),
                                            |c| c.valid(game),
                                        )
                                }
                                PopulationContent::Actors { bands, .. } => valid_bands(
                                    bands,
                                    |c| (&c.content, c.weight),
                                    |c| {
                                        c.turn_ticks > 0
                                            && c.carried_loot.as_ref().is_none_or(|policy| {
                                                policy.valid(&self.carried_item_pools)
                                            })
                                            && recipes.valid(c)
                                    },
                                ),
                            }
                    })
            })
    }
}
impl PopulationStage {
    fn maximum_attempts(&self) -> usize {
        match &self.population {
            PopulationContent::RoomCluster {
                item_count,
                actor_count,
                ..
            } => usize::from(item_count[1]) + usize::from(actor_count[1]),
            _ => usize::from(self.count[1]),
        }
    }
}
impl PopulationProfile {
    fn fits_scope(&self, rooms: usize) -> bool {
        self.stages
            .iter()
            .try_fold(0usize, |count, stage| {
                let attempts = if matches!(stage.population, PopulationContent::RoomItems { .. }) {
                    rooms
                } else {
                    stage.maximum_attempts()
                };
                count.checked_add(attempts)
            })
            .is_some_and(|count| count <= usize::from(MAX_ATTEMPTS))
    }
}
impl PreparedScope {
    fn valid(&self) -> bool {
        let mut room_cells = BTreeSet::new();
        label(&self.scope)
            && label(&self.profile)
            && self.depth > 0
            && self
                .scope
                .rsplit_once('/')
                .and_then(|(_, depth)| depth.parse::<u32>().ok())
                == Some(self.depth)
            && !self.members.is_empty()
            && self.members.len() <= MAX_SCOPE_MEMBERS
            && self.members.iter().all(|id| id.0 != 0)
            && !self.cells.is_empty()
            && self.cells.len() <= MAX_SCOPE_CELLS
            && self.cells.windows(2).all(|w| w[0] < w[1])
            && self
                .cells
                .iter()
                .all(|at| self.members.contains(&at.region))
            && self.rooms.len() <= MAX_ROOM_GROUPS
            && self.rooms.windows(2).all(|pair| pair[0].id < pair[1].id)
            && self.rooms.iter().all(|room| {
                room.id > 0
                    && room.id <= MAX_ROOM_GROUPS as u32
                    && !room.cells.is_empty()
                    && room.cells.len() <= MAX_ROOM_CELLS
                    && room.cells.windows(2).all(|pair| pair[0] < pair[1])
                    && room.cells.iter().all(|cell| {
                        self.cells.binary_search(cell).is_ok() && room_cells.insert(*cell)
                    })
            })
            && room_cells.len() <= MAX_SCOPE_CELLS
            && self.protected.len() <= MAX_SCOPE_CELLS
            && self
                .protected
                .iter()
                .all(|at| self.members.contains(&at.region))
    }
}

// Versioned, length-delimited domain separation from immutable Game.seed. This
// FNV-1a key mixer feeds the existing splitmix word stream, never rules_rng. No
// package hash, entity ID, speculative load tick or worker order is an input.
fn stream(
    seed: u64,
    scope_id: &str,
    profile_id: &str,
    profile: &PopulationProfile,
    stage: &PopulationStage,
    ordinal: u64,
    purpose: &str,
) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    add(b"tor-population-v1");
    add(&seed.to_le_bytes());
    add(scope_id.as_bytes());
    add(profile_id.as_bytes());
    add(&profile.version.to_le_bytes());
    add(stage.id.as_bytes());
    add(&stage.version.to_le_bytes());
    add(&ordinal.to_le_bytes());
    add(purpose.as_bytes());
    hash
}
pub(crate) fn index(state: &mut u64, n: usize) -> usize {
    let n = n as u64;
    let threshold = n.wrapping_neg() % n;
    loop {
        let word = crate::dice::random_word(state);
        if word >= threshold {
            return (word % n) as usize;
        }
    }
}

fn bucket(word: u64, n: usize) -> usize {
    ((u128::from(word) * n as u128) >> 64) as usize
}
fn weighted_choice<'a, T>(state: &mut u64, choices: &'a [T], weight: fn(&T) -> u32) -> &'a T {
    // Definitions are validated before configuration or restore. Positive
    // cumulative weights fit u32, and choices are bounded by MAX_CHOICES.
    let total: u32 = choices.iter().map(weight).sum();
    let mut ticket = bucket(crate::dice::random_word(state), total as usize) as u32;
    for choice in choices {
        let amount = weight(choice);
        if ticket < amount {
            return choice;
        }
        ticket -= amount;
    }
    unreachable!("validated nonempty weighted population")
}

/// A private host execution may require no-time-advance residency settlement.
/// The value is not a completed external action until the host drains the
/// boundary and recomputes the final decision. Standalone Game APIs never yield.
/// The host starts one retrieval transaction before any preload/continuation;
/// these entry points join that transaction without discarding earlier entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateStep<T> {
    Settled(T),
    AwaitingRegions(T),
}

impl Game {
    fn candidate_step<T>(&self, value: T) -> CandidateStep<T> {
        if self.region_transition_pending() {
            CandidateStep::AwaitingRegions(value)
        } else {
            CandidateStep::Settled(value)
        }
    }
    pub fn act_for_host(
        &mut self,
        id: ActorId,
        action: crate::Action,
    ) -> Result<CandidateStep<crate::ActionOutcome>, GameError> {
        if self.region_transition_pending() || self.host_region_continuation {
            return Err(invalid());
        }
        self.light_settled_witness = self.light_optical_witness();
        self.host_region_continuation = true;
        let result = self.act(id, action);
        self.host_region_continuation = false;
        self.light_settled_witness = None;
        result.map(|value| self.candidate_step(value))
    }
    pub fn execute_next_intention_for_host(
        &mut self,
    ) -> Option<CandidateStep<crate::IntentionExecution>> {
        if self.region_transition_pending() || self.host_region_continuation {
            return None;
        }
        self.light_settled_witness = self.light_optical_witness();
        self.host_region_continuation = true;
        let result = self.execute_next_intention();
        self.host_region_continuation = false;
        self.light_settled_witness = None;
        result.map(|value| self.candidate_step(value))
    }
    pub fn cancel_intention_with_step_for_host(
        &mut self,
        actor: ActorId,
        id: crate::IntentionId,
        step: Option<tor_world::Direction>,
    ) -> Result<CandidateStep<()>, GameError> {
        if self.region_transition_pending() || self.host_region_continuation {
            return Err(invalid());
        }
        self.light_settled_witness = self.light_optical_witness();
        self.host_region_continuation = true;
        let result = self.cancel_intention_with_step(actor, id, step);
        self.host_region_continuation = false;
        self.light_settled_witness = None;
        result.map(|value| self.candidate_step(value))
    }
    pub fn teleport_for_host(
        &mut self,
        actor: ActorId,
        at: Location,
    ) -> Result<CandidateStep<()>, GameError> {
        if self.region_transition_pending() || self.host_region_continuation {
            return Err(invalid());
        }
        self.light_settled_witness = self.light_optical_witness();
        self.host_region_continuation = true;
        let result = self.teleport(actor, at);
        self.host_region_continuation = false;
        self.light_settled_witness = None;
        result.map(|value| self.candidate_step(value))
    }
    pub(crate) fn activation_item_births_at(&self, tick: u64) -> BTreeSet<ItemId> {
        let mut items = BTreeSet::new();
        if let Some(state) = &self.floor_activation {
            for key in state.order.iter().rev() {
                let record = &state.floors[key];
                if record.tick != tick {
                    break;
                }
                for decision in &record.decisions {
                    for attempt in decision.result.attempts() {
                        if let PlacementDecision::Placed { entity, .. } = attempt {
                            items.extend(entity.item_birth());
                        }
                    }
                }
            }
        }
        items
    }
}

impl Game {
    pub fn configure_scenario_policy(
        &mut self,
        capabilities: ScenarioCapabilities,
        selected_character: Option<ActorId>,
    ) -> Result<(), GameError> {
        if self.tick != 0
            || !self.navigation.is_empty()
            || selected_character.is_some_and(|id| id.0 == 0)
            || self.scenario_policy != ScenarioPolicy::default()
        {
            return Err(invalid());
        }
        self.scenario_policy = ScenarioPolicy {
            capabilities,
            selected_character,
        };
        Ok(())
    }
    pub(crate) fn can_use_abstract_stairs(&self, id: ActorId) -> bool {
        !self.actors[&id].locomotion().levitating
            && (self.scenario_policy.capabilities.npc_stair_traversal
                || self.scenario_policy.selected_character == Some(id))
    }
    pub fn configure_population(&mut self, plan: PopulationPlan) -> Result<(), GameError> {
        if self.tick != 0
            || self.floor_activation.is_some()
            || !plan.valid(self)
            || self.scenario_policy.selected_character != Some(plan.protagonist)
        {
            return Err(invalid());
        }
        self.floor_activation = Some(ActivationState {
            wandering: None,
            plan: Shared::new(plan),
            scopes: Shared::default(),
            members: Shared::default(),
            floors: Shared::default(),
            order: Shared::default(),
            decision_count: 0,
        });
        Ok(())
    }
    /// Only immutable bounded input is registered. Repeating identical input is
    /// harmless; disagreement is rejected instead of depending on worker order.
    pub fn register_activation_scope(&mut self, scope: PreparedScope) -> Result<(), GameError> {
        let Some(state) = &mut self.floor_activation else {
            return Err(invalid());
        };
        if !scope.valid()
            || state
                .plan
                .profiles
                .get(&scope.profile)
                .is_none_or(|profile| !profile.fits_scope(scope.rooms.len()))
        {
            return Err(invalid());
        }
        if let Some(previous) = state.scopes.get(&scope.scope) {
            return if **previous == scope {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        if state.scopes.len() >= MAX_SCOPES {
            return Err(GameError::IdentityExhausted);
        }
        if scope
            .members
            .iter()
            .any(|member| state.members.contains_key(member))
        {
            return Err(invalid());
        }
        for member in &scope.members {
            state.members.insert(*member, scope.scope.clone());
        }
        state.scopes.insert(scope.scope.clone(), Shared::new(scope));
        Ok(())
    }
    /// Immutable compiled recipes for host-only authoring tools, never observations.
    pub fn population_spawn_plan(&self) -> Option<&PopulationPlan> {
        self.floor_activation.as_ref().map(|state| &*state.plan)
    }

    /// Wizard births share normal construction and finite stock, with their own
    /// deterministic scope. Publication owns the complete transaction.
    pub fn spawn_named_population_actor(
        &mut self,
        profile_id: &str,
        stage_id: &str,
        content: &str,
        depth: Option<u32>,
        at: Location,
    ) -> Result<(ActorId, Option<CarriedLootDecision>), GameError> {
        let mut candidate = self.clone();
        candidate.begin_effect_publication()?;
        let plan = candidate
            .floor_activation
            .as_ref()
            .ok_or_else(invalid)?
            .plan
            .clone();
        let (profile, stage, choice) = plan.named_actor(profile_id, stage_id, content, depth)?;
        let ordinal = candidate.next_actor_id;
        let seed = stream(
            candidate.seed,
            "wizard-creature",
            profile_id,
            profile,
            stage,
            ordinal,
            "build",
        );
        let build = choice
            .actor
            .instantiate_build(
                candidate.content.clone(),
                seed,
                if depth.is_some() {
                    choice.depth_growth
                } else {
                    DepthGrowth::Fixed {}
                },
                depth.unwrap_or(1),
                &choice.advancement,
            )
            .map_err(GameError::from)?;
        let stock = BirthStockContext {
            seed: candidate.seed,
            scope: "wizard-creature",
            profile_id,
            profile,
            stage,
            ordinal,
            depth,
        }
        .select(choice, &plan.carried_item_pools)?;
        let (actor, item) = candidate.spawn_prepared_actor_in_candidate(
            &choice.actor,
            at,
            NonZeroU64::new(choice.turn_ticks).ok_or_else(invalid)?,
            build,
            stock.map(|stock| (stock.quantity, stock.spec)),
        )?;
        let stock = choice.carried_loot.as_ref().map(|_| match stock {
            Some(stock) => CarriedLootDecision::Carried {
                content: stock.content.to_owned(),
                quantity: stock.quantity,
                item: item.expect("selected birth stock installed"),
            },
            None => CarriedLootDecision::NotCarried,
        });
        candidate.finish_effect_publication()?;
        *self = candidate;
        Ok((actor, stock))
    }

    /// Historical wizard births use pinned recipes and allocation high-water
    /// marks, without requiring the actor or its consumed stock to remain alive.
    pub fn validate_named_population_birth(
        &self,
        profile_id: &str,
        stage_id: &str,
        content: &str,
        depth: Option<u32>,
        actor: ActorId,
        item: Option<ItemId>,
    ) -> Result<(), GameError> {
        if actor.0 == 0
            || actor.0 >= self.next_actor_id
            || item.is_some_and(|item| item.0 == 0 || item.0 >= self.next_item_id)
        {
            return Err(invalid());
        }
        let plan = self.population_spawn_plan().ok_or_else(invalid)?;
        let (profile, stage, choice) = plan.named_actor(profile_id, stage_id, content, depth)?;
        let seed = stream(
            self.seed,
            "wizard-creature",
            profile_id,
            profile,
            stage,
            actor.0,
            "build",
        );
        choice
            .actor
            .instantiate_build(
                self.content.clone(),
                seed,
                if depth.is_some() {
                    choice.depth_growth
                } else {
                    DepthGrowth::Fixed {}
                },
                depth.unwrap_or(1),
                &choice.advancement,
            )
            .map_err(GameError::from)?;
        let stock = BirthStockContext {
            seed: self.seed,
            scope: "wizard-creature",
            profile_id,
            profile,
            stage,
            ordinal: actor.0,
            depth,
        }
        .select(choice, &plan.carried_item_pools)?;
        if stock.is_some() != item.is_some() {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn population_configuration_matches(
        &self,
        plan: Option<&PopulationPlan>,
        capabilities: &ScenarioCapabilities,
        selected_character: Option<ActorId>,
    ) -> bool {
        &self.scenario_policy.capabilities == capabilities
            && self.scenario_policy.selected_character == selected_character
            && self.floor_activation.as_ref().map(|state| &*state.plan) == plan
    }
    pub(crate) fn activation_scope_at(&self, region: RegionId) -> Option<&PreparedScope> {
        let state = self.floor_activation.as_ref()?;
        state
            .members
            .get(&region)
            .and_then(|scope| state.scopes.get(scope))
            .map(|scope| &**scope)
    }
    pub(crate) fn retrieval_population_compatible(
        &self,
        policy: &crate::unique_retrieval::UniqueRetrievalPolicy,
    ) -> bool {
        let Some(state) = &self.floor_activation else {
            return false;
        };
        self.scenario_policy.selected_character == Some(policy.protagonist)
            && state.plan.protagonist == policy.protagonist
            && state.plan.carried_item_pools.values().all(|pool| {
                pool.choices
                    .iter()
                    .all(|choice| !policy.reserves(&choice.spec))
            })
            && state.plan.profiles.values().all(|profile| {
                profile.stages.iter().all(|stage| match &stage.population {
                    PopulationContent::Items { bands }
                    | PopulationContent::RoomItems { bands, .. }
                    | PopulationContent::RoomCluster {
                        item_bands: bands, ..
                    } => bands.iter().all(|band| {
                        band.choices
                            .iter()
                            .all(|choice| !policy.reserves(&choice.spec))
                    }),
                    PopulationContent::Actors { .. } => true,
                })
            })
    }
    pub(crate) fn activation_scope(&self, id: &str) -> Option<&PreparedScope> {
        self.floor_activation
            .as_ref()?
            .scopes
            .get(id)
            .map(|scope| &**scope)
    }
    pub fn activation_scopes(&self) -> impl Iterator<Item = &PreparedScope> {
        self.floor_activation
            .iter()
            .flat_map(|state| state.scopes.values().map(|scope| &**scope))
    }
    pub fn activation_count(&self) -> usize {
        self.floor_activation.as_ref().map_or(0, |s| s.order.len())
    }
    pub fn activations_since(
        &self,
        previous_count: usize,
    ) -> Result<Vec<FloorActivationRecord>, GameError> {
        let Some(state) = &self.floor_activation else {
            return if previous_count == 0 {
                Ok(vec![])
            } else {
                Err(invalid())
            };
        };
        let suffix = state.order.get(previous_count..).ok_or_else(invalid)?;
        Ok(suffix
            .iter()
            .map(|key| (*state.floors[key]).clone())
            .collect())
    }
    pub fn activate_initial_floor(&mut self) -> Result<(), GameError> {
        let Some(state) = &self.floor_activation else {
            return Ok(());
        };
        let id = state.plan.protagonist;
        let to = self
            .actors
            .get(&id)
            .ok_or(GameError::UnknownActor)?
            .location;
        // Initial entry has the same whole-entry publication boundary as
        // movement and teleport. A later stage failure discards every prefix,
        // including ordinary identities, stock, scheduling and receipts.
        if self
            .unique_retrieval
            .as_ref()
            .is_some_and(|state| state.initialized)
        {
            // The population initial hook remains idempotent too.
            return Ok(());
        }
        let mut candidate = self.clone();
        candidate.begin_effect_publication()?;
        candidate.begin_retrieval_transaction();
        candidate.activate_entry(id, to, EntryCause::Initial)?;
        candidate.retrieval_initial_entry(id, to)?;
        candidate.finish_effect_publication()?;
        *self = candidate;
        Ok(())
    }
    pub(crate) fn population_participates_in_entry(&self, id: ActorId, to: Location) -> bool {
        self.floor_activation.as_ref().is_some_and(|state| {
            id == state.plan.protagonist && state.members.contains_key(&to.region)
        })
    }
    pub(crate) fn accepted_region_entry(
        &mut self,
        id: ActorId,
        from: Location,
        to: Location,
        cause: EntryCause,
    ) -> Result<(), GameError> {
        if from.region != to.region {
            let participates = self.population_participates_in_entry(id, to);
            let same_scope = self
                .activation_scope_at(from.region)
                .zip(self.activation_scope_at(to.region))
                .is_some_and(|(from, to)| from.scope == to.scope);
            self.activate_entry(id, to, cause)?;
            if !same_scope {
                self.retrieval_entry(id, to, cause)?;
            }
            // Revisits settle at the same exact entry boundary as first visits.
            // Ordinary games without population retain existing continuation.
            if participates && self.host_region_continuation {
                self.population_entry_boundary = Some(self.tick);
            }
        }
        Ok(())
    }
    pub fn population_entry_pending(&self) -> bool {
        self.population_entry_boundary.is_some()
    }
    pub fn region_transition_pending(&self) -> bool {
        self.population_entry_pending()
            || self.preparation_region_boundary.is_some()
            || self.trap_region_boundary.is_some()
            || self.light_region_boundary.is_some()
    }
    /// The host calls this only after successful no-time-advance horizon
    /// settlement. A subsequent entry may request another boundary; it is still
    /// the same unpublished candidate and the same ordinary scheduler.
    pub fn complete_region_transition(
        &mut self,
        report: &crate::TransitionReport,
    ) -> Result<(), GameError> {
        if self
            .preparation_region_boundary
            .as_ref()
            .is_some_and(|boundary| !self.preparation_missing_regions(boundary).is_empty())
        {
            return Err(invalid());
        }
        if self.light_region_boundary.as_ref().is_some_and(|boundary| {
            boundary.tick != self.tick || !self.light_missing_regions(boundary).is_empty()
        }) {
            return Err(invalid());
        }
        let light = self.light_region_boundary.take();
        let preparation = self.preparation_region_boundary.take();
        let pending = self.population_entry_boundary.take();
        if pending.is_some_and(|tick| tick != self.tick) {
            return Err(invalid());
        }
        if pending.is_some()
            || light.is_some()
            || preparation.is_some()
            || self.trap_region_boundary.is_some()
            || !report.frozen.is_empty()
        {
            self.light_settled_witness = self.light_optical_witness();
            self.host_region_continuation = true;
            let result = self.resume_trap_region_boundary().and_then(|_| {
                if self.trap_region_boundary.is_some() {
                    return Ok(());
                }
                self.resolve_preparations()
                    .and_then(|()| self.advance_to_next_decision())
            });
            self.host_region_continuation = false;
            self.light_settled_witness = None;
            result?;
        }
        Ok(())
    }
    /// Extend the normal load closure only. Entries into any reachable scope
    /// then have every bounded member ready before simulation can continue.
    pub(crate) fn activation_loaded_closure(&self, loaded: &mut BTreeSet<RegionId>) {
        let Some(state) = &self.floor_activation else {
            return;
        };
        let scopes: BTreeSet<_> = loaded
            .iter()
            .filter_map(|id| state.members.get(id))
            .filter(|scope| {
                !state.floors.contains_key(*scope)
                    || self
                        .unique_retrieval_policy()
                        .is_some_and(|policy| policy.selects_scope(&state.scopes[*scope]))
            })
            .cloned()
            .collect();
        for scope in scopes {
            loaded.extend(state.scopes[&scope].members.iter().copied());
        }
    }
    fn activate_entry(
        &mut self,
        id: ActorId,
        to: Location,
        cause: EntryCause,
    ) -> Result<(), GameError> {
        let Some(state) = &self.floor_activation else {
            return Ok(());
        };
        if id != state.plan.protagonist {
            return Ok(());
        }
        let Some(scope_id) = state.members.get(&to.region) else {
            return Ok(());
        };
        if state.floors.contains_key(scope_id) {
            return Ok(());
        }
        let scope = state.scopes[scope_id].clone();
        let plan = state.plan.clone();
        let profile = plan.profiles[&scope.profile].clone();
        if scope
            .members
            .iter()
            .any(|id| !self.world.loaded_regions().any(|loaded| loaded == *id))
        {
            return Err(GameError::InvalidLocation);
        }
        let budget: usize = profile
            .stages
            .iter()
            .try_fold(0usize, |total, stage| {
                total.checked_add(stage.maximum_decisions(scope.rooms.len())?)
            })
            .ok_or(GameError::IdentityExhausted)?;
        if state
            .decision_count
            .checked_add(budget)
            .is_none_or(|n| n > MAX_RETAINED_DECISIONS)
        {
            return Err(GameError::IdentityExhausted);
        }
        let ordinal = state.order.len() as u64;
        let mut decisions = Vec::new();
        let mut newborn = Vec::new();
        for stage in &profile.stages {
            let result = if !stage.population.eligible(scope.depth) {
                StageResult::Skipped {
                    reason: SkipReason::NoEligibleContent,
                }
            } else if matches!(stage.population, PopulationContent::RoomCluster { .. }) {
                self.evaluate_room_cluster(id, &scope, &profile, stage, &mut newborn)?
            } else if matches!(stage.population, PopulationContent::RoomItems { .. }) {
                self.evaluate_room_items(id, &scope, &profile, stage)?
            } else {
                let mut count_rng = stream(
                    self.seed,
                    &scope.scope,
                    &scope.profile,
                    &profile,
                    stage,
                    0,
                    "count",
                );
                let requested = stage.count[0]
                    + index(
                        &mut count_rng,
                        usize::from(stage.count[1] - stage.count[0]) + 1,
                    ) as u16;
                let attempts = self.evaluate_population_attempts(
                    id,
                    &scope,
                    &profile,
                    stage,
                    requested,
                    &mut newborn,
                    false,
                )?;
                StageResult::Evaluated {
                    requested,
                    attempts,
                }
            };
            decisions.push(StageDecision {
                id: stage.id.clone(),
                version: stage.version,
                result,
            });
        }
        let count = decisions
            .iter()
            .map(StageDecision::decision_count)
            .try_fold(0usize, usize::checked_add)
            .ok_or(GameError::IdentityExhausted)?;
        let record = FloorActivationRecord {
            scope: scope.scope.clone(),
            entrant: id,
            tick: self.tick,
            ordinal,
            cause,
            profile: scope.profile.clone(),
            profile_version: profile.version,
            decisions,
        };
        let state = self
            .floor_activation
            .as_mut()
            .expect("configured population");
        state.decision_count += count;
        state.order.push(scope.scope.clone());
        state
            .floors
            .insert(scope.scope.clone(), Shared::new(record));
        self.reconcile_wandering_activity()?;
        for actor in newborn {
            let scene = self.scene(actor)?;
            self.refresh_navigation_scene(actor, &scene);
        }
        Ok(())
    }
}

impl Game {
    // Shared ordinary stage context; the room restriction is the sole placement variation.
    #[allow(clippy::too_many_arguments)]
    fn evaluate_population_attempts(
        &mut self,
        id: ActorId,
        scope: &PreparedScope,
        profile: &PopulationProfile,
        stage: &PopulationStage,
        requested: u16,
        newborn: &mut Vec<ActorId>,
        restrict_room: bool,
    ) -> Result<Vec<PlacementDecision>, GameError> {
        let plan = self
            .floor_activation
            .as_ref()
            .ok_or_else(invalid)?
            .plan
            .clone();
        let mut attempts = Vec::new();
        for attempt in 0..requested {
            let mut choice_rng = stream(
                self.seed,
                &scope.scope,
                &scope.profile,
                profile,
                stage,
                u64::from(attempt),
                "content",
            );
            let mut placement_rng = stream(
                self.seed,
                &scope.scope,
                &scope.profile,
                profile,
                stage,
                u64::from(attempt),
                "placement",
            );
            let mut cells = scope.cells.clone();
            for i in (1..cells.len()).rev() {
                let j = index(&mut placement_rng, i + 1);
                cells.swap(i, j);
            }
            let occupied = |game: &Game, at: Location| {
                scope.protected.contains(&at)
                    || !game.world.walkable(at)
                    || game
                        .actors
                        .at(&game.world, at)
                        .keys()
                        .any(|other| *other == id || game.actors[other].alive())
            };
            let decision = match &stage.population {
                PopulationContent::RoomItems { .. } | PopulationContent::RoomCluster { .. } => {
                    unreachable!("room stage evaluated separately")
                }
                PopulationContent::Items { bands } => {
                    let choices = &eligible_band(bands, scope.depth).unwrap().choices;
                    let choice = weighted_choice(&mut choice_rng, choices, |c| c.weight);
                    let at = cells.into_iter().find(|at| {
                        !occupied(self, *at)
                            && self.items.at(ItemLocation::Ground(*at)).next().is_none()
                    });
                    if let Some(location) = at {
                        let mut quantity_rng = stream(
                            self.seed,
                            &scope.scope,
                            &scope.profile,
                            profile,
                            stage,
                            u64::from(attempt),
                            "quantity",
                        );
                        let quantity = choice
                            .quantity
                            .resolve(Some(scope.depth))?
                            .sample(&mut quantity_rng)?;
                        let item =
                            self.spawn_item_stack(location, None, quantity, choice.spec.clone())?;
                        PlacementDecision::Placed {
                            content: choice.content.clone(),
                            location,
                            entity: PopulationEntity::Item { id: item, quantity },
                        }
                    } else {
                        PlacementDecision::Skipped {
                            content: choice.content.clone(),
                            reason: SkipReason::NoEligibleCell,
                        }
                    }
                }
                PopulationContent::Actors {
                    bands,
                    avoid_protagonist_sight,
                } => {
                    let choices = &eligible_band(bands, scope.depth).unwrap().choices;
                    let choice = weighted_choice(&mut choice_rng, choices, |c| c.weight);
                    let body = choice.actor.body.clone().unwrap_or_default();
                    let seen: BTreeSet<_> = if *avoid_protagonist_sight {
                        self.scene(id)?
                            .into_iter()
                            .map(|cell| cell.location)
                            .collect()
                    } else {
                        BTreeSet::new()
                    };
                    let at = cells.into_iter().find(|at| {
                        !occupied(self, *at)
                            && !seen.contains(at)
                            && (!restrict_room
                                || room_cluster::local_body_inside(
                                    &scope.cells,
                                    &scope.protected,
                                    *at,
                                    &body,
                                ))
                            && self.body_fits(ActorId(0), *at, 0, &body)
                            && self.body_cells(*at, 0, &body).is_some_and(|cells| {
                                cells.iter().all(|(at, _)| {
                                    scope.members.contains(&at.region)
                                        && (!restrict_room
                                            || room_cluster::room_column_inside(
                                                &scope.cells,
                                                &scope.protected,
                                                *at,
                                            ))
                                        && !occupied(self, *at)
                                        && !seen.contains(at)
                                })
                            })
                    });
                    if let Some(location) = at {
                        let seed = stream(
                            self.seed,
                            &scope.scope,
                            &scope.profile,
                            profile,
                            stage,
                            u64::from(attempt),
                            "build",
                        );
                        let build = choice
                            .actor
                            .instantiate_build(
                                self.content.clone(),
                                seed,
                                choice.depth_growth,
                                scope.depth,
                                &choice.advancement,
                            )
                            .map_err(GameError::from)?;
                        let stock = BirthStockContext {
                            seed: self.seed,
                            scope: &scope.scope,
                            profile_id: &scope.profile,
                            profile,
                            stage,
                            ordinal: u64::from(attempt),
                            depth: Some(scope.depth),
                        }
                        .select(choice, &plan.carried_item_pools)?;
                        let (actor, item) = self.spawn_prepared_actor_in_candidate(
                            &choice.actor,
                            location,
                            NonZeroU64::new(choice.turn_ticks).ok_or_else(invalid)?,
                            build,
                            stock.map(|stock| (stock.quantity, stock.spec)),
                        )?;
                        let carried_loot = choice.carried_loot.as_ref().map(|_| match stock {
                            Some(stock) => CarriedLootDecision::Carried {
                                content: stock.content.to_owned(),
                                quantity: stock.quantity,
                                item: item.expect("selected birth stack was installed"),
                            },
                            None => CarriedLootDecision::NotCarried,
                        });
                        newborn.push(actor);
                        PlacementDecision::Placed {
                            content: choice.content.clone(),
                            location,
                            entity: PopulationEntity::Actor {
                                id: actor,
                                carried_loot,
                            },
                        }
                    } else {
                        PlacementDecision::Skipped {
                            content: choice.content.clone(),
                            reason: SkipReason::NoEligibleCell,
                        }
                    }
                }
            };
            attempts.push(decision);
        }
        Ok(attempts)
    }

    fn evaluate_room_items(
        &mut self,
        entrant: ActorId,
        scope: &PreparedScope,
        profile: &PopulationProfile,
        stage: &PopulationStage,
    ) -> Result<StageResult, GameError> {
        let PopulationContent::RoomItems { bands, chance } = &stage.population else {
            return Err(invalid());
        };
        let choices = &eligible_band(bands, scope.depth)
            .ok_or_else(invalid)?
            .choices;
        let mut outcomes = Vec::with_capacity(scope.rooms.len());
        for room in &scope.rooms {
            let root = |purpose: &str| {
                stream(
                    self.seed,
                    &scope.scope,
                    &scope.profile,
                    profile,
                    stage,
                    u64::from(room.id),
                    &format!("room/{purpose}"),
                )
            };
            let mut chance_rng = root("chance");
            let result = if !chance.succeeds(&mut chance_rng) {
                RoomResult::ChanceMiss
            } else {
                let mut content_rng = root("content");
                let choice = weighted_choice(&mut content_rng, choices, |choice| choice.weight);
                let mut quantity_rng = root("quantity");
                let quantity = choice
                    .quantity
                    .resolve(Some(scope.depth))?
                    .sample(&mut quantity_rng)?;
                let mut placement_rng = root("placement");
                let mut cells = room.cells.clone();
                for i in (1..cells.len()).rev() {
                    let j = index(&mut placement_rng, i + 1);
                    cells.swap(i, j);
                }
                let location = cells.into_iter().find(|at| {
                    !scope.protected.contains(at)
                        && self.world.walkable(*at)
                        && !self
                            .actors
                            .at(&self.world, *at)
                            .keys()
                            .any(|actor| *actor == entrant || self.actors[actor].alive())
                        && self.items.at(ItemLocation::Ground(*at)).next().is_none()
                });
                let decision = if let Some(location) = location {
                    let id =
                        self.spawn_item_stack(location, None, quantity, choice.spec.clone())?;
                    PlacementDecision::Placed {
                        content: choice.content.clone(),
                        location,
                        entity: PopulationEntity::Item { id, quantity },
                    }
                } else {
                    PlacementDecision::Skipped {
                        content: choice.content.clone(),
                        reason: SkipReason::NoEligibleCell,
                    }
                };
                RoomResult::Attempt { decision }
            };
            outcomes.push(RoomDecision {
                room: room.id,
                result,
            });
        }
        Ok(StageResult::RoomsEvaluated { rooms: outcomes })
    }

    fn validate_room_decisions(
        &self,
        record: &FloorActivationRecord,
        profile: &PopulationProfile,
        stage: &PopulationStage,
        depth: u32,
        rooms: &[RoomDecision],
    ) -> Result<(), GameError> {
        let PopulationContent::RoomItems { bands, chance } = &stage.population else {
            return Err(invalid());
        };
        let choices = &eligible_band(bands, depth).ok_or_else(invalid)?.choices;
        if rooms.len() > MAX_ROOM_GROUPS
            || !rooms.windows(2).all(|pair| pair[0].room < pair[1].room)
            || rooms
                .iter()
                .any(|room| room.room == 0 || room.room > MAX_ROOM_GROUPS as u32)
        {
            return Err(invalid());
        }
        let scope = self
            .floor_activation
            .as_ref()
            .and_then(|state| state.scopes.get(&record.scope));
        if scope.is_some_and(|scope| {
            scope
                .rooms
                .iter()
                .map(|room| room.id)
                .ne(rooms.iter().map(|room| room.room))
        }) {
            return Err(invalid());
        }
        for room in rooms {
            let root = |purpose: &str| {
                stream(
                    self.seed,
                    &record.scope,
                    &record.profile,
                    profile,
                    stage,
                    u64::from(room.room),
                    &format!("room/{purpose}"),
                )
            };
            let mut chance_rng = root("chance");
            let succeeds = chance.succeeds(&mut chance_rng);
            let decision = match (&room.result, succeeds) {
                (RoomResult::ChanceMiss, false) => continue,
                (RoomResult::Attempt { decision }, true) => decision,
                _ => return Err(invalid()),
            };
            let mut content_rng = root("content");
            let choice = weighted_choice(&mut content_rng, choices, |choice| choice.weight);
            let mut quantity_rng = root("quantity");
            let expected_quantity = choice
                .quantity
                .resolve(Some(depth))?
                .sample(&mut quantity_rng)?;
            match decision {
                PlacementDecision::Skipped {
                    content,
                    reason: SkipReason::NoEligibleCell,
                } if *content == choice.content => {}
                PlacementDecision::Placed {
                    content,
                    location,
                    entity: PopulationEntity::Item { id, quantity },
                } if *content == choice.content
                    && id.0 > 0
                    && id.0 < u64::MAX
                    && *quantity == expected_quantity
                    && location.region.0 > 0 =>
                {
                    if scope.is_some_and(|scope| {
                        scope
                            .rooms
                            .iter()
                            .find(|group| group.id == room.room)
                            .is_none_or(|group| group.cells.binary_search(location).is_err())
                            || scope.protected.contains(location)
                    }) {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }

    /// Validate historical origin evidence against the pinned definitions, not
    /// present ownership. Abandoned branches may name scopes not in this Game
    /// and may have later ticks/IDs than the restored boundary being audited.
    pub fn validate_activation_record(
        &self,
        record: &FloorActivationRecord,
    ) -> Result<(), GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        let profile = state
            .plan
            .profiles
            .get(&record.profile)
            .ok_or_else(invalid)?;
        let depth: u32 = record
            .scope
            .rsplit_once('/')
            .and_then(|(_, depth)| depth.parse().ok())
            .filter(|depth| *depth > 0)
            .ok_or_else(invalid)?;
        if !label(&record.scope)
            || record.entrant != state.plan.protagonist
            || record.profile_version != profile.version
            || record.ordinal >= MAX_SCOPES as u64
            || record.decisions.len() != profile.stages.len()
        {
            return Err(invalid());
        }
        for (decision, stage) in record.decisions.iter().zip(&profile.stages) {
            if decision.id != stage.id || decision.version != stage.version {
                return Err(invalid());
            }
            match &decision.result {
                StageResult::Skipped {
                    reason: SkipReason::NoEligibleContent,
                } if !stage.population.eligible(depth) => {}
                StageResult::RoomCluster { selection } if stage.population.eligible(depth) => {
                    self.validate_room_cluster(record, profile, stage, depth, selection)?;
                }
                StageResult::RoomsEvaluated { rooms } if stage.population.eligible(depth) => {
                    self.validate_room_decisions(record, profile, stage, depth, rooms)?;
                }
                StageResult::Evaluated {
                    requested,
                    attempts,
                } if !matches!(
                    stage.population,
                    PopulationContent::RoomItems { .. } | PopulationContent::RoomCluster { .. }
                ) && stage.population.eligible(depth)
                    && (stage.count[0]..=stage.count[1]).contains(requested)
                    && attempts.len() == usize::from(*requested) =>
                {
                    self.validate_population_attempts(
                        record, profile, stage, depth, *requested, attempts,
                    )?;
                }
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }
    fn validate_population_attempts(
        &self,
        record: &FloorActivationRecord,
        profile: &PopulationProfile,
        stage: &PopulationStage,
        depth: u32,
        requested: u16,
        attempts: &[PlacementDecision],
    ) -> Result<(), GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        let mut count_rng = stream(
            self.seed,
            &record.scope,
            &record.profile,
            profile,
            stage,
            0,
            "count",
        );
        let expected = stage.count[0]
            + index(
                &mut count_rng,
                usize::from(stage.count[1] - stage.count[0]) + 1,
            ) as u16;
        if requested != expected {
            return Err(invalid());
        }
        for (ordinal, attempt) in attempts.iter().enumerate() {
            let content = match attempt {
                PlacementDecision::Placed {
                    content,
                    location,
                    entity,
                } => {
                    if location.region.0 == 0
                        || match (entity, &stage.population) {
                            (
                                PopulationEntity::Actor { id, .. },
                                PopulationContent::Actors { .. },
                            ) => id.0 == 0 || id.0 == u64::MAX,
                            (
                                PopulationEntity::Item { id, .. },
                                PopulationContent::Items { .. },
                            ) => id.0 == 0 || id.0 == u64::MAX,
                            _ => true,
                        }
                    {
                        return Err(invalid());
                    }
                    content
                }
                PlacementDecision::Skipped {
                    content,
                    reason: SkipReason::NoEligibleCell,
                } => content,
                _ => return Err(invalid()),
            };
            let mut choice_rng = stream(
                self.seed,
                &record.scope,
                &record.profile,
                profile,
                stage,
                ordinal as u64,
                "content",
            );
            let expected = match &stage.population {
                PopulationContent::Actors { bands, .. } => {
                    let choices = &eligible_band(bands, depth).unwrap().choices;
                    let choice = weighted_choice(&mut choice_rng, choices, |c| c.weight);
                    if let PlacementDecision::Placed {
                        entity: PopulationEntity::Actor { carried_loot, .. },
                        ..
                    } = attempt
                    {
                        let stock = BirthStockContext {
                            seed: self.seed,
                            scope: &record.scope,
                            profile_id: &record.profile,
                            profile,
                            stage,
                            ordinal: ordinal as u64,
                            depth: Some(depth),
                        }
                        .select(choice, &state.plan.carried_item_pools)?;
                        let valid = match (&choice.carried_loot, stock, carried_loot) {
                            (None, None, None) => true,
                            (Some(_), None, Some(CarriedLootDecision::NotCarried)) => true,
                            (
                                Some(_),
                                Some(expected),
                                Some(CarriedLootDecision::Carried {
                                    content,
                                    quantity,
                                    item,
                                }),
                            ) => {
                                *content == expected.content
                                    && *quantity == expected.quantity
                                    && item.0 > 0
                                    && item.0 < u64::MAX
                            }
                            _ => false,
                        };
                        if !valid {
                            return Err(invalid());
                        }
                    }
                    if choice.depth_growth != (DepthGrowth::Fixed {})
                        && matches!(attempt, PlacementDecision::Placed { .. })
                    {
                        // A historical success must describe a possible exact birth,
                        // even after the actor died, drained or left retained state.
                        let seed = stream(
                            self.seed,
                            &record.scope,
                            &record.profile,
                            profile,
                            stage,
                            ordinal as u64,
                            "build",
                        );
                        choice
                            .actor
                            .instantiate_build(
                                self.content.clone(),
                                seed,
                                choice.depth_growth,
                                depth,
                                &choice.advancement,
                            )
                            .map_err(|_| invalid())?;
                    }
                    &choice.content
                }
                PopulationContent::Items { bands } => {
                    let choices = &eligible_band(bands, depth).unwrap().choices;
                    let choice = weighted_choice(&mut choice_rng, choices, |c| c.weight);
                    if let PlacementDecision::Placed {
                        entity: PopulationEntity::Item { quantity, .. },
                        ..
                    } = attempt
                    {
                        let mut quantity_rng = stream(
                            self.seed,
                            &record.scope,
                            &record.profile,
                            profile,
                            stage,
                            ordinal as u64,
                            "quantity",
                        );
                        if *quantity
                            != choice
                                .quantity
                                .resolve(Some(depth))?
                                .sample(&mut quantity_rng)?
                        {
                            return Err(invalid());
                        }
                    }
                    &choice.content
                }
                PopulationContent::RoomItems { .. } | PopulationContent::RoomCluster { .. } => {
                    return Err(invalid())
                }
            };
            if expected != content {
                return Err(invalid());
            }
        }
        Ok(())
    }

    /// Check high-water identities in an audit context that retains all branch
    /// allocation ceilings. Historical origin does not require live ownership.
    pub fn validate_activation_record_identity_bounds(
        &self,
        record: &FloorActivationRecord,
    ) -> Result<(), GameError> {
        let mut actors = BTreeSet::new();
        let mut items = BTreeSet::new();
        for decision in &record.decisions {
            for attempt in decision.result.attempts() {
                if let PlacementDecision::Placed { entity, .. } = attempt {
                    let valid = entity.actor_birth().is_none_or(|id| {
                        id.0 > 0 && id.0 < self.next_actor_id && actors.insert(id)
                    }) && entity
                        .item_birth()
                        .is_none_or(|id| id.0 > 0 && id.0 < self.next_item_id && items.insert(id));
                    if !valid {
                        return Err(invalid());
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn activation_state_valid(&self) -> bool {
        self.floor_activation
            .as_ref()
            .is_none_or(|state| state.plan.valid(self))
            && self.activation_boundary_valid()
    }

    /// Validate every boundary-dependent condition. The caller must already
    /// have validated this immutable plan against this game's Content.
    pub(crate) fn activation_boundary_valid(&self) -> bool {
        if self
            .scenario_policy
            .selected_character
            .is_some_and(|id| id.0 == 0)
        {
            return false;
        }
        let Some(state) = &self.floor_activation else {
            return true;
        };
        if self.scenario_policy.selected_character != Some(state.plan.protagonist)
            || state.scopes.len() > MAX_SCOPES
            || state.floors.len() != state.order.len()
            || state.floors.len() > MAX_SCOPES
            || state.decision_count > MAX_RETAINED_DECISIONS
            || !self.wandering_state_valid()
        {
            return false;
        }
        let mut members = BTreeMap::new();
        for (key, scope) in state.scopes.iter() {
            if key != &scope.scope
                || !scope.valid()
                || state
                    .plan
                    .profiles
                    .get(&scope.profile)
                    .is_none_or(|profile| !profile.fits_scope(scope.rooms.len()))
            {
                return false;
            }
            for member in &scope.members {
                if members.insert(*member, key.clone()).is_some() {
                    return false;
                }
            }
        }
        if members != *state.members {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut actor_origins = BTreeSet::new();
        let mut item_origins = BTreeSet::new();
        let mut decisions = 0usize;
        for (ordinal, scope_id) in state.order.iter().enumerate() {
            if !seen.insert(scope_id) {
                return false;
            }
            let Some(record) = state.floors.get(scope_id) else {
                return false;
            };
            let Some(scope) = state.scopes.get(scope_id) else {
                return false;
            };
            if record.scope != *scope_id
                || record.profile != scope.profile
                || record.tick > self.tick
                || record.ordinal != ordinal as u64
                || self.validate_activation_record(record).is_err()
                || self
                    .validate_activation_record_identity_bounds(record)
                    .is_err()
            {
                return false;
            }
            for decision in &record.decisions {
                decisions += decision.decision_count();
                for attempt in decision.result.attempts() {
                    if let PlacementDecision::Placed {
                        location, entity, ..
                    } = attempt
                    {
                        let unique = entity
                            .actor_birth()
                            .is_none_or(|id| actor_origins.insert(id))
                            && entity.item_birth().is_none_or(|id| item_origins.insert(id));
                        if !unique {
                            return false;
                        }
                        if !scope.members.contains(&location.region)
                            || scope.cells.binary_search(location).is_err()
                            || scope.protected.contains(location)
                        {
                            return false;
                        }
                    }
                }
            }
        }
        decisions == state.decision_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, BodySpec, Direction, MemoryRecords, RegionTransition};
    use tor_world::Position;

    include!("floor_activation_transaction_tests.rs");
    include!("floor_activation_checkpoint_tests.rs");
    include!("population_depth_tests.rs");
    include!("population_recipe_validation_tests.rs");
    include!("spawn_carried_loot_tests.rs");
    include!("gold_population_tests.rs");
    include!("room_cluster_tests.rs");

    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }
    fn marker() -> ItemSpec {
        let mut spec = ItemSpec::ordinary("marker".into());
        spec.stackable = true;
        spec
    }
    fn game() -> Game {
        let content =
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap();
        let mut game = Game::region_corridor(42, 2, content);
        game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
            .unwrap();
        game
    }
    fn item_stage(minimum_depth: u32) -> PopulationStage {
        PopulationStage {
            id: "stock".into(),
            version: 1,
            count: [1, 1],
            population: PopulationContent::Items {
                bands: vec![DepthBand {
                    depth: DepthInterval::Open {
                        minimum: minimum_depth,
                    },
                    choices: vec![PreparedItemChoice {
                        content: "marker".into(),
                        weight: 1,
                        quantity: QuantityExpr::Fixed { quantity: 1 },
                        spec: marker(),
                    }],
                }],
            },
        }
    }
    fn scope(depth: u32, cells: Vec<Location>) -> PreparedScope {
        PreparedScope {
            scope: format!("dungeon/{depth}"),
            depth,
            profile: "baseline".into(),
            members: BTreeSet::from([RegionId(u64::from(depth))]),
            cells,
            protected: BTreeSet::new(),
            rooms: vec![],
        }
    }
    fn plan(stages: Vec<PopulationStage>) -> PopulationPlan {
        PopulationPlan {
            protagonist: ActorId(1),
            carried_item_pools: BTreeMap::new(),
            profiles: BTreeMap::from([(
                "baseline".into(),
                PopulationProfile { version: 1, stages },
            )]),
        }
    }

    #[test]
    fn prepared_scopes_allocate_nothing_and_initial_receipt_is_once_only() {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut later = item_stage(3);
        later.id = "later".into();
        game.configure_population(plan(vec![item_stage(1), later]))
            .unwrap();
        let before = (
            game.next_actor_id,
            game.next_item_id,
            game.rules_rng,
            game.tick,
        );
        let prepared = scope(1, vec![at(1, 2, 1)]);
        game.register_activation_scope(prepared.clone()).unwrap();
        game.register_activation_scope(prepared).unwrap();
        assert_eq!(
            (
                game.next_actor_id,
                game.next_item_id,
                game.rules_rng,
                game.tick
            ),
            before
        );
        assert_eq!(game.activation_count(), 0);
        game.activate_initial_floor().unwrap();
        let records = game.activations_since(0).unwrap();
        assert_eq!(records.len(), 1);
        assert!(matches!(
            records[0].decisions[0].result,
            StageResult::Evaluated { requested: 1, .. }
        ));
        assert_eq!(
            records[0].decisions[1].result,
            StageResult::Skipped {
                reason: SkipReason::NoEligibleContent
            }
        );
        let after = game.clone();
        game.activate_initial_floor().unwrap();
        assert_eq!(game, after);
        assert_eq!(game.rules_rng, before.2);
    }

    #[test]
    fn merged_origin_id_remains_valid_historical_evidence() {
        let mut game = game();
        let owner = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_population(plan(vec![item_stage(1)]))
            .unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let record = game.activations_since(0).unwrap().remove(0);
        let carried = game
            .spawn_item_stack(at(1, 1, 1), Some(owner), 1, marker())
            .unwrap();
        let origin = ItemId(1);
        // Ordinary pickup requires occupying the ground stack's exact cell.
        game.act(owner, Action::Move(Direction::East)).unwrap();
        assert_eq!(game.actors[&owner].location, at(1, 2, 1));
        game.act(
            owner,
            Action::Take {
                item: origin,
                quantity: None,
            },
        )
        .unwrap();
        assert!(
            !game.items.contains_key(&origin),
            "ordinary pickup must merge and retire the origin identity"
        );
        assert_eq!(game.items[&carried].quantity, 2);
        game.validate_activation_record(&record).unwrap();
        game.validate_activation_record_identity_bounds(&record)
            .unwrap();
        assert!(game.activation_state_valid());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
        assert_eq!(restored, game);
    }

    #[test]
    fn aggregate_attempt_limit_rejects_before_any_mutation() {
        let mut game = game();
        let before = game.clone();
        let mut a = item_stage(1);
        a.count = [MAX_ATTEMPTS, MAX_ATTEMPTS];
        let mut b = item_stage(1);
        b.id = "extra".into();
        assert!(game.configure_population(plan(vec![a, b])).is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn preparation_order_does_not_change_choices_and_clones_share_immutable_inputs() {
        let mut forward = game();
        forward
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        forward
            .configure_population(plan(vec![item_stage(1)]))
            .unwrap();
        let mut reverse = forward.clone();
        let first = scope(1, vec![at(1, 2, 1), at(1, 3, 1)]);
        let second = scope(2, vec![at(2, 2, 1), at(2, 3, 1)]);
        forward.register_activation_scope(first.clone()).unwrap();
        forward.register_activation_scope(second.clone()).unwrap();
        reverse.register_activation_scope(second).unwrap();
        reverse.register_activation_scope(first).unwrap();
        forward.activate_initial_floor().unwrap();
        reverse.activate_initial_floor().unwrap();
        forward.teleport(ActorId(1), at(2, 0, 1)).unwrap();
        reverse.teleport(ActorId(1), at(2, 0, 1)).unwrap();
        assert_eq!(forward.activations_since(0), reverse.activations_since(0));
        assert_eq!(forward, reverse);
        let cloned = forward.clone();
        let original = forward.floor_activation.as_ref().unwrap();
        let copy = cloned.floor_activation.as_ref().unwrap();
        assert!(original.plan.shares_storage(&copy.plan));
        assert!(original.scopes.shares_storage(&copy.scopes));
        assert!(original.floors.shares_storage(&copy.floors));
        assert!(original.order.shares_storage(&copy.order));
    }

    #[test]
    fn persisted_population_rejects_duplicate_keys_positional_records_and_missing_fields() {
        let encoded = serde_json::to_string(&plan(vec![item_stage(1)])).unwrap();
        let decoded: PopulationPlan = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, plan(vec![item_stage(1)]));
        let profile = serde_json::to_string(&decoded.profiles["baseline"]).unwrap();
        let duplicate = format!(
            r#"{{"protagonist":1,"profiles":{{"baseline":{profile},"baseline":{profile}}}}}"#
        );
        assert!(serde_json::from_str::<PopulationPlan>(&duplicate).is_err());
        assert!(serde_json::from_str::<PopulationPlan>(r#"[1,{}]"#).is_err());
        let mut scope_value = serde_json::to_value(scope(1, vec![at(1, 2, 1)])).unwrap();
        scope_value["cells"][0]["position"] = serde_json::json!([2, 1, 0]);
        assert!(serde_json::from_value::<PreparedScope>(scope_value).is_err());
        assert!(serde_json::from_str::<PreparedActor>(r#"{"known_identities":[]}"#).is_err());
    }

    #[test]
    fn receipt_identity_bounds_reject_forged_origins_without_requiring_live_ownership() {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_population(plan(vec![item_stage(1)]))
            .unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let mut record = game.activations_since(0).unwrap().remove(0);
        if let StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result {
            if let PlacementDecision::Placed { entity, .. } = &mut attempts[0] {
                *entity = PopulationEntity::Item {
                    id: ItemId(game.next_item_id),
                    quantity: 1,
                };
            } else {
                panic!("fixture must place an item");
            }
        } else {
            panic!("fixture must evaluate a stage");
        }
        assert!(game
            .validate_activation_record_identity_bounds(&record)
            .is_err());
    }

    #[test]
    fn receipt_audit_rejects_another_valid_but_unselected_content_choice() {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut stock = item_stage(1);
        if let PopulationContent::Items { bands } = &mut stock.population {
            let mut alternate = bands[0].choices[0].clone();
            alternate.content = "alternate".into();
            bands[0].choices.push(alternate);
        }
        game.configure_population(plan(vec![stock])).unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let mut record = game.activations_since(0).unwrap().remove(0);
        game.validate_activation_record(&record).unwrap();
        if let StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result {
            if let PlacementDecision::Placed { content, .. } = &mut attempts[0] {
                *content = if content == "marker" {
                    "alternate"
                } else {
                    "marker"
                }
                .into();
            } else {
                panic!("fixture must place an item");
            }
        } else {
            panic!("fixture must evaluate a stage");
        }
        assert!(game.validate_activation_record(&record).is_err());
    }

    fn falling_entry() -> (Game, ActorId, ActorId, ItemId, MemoryRecords) {
        falling_entry_from(game())
    }

    fn falling_entry_from(game: Game) -> (Game, ActorId, ActorId, ItemId, MemoryRecords) {
        falling_entry_from_with_wandering(game, false)
    }

    fn falling_entry_from_with_wandering(
        mut game: Game,
        wandering: bool,
    ) -> (Game, ActorId, ActorId, ItemId, MemoryRecords) {
        let protagonist = game
            .spawn_actor(at(1, 8, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let driver = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mover = game
            .spawn_actor(at(1, 2, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.actors.get_mut(&protagonist).unwrap().ready_at = 100;
        game.actors.get_mut(&mover).unwrap().ready_at = 100;
        game.set_actor_velocity(protagonist, [crate::physics::TERMINAL, 0, 0])
            .unwrap();
        game.set_actor_velocity(mover, [crate::physics::TERMINAL, 0, 0])
            .unwrap();
        let old_item = game
            .spawn_item_stack(at(1, 2, 0), None, 1, marker())
            .unwrap();
        game.items
            .edit(old_item, |item| {
                item.motion.velocity = [crate::physics::TERMINAL, 0, 0]
            })
            .unwrap();
        game.set_gravity(RegionId(2), [1, 0, 0]).unwrap();
        let actor = PreparedActor {
            build: None,
            nutrition: crate::nutrition::NutritionRequirement::Exempt,
            identity: None,
            anatomy: None,
            body: Some(BodySpec::default()),
            ai: None,
            known_identities: vec![],
            velocity: None,
            asset: None,
        };
        game.configure_population(plan(vec![
            item_stage(2),
            PopulationStage {
                id: "newborn".into(),
                version: 1,
                count: [1, 1],
                population: PopulationContent::Actors {
                    bands: vec![DepthBand {
                        depth: DepthInterval::Open { minimum: 2 },
                        choices: vec![PreparedActorChoice {
                            advancement: vec![],
                            carried_loot: None,
                            depth_growth: crate::prepared_spawn::DepthGrowth::Fixed {},
                            content: "ordinary actor".into(),
                            weight: 1,
                            turn_ticks: 100,
                            actor,
                        }],
                    }],
                    avoid_protagonist_sight: false,
                },
            },
        ]))
        .unwrap();
        if wandering {
            game.configure_wandering_population(WanderingPolicy {
                stage_id: "newborn".into(),
                excluded_wandering_content: BTreeSet::new(),
                initial_delay: [7000, 7000],
                retry_ticks: 400,
                chance: CarryChance {
                    numerator: 1,
                    denominator: 6,
                },
                living_cap: 8,
            })
            .unwrap();
        }
        game.register_activation_scope(scope(1, vec![at(1, 4, 1)]))
            .unwrap();
        game.register_activation_scope(scope(2, vec![at(2, 3, 1), at(2, 4, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let mut records = MemoryRecords::default();
        game.transition_regions_counted_deferred(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records,
        )
        .unwrap();
        assert!(
            game.region_frozen(RegionId(2)),
            "destination must really be frozen before motion"
        );
        (game, driver, mover, old_item, records)
    }

    #[test]
    fn first_physical_floor_entry_does_not_erase_existing_wandering_clock_fraction() {
        let (mut game, driver, _, _, _) = falling_entry_from_with_wandering(game(), true);
        assert_eq!(game.next_wandering_event_tick(), Some(7000));
        assert!(matches!(
            game.act_for_host(driver, Action::Wait).unwrap(),
            CandidateStep::AwaitingRegions(_)
        ));
        assert_eq!(
            game.tick, 32,
            "fixture reaches a physical entry before any wandering deadline"
        );
        assert_eq!(
            game.next_wandering_event_tick(),
            Some(7000),
            "entry-time reconciliation preserves the old floor's elapsed tick"
        );
    }

    #[test]
    fn host_physics_boundary_finishes_old_tick_without_backdating_newborns() {
        let (mut game, driver, mover, item, mut records) = falling_entry();
        let result = game.act_for_host(driver, Action::Wait).unwrap();
        assert!(matches!(result, CandidateStep::AwaitingRegions(_)));
        let entered = game.activations_since(1).unwrap();
        assert_eq!(entered.len(), 1);
        let tick = entered[0].tick;
        assert_eq!(tick, 32);
        assert_eq!(game.tick, tick);
        assert_eq!(
            game.actors[&mover].location,
            at(1, 6, 2),
            "higher-ID mover must finish tick T"
        );
        assert_eq!(
            game.items[&item].location,
            ItemLocation::Ground(at(1, 6, 0)),
            "old loose item must finish tick T"
        );
        let newborn = ActorId(4);
        assert_eq!(game.actors[&newborn].active_since, tick);
        assert_eq!(game.actors[&newborn].ready_at, tick);
        assert!(game.actor_frozen(newborn));
        let born_item = ItemId(2);
        assert_eq!(
            game.items[&born_item].motion.velocity, [0; 3],
            "new item gets no retroactive birth-tick gravity"
        );
        let (_, report, _) = game
            .transition_regions_counted_deferred(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(2)]),
                    loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                },
                &mut records,
            )
            .unwrap();
        game.complete_region_transition(&report).unwrap();
        assert!(!game.population_entry_pending());
        assert_eq!(game.tick, tick);
        assert_eq!(game.next_actor(), Some(newborn));
        assert_eq!(game.items[&born_item].motion.velocity, [0; 3]);
    }

    #[test]
    fn lethal_post_entry_slide_finishes_the_original_host_physics_tick() {
        for destination_active in [false, true] {
            let world = tor_world::World::new(
                (1..=2)
                    .map(|id| tor_world::Region {
                        id: RegionId(id),
                        name: format!("wide corridor {id}"),
                        bounds: tor_world::Extent::new(12, 9, 1).unwrap(),
                    })
                    .collect(),
                vec![],
            )
            .unwrap();
            let mut game = Game::new(world, 42, crate::test_creatures::content());
            game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
                .unwrap();
            for (from, direction, to) in [
                (at(1, 11, 0), Direction::East, at(2, 0, 0)),
                (at(2, 0, 0), Direction::West, at(1, 11, 0)),
            ] {
                game.connect_area(
                    tor_world::Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                    9,
                    1,
                )
                .unwrap();
            }
            let (mut game, driver, mover, item, mut records) = falling_entry_from(game);
            let protagonist = ActorId(1);
            crate::test_creatures::configure(
                &mut game,
                protagonist,
                "player",
                crate::test_creatures::species(),
            );
            crate::test_creatures::injure(&mut game, protagonist, 29).unwrap();
            game.configure_run(
                protagonist,
                BTreeSet::from([protagonist]),
                None,
                BTreeMap::new(),
            )
            .unwrap();
            game.set_actor_velocity(protagonist, [4096, 4096, 0])
                .unwrap();
            game.set_wall(at(2, 0, 5), true).unwrap();
            assert!(game.region_frozen(RegionId(2)));
            if destination_active {
                game.transition_regions_counted_deferred(
                    &RegionTransition {
                        active: BTreeSet::from([RegionId(1), RegionId(2)]),
                        loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                    },
                    &mut records,
                )
                .unwrap();
                assert!(!game.region_frozen(RegionId(2)));
            }
            assert_eq!(game.health(protagonist), Some((1, 30)));
            let step = game.act_for_host(driver, Action::Wait).unwrap();
            assert!(matches!(step, CandidateStep::AwaitingRegions(_)));
            let entry = game.activations_since(1).unwrap();
            assert_eq!(entry.len(), 1);
            assert_eq!((entry[0].tick, game.tick), (64, 64));
            assert_eq!(game.actors[&protagonist].location, at(2, 0, 4));
            assert!(!game.alive(protagonist));
            assert!(
                game.run_outcome().terminal,
                "death must exercise the terminal early-return path"
            );
            assert_eq!(
                game.actors[&mover].location,
                at(1, 10, 2),
                "first red: a terminal post-entry collision must not omit the higher-ID original mover at tick T"
            );
            assert_eq!(
                game.items[&item].location,
                ItemLocation::Ground(at(1, 10, 0)),
                "terminal entry must finish the original loose item's tick T"
            );
            let newborn = ActorId(4);
            assert_eq!(game.actors[&newborn].active_since, 64);
            assert_eq!(game.actors[&newborn].ready_at, 64);
            assert_eq!(
                game.items[&ItemId(2)].motion.velocity,
                [0; 3],
                "new population items receive no retroactive birth-tick gravity"
            );
            let corpse = game
                .items
                .values()
                .find(|item| item.spec.class == crate::ItemClass::Corpse)
                .unwrap();
            assert_eq!(
                corpse.motion.displacement, [0; 3],
                "the new corpse also receives no birth-tick movement"
            );
            assert_eq!(corpse.motion.velocity, [4096, 0, 0]);
            let (_, report, _) = game
                .transition_regions_counted_deferred(
                    &RegionTransition {
                        active: BTreeSet::from([RegionId(1), RegionId(2)]),
                        loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                    },
                    &mut records,
                )
                .unwrap();
            game.complete_region_transition(&report).unwrap();
            assert!(!game.population_entry_pending());
            assert_eq!(game.tick, 64);
            assert_eq!(game.activation_count(), 2);
        }
    }

    #[test]
    fn ordinary_standalone_action_still_returns_a_settled_decision() {
        let (mut game, driver, _, _, _) = falling_entry();
        let result = game.act(driver, Action::Wait).unwrap();
        assert!(!game.population_entry_pending());
        assert_eq!(game.tick, 100);
        assert_eq!(result.next_tick, game.tick);
        assert_eq!(result.next_actor, game.next_actor());
        assert_eq!(game.next_actor(), Some(driver));
    }

    #[test]
    fn npc_abstract_stairs_are_role_gated_without_blocking_physical_portals() {
        let mut game = Game::region_corridor(
            42,
            2,
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap(),
        );
        game.configure_scenario_policy(
            ScenarioCapabilities {
                npc_stair_traversal: false,
            },
            Some(ActorId(1)),
        )
        .unwrap();
        let player = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let npc = game
            .spawn_actor(at(1, 11, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.connect(
            tor_world::Passage {
                from: at(1, 11, 1),
                direction: Direction::Down,
                to: at(2, 5, 1),
            },
            0,
        )
        .unwrap();
        assert!(game.actor_translation(npc, Direction::Down).is_none());
        assert!(
            game.actor_translation(npc, Direction::East).is_some(),
            "ordinary physical cross-region movement remains legal"
        );
        game.refresh_navigation();
        assert!(!game.navigation[&npc]
            .edges
            .contains_key(&(at(1, 11, 1), Direction::Down)));
        // Moving the declared character to the same stair does not depend on AI
        // settings or any server controller assignment.
        game.teleport(npc, at(1, 8, 1)).unwrap();
        game.teleport(player, at(1, 11, 1)).unwrap();
        assert!(game.actor_translation(player, Direction::Down).is_some());
    }
}
