//! Stored scenario metadata is independent of authoring and runtime serializers.
//! Remote schemas borrow values during encoding and move decoded fields into the
//! runtime. Package sources, caches and diagnostic text never enter this schema.
use crate::engine::{invalid_archive, storage_failure};
use crate::scenario_package as source;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// Only explicitly registered values can cross this boundary. There is no
// Serialize/Deserialize blanket implementation that could reintroduce a runtime
// serializer through a nested field. Containers preserve that rule recursively.
trait Stored: Sized {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>;
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}

struct Borrowed<'a, T>(&'a T);
impl<T: Stored> Serialize for Borrowed<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.encode(serializer)
    }
}

struct Owned<T>(T);
impl<'de, T: Stored> Deserialize<'de> for Owned<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::decode(deserializer).map(Self)
    }
}
impl<T: Stored> Serialize for Owned<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.encode(serializer)
    }
}

mod mapped {
    use super::*;
    pub(super) fn serialize<T: Stored, S: serde::Serializer>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.encode(serializer)
    }
    pub(super) fn deserialize<'de, T: Stored, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<T, D::Error> {
        T::decode(deserializer)
    }
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}
fn bounded_plain<'de, D, T, const MAXIMUM: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Values<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Values<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "at most {N} stored values")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Vec<T>, A::Error> {
            use serde::de::Error;
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(A::Error::custom("stored collection exceeds limit"));
            }
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(N));
            while values.len() < N {
                let Some(value) = sequence.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("stored collection exceeds limit"));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Values::<T, MAXIMUM>(std::marker::PhantomData))
}
fn bounded_stored<'de, D, T, const MAXIMUM: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Stored,
{
    bounded_plain::<_, Owned<T>, MAXIMUM>(deserializer)
        .map(|values| values.into_iter().map(|value| value.0).collect())
}
fn bounded_stored_map<'de, D, T, const MAXIMUM: usize>(
    deserializer: D,
) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Stored,
{
    struct Values<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Stored, const N: usize> serde::de::Visitor<'de> for Values<T, N> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "at most {N} uniquely named stored entries")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut mapping: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if mapping.size_hint().is_some_and(|size| size > N) {
                return Err(A::Error::custom("stored mapping exceeds limit"));
            }
            let mut values = BTreeMap::new();
            while let Some(key) = mapping.next_key::<String>()? {
                if values.len() >= N {
                    return Err(A::Error::custom("stored mapping exceeds limit"));
                }
                if values.contains_key(&key) {
                    return Err(A::Error::custom("duplicate stored mapping key"));
                }
                values.insert(key, mapping.next_value::<Owned<T>>()?.0);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Values::<T, MAXIMUM>(std::marker::PhantomData))
}

impl<T: Stored> Stored for Option<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().map(Borrowed).serialize(serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<Owned<T>>::deserialize(deserializer).map(|value| value.map(|value| value.0))
    }
}
impl<T: Stored> Stored for Arc<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().encode(serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::decode(deserializer).map(Arc::new)
    }
}
impl<T: Stored> Stored for Vec<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(Borrowed))
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Values<T>(std::marker::PhantomData<T>);
        impl<'de, T: Stored> serde::de::Visitor<'de> for Values<T> {
            type Value = Vec<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of stored values")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(Owned(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(values)
            }
        }
        deserializer.deserialize_seq(Values::<T>(std::marker::PhantomData))
    }
}
impl<T: Stored + Ord> Stored for BTreeSet<T> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(Borrowed))
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Values<T>(std::marker::PhantomData<T>);
        impl<'de, T: Stored + Ord> serde::de::Visitor<'de> for Values<T> {
            type Value = BTreeSet<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of unique stored values")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = BTreeSet::new();
                while let Some(Owned(value)) = sequence.next_element()? {
                    if !values.insert(value) {
                        return Err(serde::de::Error::custom("duplicate stored set value"));
                    }
                }
                Ok(values)
            }
        }
        deserializer.deserialize_seq(Values::<T>(std::marker::PhantomData))
    }
}
impl<K: Stored + Ord, V: Stored> Stored for BTreeMap<K, V> {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            self.iter()
                .map(|(key, value)| (Borrowed(key), Borrowed(value))),
        )
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Values<K, V>(std::marker::PhantomData<(K, V)>);
        impl<'de, K: Stored + Ord, V: Stored> serde::de::Visitor<'de> for Values<K, V> {
            type Value = BTreeMap<K, V>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a mapping of unique stored keys to stored values")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut mapping: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((Owned(key), Owned(value))) = mapping.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate stored mapping key"));
                    }
                }
                Ok(values)
            }
        }
        deserializer.deserialize_map(Values::<K, V>(std::marker::PhantomData))
    }
}

// Scalar leaves have fixed JSON representations; domain values below use only
// their save-owned remote schema, never their own serde implementation.
macro_rules! scalar {
    ($($value:ty),+ $(,)?) => {$(
        impl Stored for $value {
            fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.serialize(serializer)
            }
            fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::deserialize(deserializer)
            }
        }
    )+};
}
scalar!(String, u16, u32, u64);

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::Scenario", deny_unknown_fields)]
struct Scenario {
    seed: u64,
    #[serde(with = "mapped")]
    actors: Vec<crate::ActorSetup>,
    regions: u64,
    workload_version: Option<u32>,
    #[serde(with = "mapped")]
    package: Option<Arc<source::Package>>,
    #[serde(with = "mapped")]
    streaming: Option<crate::Streaming>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::ActorSetup", deny_unknown_fields)]
struct ActorSetup {
    #[serde(with = "mapped")]
    position: crate::journal::Position,
    turn_ticks: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::journal::Position", deny_unknown_fields)]
struct Position {
    region: u64,
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::Streaming", deny_unknown_fields)]
struct Streaming {
    active_radius: u32,
    load_radius: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::spatial_authoring::AuthoredSpatialScope",
    deny_unknown_fields
)]
struct SpatialScopeSource {
    #[serde(with = "mapped")]
    members: Vec<u64>,
    #[serde(with = "mapped")]
    rooms: BTreeMap<u32, Vec<source::spatial_authoring::RoomVolume>>,
    #[serde(with = "mapped")]
    passages: Vec<source::spatial_authoring::SourceRun>,
    #[serde(with = "mapped")]
    doorways: Vec<source::spatial_authoring::OwnedRun>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::spatial_authoring::RoomVolume", deny_unknown_fields)]
struct SpatialRoomVolume {
    region: u64,
    min: [i32; 3],
    max: [i32; 3],
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::spatial_authoring::SourceRun", deny_unknown_fields)]
struct SpatialSourceRun {
    region: u64,
    z: i32,
    y: i32,
    first_x: i32,
    last_x: i32,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::spatial_authoring::OwnedRun", deny_unknown_fields)]
struct SpatialOwnedRun {
    room: u32,
    region: u64,
    z: i32,
    y: i32,
    first_x: i32,
    last_x: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Manifest", deny_unknown_fields)]
struct Manifest {
    #[serde(with = "mapped")]
    spatial_scopes: BTreeMap<String, source::spatial_authoring::AuthoredSpatialScope>,
    #[serde(with = "mapped")]
    unique_retrieval: Option<source::UniqueRetrieval>,
    #[serde(with = "mapped")]
    floor_activation: Option<source::FloorActivation>,
    #[serde(with = "mapped")]
    capabilities: Option<source::ScenarioCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arena: Option<source::Arena>,
    #[serde(
        default,
        skip_serializing_if = "crate::creature_authoring::Catalog::is_empty"
    )]
    #[serde(with = "mapped")]
    creatures: crate::creature_authoring::Catalog,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", with = "mapped")]
    generation_recipes: BTreeMap<String, crate::generation_recipe::Recipe>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", with = "mapped")]
    generation_groups: BTreeMap<String, crate::generation_recipe::GenerationGroup>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", with = "mapped")]
    procedural_worlds: BTreeMap<String, crate::procedural_world::WorldFamily>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", with = "mapped")]
    stair_pairs: BTreeMap<String, source::StairPair>,
    factions: BTreeMap<String, BTreeSet<String>>,
    #[serde(with = "mapped")]
    ai_profiles: BTreeMap<String, source::AiProfile>,
    format: u32,
    id: String,
    version: String,
    ruleset: String,
    default_character: u64,
    themes: Vec<String>,
    #[serde(with = "mapped")]
    zones: BTreeMap<String, source::Zone>,
    #[serde(with = "mapped")]
    archetypes: BTreeMap<String, source::Archetype>,
    #[serde(with = "mapped")]
    appearance_pools: BTreeMap<String, source::AppearancePool>,
    #[serde(with = "mapped")]
    characters: Vec<source::Character>,
    #[serde(with = "mapped")]
    objective: Option<source::Objective>,
    assets: BTreeMap<String, Vec<String>>,
    #[serde(with = "mapped")]
    terrain: Option<source::TerrainAssets>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::UniqueRetrieval", deny_unknown_fields)]
struct UniqueRetrieval {
    version: u32,
    #[serde(with = "mapped")]
    protagonist: source::Protagonist,
    family: String,
    minimum_depth: u32,
    archetype: String,
    exit_anchor: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ScenarioCapabilities", deny_unknown_fields)]
struct ScenarioCapabilities {
    npc_stair_traversal: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Protagonist", untagged)]
enum Protagonist {
    Actor(u64),
    Selected(String),
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::FloorActivation", deny_unknown_fields)]
struct FloorActivation {
    #[serde(deserialize_with = "required_option")]
    wandering: Option<tor_simulation::floor_activation::WanderingPolicy>,
    version: u32,
    #[serde(with = "mapped")]
    protagonist: source::Protagonist,
    families: BTreeMap<String, String>,
    #[serde(with = "mapped")]
    profiles: BTreeMap<String, source::PopulationProfile>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored_map::<_, source::CarriedItemPool, 32>"
    )]
    carried_item_pools: BTreeMap<String, source::CarriedItemPool>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::PopulationProfile", deny_unknown_fields)]
struct PopulationProfile {
    version: u32,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, source::PopulationStage, 32>"
    )]
    stages: Vec<source::PopulationStage>,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::PopulationStage",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum PopulationStage {
    Items {
        id: String,
        version: u32,
        count: [u16; 2],
        #[serde(
            serialize_with = "mapped::serialize",
            deserialize_with = "bounded_stored::<_, source::PopulationBand, 32>"
        )]
        bands: Vec<source::PopulationBand>,
        #[serde(with = "mapped")]
        quantity: source::QuantityExpression,
    },
    RoomItems {
        id: String,
        version: u32,
        #[serde(
            serialize_with = "mapped::serialize",
            deserialize_with = "bounded_stored::<_, source::PopulationBand, 32>"
        )]
        bands: Vec<source::PopulationBand>,
        #[serde(with = "mapped")]
        quantity: source::QuantityExpression,
        #[serde(with = "mapped")]
        chance: source::CarryChance,
    },
    Actors {
        id: String,
        version: u32,
        count: [u16; 2],
        #[serde(
            serialize_with = "mapped::serialize",
            deserialize_with = "bounded_stored::<_, source::ActorPopulationBand, 32>"
        )]
        bands: Vec<source::ActorPopulationBand>,
        #[serde(deserialize_with = "required_option")]
        ai: Option<String>,
        #[serde(with = "mapped")]
        depth_growth: source::PopulationDepthGrowth,
        avoid_protagonist_sight: bool,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::PopulationQuantityExpr",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum PopulationQuantityExpr {
    Fixed {
        quantity: u64,
    },
    Uniform {
        minimum: u64,
        maximum: u64,
    },
    DepthScaledUniform {
        minimum: u64,
        maximum_base: u64,
        per_depth: u64,
    },
}

// Numeric authoring sugar is canonicalized once. Saved inputs always require
// the current tagged representation and never accept an older scalar payload.
impl Stored for source::QuantityExpression {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Fixed(quantity) => source::PopulationQuantityExpr::Fixed {
                quantity: *quantity,
            }
            .encode(serializer),
            Self::Expression(expression) => expression.encode(serializer),
        }
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        source::PopulationQuantityExpr::decode(deserializer).map(|value| match value {
            source::PopulationQuantityExpr::Fixed { quantity } => Self::Fixed(quantity),
            expression => Self::Expression(expression),
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::PopulationDepthGrowth",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum PopulationDepthGrowth {
    Fixed {},
    RacialAfter { depth: u32 },
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::PopulationDepth",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum PopulationDepth {
    Finite { minimum: u32, maximum: u32 },
    Open { minimum: u32 },
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::PopulationBand", deny_unknown_fields)]
struct PopulationBand {
    #[serde(with = "mapped")]
    depth: source::PopulationDepth,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, source::PopulationChoice, 64>"
    )]
    choices: Vec<source::PopulationChoice>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::PopulationChoice", deny_unknown_fields)]
struct PopulationChoice {
    archetype: String,
    weight: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ActorPopulationBand", deny_unknown_fields)]
struct ActorPopulationBand {
    #[serde(with = "mapped")]
    depth: source::PopulationDepth,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, source::ActorPopulationChoice, 64>"
    )]
    choices: Vec<source::ActorPopulationChoice>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ActorPopulationChoice", deny_unknown_fields)]
struct ActorPopulationChoice {
    archetype: String,
    weight: u32,
    #[serde(with = "mapped")]
    carried_loot: Option<source::CarriedLoot>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, source::PopulationAdvancement, 256>"
    )]
    advancement: Vec<source::PopulationAdvancement>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::PopulationAdvancement", deny_unknown_fields)]
struct PopulationAdvancement {
    #[serde(deserialize_with = "stored_birth_ordinal")]
    ordinal: u64,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, crate::creature_authoring::Purchase, 256>"
    )]
    purchases: Vec<crate::creature_authoring::Purchase>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CarriedItemPool", deny_unknown_fields)]
struct CarriedItemPool {
    version: u32,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, source::CarriedItemChoice, 64>"
    )]
    choices: Vec<source::CarriedItemChoice>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CarriedItemChoice", deny_unknown_fields)]
struct CarriedItemChoice {
    archetype: String,
    weight: u32,
    #[serde(with = "mapped")]
    quantity: source::QuantityExpression,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CarriedLoot", deny_unknown_fields)]
struct CarriedLoot {
    version: u32,
    pool: String,
    #[serde(with = "mapped")]
    chance: source::CarryChance,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CarryChance", deny_unknown_fields)]
struct CarryChance {
    numerator: u32,
    denominator: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CharacterLoadout", deny_unknown_fields)]
struct CharacterLoadout {
    archetype: String,
    quantity: u64,
    #[serde(deserialize_with = "required_option")]
    equipped_slot: Option<u16>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::StairPair", deny_unknown_fields)]
struct StairPair {
    upper: String,
    lower: String,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::generation_recipe::GenerationGroup",
    deny_unknown_fields
)]
struct GenerationGroup {
    depth: u32,
    recipe: String,
    grid: [u32; 2],
    members: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generation_recipe::Recipe", deny_unknown_fields)]
struct Recipe {
    version: u32,
    #[serde(with = "mapped")]
    stages: Vec<crate::generation_recipe::Stage>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generation_recipe::Stage", deny_unknown_fields)]
struct Stage {
    id: String,
    version: u32,
    #[serde(with = "mapped")]
    operation: crate::generation_recipe::Operation,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::generation_recipe::Operation",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Operation {
    StoneFill,
    GridPartition,
    Rooms {
        width: [u32; 2],
        height: [u32; 2],
    },
    ConnectedGraph {
        extra: [u32; 2],
    },
    Corridors,
    Stairs,
    RoomOmissions {
        #[serde(with = "mapped")]
        config: crate::generation_recipe::RoomOmissions,
    },
    MazeRooms {
        #[serde(with = "mapped")]
        config: crate::generation_recipe::MazeRooms,
    },
    RoomLighting {
        darkness_roll: u32,
        darkness_start: u32,
    },
    SecretEntrances {
        #[serde(with = "mapped")]
        config: crate::generation_recipe::SecretEntrances,
    },
    HiddenTraps {
        #[serde(with = "mapped")]
        config: crate::generation_recipe::HiddenTraps,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::generation_recipe::RoomOmissions",
    deny_unknown_fields
)]
struct RoomOmissions {
    count: [u32; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generation_recipe::MazeRooms", deny_unknown_fields)]
struct MazeRooms {
    one_in: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::generation_recipe::SecretEntrances",
    deny_unknown_fields
)]
struct SecretEntrances {
    chance_per_mille: u32,
    maximum: u32,
    dc: i32,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generation_recipe::HiddenTraps", deny_unknown_fields)]
struct HiddenTraps {
    #[serde(deserialize_with = "required_option")]
    sleep_exposure: Option<String>,
    count: [u32; 2],
    dc: i32,
    rust_slot: String,
    rust_amount: u32,
    sleep_condition: String,
    sleep_duration: u64,
    #[serde(with = "mapped")]
    hazards: Option<crate::generation_recipe::HazardTraps>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generation_recipe::HazardTraps", deny_unknown_fields)]
struct HazardTraps {
    avoidance_dc: i32,
    bear_condition: String,
    bear_duration: u64,
    arrow_ammunition: String,
    attack_bonus: i32,
    poison_exposure: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Zone", deny_unknown_fields)]
struct Zone {
    themes: Option<Vec<String>>,
    #[serde(with = "mapped")]
    terrain: Option<source::TerrainAssets>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::TerrainAssets", deny_unknown_fields)]
struct TerrainAssets {
    floor: Option<String>,
    wall: Option<String>,
    door: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AppearancePool", deny_unknown_fields)]
struct AppearancePool {
    appearances: Vec<String>,
    confounding: bool,
    asset: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::scenario_package::ItemClass",
    rename_all = "snake_case"
)]
enum ItemClass {
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

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CurrencyKind", rename_all = "snake_case")]
enum CurrencyKind {
    Gold,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Archetype", deny_unknown_fields)]
struct Archetype {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "mapped")]
    creature: Option<crate::creature_authoring::BuildSpec>,
    #[serde(with = "mapped")]
    anatomy: Option<source::AnatomySpec>,
    #[serde(with = "mapped")]
    equipment: Option<source::EquipmentSpec>,
    #[serde(with = "mapped")]
    projectile: Option<source::ProjectileSpec>,
    #[serde(with = "mapped")]
    equipment_generation: Option<source::EquipmentGeneration>,
    #[serde(with = "mapped")]
    consumable: Option<source::ConsumableSpec>,
    #[serde(with = "mapped")]
    readable: Option<source::ReadableSpec>,
    #[serde(with = "mapped")]
    ground_scare: Option<tor_simulation::control_scrolls::ScareSpec>,
    #[serde(with = "mapped")]
    device: Option<source::DeviceSpec>,
    #[serde(with = "mapped")]
    edible: Option<source::EdibleSpec>,
    #[serde(deserialize_with = "required_option")]
    supply: Option<String>,
    magical: bool,
    #[serde(with = "mapped")]
    currency: Option<source::CurrencyKind>,
    #[serde(with = "mapped")]
    class: crate::scenario_package::ItemClass,
    #[serde(with = "mapped")]
    body: Option<source::BodySpec>,
    identity: Option<String>,
    appearance_pool: Option<String>,
    stackable: bool,
    properties: BTreeMap<String, String>,
    name: Option<String>,
    turn_ticks: Option<u64>,
    asset: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Character", deny_unknown_fields)]
struct Character {
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, source::CharacterLoadout, 64>"
    )]
    loadout: Vec<source::CharacterLoadout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(with = "mapped")]
    creature: Option<crate::creature_authoring::BuildSpec>,
    #[serde(with = "mapped")]
    anatomy: Option<source::AnatomySpec>,
    #[serde(with = "mapped")]
    body: Option<source::BodySpec>,
    velocity: Option<[i64; 3]>,
    known_identities: Vec<String>,
    id: u64,
    anchor: String,
    turn_ticks: u64,
    unselected: String,
    ai: Option<String>,
    asset: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Objective", deny_unknown_fields)]
struct Objective {
    anchor: String,
    item: Option<u64>,
    disclosed: bool,
    continue_play: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AiProfile", deny_unknown_fields)]
struct AiProfile {
    #[serde(with = "mapped")]
    idle: Option<crate::creature_authoring::IdleBehavior>,
    #[serde(deserialize_with = "required_option")]
    doors: Option<bool>,
    #[serde(deserialize_with = "required_option")]
    care: Option<bool>,
    #[serde(deserialize_with = "required_option")]
    equipment: Option<bool>,
    #[serde(deserialize_with = "required_option")]
    loot: Option<bool>,
    #[serde(deserialize_with = "required_option")]
    gold: Option<bool>,
    #[serde(deserialize_with = "stored_optional_flee")]
    flee_percent: Option<u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AnatomySpec", deny_unknown_fields)]
struct AnatomySpec {
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::EQUIPMENT_SOCKETS }>"
    )]
    slots: Vec<String>,
    #[serde(deserialize_with = "stored_anatomy_capacities")]
    capacities: BTreeMap<String, u16>,
    #[serde(with = "mapped")]
    primary_attack_socket: source::PrimaryAttackSocket,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::PrimaryAttackSocket",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SourcePrimaryAttackSocket {
    None {},
    Socket { slot: u16 },
}
fn stored_anatomy_capacities<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, u16>, D::Error> {
    let values = bounded_stored_map::<_, u16, { tor_simulation::limits::EQUIPMENT_SOCKETS }>(d)?;
    if values
        .values()
        .any(|units| usize::from(*units) > tor_simulation::limits::EQUIPMENT_SOCKETS)
    {
        return Err(serde::de::Error::custom(
            "invalid stored equipment capacities",
        ));
    }
    Ok(values)
}
impl Stored for tor_simulation::equipment_capacity::EquipmentCapacities {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        for (id, units) in self.entries() {
            map.serialize_entry(id.as_str(), &units)?;
        }
        map.end()
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let values = stored_anatomy_capacities(d)?
            .into_iter()
            .map(|(id, units)| {
                tor_simulation::content_ids::EquipmentCapacityId::new(id)
                    .map(|id| (id, units))
                    .map_err(|_| serde::de::Error::custom("invalid stored equipment capacity ID"))
            })
            .collect::<Result<BTreeMap<_, _>, D::Error>>()?;
        Self::new(values)
            .ok_or_else(|| serde::de::Error::custom("invalid stored equipment capacities"))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::AnatomySpec", deny_unknown_fields)]
struct RulesAnatomySpec {
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::EQUIPMENT_SOCKETS }>"
    )]
    slots: Vec<tor_simulation::EquipmentSlot>,
    #[serde(with = "mapped")]
    capacities: tor_simulation::equipment_capacity::EquipmentCapacities,
    #[serde(with = "mapped")]
    primary_attack_socket: tor_simulation::equipment_capacity::PrimaryAttackSocket,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::Constants", deny_unknown_fields)]
struct RulesConstants {
    base_defense: i32,
    attribute_cap: u16,
    derived_attribute_cap: u16,
    attribute_opportunity_interval: u16,
    racial_point_grants: Vec<tor_simulation::rules::PointGrant>,
    duration_scale_numerator: u16,
    duration_scale_offset: u16,
    default_abilities: Vec<tor_simulation::content_ids::AbilityId>,
    item_timing: tor_simulation::rules::ItemTiming,
    #[serde(with = "mapped")]
    default_anatomy: tor_simulation::AnatomySpec,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::EquipmentSlotDef",
    deny_unknown_fields
)]
struct RulesEquipmentSlotDef {
    id: tor_simulation::EquipmentSlot,
    name: String,
    equipped_phrase: String,
    compatible_classes: Vec<tor_simulation::ItemClass>,
    requires_attack: bool,
    #[serde(with = "mapped")]
    capacity: tor_simulation::equipment_capacity::EquipmentCapacityUse,
    equip_turns: u64,
    unequip_turns: u64,
}
impl Stored for tor_simulation::equipment_capacity::PrimaryAttackSocket {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = match self {
            Self::None => source::PrimaryAttackSocket::None {},
            Self::Socket { slot } => source::PrimaryAttackSocket::Socket { slot: slot.0 },
        };
        SourcePrimaryAttackSocket::serialize(&value, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <source::PrimaryAttackSocket as Stored>::decode(d)?;
        Ok(match value {
            source::PrimaryAttackSocket::None {} => Self::None,
            source::PrimaryAttackSocket::Socket { slot }
                if usize::from(slot) < tor_simulation::limits::EQUIPMENT_SOCKETS =>
            {
                Self::Socket {
                    slot: tor_simulation::EquipmentSlotId(slot),
                }
            }
            _ => return Err(serde::de::Error::custom("invalid stored primary socket")),
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StoredCapacityUse {
    None {},
    Shared { resource: String, units: u16 },
}
impl Stored for tor_simulation::equipment_capacity::EquipmentCapacityUse {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::None => StoredCapacityUse::None {},
            Self::Shared { resource, units } => StoredCapacityUse::Shared {
                resource: resource.as_str().into(),
                units: *units,
            },
        }
        .serialize(serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match super::named_map(d)? {
            StoredCapacityUse::None {} => Self::None,
            StoredCapacityUse::Shared { resource, units }
                if (1..=tor_simulation::limits::EQUIPMENT_SOCKETS)
                    .contains(&usize::from(units)) =>
            {
                Self::Shared {
                    resource: tor_simulation::content_ids::EquipmentCapacityId::new(resource)
                        .map_err(|_| {
                            serde::de::Error::custom("invalid stored equipment capacity ID")
                        })?,
                    units,
                }
            }
            _ => {
                return Err(serde::de::Error::custom(
                    "invalid stored equipment capacity use",
                ))
            }
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ArmorMaterial", rename_all = "snake_case")]
enum ArmorMaterial {
    Metal,
    Nonmetal,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ArmorSpec", deny_unknown_fields)]
struct ArmorSpec {
    #[serde(deserialize_with = "stored_armor_reduction")]
    physical_reduction: u32,
    #[serde(with = "mapped")]
    material: source::ArmorMaterial,
    rustproof: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::WeaponRangeBands", deny_unknown_fields)]
struct WeaponRangeBands {
    short_feet: u32,
    long_feet: u32,
}
impl Stored for source::WeaponRangeBands {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WeaponRangeBands::serialize(self, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Decode through the independent save record, then validate physical units.
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = source::WeaponRangeBands;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("named weapon range bands")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                fields: A,
            ) -> Result<Self::Value, A::Error> {
                WeaponRangeBands::deserialize(serde::de::value::MapAccessDeserializer::new(fields))
            }
        }
        let value = d.deserialize_map(Named)?;
        tor_simulation::weapon_profiles::RangeBands::from(value)
            .validate()
            .map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::WeaponTrait",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum WeaponTrait {
    TwoHanded {},
    Offhand {},
    Quickdraw {},
    Thrown {
        #[serde(with = "mapped")]
        range: source::WeaponRangeBands,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::WeaponTraitKind", rename_all = "snake_case")]
enum WeaponTraitKind {
    TwoHanded,
    Offhand,
    Quickdraw,
    Thrown,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::WeaponTraitChanges", deny_unknown_fields)]
struct WeaponTraitChanges {
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, 4>"
    )]
    add: Vec<source::WeaponTrait>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, 4>"
    )]
    remove: Vec<source::WeaponTraitKind>,
}
fn stored_launcher_family<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let family = String::deserialize(d)?;
    tor_simulation::content_ids::LauncherFamilyId::new(&family)
        .map_err(|_| serde::de::Error::custom("Invalid launcher family ID"))?;
    Ok(family)
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::LauncherProfile", deny_unknown_fields)]
struct LauncherProfile {
    #[serde(deserialize_with = "stored_launcher_family")]
    family: String,
    #[serde(with = "mapped")]
    range: source::WeaponRangeBands,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::HitRecovery", rename_all = "snake_case")]
enum HitRecovery {
    ConsumedOnHit,
    Recoverable,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ProjectileSpec", deny_unknown_fields)]
struct ProjectileSpec {
    #[serde(deserialize_with = "stored_launcher_family")]
    family: String,
    #[serde(with = "mapped")]
    damage: crate::creature_authoring::Damage,
    #[serde(with = "HitRecovery")]
    recovery: source::HitRecovery,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::WeaponProfileSpec", deny_unknown_fields)]
struct WeaponProfileSpec {
    expertise: String,
    #[serde(with = "mapped")]
    launcher: Option<source::LauncherProfile>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, 4>"
    )]
    base_traits: Vec<source::WeaponTrait>,
    #[serde(with = "mapped")]
    expert_traits: source::WeaponTraitChanges,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::EquipmentSpec", deny_unknown_fields)]
struct EquipmentSpec {
    #[serde(with = "mapped")]
    periodic_relocation: Option<source::PeriodicRelocation>,
    #[serde(with = "mapped")]
    weapon_profile: Option<source::WeaponProfileSpec>,
    slot: String,
    #[serde(with = "mapped")]
    armor: Option<source::ArmorSpec>,
    #[serde(with = "mapped")]
    attack: Option<crate::creature_authoring::Melee>,
    #[serde(with = "mapped")]
    modifiers: Vec<crate::creature_authoring::Modifier>,
    #[serde(with = "mapped")]
    potency_modifiers: Vec<crate::creature_authoring::Modifier>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::PeriodicRelocation", deny_unknown_fields)]
struct PeriodicRelocation {
    interval: u64,
    chance_denominator: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::DiceExpr", deny_unknown_fields)]
struct DiceExpr {
    count: u16,
    sides: u16,
    bonus: i32,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::AmountExpr",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum AmountExpr {
    Fixed {
        amount: u32,
    },
    Rolled {
        #[serde(with = "mapped")]
        dice: source::DiceExpr,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::DamageComponentExpr", deny_unknown_fields)]
struct DamageComponentExpr {
    #[serde(with = "mapped")]
    category: source::DamageType,
    descriptor: Option<String>,
    #[serde(with = "mapped")]
    amount: source::AmountExpr,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::DamageExpr",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum DamageExpr {
    SelectedWeapon {
        impact_bonus: u32,
    },
    Components {
        #[serde(
            serialize_with = "mapped::serialize",
            deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::DAMAGE_COMPONENTS }>"
        )]
        components: Vec<source::DamageComponentExpr>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::TheftSelection",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum TheftSelection {
    Currency {
        #[serde(with = "mapped")]
        currency: source::CurrencyKind,
        #[serde(with = "mapped")]
        amount: source::PopulationQuantityExpr,
    },
    MagicalUnit,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::RelocationPolicy", rename_all = "snake_case")]
enum RelocationPolicy {
    SameFloor,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::RelocationGate", deny_unknown_fields)]
struct RelocationGate {
    flag: String,
    #[serde(with = "mapped")]
    policy: source::RelocationPolicy,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::HealingOverflow", rename_all = "snake_case")]
enum HealingOverflow {
    None,
    ImproveEarliestHitDie,
    ImproveEarliestHitDiceByExcess,
}
#[derive(Serialize)]
#[serde(
    remote = "source::Effect",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Effect {
    Theft {
        #[serde(with = "mapped")]
        selection: source::TheftSelection,
        #[serde(with = "mapped")]
        relocation: Option<source::RelocationGate>,
    },
    AlterEquipment {
        #[serde(with = "mapped")]
        selection: source::EquippedSelection,
        #[serde(with = "mapped")]
        alteration: source::EquipmentAlteration,
    },
    CorrodeArmor {
        slot: String,
        amount: u32,
    },
    ApplyLinkedHold {
        #[serde(with = "mapped")]
        policy: source::HoldPolicy,
    },
    HoldDamage {
        #[serde(with = "DamageType")]
        category: source::DamageType,
        descriptor: Option<String>,
    },
    DrainLatestHitDie,
    RestoreHitDie,
    RoomLight,
    StepSelfOneCell,
    TeleportRandom,
    TeleportToRayOrigin,
    ArrestDescent,
    PushTargetOneCell {
        #[serde(with = "PushDirection")]
        direction: source::PushDirection,
    },
    Damage {
        #[serde(with = "mapped")]
        damage: source::DamageExpr,
    },
    Heal {
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    HealPerHitDie {
        sides: u16,
        bonus: i32,
        #[serde(with = "mapped")]
        overflow: source::HealingOverflow,
    },
    LoseAttribute {
        attribute: String,
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    RestoreAttribute {
        attribute: String,
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    LoseMaximumHealth {
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    RestoreMaximumHealth {
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    Nourish {
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    ApplyCondition {
        condition: String,
        duration: u64,
    },
    ApplyPersistentCondition {
        condition: String,
        #[serde(deserialize_with = "required_option")]
        cancels: Option<String>,
    },
    ApplyConditionForTurns {
        condition: String,
        #[serde(with = "mapped")]
        duration: source::AmountExpr,
    },
    RemoveCondition {
        condition: String,
    },
    Restore {
        resource: String,
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    SearchAdjacentGuises,
    ObservePublicPlan,
    ApplyQualifiedConditionToSource {
        condition: String,
        duration: u64,
        ability_tag: String,
    },
}
#[derive(Serialize)]
#[serde(remote = "source::HoldPolicy", deny_unknown_fields)]
struct HoldPolicy {
    sustaining: String,
    preparation: u64,
    recovery: u64,
    protection: u64,
}
impl HoldPolicy {
    fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<source::HoldPolicy, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            sustaining: String,
            preparation: u64,
            recovery: u64,
            protection: u64,
        }
        let f: Fields = super::named_map(d)?;
        if tor_simulation::content_ids::FlagId::new(&f.sustaining).is_err()
            || ![f.preparation, f.recovery, f.protection]
                .into_iter()
                .all(|value| (1..=tor_simulation::limits::NUMERIC as u64).contains(&value))
        {
            return Err(serde::de::Error::custom("Invalid stored hold policy"));
        }
        Ok(source::HoldPolicy {
            sustaining: f.sustaining,
            preparation: f.preparation,
            recovery: f.recovery,
            protection: f.protection,
        })
    }
}

#[cfg(test)]
mod linked_hold_storage_tests {
    use super::*;
    #[test]
    fn stored_linked_hold_effects_use_independent_strict_named_fields() {
        for raw in [
            r#"{"kind":"apply_linked_hold","policy":{"sustaining":"clutch","preparation":100,"recovery":100,"protection":400}}"#,
            r#"{"kind":"hold_damage","category":"impact","descriptor":null}"#,
        ] {
            let restored: Owned<source::Effect> =
                crate::storage::codec::strict(raw.as_bytes()).unwrap();
            assert_eq!(
                serde_json::to_value(Borrowed(&restored.0)).unwrap(),
                serde_json::from_str::<serde_json::Value>(raw).unwrap()
            );
        }
        for raw in [
            r#"{"kind":"apply_linked_hold","policy":["clutch",100,100,400]}"#,
            r#"{"kind":"apply_linked_hold","policy":{"sustaining":"clutch","preparation":0,"recovery":100,"protection":400}}"#,
            r#"{"kind":"apply_linked_hold","policy":{"sustaining":"clutch","preparation":100,"recovery":100}}"#,
            r#"{"kind":"apply_linked_hold","policy":{"sustaining":"clutch","preparation":100,"preparation":101,"recovery":100,"protection":400}}"#,
            r#"{"kind":"hold_damage","category":"impact"}"#,
            r#"{"kind":"hold_damage","category":"impact","descriptor":null,"holder":1}"#,
            r#"["hold_damage","impact",null]"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Owned<source::Effect>>(raw.as_bytes()).is_err(),
                "{raw}"
            );
        }
    }
    #[test]
    fn compiled_hold_damage_rules_require_the_explicit_nullable_descriptor() {
        use tor_simulation::rules::{Effect as RuleEffect, Ruleset, RulesetDocument, StrikeSource};
        let mut document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let strike = serde_json::from_value(serde_json::json!({
            "offset":0,"check":{"actor":"user","skill":{"kind":"natural_attack"},
                "against":{"kind":"defense","defense":"physical"}},
            "on_success":[RuleEffect::HoldDamage {
                category:tor_simulation::combat::DamageType::Impact,descriptor:None}],
            "on_failure":[],"descriptors":[],"riders":[]}))
        .unwrap();
        document
            .abilities
            .iter_mut()
            .find(|ability| ability.id.as_str() == "clutch")
            .unwrap()
            .strikes = StrikeSource::Authored {
            strikes: vec![strike],
        };
        Ruleset::validate(document.clone()).unwrap();
        let current = encode_rules_document(&document).unwrap();
        decode_rules_document(&current).unwrap();
        let marked = r#"{"kind":"hold_damage","category":"impact","descriptor":null}"#;
        assert_eq!(current.matches(marked).count(), 1);
        // Preserve every other byte, so canonical key order cannot mask the field check.
        let missing = current.replacen(marked, r#"{"kind":"hold_damage","category":"impact"}"#, 1);
        assert!(decode_rules_document(&missing).is_err());
    }
    #[test]
    fn hold_damage_increment_modifier_round_trips_authored_stored_and_compiled_forms() {
        let raw = serde_json::json!({"scope":"always","op":{"kind":"add",
            "stat":{"kind":"ability","ability":"clutch","stat":"hold_damage_increment"},"value":1}});
        let authored: crate::creature_authoring::Modifier = serde_json::from_value(raw).unwrap();
        let compiled = authored.compile().unwrap();
        let stored = serde_json::to_value(Borrowed(&authored)).unwrap();
        let restored: Owned<crate::creature_authoring::Modifier> =
            serde_json::from_value(stored).unwrap();
        assert_eq!(restored.0.compile().unwrap(), compiled);
        assert!(serde_json::to_string(&compiled)
            .unwrap()
            .contains("hold_damage_increment"));
    }
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ConsumableSpec", deny_unknown_fields)]
struct ConsumableSpec {
    descriptors: Vec<String>,
    #[serde(with = "mapped")]
    discovery: source::ItemDiscovery,
    #[serde(with = "mapped")]
    effects: Vec<source::Effect>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::EquipmentAlteration", rename_all = "snake_case")]
enum EquipmentAlteration {
    EnchantArmor,
    EnchantWeapon,
    ProtectArmor,
    RemoveCurse,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::EquippedSelection",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum EquippedSelection {
    Slot {
        slot: String,
        #[serde(with = "ItemClass")]
        class: source::ItemClass,
        last: bool,
    },
    All {},
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ItemDiscovery", rename_all = "snake_case")]
enum ItemDiscovery {
    SelfEffect,
    OnUse,
    Never,
}
fn read_control_duration<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let duration = u64::deserialize(d)?;
    if !(1..=tor_simulation::limits::NUMERIC as u64).contains(&duration) {
        return Err(serde::de::Error::custom("invalid stored control duration"));
    }
    Ok(duration)
}
fn read_control_radius<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let radius = u8::deserialize(d)?;
    if !(1..=8).contains(&radius) {
        return Err(serde::de::Error::custom("invalid stored control radius"));
    }
    Ok(radius)
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::control_scrolls::ControlSpec",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ControlSpec {
    HoldNearby {
        condition: tor_simulation::content_ids::ConditionId,
        #[serde(deserialize_with = "read_control_duration")]
        duration: u64,
        #[serde(deserialize_with = "read_control_radius")]
        radius: u8,
    },
    ArmConfusion {
        condition: tor_simulation::content_ids::ConditionId,
        #[serde(deserialize_with = "read_control_duration")]
        duration: u64,
    },
    Aggravate {},
    CreateMonster {},
    Nothing {},
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::control_scrolls::ConditionPayload",
    deny_unknown_fields
)]
struct ConditionPayload {
    condition: tor_simulation::content_ids::ConditionId,
    #[serde(deserialize_with = "read_control_duration")]
    duration: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ReadableSpec", deny_unknown_fields)]
struct ReadableSpec {
    #[serde(with = "mapped")]
    control: Option<tor_simulation::control_scrolls::ControlSpec>,
    descriptors: Vec<String>,
    #[serde(with = "mapped")]
    effects: Vec<source::Effect>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "read_saved_identification_classes"
    )]
    identify: Option<Vec<source::ItemClass>>,
    #[serde(with = "mapped")]
    discovery: source::ItemDiscovery,
}
fn read_saved_identification_classes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<source::ItemClass>>, D::Error> {
    struct Classes(Vec<source::ItemClass>);
    impl<'de> Deserialize<'de> for Classes {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            struct Token(#[serde(deserialize_with = "read_class")] source::ItemClass);
            fn read_class<'de, D: serde::Deserializer<'de>>(
                d: D,
            ) -> Result<source::ItemClass, D::Error> {
                let token = String::deserialize(d)?;
                ItemClass::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(token))
            }
            let tokens: Vec<Token> = crate::creature_authoring::bounded_strike_vec::<_, _, 14>(d)?;
            let classes: Vec<_> = tokens.into_iter().map(|token| token.0).collect();
            if classes.is_empty() || !classes.windows(2).all(|pair| pair[0] < pair[1]) {
                return Err(serde::de::Error::custom(
                    "invalid stored identification class set",
                ));
            }
            Ok(Self(classes))
        }
    }
    Option::<Classes>::deserialize(deserializer).map(|classes| classes.map(|classes| classes.0))
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::EquipmentGeneration",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum EquipmentGeneration {
    Cursed,
    SignedPotency {
        negative: u32,
        positive: u32,
    },
    Authored {
        hit_enchantment: i32,
        damage_enchantment: i32,
    },
    Weapon {
        curse_percent: u8,
        enchant_percent: u8,
        minimum: u32,
        maximum: u32,
    },
    Armor {
        curse_percent: u8,
        enchant_percent: u8,
        minimum: u32,
        maximum: u32,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ChargeRange", deny_unknown_fields)]
struct ChargeRange {
    minimum: u16,
    maximum: u16,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::DeviceSpec", deny_unknown_fields)]
struct DeviceSpec {
    ability: String,
    #[serde(with = "mapped")]
    initial_charges: source::ChargeRange,
    #[serde(with = "mapped")]
    discovery: source::ItemDiscovery,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Catalog", deny_unknown_fields)]
struct CreatureCatalog {
    #[serde(with = "mapped")]
    species: BTreeMap<String, crate::creature_authoring::Species>,
    #[serde(with = "mapped")]
    templates: BTreeMap<String, crate::creature_authoring::Template>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Disposition",
    rename_all = "snake_case"
)]
enum BehaviorDisposition {
    Passive,
    Mean,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::IdleBehavior",
    rename_all = "snake_case"
)]
enum BehaviorIdle {
    Wait,
    Explore,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::Interaction",
    rename_all = "snake_case"
)]
enum AbilityInteraction {
    NonHostile,
    HostileToTarget,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::BehaviorPolicy",
    deny_unknown_fields
)]
struct BehaviorPolicy {
    #[serde(with = "mapped")]
    disposition: crate::creature_authoring::Disposition,
    #[serde(with = "mapped")]
    idle: crate::creature_authoring::IdleBehavior,
    #[serde(deserialize_with = "stored_behavior_memory")]
    memory_ticks: u64,
    doors: bool,
    care: bool,
    equipment: bool,
    loot: bool,
    gold: bool,
    #[serde(deserialize_with = "stored_flee")]
    flee_percent: u32,
}

fn stored_flee<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(d)?;
    if value > 100 {
        return Err(serde::de::Error::custom(
            "stored flee threshold exceeds 100",
        ));
    }
    Ok(value)
}
fn stored_optional_flee<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    let value: Option<u32> = required_option(d)?;
    if value.is_some_and(|value| value > 100) {
        return Err(serde::de::Error::custom(
            "stored flee threshold exceeds 100",
        ));
    }
    Ok(value)
}

fn stored_behavior_memory<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    let ticks = u64::deserialize(deserializer)?;
    if ticks > tor_simulation::limits::NUMERIC as u64 {
        return Err(serde::de::Error::custom(
            "stored behavior memory exceeds limit",
        ));
    }
    Ok(ticks)
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Species", deny_unknown_fields)]
struct Species {
    #[serde(with = "mapped")]
    behavior: crate::creature_authoring::BehaviorPolicy,
    #[serde(with = "mapped")]
    displacement: Option<crate::creature_authoring::DisplacementProfile>,
    kind: String,
    subtypes: Vec<String>,
    attributes: tor_simulation::attributes::ByAttribute<u16>,
    #[serde(with = "mapped")]
    anatomy: Option<source::AnatomySpec>,
    #[serde(with = "mapped")]
    melee: crate::creature_authoring::Natural,
    #[serde(with = "mapped")]
    modifiers: Vec<crate::creature_authoring::Modifier>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Template", deny_unknown_fields)]
struct Template {
    priority: i16,
    kind: Option<String>,
    add_subtypes: Vec<String>,
    remove_subtypes: Vec<String>,
    adjustments: tor_simulation::attributes::ByAttribute<i16>,
    overrides: tor_simulation::attributes::ByAttribute<Option<u16>>,
    #[serde(with = "mapped")]
    modifiers: Vec<crate::creature_authoring::Modifier>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Natural", deny_unknown_fields)]
struct Natural {
    skill: String,
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::STRIKES }>"
    )]
    strikes: Vec<crate::creature_authoring::Strike>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Strike", deny_unknown_fields)]
struct Strike {
    offset: u64,
    #[serde(deserialize_with = "required_option")]
    unlock: Option<String>,
    #[serde(with = "mapped")]
    check: Option<crate::creature_authoring::Check>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::EFFECTS_PER_BRANCH }>"
    )]
    on_success: Vec<source::Effect>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::EFFECTS_PER_BRANCH }>"
    )]
    on_failure: Vec<source::Effect>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
    )]
    descriptors: Vec<String>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::RIDERS_PER_STRIKE }>"
    )]
    riders: Vec<crate::creature_authoring::Rider>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Rider", deny_unknown_fields)]
struct Rider {
    #[serde(with = "RiderTrigger")]
    trigger: crate::creature_authoring::RiderTrigger,
    #[serde(with = "mapped")]
    condition: Option<crate::creature_authoring::RiderCondition>,
    #[serde(with = "mapped")]
    check: Option<crate::creature_authoring::RiderCheck>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::EFFECTS_PER_BRANCH }>"
    )]
    effects: Vec<source::Effect>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
    )]
    descriptors: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::RiderTrigger",
    rename_all = "snake_case"
)]
enum RiderTrigger {
    Success,
    Failure,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::RiderCondition",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum RiderCondition {
    SequenceSuccessfulHits {
        #[serde(deserialize_with = "sequence_minimum")]
        minimum: u16,
    },
}
fn sequence_minimum<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let minimum = u16::deserialize(d)?;
    if !(1..=tor_simulation::limits::STRIKES as u16).contains(&minimum) {
        return Err(serde::de::Error::custom(
            "invalid stored sequence hit minimum",
        ));
    }
    Ok(minimum)
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Check", deny_unknown_fields)]
struct StrikeCheck {
    #[serde(with = "mapped")]
    actor: crate::creature_authoring::CheckActor,
    #[serde(with = "mapped")]
    skill: crate::creature_authoring::SkillSelector,
    #[serde(with = "mapped")]
    against: crate::creature_authoring::Against,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::RiderCheck", deny_unknown_fields)]
struct RiderCheck {
    #[serde(with = "mapped")]
    skill: crate::creature_authoring::SkillSelector,
    defense: String,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::CheckActor",
    rename_all = "snake_case"
)]
enum CheckActor {
    User,
    Target,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::SkillSelector",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SkillSelector {
    Named { skill: String },
    NaturalAttack,
    SelectedWeapon,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Against",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum StrikeAgainst {
    Defense {
        defense: String,
    },
    Difficulty {
        #[serde(with = "mapped")]
        formula: crate::creature_authoring::Formula,
    },
    Opposed {
        #[serde(with = "mapped")]
        skill: crate::creature_authoring::SkillSelector,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Formula", deny_unknown_fields)]
struct StrikeFormula {
    base: i32,
    #[serde(with = "mapped")]
    skill: Option<crate::creature_authoring::SkillSelector>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Melee", deny_unknown_fields)]
struct Melee {
    skill: String,
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    #[serde(with = "mapped")]
    damage: crate::creature_authoring::Damage,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Damage", deny_unknown_fields)]
struct NaturalDamage {
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, 32>"
    )]
    components: Vec<crate::creature_authoring::Component>,
    #[serde(with = "mapped")]
    primary: Option<crate::creature_authoring::DamageKey>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Component", deny_unknown_fields)]
struct NaturalComponent {
    #[serde(with = "mapped")]
    category: source::DamageType,
    #[serde(deserialize_with = "required_option")]
    descriptor: Option<String>,
    #[serde(with = "mapped")]
    amount: crate::creature_authoring::Amount,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::DamageKey", deny_unknown_fields)]
struct NaturalDamageKey {
    #[serde(with = "mapped")]
    category: source::DamageType,
    #[serde(deserialize_with = "required_option")]
    descriptor: Option<String>,
    #[serde(deserialize_with = "required_option")]
    sides: Option<u16>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Amount",
    tag = "type",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum NaturalAmount {
    Fixed { value: u32 },
    Rolled { count: u16, sides: u16, bonus: i32 },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Modifier", deny_unknown_fields)]
struct Modifier {
    #[serde(with = "mapped")]
    scope: crate::creature_authoring::Scope,
    #[serde(with = "mapped")]
    op: crate::creature_authoring::Op,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Scope", rename_all = "snake_case")]
enum ModifierScope {
    Always,
    VersusSource,
    WhileNoSensedHostile,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Op",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ModifierOp {
    SignedReduction {
        #[serde(with = "mapped")]
        selector: crate::creature_authoring::Selector,
        amount: i32,
    },
    AttributeWithoutDefense {
        attribute: String,
        defense: String,
        value: i64,
    },
    SustainAttribute {
        attribute: String,
    },
    ProtectArmor {},
    HealthRecoveryRate {
        multiplier: u16,
    },
    ActionDuration {
        #[serde(with = "mapped")]
        value: crate::creature_authoring::ActionDuration,
    },
    Perception {
        #[serde(with = "mapped")]
        value: crate::creature_authoring::Perception,
    },
    Locomotion {
        #[serde(with = "mapped")]
        value: crate::creature_authoring::Locomotion,
    },
    Add {
        #[serde(with = "mapped")]
        stat: crate::creature_authoring::Stat,
        value: i64,
    },
    Set {
        #[serde(with = "mapped")]
        stat: crate::creature_authoring::Stat,
        value: i64,
    },
    Flag {
        flag: String,
    },
    Immunity {
        #[serde(with = "mapped")]
        selector: crate::creature_authoring::Selector,
    },
    Reduction {
        #[serde(with = "mapped")]
        selector: crate::creature_authoring::Selector,
        amount: u32,
    },
    TalentReduction {
        category: String,
        amount: u32,
    },
    GrantAbility {
        ability: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::ActionDuration",
    rename_all = "snake_case"
)]
enum ActionDuration {
    Normal,
    Haste,
    Slow,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Perception",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Perception {
    Invisible {
        enabled: bool,
    },
    SeeInvisible {
        enabled: bool,
    },
    Sight {
        enabled: bool,
    },
    ObjectGuise {
        #[serde(with = "mapped")]
        appearance: Option<crate::creature_authoring::GuiseAppearance>,
        skill: String,
        edge: i64,
    },
    AmbushFromGuise {
        enabled: bool,
    },
    ReadiedTouchAmbush {
        enabled: bool,
    },
    ContinualStealth {
        edge: u32,
    },
    Aggravation {
        interval: u64,
        chance_denominator: u32,
        stealth_disadvantage: u32,
    },
    PeriodicSearch {
        interval: u64,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::GuiseAppearance",
    deny_unknown_fields
)]
struct GuiseAppearance {
    name: String,
    description: String,
    #[serde(with = "mapped")]
    class: crate::scenario_package::ItemClass,
    #[serde(with = "mapped")]
    asset: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Locomotion",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Locomotion {
    Stationary {
        enabled: bool,
    },
    Flying {
        enabled: bool,
    },
    Levitating {
        enabled: bool,
    },
    Erratic {
        #[serde(with = "mapped")]
        chance: Option<crate::creature_authoring::MovementChance>,
    },
    Confused {
        #[serde(with = "mapped")]
        chance: Option<crate::creature_authoring::MovementChance>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::MovementChance",
    deny_unknown_fields
)]
struct MovementChance {
    numerator: u16,
    denominator: u16,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Stat",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ModifierStat {
    Attribute { attribute: String },
    Defense { defense: String },
    Health,
    HealthRecoveryPeriod,
    HealthRecoveryPeriodPercent,
    HealthRecoveryResetPercent,
    AttackDamageRollBonus,
    NutritionRateThirtieths,
    ResourceMax { resource: String },
    Ability { ability: String, stat: String },
    AbilityTag { tag: String, stat: String },
    Edge,
    CheckEdge,
    IncomingEdge { defense: Option<String> },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Selector",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ModifierSelector {
    Category {
        #[serde(with = "mapped")]
        category: source::DamageType,
    },
    Descriptor {
        descriptor: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Advancement", deny_unknown_fields)]
struct Advancement {
    source: String,
    #[serde(with = "mapped")]
    purchases: Vec<crate::creature_authoring::Purchase>,
    attribute: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Purchase", deny_unknown_fields)]
struct Purchase {
    pool: String,
    #[serde(with = "mapped")]
    selection: crate::creature_authoring::PurchaseSelection,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::PurchaseSelection",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum PurchaseSelection {
    Expertise { expertise: String },
    SkillRank { skill: String },
    Talent { talent: String },
    TalentForm { talent: String, form: String },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Restoration",
    rename_all = "snake_case"
)]
enum Restoration {
    ChooseAgain,
    Authored,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::BuildSpec", deny_unknown_fields)]
struct BuildSpec {
    #[serde(with = "mapped")]
    future_growth: crate::creature_authoring::FutureGrowth,
    starting_skills: [String; 2],
    #[serde(with = "mapped")]
    restoration: crate::creature_authoring::Restoration,
    #[serde(with = "mapped")]
    nutrition: tor_simulation::nutrition::NutritionRequirement,
    species: String,
    name: String,
    faction: String,
    attributes: Option<tor_simulation::attributes::ByAttribute<u16>>,
    binding: String,
    #[serde(with = "mapped")]
    hit_dice: Vec<crate::creature_authoring::Advancement>,
    templates: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::BodySpec", deny_unknown_fields)]
struct BodySpec {
    cells: Vec<[i32; 3]>,
    eye: [i32; 3],
    mass: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::DamageType", rename_all = "snake_case")]
enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
    Psychic,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Certificate", deny_unknown_fields)]
struct Certificate {
    validator: String,
    ruleset: String,
    ruleset_hash: String,
    ruleset_dependencies: BTreeMap<String, String>,
    content_hash: String,
    files: BTreeMap<String, String>,
    regions: usize,
    model_hash: String,
    coverage: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::RegionIndex", deny_unknown_fields)]
struct RegionIndex {
    #[serde(with = "mapped")]
    regions: Vec<source::IndexedRegion>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::IndexedRegion", deny_unknown_fields)]
struct IndexedRegion {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    boundary_anchors: Vec<String>,
    id: u64,
    file: String,
    hash: String,
    compiled_hash: String,
    name: String,
    size: [i32; 3],
    chamber: bool,
    zone: Option<String>,
    anchors: BTreeMap<String, [i32; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    stair_anchors: Vec<String>,
    portals: Vec<String>,
    #[serde(with = "mapped")]
    actors: Vec<source::IndexedActor>,
    #[serde(with = "mapped")]
    items: Vec<source::IndexedItem>,
    doors: Vec<u64>,
    traps: Vec<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    generated: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::IndexedActor", deny_unknown_fields)]
struct IndexedActor {
    id: u64,
    at: [i32; 3],
    turn_ticks: Option<u64>,
    archetype: Option<String>,
    external: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::IndexedItem", deny_unknown_fields)]
struct IndexedItem {
    id: u64,
    carried_by: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::RegionDef", deny_unknown_fields)]
struct RegionDef {
    #[serde(deserialize_with = "required_option")]
    trapdoor_destination: Option<u64>,
    lit: bool,
    #[serde(with = "mapped")]
    lighting: Vec<source::CellLight>,
    id: u64,
    name: String,
    size: [i32; 3],
    chamber: bool,
    zone: Option<String>,
    gravity: Option<[i32; 3]>,
    #[serde(with = "mapped")]
    gravity_overrides: Vec<source::Gravity>,
    anchors: BTreeMap<String, [i32; 3]>,
    walls: Vec<[i32; 3]>,
    openings: Vec<[i32; 3]>,
    #[serde(with = "mapped")]
    places: Vec<source::PlaceDef>,
    #[serde(with = "mapped")]
    portals: Vec<source::Portal>,
    #[serde(with = "mapped")]
    doors: Vec<source::Door>,
    #[serde(with = "mapped")]
    traps: Vec<source::Trap>,
    #[serde(with = "mapped")]
    items: Vec<source::Item>,
    #[serde(with = "mapped")]
    actors: Vec<source::Actor>,
    #[serde(with = "mapped")]
    generate: Option<crate::generator::Generate>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::CellLight", deny_unknown_fields)]
struct CellLight {
    at: [i32; 3],
    lit: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Gravity", deny_unknown_fields)]
struct Gravity {
    at: [i32; 3],
    vector: [i32; 3],
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::NamedPlace", deny_unknown_fields)]
struct NamedPlace {
    at: [i32; 3],
    name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Portal", deny_unknown_fields)]
struct Portal {
    rotation: Option<u8>,
    kind: Option<String>,
    at: [i32; 3],
    direction: String,
    to: String,
    turns: u8,
    width: u16,
    height: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Door", deny_unknown_fields)]
struct Door {
    #[serde(with = "mapped")]
    concealment: Option<source::Concealment>,
    id: u64,
    at: [i32; 3],
    open: bool,
    height: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Concealment", deny_unknown_fields)]
struct Concealment {
    dc: i32,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::TrapKind",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum TrapKind {
    Rust {
        slot: String,
        amount: u32,
    },
    SleepingGas {
        #[serde(deserialize_with = "required_option")]
        exposure: Option<String>,
        condition: String,
        duration: u64,
    },
    TrapDoor {
        #[serde(with = "mapped")]
        destination: tor_world::NamedAnchor,
        open: bool,
    },
    Arrow {
        attack_bonus: i32,
        ammunition: String,
    },
    BearTrap {
        condition: String,
        duration: u64,
    },
    Teleport {},
    PoisonDart {
        attack_bonus: i32,
        exposure: String,
    },
    Mysterious {},
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_world::NamedAnchor", deny_unknown_fields)]
struct TrapAnchor {
    region: tor_world::RegionId,
    name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Trap", deny_unknown_fields)]
struct Trap {
    #[serde(deserialize_with = "required_option")]
    avoidance_dc: Option<i32>,
    id: u64,
    at: [i32; 3],
    dc: i32,
    #[serde(with = "mapped")]
    kind: source::TrapKind,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Item", deny_unknown_fields)]
struct Item {
    equipped_slot: Option<u16>,
    #[serde(with = "mapped")]
    class: Option<source::ItemClass>,
    quantity: u64,
    stackable: Option<bool>,
    properties: BTreeMap<String, String>,
    id: u64,
    at: [i32; 3],
    archetype: Option<String>,
    name: Option<String>,
    carried_by: Option<u64>,
    seed_names: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::Actor", deny_unknown_fields)]
struct Actor {
    #[serde(with = "mapped")]
    creature: Option<crate::creature_authoring::BuildSpec>,
    #[serde(with = "mapped")]
    anatomy: Option<source::AnatomySpec>,
    known_identities: Vec<String>,
    #[serde(with = "mapped")]
    body: Option<source::BodySpec>,
    velocity: Option<[i64; 3]>,
    id: u64,
    at: [i32; 3],
    archetype: Option<String>,
    turn_ticks: Option<u64>,
    controller: String,
    ai: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generator::Generate", deny_unknown_fields)]
struct Generate {
    group: Option<String>,
    boundary_anchors: Vec<String>,
    generator: String,
    version: u32,
    salt: u64,
    rooms: [u32; 2],
    stair_anchors: Vec<String>,
    #[serde(with = "mapped")]
    actors: Option<crate::generator::ActorPool>,
    #[serde(with = "mapped")]
    items: Option<crate::generator::ItemPool>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generator::ActorPool", deny_unknown_fields)]
struct ActorPool {
    archetypes: Vec<String>,
    ai: String,
    count: [u32; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::generator::ItemPool", deny_unknown_fields)]
struct ItemPool {
    archetypes: Vec<String>,
    count: [u32; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::procedural_world::WorldFamily", deny_unknown_fields)]
struct WorldFamily {
    #[serde(deserialize_with = "required_option")]
    gravity: Option<[i32; 3]>,
    #[serde(with = "mapped")]
    kind: crate::procedural_world::FamilyKind,
    version: u32,
    base_region: u64,
    #[serde(with = "mapped")]
    domain: crate::procedural_world::Domain,
    grid: [u32; 2],
    size: [i32; 3],
    recipe: String,
    zone: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::PlaceDef",
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum PlaceDef {
    At([i32; 3]),
    Named(#[serde(with = "mapped")] source::NamedPlace),
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::procedural_world::Domain",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Domain {
    Finite { count: u32 },
    SeededFinite { min: u32, max: u32 },
    Open { max_depth: u32 },
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::procedural_world::FamilyKind",
    rename_all = "snake_case"
)]
enum FamilyKind {
    GridChain,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::EdibleSpec", deny_unknown_fields)]
struct EdibleSpec {
    #[serde(with = "mapped")]
    nutrition: tor_simulation::rules::AmountExpr,
}
// EdibleSpec is a simulation alias. Its amount/dice are not the authoring
// source::AmountExpr/source::DiceExpr mapped above; save ownership is recursive.
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::DiceExpr", deny_unknown_fields)]
struct EdibleDiceExpr {
    count: u16,
    sides: u16,
    bonus: i32,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::AmountExpr",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum EdibleAmountExpr {
    Fixed {
        amount: u32,
    },
    Rolled {
        #[serde(with = "mapped")]
        dice: tor_simulation::rules::DiceExpr,
    },
}
#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::nutrition::NutritionRequirement",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum NutritionRequirement {
    Exempt,
    Required,
}
#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::health_recovery::RecoveryNutrition",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum RecoveryNutrition {
    Normal,
    Slowed { multiplier: u16 },
    Stopped,
}
#[derive(Serialize)]
#[serde(remote = "tor_simulation::nutrition::RegenerationCost")]
struct RegenerationCost {
    denominator: u32,
    health: u32,
    stamina: u32,
    focus: u32,
    mana: u32,
}
impl RegenerationCost {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::nutrition::RegenerationCost, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            denominator: u32,
            health: u32,
            stamina: u32,
            focus: u32,
            mana: u32,
        }
        let value: Fields = super::named_map(d)?;
        Ok(tor_simulation::nutrition::RegenerationCost {
            denominator: value.denominator,
            health: value.health,
            stamina: value.stamina,
            focus: value.focus,
            mana: value.mana,
        })
    }
}
#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::nutrition::NutritionPolicy",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum NutritionPolicy {
    Disabled,
    Enabled {
        initial: u32,
        maximum: u32,
        hungry_below: u32,
        weak_below: u32,
        starve_at: i32,
        unit_ticks: u64,
        #[serde(with = "mapped")]
        hungry_recovery: tor_simulation::health_recovery::RecoveryNutrition,
        #[serde(with = "mapped")]
        weak_recovery: tor_simulation::health_recovery::RecoveryNutrition,
        #[serde(with = "mapped")]
        empty_recovery: tor_simulation::health_recovery::RecoveryNutrition,
        #[serde(with = "mapped")]
        regeneration_cost: tor_simulation::nutrition::RegenerationCost,
    },
}

// The depletion policy and every nested operand have save-owned exact shapes.
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::nutrition::NutritionChance",
    deny_unknown_fields
)]
struct NutritionChance {
    one_in: u16,
}

#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::nutrition::NutritionConditionTrigger",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum NutritionConditionTrigger {
    Disabled,
    OnEmptyDepletion {
        condition: tor_simulation::content_ids::ConditionId,
        #[serde(with = "mapped")]
        chance: tor_simulation::nutrition::NutritionChance,
        #[serde(with = "mapped")]
        duration: tor_simulation::rules::AmountExpr,
        ticks_per_unit: u64,
    },
}
// Conditions have a save-owned schema at every condition semantic boundary.
// Shared modifier and identifier operands retain their existing stored shapes.
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::ConditionDef", deny_unknown_fields)]
struct ConditionDefinition {
    id: tor_simulation::content_ids::ConditionId,
    family: tor_simulation::content_ids::ConditionFamilyId,
    #[serde(with = "mapped")]
    lifecycle: tor_simulation::rules::ConditionLifecycle,
    name: String,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::MODIFIERS_PER_SOURCE }>"
    )]
    modifiers: Vec<tor_simulation::rules::Modifier>,
    #[serde(with = "mapped")]
    stacking: tor_simulation::rules::Stacking,
    source_scoped: bool,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
    )]
    descriptors: Vec<tor_simulation::content_ids::DescriptorId>,
    #[serde(with = "mapped")]
    capacity: tor_simulation::rules::Eviction,
    #[serde(with = "mapped")]
    timing: tor_simulation::rules::ConditionTiming,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "condition_restrictions"
    )]
    restrictions: Vec<tor_simulation::rules::ConditionRestriction>,
    #[serde(with = "mapped")]
    preparation: tor_simulation::rules::ConditionPreparation,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::ConditionLifecycle",
    rename_all = "snake_case"
)]
enum ConditionLifecycle {
    Timed,
    NextCheck,
    NextCheckAgainst,
}

#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::ConditionTiming",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ConditionTiming {
    UntilRemoved,
    ActorActive {
        #[serde(with = "mapped")]
        on_expiry: tor_simulation::rules::ConditionExpiry,
    },
}

impl ConditionTiming {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::ConditionTiming, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            UntilRemoved {},
            ActorActive {
                #[serde(with = "mapped")]
                on_expiry: tor_simulation::rules::ConditionExpiry,
            },
        }
        Ok(match super::named_map(deserializer)? {
            Fields::UntilRemoved {} => tor_simulation::rules::ConditionTiming::UntilRemoved,
            Fields::ActorActive { on_expiry } => {
                tor_simulation::rules::ConditionTiming::ActorActive { on_expiry }
            }
        })
    }
}

#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::ConditionExpiry",
    tag = "kind",
    rename_all = "snake_case"
)]
enum ConditionExpiry {
    Remove,
    Protect {
        #[serde(with = "mapped")]
        duration: tor_simulation::rules::ConditionTicks,
    },
}
impl ConditionExpiry {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::ConditionExpiry, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Remove {},
            Protect {
                #[serde(with = "mapped")]
                duration: tor_simulation::rules::ConditionTicks,
            },
        }
        Ok(match super::named_map(deserializer)? {
            Fields::Remove {} => tor_simulation::rules::ConditionExpiry::Remove,
            Fields::Protect { duration } => {
                tor_simulation::rules::ConditionExpiry::Protect { duration }
            }
        })
    }
}
impl Stored for tor_simulation::rules::ConditionTicks {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.get())
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("invalid stored condition duration"))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::ConditionRestriction",
    rename_all = "snake_case"
)]
enum ConditionRestriction {
    VoluntaryActions,
    OrdinaryMovement,
    MovementAbilities,
}
fn condition_restrictions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<tor_simulation::rules::ConditionRestriction>, D::Error> {
    let values = bounded_stored::<_, _, 3>(deserializer)?;
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            return Err(serde::de::Error::custom(
                "duplicate stored condition restriction",
            ));
        }
    }
    Ok(values)
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::ConditionPreparation",
    rename_all = "snake_case"
)]
enum ConditionPreparation {
    Preserve,
    Pause,
}

#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::Stacking",
    tag = "kind",
    rename_all = "snake_case"
)]
enum ConditionStacking {
    Refresh,
    Longest,
    Extend,
    Keep,
    Stack { maximum: u16 },
}
impl ConditionStacking {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::Stacking, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Refresh {},
            Longest {},
            Extend {},
            Keep {},
            Stack { maximum: u16 },
        }
        Ok(match super::named_map(deserializer)? {
            Fields::Refresh {} => tor_simulation::rules::Stacking::Refresh,
            Fields::Longest {} => tor_simulation::rules::Stacking::Longest,
            Fields::Extend {} => tor_simulation::rules::Stacking::Extend,
            Fields::Keep {} => tor_simulation::rules::Stacking::Keep,
            Fields::Stack { maximum } => tor_simulation::rules::Stacking::Stack { maximum },
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::Eviction", rename_all = "snake_case")]
enum ConditionEviction {
    Oldest,
    Reject,
}

// The macro only connects an explicit schema to its runtime type. All fields,
// tags and nested mappings remain visible in the declarations above.
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::NamedDescriptor", deny_unknown_fields)]
struct NamedDescriptor {
    id: tor_simulation::content_ids::DescriptorId,
    name: String,
    #[serde(with = "mapped")]
    resistance: Option<tor_simulation::rules::DescriptorResistance>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::DescriptorResistance",
    deny_unknown_fields
)]
struct DescriptorResistance {
    skill: tor_simulation::attributes::Skill,
    difficulty: i32,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::RulesetDocument", deny_unknown_fields)]
struct RulesDocument {
    id: tor_simulation::content_ids::RulesetId,
    #[serde(with = "mapped")]
    constants: tor_simulation::rules::Constants,
    point_pools: Vec<tor_simulation::rules::PointPoolDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    supplies: Vec<tor_simulation::rules::SupplyDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    abilities: Vec<tor_simulation::rules::AbilityDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    forms: Vec<tor_simulation::rules::FormDef>,
    ability_tags: Vec<tor_simulation::rules::AbilityTagDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    talents: Vec<tor_simulation::rules::TalentDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    expertises: Vec<tor_simulation::expertise::ExpertiseDef>,
    creature_types: Vec<tor_simulation::rules::TypeDef>,
    subtypes: Vec<tor_simulation::rules::SubtypeDef>,
    classes: Vec<tor_simulation::rules::ClassDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    conditions: Vec<tor_simulation::rules::ConditionDef>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    descriptors: Vec<tor_simulation::rules::NamedDescriptor>,
    flags: Vec<tor_simulation::rules::FlagDef>,
    resources: Vec<tor_simulation::rules::ResourceDef>,
    health_recovery: tor_simulation::rules::HealthRecoveryDef,
    #[serde(with = "mapped")]
    nutrition: tor_simulation::nutrition::NutritionPolicy,
    #[serde(with = "mapped")]
    nutrition_condition_trigger: tor_simulation::nutrition::NutritionConditionTrigger,
    #[serde(with = "mapped")]
    advancement: tor_simulation::rules::AdvancementPolicy,
    #[serde(with = "mapped")]
    equipment_slots: Vec<tor_simulation::rules::EquipmentSlotDef>,
    #[serde(with = "mapped")]
    growth: tor_simulation::rules::GrowthPolicy,
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::GrowthSource",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum GrowthSource {
    Racial {},
    Class { class: String },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::FutureGrowth",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum FutureGrowth {
    Disabled {},
    Selected {
        #[serde(with = "mapped")]
        source: crate::creature_authoring::GrowthSource,
    },
    Authored {},
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::ThreatSource",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ThreatSource {
    Expertise {
        id: tor_simulation::content_ids::ExpertiseId,
    },
    Species {
        id: tor_simulation::content_ids::SpeciesId,
    },
    Type {
        id: tor_simulation::content_ids::TypeId,
    },
    Subtype {
        id: tor_simulation::content_ids::SubtypeId,
    },
    Class {
        id: tor_simulation::content_ids::ClassId,
    },
    Template {
        id: tor_simulation::content_ids::TemplateId,
    },
    Talent {
        id: tor_simulation::content_ids::TalentId,
    },
}

#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::GrowthPolicy")]
struct GrowthPolicy {
    threshold_base: u32,
    threshold_per_tier: u32,
    reward_base: u32,
    reward_per_relative_tier: u32,
    minimum_reward: u32,
    maximum_reward: u32,
    recovery: u64,
    #[serde(with = "mapped")]
    threat: tor_simulation::rules::ThreatPolicy,
}
impl GrowthPolicy {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::GrowthPolicy, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            threshold_base: u32,
            threshold_per_tier: u32,
            reward_base: u32,
            reward_per_relative_tier: u32,
            minimum_reward: u32,
            maximum_reward: u32,
            recovery: u64,
            #[serde(with = "mapped")]
            threat: tor_simulation::rules::ThreatPolicy,
        }
        let value: Fields = super::named_map(deserializer)?;
        Ok(tor_simulation::rules::GrowthPolicy {
            threshold_base: value.threshold_base,
            threshold_per_tier: value.threshold_per_tier,
            reward_base: value.reward_base,
            reward_per_relative_tier: value.reward_per_relative_tier,
            minimum_reward: value.minimum_reward,
            maximum_reward: value.maximum_reward,
            recovery: value.recovery,
            threat: value.threat,
        })
    }
}

#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::ThreatPolicy")]
struct ThreatPolicy {
    points_per_tier: u32,
    hit_die: u32,
    trained_rank: u32,
    species: u32,
    creature_type: u32,
    subtype: u32,
    class: u32,
    template: u32,
    talent: u32,
    expertise: u32,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
    )]
    overrides: Vec<tor_simulation::rules::ThreatOverride>,
}
impl ThreatPolicy {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::ThreatPolicy, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            points_per_tier: u32,
            hit_die: u32,
            trained_rank: u32,
            species: u32,
            creature_type: u32,
            subtype: u32,
            class: u32,
            template: u32,
            talent: u32,
            expertise: u32,
            #[serde(
                serialize_with = "mapped::serialize",
                deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
            )]
            overrides: Vec<tor_simulation::rules::ThreatOverride>,
        }
        let value: Fields = super::named_map(deserializer)?;
        Ok(tor_simulation::rules::ThreatPolicy {
            points_per_tier: value.points_per_tier,
            hit_die: value.hit_die,
            trained_rank: value.trained_rank,
            species: value.species,
            creature_type: value.creature_type,
            subtype: value.subtype,
            class: value.class,
            template: value.template,
            talent: value.talent,
            expertise: value.expertise,
            overrides: value.overrides,
        })
    }
}

#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::ThreatOverride")]
struct ThreatOverride {
    #[serde(with = "mapped")]
    source: tor_simulation::rules::ThreatSource,
    points: u32,
}
impl ThreatOverride {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::ThreatOverride, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "mapped")]
            source: tor_simulation::rules::ThreatSource,
            points: u32,
        }
        let value: Fields = super::named_map(deserializer)?;
        Ok(tor_simulation::rules::ThreatOverride {
            source: value.source,
            points: value.points,
        })
    }
}

// Required stored form catalogs and choices have independent named schemas.
// Source authoring's omitted-empty shorthand is never a saved compatibility default.
#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::FormDef")]
struct FormDefinition {
    id: tor_simulation::content_ids::FormId,
    name: String,
    ability: tor_simulation::content_ids::AbilityId,
}
impl FormDefinition {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::rules::FormDef, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: tor_simulation::content_ids::FormId,
            #[serde(deserialize_with = "form_display_name")]
            name: String,
            ability: tor_simulation::content_ids::AbilityId,
        }
        let f: Fields = super::named_map(d)?;
        Ok(tor_simulation::rules::FormDef {
            id: f.id,
            name: f.name,
            ability: f.ability,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::expertise::ExpertiseCategory",
    rename_all = "snake_case"
)]
enum ExpertiseCategory {
    Cultural,
    Equipment,
    Utility,
    Specialist,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::expertise::ExpertiseDef",
    deny_unknown_fields
)]
struct ExpertiseDefinition {
    id: tor_simulation::content_ids::ExpertiseId,
    #[serde(deserialize_with = "form_display_name")]
    name: String,
    #[serde(with = "mapped")]
    category: tor_simulation::expertise::ExpertiseCategory,
}
#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::TalentDef")]
struct TalentDefinition {
    id: tor_simulation::content_ids::TalentId,
    name: String,
    prerequisites: Vec<tor_simulation::content_ids::TalentId>,
    requirement: tor_simulation::rules::Requirement,
    modifiers: Vec<tor_simulation::rules::Modifier>,
    form_choices: Vec<tor_simulation::content_ids::FormId>,
    expertise_grants: Vec<tor_simulation::content_ids::ExpertiseId>,
}
impl TalentDefinition {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::rules::TalentDef, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: tor_simulation::content_ids::TalentId,
            #[serde(deserialize_with = "form_display_name")]
            name: String,
            #[serde(
                deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::MODIFIERS_PER_SOURCE }>"
            )]
            prerequisites: Vec<tor_simulation::content_ids::TalentId>,
            requirement: tor_simulation::rules::Requirement,
            #[serde(
                deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::MODIFIERS_PER_SOURCE }>"
            )]
            modifiers: Vec<tor_simulation::rules::Modifier>,
            #[serde(
                deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::FORMS_PER_TALENT }>"
            )]
            form_choices: Vec<tor_simulation::content_ids::FormId>,
            #[serde(
                deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::MODIFIERS_PER_SOURCE }>"
            )]
            expertise_grants: Vec<tor_simulation::content_ids::ExpertiseId>,
        }
        let f: Fields = super::named_map(d)?;
        Ok(tor_simulation::rules::TalentDef {
            id: f.id,
            name: f.name,
            prerequisites: f.prerequisites,
            requirement: f.requirement,
            modifiers: f.modifiers,
            form_choices: f.form_choices,
            expertise_grants: f.expertise_grants,
        })
    }
}
fn form_display_name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value.trim().is_empty()
        || value.len() > tor_simulation::limits::DISPLAY_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(serde::de::Error::custom(
            "invalid stored form or talent name",
        ));
    }
    Ok(value)
}

#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::AdvancementPolicy")]
struct AdvancementPolicy {
    purchase_recovery: u64,
    respec_prepare: u64,
    respec_recovery: u64,
    received_loss_interval: u64,
    #[serde(with = "mapped")]
    respec_ration: tor_simulation::rules::SupplyCost,
}
impl AdvancementPolicy {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::AdvancementPolicy, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            purchase_recovery: u64,
            respec_prepare: u64,
            respec_recovery: u64,
            received_loss_interval: u64,
            #[serde(with = "mapped")]
            respec_ration: tor_simulation::rules::SupplyCost,
        }
        let value: Fields = super::named_map(deserializer)?;
        Ok(tor_simulation::rules::AdvancementPolicy {
            purchase_recovery: value.purchase_recovery,
            respec_prepare: value.respec_prepare,
            respec_recovery: value.respec_recovery,
            received_loss_interval: value.received_loss_interval,
            respec_ration: value.respec_ration,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::AbilityDef", deny_unknown_fields)]
struct AbilityDefinition {
    #[serde(with = "mapped")]
    interaction: tor_simulation::rules::Interaction,
    #[serde(with = "InvocationKind")]
    invocation: tor_simulation::abilities::InvocationKind,
    id: tor_simulation::content_ids::AbilityId,
    tags: Vec<tor_simulation::content_ids::AbilityTagId>,
    name: String,
    basic_attack: bool,
    #[serde(with = "OutgoingEdgePolicy")]
    outgoing_edge_policy: tor_simulation::rules::OutgoingEdgePolicy,
    #[serde(deserialize_with = "required_option")]
    cost: Option<tor_simulation::rules::Cost>,
    #[serde(deserialize_with = "required_option")]
    cooldown: Option<std::num::NonZeroU64>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::SUPPLY_COSTS }>"
    )]
    supply_costs: Vec<tor_simulation::rules::SupplyCost>,
    timing: tor_simulation::rules::Timing,
    #[serde(with = "AbilityReach")]
    reach: tor_simulation::rules::Reach,
    #[serde(with = "mapped")]
    strikes: tor_simulation::rules::StrikeSource,
    ai: tor_simulation::rules::AiHints,
}

// The current stored rider requires an explicit condition even when authoring
// omits it. These bounded adapters do not accept an earlier source vocabulary.
#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::StrikeSource",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum RulesStrikeSource {
    Authored {
        #[serde(
            serialize_with = "mapped::serialize",
            deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::STRIKES }>"
        )]
        strikes: Vec<tor_simulation::rules::StrikeDef>,
    },
    SelectedMelee,
    Projectile {
        #[serde(with = "RulesProjectileDeliveryKind")]
        delivery: tor_simulation::projectile_profiles::ProjectileDeliveryKind,
    },
    NaturalMelee {
        #[serde(
            serialize_with = "mapped::serialize",
            deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::RIDERS_PER_STRIKE }>"
        )]
        additional_riders: Vec<tor_simulation::rules::RiderDef>,
    },
}
impl RulesStrikeSource {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::rules::StrikeSource, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Authored {
                #[serde(
                    deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::STRIKES }>"
                )]
                strikes: Vec<tor_simulation::rules::StrikeDef>,
            },
            SelectedMelee {},
            Projectile {
                #[serde(with = "RulesProjectileDeliveryKind")]
                delivery: tor_simulation::projectile_profiles::ProjectileDeliveryKind,
            },
            NaturalMelee {
                #[serde(
                    deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::RIDERS_PER_STRIKE }>"
                )]
                additional_riders: Vec<tor_simulation::rules::RiderDef>,
            },
        }
        Ok(match Fields::deserialize(d)? {
            Fields::Authored { strikes } => {
                tor_simulation::rules::validate_strike_shapes(&strikes)
                    .map_err(serde::de::Error::custom)?;
                tor_simulation::rules::StrikeSource::Authored { strikes }
            }
            Fields::SelectedMelee {} => tor_simulation::rules::StrikeSource::SelectedMelee,
            Fields::Projectile { delivery } => {
                tor_simulation::rules::StrikeSource::Projectile { delivery }
            }
            Fields::NaturalMelee { additional_riders } => {
                tor_simulation::rules::validate_rider_shapes(&additional_riders)
                    .map_err(serde::de::Error::custom)?;
                tor_simulation::rules::StrikeSource::NaturalMelee { additional_riders }
            }
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::projectile_profiles::ProjectileDeliveryKind",
    rename_all = "snake_case"
)]
enum RulesProjectileDeliveryKind {
    Throw,
    Launch,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::StrikeDef", deny_unknown_fields)]
struct RulesStrike {
    offset: u64,
    unlock: Option<tor_simulation::content_ids::TalentId>,
    check: Option<tor_simulation::rules::CheckDef>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::EFFECTS_PER_BRANCH }>"
    )]
    on_success: Vec<tor_simulation::rules::Effect>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::EFFECTS_PER_BRANCH }>"
    )]
    on_failure: Vec<tor_simulation::rules::Effect>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
    )]
    descriptors: Vec<tor_simulation::content_ids::DescriptorId>,
    #[serde(
        serialize_with = "mapped::serialize",
        deserialize_with = "bounded_stored::<_, _, { tor_simulation::limits::RIDERS_PER_STRIKE }>"
    )]
    riders: Vec<tor_simulation::rules::RiderDef>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "tor_simulation::rules::RiderDef", deny_unknown_fields)]
struct RulesRider {
    trigger: tor_simulation::rules::RiderTrigger,
    #[serde(with = "mapped")]
    condition: Option<tor_simulation::rules::RiderCondition>,
    check: Option<tor_simulation::rules::RiderCheckDef>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::EFFECTS_PER_BRANCH }>"
    )]
    effects: Vec<tor_simulation::rules::Effect>,
    #[serde(
        deserialize_with = "bounded_plain::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
    )]
    descriptors: Vec<tor_simulation::content_ids::DescriptorId>,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::RiderCondition",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum RulesRiderCondition {
    SequenceSuccessfulHits {
        #[serde(deserialize_with = "sequence_minimum")]
        minimum: u16,
    },
}

#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::OutgoingEdgePolicy",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum OutgoingEdgePolicy {
    Ordinary,
    IgnoreNegative,
}
impl OutgoingEdgePolicy {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::OutgoingEdgePolicy, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Ordinary {},
            IgnoreNegative {},
        }
        Ok(match super::named_map(deserializer)? {
            Fields::Ordinary {} => tor_simulation::rules::OutgoingEdgePolicy::Ordinary,
            Fields::IgnoreNegative {} => tor_simulation::rules::OutgoingEdgePolicy::IgnoreNegative,
        })
    }
}

#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::Reach",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum AbilityReach {
    Melee,
    Projectile,
    Sight {
        range: u16,
    },
    MutualSight {
        range: u16,
    },
    SelfOnly,
    SelfOrAlly {
        #[serde(with = "AllyRange")]
        range: tor_simulation::rules::AllyRange,
    },
}
impl AbilityReach {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::Reach, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Melee {},
            Projectile {},
            Sight {
                range: u16,
            },
            MutualSight {
                range: u16,
            },
            SelfOnly {},
            SelfOrAlly {
                #[serde(with = "AllyRange")]
                range: tor_simulation::rules::AllyRange,
            },
        }
        Ok(match super::named_map(deserializer)? {
            Fields::Melee {} => tor_simulation::rules::Reach::Melee,
            Fields::Projectile {} => tor_simulation::rules::Reach::Projectile,
            Fields::Sight { range } => tor_simulation::rules::Reach::Sight { range },
            Fields::MutualSight { range } => tor_simulation::rules::Reach::MutualSight { range },
            Fields::SelfOnly {} => tor_simulation::rules::Reach::SelfOnly,
            Fields::SelfOrAlly { range } => tor_simulation::rules::Reach::SelfOrAlly { range },
        })
    }
}
#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::rules::AllyRange",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum AllyRange {
    Touch,
    Sight { range: u16 },
}

impl AllyRange {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::AllyRange, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Touch {},
            Sight { range: u16 },
        }
        Ok(match super::named_map(deserializer)? {
            Fields::Touch {} => tor_simulation::rules::AllyRange::Touch,
            Fields::Sight { range } => tor_simulation::rules::AllyRange::Sight { range },
        })
    }
}

#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::SupplyDef", deny_unknown_fields)]
struct SupplyDefinition {
    #[serde(with = "supply_id")]
    id: tor_simulation::content_ids::SupplyId,
    name: String,
}

#[derive(Serialize)]
#[serde(remote = "tor_simulation::rules::SupplyCost", deny_unknown_fields)]
struct SupplyCost {
    #[serde(with = "supply_id")]
    supply: tor_simulation::content_ids::SupplyId,
    quantity: u64,
    #[serde(with = "SupplyAccess")]
    access: tor_simulation::rules::SupplyAccess,
    #[serde(with = "SupplySpendAt")]
    spend_at: tor_simulation::rules::SupplySpendAt,
}

impl SupplyDefinition {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::SupplyDef, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "supply_id")]
            id: tor_simulation::content_ids::SupplyId,
            name: String,
        }
        let value: Fields = super::named_map(deserializer)?;
        Ok(tor_simulation::rules::SupplyDef {
            id: value.id,
            name: value.name,
        })
    }
}
impl SupplyCost {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::rules::SupplyCost, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "supply_id")]
            supply: tor_simulation::content_ids::SupplyId,
            quantity: u64,
            #[serde(with = "SupplyAccess")]
            access: tor_simulation::rules::SupplyAccess,
            #[serde(with = "SupplySpendAt")]
            spend_at: tor_simulation::rules::SupplySpendAt,
        }
        let value: Fields = super::named_map(deserializer)?;
        Ok(tor_simulation::rules::SupplyCost {
            supply: value.supply,
            quantity: value.quantity,
            access: value.access,
            spend_at: value.spend_at,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::SupplyAccess",
    rename_all = "snake_case"
)]
enum SupplyAccess {
    CarriedOnly,
    CarriedOrGroundTouch,
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "tor_simulation::rules::SupplySpendAt",
    rename_all = "snake_case"
)]
enum SupplySpendAt {
    FirstResolution,
    ReplacementCommit,
}
mod supply_id {
    use serde::{Deserialize, Serializer};
    pub(super) fn serialize<S: Serializer>(
        value: &tor_simulation::content_ids::SupplyId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(value.as_str())
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::content_ids::SupplyId, D::Error> {
        let value = String::deserialize(deserializer)?;
        tor_simulation::content_ids::SupplyId::new(value)
            .map_err(|_| serde::de::Error::custom("invalid stored supply ID"))
    }
}

impl Stored for source::AnatomySpec {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AnatomySpec::serialize(self, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(transparent)]
        struct Named(#[serde(with = "AnatomySpec")] source::AnatomySpec);
        let value: Named = super::named_map(d)?;
        if matches!(value.0.primary_attack_socket, source::PrimaryAttackSocket::Socket { slot } if usize::from(slot) >= value.0.slots.len())
        {
            return Err(serde::de::Error::custom(
                "stored primary socket outside anatomy",
            ));
        }
        Ok(value.0)
    }
}
impl Stored for tor_simulation::AnatomySpec {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RulesAnatomySpec::serialize(self, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(transparent)]
        struct Named(#[serde(with = "RulesAnatomySpec")] tor_simulation::AnatomySpec);
        let value: Named = super::named_map(d)?;
        if value
            .0
            .primary_attack_socket
            .socket()
            .is_some_and(|slot| usize::from(slot.0) >= value.0.slots.len())
        {
            return Err(serde::de::Error::custom(
                "stored primary socket outside anatomy",
            ));
        }
        Ok(value.0)
    }
}

macro_rules! remote {
    ($($schema:ident => $value:ty),+ $(,)?) => {$(
        impl Stored for $value {
            fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                $schema::serialize(self, serializer)
            }
            fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $schema::deserialize(deserializer)
            }
        }
    )+};
}
impl Stored for crate::creature_authoring::MovementChance {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MovementChance::serialize(self, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = crate::creature_authoring::MovementChance;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a bounded named movement probability")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                fields: A,
            ) -> Result<Self::Value, A::Error> {
                let value = MovementChance::deserialize(
                    serde::de::value::MapAccessDeserializer::new(fields),
                )?;
                if value.denominator == 0 || value.numerator > value.denominator {
                    return Err(serde::de::Error::custom("invalid stored movement chance"));
                }
                Ok(value)
            }
        }
        deserializer.deserialize_map(Named)
    }
}
impl Stored for crate::creature_authoring::Natural {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Natural::serialize(self, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let attack = Natural::deserialize(deserializer)?;
        if !attack.valid_layout() {
            return Err(serde::de::Error::custom(
                "invalid stored natural strike layout",
            ));
        }
        Ok(attack)
    }
}
// Guard the changed enclosing record without duplicating its source-owned
// remote field schema or using the simulation's serializer as saved authority.
impl Stored for tor_simulation::rules::RulesetDocument {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RulesDocument::serialize(self, serializer)
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NamedRules;
        impl<'de> serde::de::Visitor<'de> for NamedRules {
            type Value = tor_simulation::rules::RulesetDocument;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a named stored rules document")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                fields: A,
            ) -> Result<Self::Value, A::Error> {
                RulesDocument::deserialize(serde::de::value::MapAccessDeserializer::new(fields))
            }
        }
        deserializer.deserialize_map(NamedRules)
    }
}

// Changed source and policy records reject positional aliases as well as
// unknown/missing fields. The remote DTO remains the save-owned schema.
macro_rules! named_remote {
    ($($schema:ident => $value:ty),+ $(,)?) => {$(
        impl Stored for $value {
            fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                $schema::serialize(self, serializer)
            }
            fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Named;
                impl<'de> serde::de::Visitor<'de> for Named {
                    type Value = $value;
                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("a named stored source record")
                    }
                    fn visit_map<A: serde::de::MapAccess<'de>>(self, fields: A) -> Result<Self::Value, A::Error> {
                        $schema::deserialize(serde::de::value::MapAccessDeserializer::new(fields))
                    }
                }
                deserializer.deserialize_map(Named)
            }
        }
    )+};
}
named_remote!(
    SpatialScopeSource => source::spatial_authoring::AuthoredSpatialScope,
    SpatialRoomVolume => source::spatial_authoring::RoomVolume,
    SpatialSourceRun => source::spatial_authoring::SourceRun,
    SpatialOwnedRun => source::spatial_authoring::OwnedRun,
    TrapKind => source::TrapKind,
    NamedDescriptor => tor_simulation::rules::NamedDescriptor,
    DescriptorResistance => tor_simulation::rules::DescriptorResistance,
    NaturalDamage => crate::creature_authoring::Damage,
    NaturalComponent => crate::creature_authoring::Component,
    NaturalDamageKey => crate::creature_authoring::DamageKey,
    NaturalAmount => crate::creature_authoring::Amount,
    LauncherProfile => source::LauncherProfile,
    ProjectileSpec => source::ProjectileSpec,
    WeaponTrait => source::WeaponTrait,
    WeaponTraitChanges => source::WeaponTraitChanges,
    WeaponProfileSpec => source::WeaponProfileSpec,
    SourcePrimaryAttackSocket => source::PrimaryAttackSocket,
    RulesConstants => tor_simulation::rules::Constants,
    RulesEquipmentSlotDef => tor_simulation::rules::EquipmentSlotDef,
    ExpertiseDefinition => tor_simulation::expertise::ExpertiseDef,
    EquippedSelection => source::EquippedSelection,
    ConsumableSpec => source::ConsumableSpec,
    ReadableSpec => source::ReadableSpec,
    ControlSpec => tor_simulation::control_scrolls::ControlSpec,
    ConditionPayload => tor_simulation::control_scrolls::ConditionPayload,
    DeviceSpec => source::DeviceSpec,
    ChargeRange => source::ChargeRange,
    EquipmentGeneration => source::EquipmentGeneration,
    PeriodicRelocation => source::PeriodicRelocation,
    Purchase => crate::creature_authoring::Purchase,
    EquipmentSpec => source::EquipmentSpec,
    ArmorSpec => source::ArmorSpec,
    Perception => crate::creature_authoring::Perception,
    GuiseAppearance => crate::creature_authoring::GuiseAppearance,
    Locomotion => crate::creature_authoring::Locomotion,
    Rider => crate::creature_authoring::Rider,
    RiderCondition => crate::creature_authoring::RiderCondition,
    RulesStrikeSource => tor_simulation::rules::StrikeSource,
    RulesStrike => tor_simulation::rules::StrikeDef,
    RulesRider => tor_simulation::rules::RiderDef,
    RulesRiderCondition => tor_simulation::rules::RiderCondition,
    UniqueRetrieval => source::UniqueRetrieval,
    BehaviorPolicy => crate::creature_authoring::BehaviorPolicy,
    AiProfile => source::AiProfile,
    Species => crate::creature_authoring::Species,
    AbilityDefinition => tor_simulation::rules::AbilityDef,
    ConditionDefinition => tor_simulation::rules::ConditionDef,
    ConditionTiming => tor_simulation::rules::ConditionTiming,
    EdibleSpec => source::EdibleSpec,
    EdibleAmountExpr => tor_simulation::rules::AmountExpr,
    EdibleDiceExpr => tor_simulation::rules::DiceExpr,
    NutritionRequirement => tor_simulation::nutrition::NutritionRequirement,
    NutritionPolicy => tor_simulation::nutrition::NutritionPolicy,
    RegenerationCost => tor_simulation::nutrition::RegenerationCost,
    NutritionChance => tor_simulation::nutrition::NutritionChance,
    NutritionConditionTrigger => tor_simulation::nutrition::NutritionConditionTrigger,
    RecoveryNutrition => tor_simulation::health_recovery::RecoveryNutrition,
    ThreatSource => tor_simulation::rules::ThreatSource,
    GrowthSource => crate::creature_authoring::GrowthSource,
    FutureGrowth => crate::creature_authoring::FutureGrowth,
    BuildSpec => crate::creature_authoring::BuildSpec,
    ScenarioCapabilities => source::ScenarioCapabilities,
    FloorActivation => source::FloorActivation,
    PopulationProfile => source::PopulationProfile,
    PopulationStage => source::PopulationStage,
    PopulationQuantityExpr => source::PopulationQuantityExpr,
    TheftSelection => source::TheftSelection,
    RelocationGate => source::RelocationGate,
    PopulationDepth => source::PopulationDepth,
    PopulationDepthGrowth => source::PopulationDepthGrowth,
    PopulationBand => source::PopulationBand,
    PopulationChoice => source::PopulationChoice,
    ActorPopulationBand => source::ActorPopulationBand,
    ActorPopulationChoice => source::ActorPopulationChoice,
    PopulationAdvancement => source::PopulationAdvancement,
    CarriedItemPool => source::CarriedItemPool,
    CarriedItemChoice => source::CarriedItemChoice,
    CarriedLoot => source::CarriedLoot,
    CarryChance => source::CarryChance,
    CharacterLoadout => source::CharacterLoadout,
    RegionDef => source::RegionDef,
    IndexedRegion => source::IndexedRegion,
    Door => source::Door,
    Trap => source::Trap,
    Concealment => source::Concealment,
    Operation => crate::generation_recipe::Operation,
    RoomOmissions => crate::generation_recipe::RoomOmissions,
    MazeRooms => crate::generation_recipe::MazeRooms,
    SecretEntrances => crate::generation_recipe::SecretEntrances,
    HiddenTraps => crate::generation_recipe::HiddenTraps,
    HazardTraps => crate::generation_recipe::HazardTraps,
    TrapAnchor => tor_world::NamedAnchor,
);

// String-valued condition enums cannot acquire externally tagged object aliases.
macro_rules! string_remote {
    ($($schema:ident => $value:ty),+ $(,)?) => {$(
        impl Stored for $value {
            fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                $schema::serialize(self, serializer)
            }
            fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                $schema::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
            }
        }
    )+};
}
string_remote!(
    ActionDuration => crate::creature_authoring::ActionDuration,
    WeaponTraitKind => source::WeaponTraitKind,
    ExpertiseCategory => tor_simulation::expertise::ExpertiseCategory,
    HealingOverflow => source::HealingOverflow,
    ItemDiscovery => source::ItemDiscovery,
    EquipmentAlteration => source::EquipmentAlteration,
    ArmorMaterial => source::ArmorMaterial,
    BehaviorDisposition => crate::creature_authoring::Disposition,
    BehaviorIdle => crate::creature_authoring::IdleBehavior,
    AbilityInteraction => tor_simulation::rules::Interaction,
    ConditionLifecycle => tor_simulation::rules::ConditionLifecycle,
    ConditionRestriction => tor_simulation::rules::ConditionRestriction,
    ConditionPreparation => tor_simulation::rules::ConditionPreparation,
    ConditionEviction => tor_simulation::rules::Eviction,
);

remote!(
    ConditionExpiry => tor_simulation::rules::ConditionExpiry,
    ConditionStacking => tor_simulation::rules::Stacking,
    FormDefinition => tor_simulation::rules::FormDef,
    TalentDefinition => tor_simulation::rules::TalentDef,
    GrowthPolicy => tor_simulation::rules::GrowthPolicy,
    ThreatPolicy => tor_simulation::rules::ThreatPolicy,
    ThreatOverride => tor_simulation::rules::ThreatOverride,
    AdvancementPolicy => tor_simulation::rules::AdvancementPolicy,
    SupplyDefinition => tor_simulation::rules::SupplyDef,
    SupplyCost => tor_simulation::rules::SupplyCost,
    CellLight => source::CellLight,
    Gravity => source::Gravity,
    NamedPlace => source::NamedPlace,
    Portal => source::Portal,
    Item => source::Item,
    Actor => source::Actor,
    Generate => crate::generator::Generate,
    ActorPool => crate::generator::ActorPool,
    ItemPool => crate::generator::ItemPool,
    WorldFamily => crate::procedural_world::WorldFamily,
    PlaceDef => source::PlaceDef,
    Domain => crate::procedural_world::Domain,
    FamilyKind => crate::procedural_world::FamilyKind,
    GenerationGroup => crate::generation_recipe::GenerationGroup,
    Recipe => crate::generation_recipe::Recipe,
    Stage => crate::generation_recipe::Stage,
    Scenario => crate::Scenario,
    ActorSetup => crate::ActorSetup,
    Position => crate::journal::Position,
    Streaming => crate::Streaming,
    Manifest => source::Manifest,
    Protagonist => source::Protagonist,
    StairPair => source::StairPair,
    Zone => source::Zone,
    TerrainAssets => source::TerrainAssets,
    AppearancePool => source::AppearancePool,
    Archetype => source::Archetype,
    CurrencyKind => source::CurrencyKind,
    RelocationPolicy => source::RelocationPolicy,
    ItemClass => crate::scenario_package::ItemClass,
    Character => source::Character,
    Objective => source::Objective,
    DiceExpr => source::DiceExpr,
    AmountExpr => source::AmountExpr,
    DamageComponentExpr => source::DamageComponentExpr,
    DamageExpr => source::DamageExpr,
    Effect => source::Effect,
    HoldPolicy => source::HoldPolicy,
    CreatureCatalog => crate::creature_authoring::Catalog,
    DisplacementProfile => crate::creature_authoring::DisplacementProfile,
    Template => crate::creature_authoring::Template,
    Melee => crate::creature_authoring::Melee,
    Strike => crate::creature_authoring::Strike,
    StrikeCheck => crate::creature_authoring::Check,
    RiderCheck => crate::creature_authoring::RiderCheck,
    CheckActor => crate::creature_authoring::CheckActor,
    SkillSelector => crate::creature_authoring::SkillSelector,
    StrikeAgainst => crate::creature_authoring::Against,
    StrikeFormula => crate::creature_authoring::Formula,
    Modifier => crate::creature_authoring::Modifier,
    ModifierScope => crate::creature_authoring::Scope,
    ModifierOp => crate::creature_authoring::Op,
    ModifierStat => crate::creature_authoring::Stat,
    ModifierSelector => crate::creature_authoring::Selector,
    Advancement => crate::creature_authoring::Advancement,
    PurchaseSelection => crate::creature_authoring::PurchaseSelection,
    Restoration => crate::creature_authoring::Restoration,
    BodySpec => source::BodySpec,
    DamageType => source::DamageType,
    Certificate => source::Certificate,
    RegionIndex => source::RegionIndex,
    IndexedActor => source::IndexedActor,
    IndexedItem => source::IndexedItem,
);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    #[serde(with = "mapped")]
    manifest: source::Manifest,
    rules_document: String,
    #[serde(with = "mapped")]
    certificate: source::Certificate,
    validated: bool,
    selected: u64,
    directory: Option<String>,
}

impl Stored for source::Package {
    fn encode<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut value = serializer.serialize_struct("Package", 6)?;
        value.serialize_field("manifest", &Borrowed(&self.manifest))?;
        value.serialize_field("rules_document", &self.rules_document)?;
        value.serialize_field("certificate", &Borrowed(&self.certificate))?;
        value.serialize_field("validated", &self.validated)?;
        value.serialize_field("selected", &self.selected)?;
        value.serialize_field("directory", &self.directory)?;
        value.end()
    }
    fn decode<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Package::deserialize(deserializer)?;
        Ok(Self::from_saved_metadata(
            value.manifest,
            value.rules_document,
            value.certificate,
            value.validated,
            value.selected,
            value.directory,
        ))
    }
}

// Package metadata is embedded in the base journal frame; keep a fixed
// envelope reserve without raising that frame's capacity. The index is chunked.
pub(crate) const MAX_SAVE_METADATA_BYTES: usize = crate::storage::MAX_PAYLOAD - 64 * 1024;
pub(crate) const MAX_COMPILED_PACKAGE_BYTES: u64 =
    source::MAX_INDEX_BYTES + MAX_SAVE_METADATA_BYTES as u64 + 4096;
pub(crate) const MAX_RULES_INPUT_BYTES: u64 = 8 * 1024 * 1024;

/// The rules vocabulary is owned by simulation; this versioned compiled
/// document envelope pins its persistence contract independently of TOML.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesInput {
    format: u32,
    #[serde(with = "mapped")]
    document: tor_simulation::rules::RulesetDocument,
}
const RULES_INPUT_FORMAT: u32 = 24;
pub(crate) fn encode_rules_document(
    document: &tor_simulation::rules::RulesetDocument,
) -> Result<String, crate::Failure> {
    // Catalog declaration order is not engine order: runtime lookups use stable
    // identities, resource writes use disjoint named slots, and talent order is
    // topological. Nested effect/modifier/requirement sequences stay ordered.
    let mut document = document.clone();
    macro_rules! catalog { ($($field:ident),+) => { $(document.$field.sort_by(|a, b| a.id.cmp(&b.id));)+ }; }
    catalog!(
        point_pools,
        supplies,
        abilities,
        forms,
        ability_tags,
        talents,
        creature_types,
        subtypes,
        classes,
        conditions,
        descriptors,
        flags,
        equipment_slots
    );
    document
        .resources
        .sort_by_key(|definition| definition.resource);
    #[derive(Serialize)]
    struct Input<'a> {
        format: u32,
        #[serde(serialize_with = "RulesDocument::serialize")]
        document: &'a tor_simulation::rules::RulesetDocument,
    }
    let text = serde_json::to_string(&Input {
        format: RULES_INPUT_FORMAT,
        document: &document,
    })
    .map_err(|_| storage_failure())?;
    if text.len() as u64 > MAX_RULES_INPUT_BYTES {
        return Err(storage_failure());
    }
    Ok(text)
}
pub(crate) fn decode_rules_document(
    text: &str,
) -> Result<tor_simulation::rules::RulesetDocument, crate::Failure> {
    if text.len() as u64 > MAX_RULES_INPUT_BYTES {
        return Err(invalid_archive());
    }
    let input: RulesInput = crate::storage::codec::strict(text.as_bytes())?;
    if input.format != RULES_INPUT_FORMAT || encode_rules_document(&input.document)? != text {
        return Err(invalid_archive());
    }
    Ok(input.document)
}

/// Save-owned, versioned canonical compiled inputs. No authoring parser is used.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegionInput {
    format: u32,
    #[serde(with = "mapped")]
    region: source::RegionDef,
}
const REGION_INPUT_FORMAT: u32 = 8;
pub(crate) fn encode_region_input(region: &source::RegionDef) -> Result<String, crate::Failure> {
    #[derive(Serialize)]
    struct Input<'a> {
        format: u32,
        region: Borrowed<'a, source::RegionDef>,
    }
    let text = serde_json::to_string(&Input {
        format: REGION_INPUT_FORMAT,
        region: Borrowed(region),
    })
    .map_err(|_| storage_failure())?;
    if text.len() as u64 > source::MAX_REGION_BYTES {
        return Err(storage_failure());
    }
    Ok(text)
}
pub(crate) fn decode_region_input(text: &str) -> Result<source::RegionDef, crate::Failure> {
    if text.len() as u64 > source::MAX_REGION_BYTES {
        return Err(invalid_archive());
    }
    let input: RegionInput = crate::storage::codec::strict(text.as_bytes())?;
    if input.format != REGION_INPUT_FORMAT {
        return Err(invalid_archive());
    }
    Ok(input.region)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContentPackage {
    format: u32,
    #[serde(with = "mapped")]
    package: source::Package,
    #[serde(with = "mapped")]
    index: source::RegionIndex,
}
const CONTENT_PACKAGE_FORMAT: u32 = 28;
pub(crate) fn check_metadata_capacity(package: &source::Package) -> Result<(), crate::Failure> {
    let bytes = serde_json::to_vec(&Borrowed(package)).map_err(|_| storage_failure())?;
    if bytes.len() > MAX_SAVE_METADATA_BYTES {
        return Err(crate::Failure::new(
            tor_protocol::ErrorCode::InvalidAction,
            "Compiled scenario metadata exceeds the save metadata limit (960 KiB); reduce catalogs, rules or procedural descriptors",
        ));
    }
    Ok(())
}

pub(crate) fn encode_content_package(package: &source::Package) -> Result<Vec<u8>, crate::Failure> {
    check_metadata_capacity(package)?;
    let mut package = package.clone();
    package.directory = None;
    let index = (*package.index).clone();
    let bytes = serde_json::to_vec(&ContentPackage {
        format: CONTENT_PACKAGE_FORMAT,
        package,
        index,
    })
    .map_err(|_| storage_failure())?;
    if bytes.len() as u64 > MAX_COMPILED_PACKAGE_BYTES {
        return Err(storage_failure());
    }
    Ok(bytes)
}
pub(crate) fn decode_content_package(bytes: &[u8]) -> Result<source::Package, crate::Failure> {
    if bytes.len() as u64 > MAX_COMPILED_PACKAGE_BYTES {
        return Err(invalid_archive());
    }
    let mut value: ContentPackage = crate::storage::codec::strict(bytes)?;
    if value.format != CONTENT_PACKAGE_FORMAT
        || value.package.directory.is_some()
        || !value
            .index
            .regions
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    {
        return Err(invalid_archive());
    }
    value.package.index = Arc::new(value.index);
    value.package.check_identity()?;
    Ok(value.package)
}

pub(crate) fn serialize<S: serde::Serializer>(
    value: &crate::Scenario,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.encode(serializer)
}
pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<crate::Scenario, D::Error> {
    crate::Scenario::decode(deserializer)
}

pub(crate) fn encode_index(index: &source::RegionIndex) -> Result<Vec<u8>, crate::Failure> {
    let mut bytes = serde_json::to_vec(&Borrowed(index)).map_err(|_| storage_failure())?;
    bytes.push(b'\n');
    Ok(bytes)
}
pub(crate) fn decode_index(bytes: &[u8]) -> Result<source::RegionIndex, crate::Failure> {
    if bytes.len() as u64 > source::MAX_INDEX_BYTES {
        return Err(invalid_archive());
    }
    let Owned(index) = crate::storage::codec::strict::<Owned<source::RegionIndex>>(bytes)?;
    if !index.regions.windows(2).all(|pair| pair[0].id < pair[1].id) {
        return Err(invalid_archive());
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    #[test]
    fn stored_action_duration_modifier_roundtrips_exact_named_vocabulary() {
        use super::{Borrowed, Owned};
        for rate in ["normal", "haste", "slow"] {
            let value =
                serde_json::json!({"scope":"always","op":{"kind":"action_duration","value":rate}});
            let decoded: Owned<crate::creature_authoring::Modifier> =
                serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), value);
            assert!(decoded.0.compile().is_ok());
            for op in [
                serde_json::json!({"kind":"action_duration"}),
                serde_json::json!({"kind":"action_duration","value":"unknown"}),
                serde_json::json!({"kind":"action_duration","value":{(rate):null}}),
                serde_json::json!({"kind":"action_duration","value":[rate]}),
                serde_json::json!({"kind":"action_duration","value":null}),
                serde_json::json!({"kind":"action_duration","value":true}),
                serde_json::json!({"kind":"action_duration","value":1}),
                serde_json::json!({"kind":"action_duration","value":rate,"extra":true}),
            ] {
                let mut invalid = value.clone();
                invalid["op"] = op;
                assert!(
                    serde_json::from_value::<Owned<crate::creature_authoring::Modifier>>(invalid)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn stored_locomotion_requires_nullable_chance_and_bounded_named_fraction() {
        use super::{Borrowed, Owned};
        use serde_json::json;
        type Value = crate::creature_authoring::Locomotion;
        for value in [
            json!({"kind":"confused","chance":null}),
            json!({"kind":"confused","chance":{"numerator":4,"denominator":5}}),
            json!({"kind":"erratic","chance":null}),
            json!({"kind":"erratic","chance":{"numerator":1,"denominator":5}}),
        ] {
            let decoded: Owned<Value> = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), value);
        }
        for value in [
            json!({"kind":"confused"}),
            json!({"kind":"confused","chance":[4,5]}),
            json!({"kind":"confused","chance":{"numerator":5,"denominator":4}}),
            json!({"kind":"confused","chance":null,"enabled":true}),
            json!({"kind":"erratic"}),
            json!({"kind":"erratic","chance":[1,5]}),
            json!({"kind":"erratic","chance":{"numerator":2,"denominator":1}}),
            json!({"kind":"erratic","chance":{"numerator":0,"denominator":0}}),
            json!({"kind":"erratic","chance":{"numerator":1,"denominator":65536}}),
            json!({"kind":"erratic","chance":null,"enabled":true}),
            json!(["flying", true]),
        ] {
            assert!(serde_json::from_value::<Owned<Value>>(value).is_err());
        }
    }

    #[test]
    fn stored_mutual_sight_requires_exact_named_range() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Record {
            #[serde(with = "super::AbilityReach")]
            reach: tor_simulation::rules::Reach,
        }
        for reach in [
            serde_json::json!({"kind":"melee"}),
            serde_json::json!({"kind":"sight","range":6}),
            serde_json::json!({"kind":"mutual_sight","range":4}),
            serde_json::json!({"kind":"self_only"}),
            serde_json::json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
            serde_json::json!({"kind":"self_or_ally","range":{"kind":"sight","range":3}}),
        ] {
            let good = serde_json::json!({"reach":reach});
            let decoded: Record = serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), good);
        }
        for reach in [
            serde_json::json!({"kind":"mutual_sight"}),
            serde_json::json!({"kind":"mutual_sight","range":4,"awareness":true}),
            serde_json::json!(["mutual_sight", 4]),
            serde_json::json!(["melee"]),
            serde_json::json!(["sight", 6]),
            serde_json::json!(["self_only"]),
            serde_json::json!(["self_or_ally", {"kind":"touch"}]),
            serde_json::json!({"kind":"melee","range":1}),
            serde_json::json!({"kind":"self_only","range":0}),
            serde_json::json!({"kind":"sight"}),
            serde_json::json!({"kind":"self_or_ally"}),
            serde_json::json!({"kind":"self_or_ally","range":["touch"]}),
            serde_json::json!({"kind":"self_or_ally","range":["sight",3]}),
            serde_json::json!({"kind":"self_or_ally","range":{"kind":"touch","range":1}}),
            serde_json::json!({"kind":"self_or_ally","range":{"kind":"sight"}}),
        ] {
            assert!(
                serde_json::from_value::<Record>(serde_json::json!({"reach":reach})).is_err(),
                "malformed stored reach accepted: {reach}"
            );
        }
    }

    #[test]
    fn sequence_rider_stored_source_and_rules_require_independent_bounded_conditions() {
        use super::{Borrowed, Owned};
        use serde_json::json;
        let good = json!({"trigger":"success",
            "condition":{"kind":"sequence_successful_hits","minimum":2},
            "check":null,"effects":[],"descriptors":[]});
        for value in [good.clone(), {
            let mut ordinary = good.clone();
            ordinary["condition"] = json!(null);
            ordinary
        }] {
            let source: Owned<crate::creature_authoring::Rider> =
                serde_json::from_value(value.clone()).unwrap();
            let rules: Owned<tor_simulation::rules::RiderDef> =
                serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&source.0)).unwrap(), value);
            assert_eq!(serde_json::to_value(Borrowed(&rules.0)).unwrap(), value);
        }
        let mut malformed = Vec::new();
        let mut omitted = good.clone();
        omitted.as_object_mut().unwrap().remove("condition");
        malformed.push(omitted);
        for condition in [
            json!({"kind":"sequence_successful_hits"}),
            json!({"kind":"sequence_successful_hits","minimum":2,"group":"arbitrary"}),
            json!({"kind":"any_effect","minimum":2}),
            json!(["sequence_successful_hits", 2]),
        ]
        .into_iter()
        .chain(
            [0, 17, 65536]
                .map(|minimum| json!({"kind":"sequence_successful_hits","minimum":minimum})),
        ) {
            let mut bad = good.clone();
            bad["condition"] = condition;
            malformed.push(bad);
        }
        for bad in malformed {
            assert!(
                serde_json::from_value::<Owned<crate::creature_authoring::Rider>>(bad.clone())
                    .is_err()
            );
            assert!(serde_json::from_value::<Owned<tor_simulation::rules::RiderDef>>(bad).is_err());
        }
        let mut failed_gate = good;
        failed_gate["trigger"] = json!("failure");
        let source = json!({"kind":"natural_melee","additional_riders":[failed_gate]});
        assert!(
            serde_json::from_value::<Owned<tor_simulation::rules::StrikeSource>>(source).is_err()
        );
    }

    #[test]
    fn sequence_rider_stored_conditions_reject_duplicate_and_positional_records() {
        use super::Owned;
        let good = r#"{"trigger":"success","condition":{"kind":"sequence_successful_hits","minimum":2},"check":null,"effects":[],"descriptors":[]}"#;
        let duplicates = [
            good.replacen("\"condition\":", "\"condition\":null,\"condition\":", 1),
            good.replace("\"minimum\":2", "\"minimum\":2,\"minimum\":3"),
            good.replace(
                "\"kind\":\"sequence_successful_hits\"",
                "\"kind\":\"sequence_successful_hits\",\"kind\":\"sequence_successful_hits\"",
            ),
        ];
        for bad in duplicates {
            assert!(serde_json::from_str::<Owned<crate::creature_authoring::Rider>>(&bad).is_err());
            assert!(serde_json::from_str::<Owned<tor_simulation::rules::RiderDef>>(&bad).is_err());
        }
        let positional =
            r#"["success",{"kind":"sequence_successful_hits","minimum":2},null,[],[]]"#;
        assert!(
            serde_json::from_str::<Owned<crate::creature_authoring::Rider>>(positional).is_err()
        );
        assert!(
            serde_json::from_str::<Owned<tor_simulation::rules::RiderDef>>(positional).is_err()
        );
    }

    #[test]
    fn stored_generation_operations_reject_positional_outer_aliases() {
        use super::{Borrowed, Owned};
        type Operation = crate::generation_recipe::Operation;
        // Unit, ordinary field-bearing and new nested-config variants all use
        // the same independent save-owned tagged-map boundary.
        for (good, positional) in [
            (r#"{"kind":"stone_fill"}"#, r#"["stone_fill"]"#),
            (
                r#"{"kind":"rooms","width":[4,6],"height":[3,5]}"#,
                r#"["rooms",[4,6],[3,5]]"#,
            ),
            (
                r#"{"kind":"room_omissions","config":{"count":[0,3]}}"#,
                r#"["room_omissions",{"count":[0,3]}]"#,
            ),
        ] {
            let Owned(operation) =
                crate::storage::codec::strict::<Owned<Operation>>(good.as_bytes()).unwrap();
            assert_eq!(
                serde_json::to_value(Borrowed(&operation)).unwrap(),
                serde_json::from_str::<serde_json::Value>(good).unwrap()
            );
            assert!(serde_json::from_str::<Owned<Operation>>(positional).is_err());
            assert!(
                crate::storage::codec::strict::<Owned<Operation>>(positional.as_bytes()).is_err()
            );
        }
    }

    #[test]
    fn stored_room_omissions_require_exact_named_config() {
        use super::{Borrowed, Owned};
        type Operation = crate::generation_recipe::Operation;
        let good = r#"{"kind":"room_omissions","config":{"count":[0,3]}}"#;
        let Owned(operation) = serde_json::from_str::<Owned<Operation>>(good).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&operation)).unwrap(),
            serde_json::from_str::<serde_json::Value>(good).unwrap()
        );
        // Each negative starts from the independently authored accepted shape.
        for bad in [
            r#"{"kind":"room_omissions"}"#,
            r#"{"kind":"room_omissions","config":null}"#,
            r#"{"kind":"room_omissions","config":{}}"#,
            r#"{"kind":"room_omissions","config":{"count":[0,3],"surprise":0}}"#,
            r#"{"kind":"room_omissions","config":{"count":[0,3]},"surprise":0}"#,
            r#"{"kind":"room_omissions","config":[[0,3]]}"#,
            r#"["room_omissions",{"count":[0,3]}]"#,
            r#"{"kind":"room_omissions","config":{"count":[0]}}"#,
            r#"{"kind":"room_omissions","config":{"count":[0,3,4]}}"#,
            r#"{"kind":"room_omissions","config":{"count":[false,3]}}"#,
            r#"{"kind":"room_omissions","config":{"count":[-1,3]}}"#,
            r#"{"kind":"room_omissions","config":{"count":[0,4294967296]}}"#,
            r#"{"kind":"room_omissions","config":{"count":[0,3],"count":[0,3]}}"#,
            r#"{"kind":"room_omissions","config":{"count":[0,3]},"config":{"count":[0,3]}}"#,
            r#"{"kind":"room_omissions","kind":"room_omissions","config":{"count":[0,3]}}"#,
        ] {
            assert!(
                serde_json::from_str::<Owned<Operation>>(bad).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn stored_room_omission_recipe_preserves_bounds_and_stage_identity() {
        use super::{Borrowed, Owned};
        use crate::generation_recipe::{check, Recipe};
        let good = serde_json::json!({"version":4,"stages":[
            {"id":"stone","version":1,"operation":{"kind":"stone_fill"}},
            {"id":"slots","version":1,"operation":{"kind":"grid_partition"}},
            {"id":"rooms","version":1,"operation":{"kind":"rooms","width":[4,6],"height":[3,5]}},
            {"id":"connections","version":1,"operation":{"kind":"connected_graph","extra":[0,1]}},
            {"id":"corridors","version":1,"operation":{"kind":"corridors"}},
            {"id":"stairs","version":1,"operation":{"kind":"stairs"}},
            {"id":"room_omissions","version":1,"operation":{"kind":"room_omissions","config":{"count":[0,3]}}}
        ]});
        let bytes = serde_json::to_vec(&good).unwrap();
        let Owned(recipe) = crate::storage::codec::strict::<Owned<Recipe>>(&bytes).unwrap();
        check(&recipe).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&recipe)).unwrap(), good);
        for (path, value) in [
            ("/version", serde_json::json!(3)),
            ("/stages/6/version", serde_json::json!(2)),
            ("/stages/6/id", serde_json::json!("stairs")),
            (
                "/stages/6/operation/config/count",
                serde_json::json!([3, 2]),
            ),
            (
                "/stages/6/operation/config/count",
                serde_json::json!([0, 64]),
            ),
        ] {
            let mut invalid = good.clone();
            *invalid.pointer_mut(path).unwrap() = value;
            let Owned(recipe) = serde_json::from_value::<Owned<Recipe>>(invalid).unwrap();
            assert!(check(&recipe).is_err(), "{path}");
        }
    }

    #[test]
    fn stored_maze_rooms_require_exact_named_config() {
        use super::{Borrowed, Owned};
        type Operation = crate::generation_recipe::Operation;
        let good = r#"{"kind":"maze_rooms","config":{"one_in":1}}"#;
        let Owned(operation) =
            crate::storage::codec::strict::<Owned<Operation>>(good.as_bytes()).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&operation)).unwrap(),
            serde_json::from_str::<serde_json::Value>(good).unwrap()
        );
        for bad in [
            r#"{"kind":"maze_rooms"}"#,
            r#"{"kind":"maze_rooms","config":null}"#,
            r#"{"kind":"maze_rooms","config":{}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":1,"surprise":0}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":1},"surprise":0}"#,
            r#"{"kind":"maze_rooms","config":[1]}"#,
            r#"["maze_rooms",{"one_in":1}]"#,
            r#"{"kind":"maze_rooms","config":{"one_in":false}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":-1}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":4294967296}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":1.0}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":1,"one_in":1}}"#,
            r#"{"kind":"maze_rooms","config":{"one_in":1},"config":{"one_in":1}}"#,
            r#"{"kind":"maze_rooms","kind":"maze_rooms","config":{"one_in":1}}"#,
        ] {
            assert!(
                serde_json::from_str::<Owned<Operation>>(bad).is_err(),
                "{bad}"
            );
            assert!(
                crate::storage::codec::strict::<Owned<Operation>>(bad.as_bytes()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn stored_maze_recipe_preserves_lighting_order_and_version_ownership() {
        use super::{Borrowed, Owned};
        use crate::generation_recipe::{check, Recipe};
        let good = serde_json::json!({"version":5,"stages":[
            {"id":"stone","version":1,"operation":{"kind":"stone_fill"}},
            {"id":"slots","version":1,"operation":{"kind":"grid_partition"}},
            {"id":"rooms","version":1,"operation":{"kind":"rooms","width":[4,6],"height":[3,5]}},
            {"id":"connections","version":1,"operation":{"kind":"connected_graph","extra":[0,1]}},
            {"id":"corridors","version":1,"operation":{"kind":"corridors"}},
            {"id":"stairs","version":1,"operation":{"kind":"stairs"}},
            {"id":"maze_rooms","version":1,"operation":{"kind":"maze_rooms","config":{"one_in":1}}},
            {"id":"lighting","version":1,"operation":{"kind":"room_lighting","darkness_roll":10,"darkness_start":1}}
        ]});
        let Owned(recipe) =
            crate::storage::codec::strict::<Owned<Recipe>>(&serde_json::to_vec(&good).unwrap())
                .unwrap();
        check(&recipe).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&recipe)).unwrap(), good);
        for (path, value) in [
            ("/version", serde_json::json!(4)),
            ("/version", serde_json::json!(6)),
            ("/stages/6/version", serde_json::json!(2)),
            ("/stages/6/id", serde_json::json!("stairs")),
            ("/stages/6/operation/config/one_in", serde_json::json!(0)),
            (
                "/stages/6/operation/config/one_in",
                serde_json::json!(10001),
            ),
        ] {
            let mut invalid = good.clone();
            *invalid.pointer_mut(path).unwrap() = value;
            let Owned(recipe) = serde_json::from_value::<Owned<Recipe>>(invalid).unwrap();
            assert!(check(&recipe).is_err(), "{path}");
        }
        for mutation in 0..4 {
            let mut invalid = good.clone();
            let stages = invalid["stages"].as_array_mut().unwrap();
            match mutation {
                0 => {
                    stages.remove(7);
                }
                1 => {
                    stages.remove(6);
                }
                2 => stages.swap(6, 7),
                3 => {
                    let mut duplicate = stages[6].clone();
                    duplicate["id"] = serde_json::json!("other_maze");
                    stages.insert(7, duplicate);
                }
                _ => unreachable!(),
            }
            let Owned(recipe) = serde_json::from_value::<Owned<Recipe>>(invalid).unwrap();
            assert!(check(&recipe).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn discovery_formats_match_independent_payload_boundaries() {
        assert_eq!(
            (
                crate::engine::ARCHIVE_VERSION,
                crate::scenario_package::FORMAT,
                crate::scenario_package::RULESET,
                super::RULES_INPUT_FORMAT,
                super::REGION_INPUT_FORMAT,
                super::CONTENT_PACKAGE_FORMAT,
                crate::arena_evaluation::FORMAT,
            ),
            (59, 31, "interactions-v52", 24, 8, 28, "tor-arena-run-v5")
        );
    }

    #[test]
    fn discovery_formats_preserve_physical_envelopes() {
        assert_eq!(tor_protocol::PROTOCOL_VERSION, 61);
        assert_eq!(super::RULES_INPUT_FORMAT, 24);
        assert_eq!(super::REGION_INPUT_FORMAT, 8);
        // Semantic rule, region and package payload versions advance independently
        // of their physical framing, which remains unchanged.
        let base = crate::storage::codec::frame(0, 0, &serde_json::json!({})).unwrap();
        assert_eq!(&base[..6], b"TORB\x06\x00");
        let journal = crate::storage::codec::frame(1, 0, &serde_json::json!({})).unwrap();
        assert_eq!(&journal[..6], b"TORJ\x06\x00");
        let content = crate::Scenario::two_room(42).content().unwrap();
        let record = tor_simulation::Game::two_room_in_stone(42, content)
            .into_region_record(tor_world::RegionId(2))
            .unwrap();
        let region = crate::storage::encode_region(1, &record).unwrap();
        assert_eq!(&region[..6], b"TORR\x06\x00");
        // The bounded zlib envelope still wraps an independently validated JSON
        // payload. Its physical representation has not changed.
        let checkpoint =
            crate::storage::checkpoint_codec::write_json(&serde_json::json!({}), Vec::new())
                .unwrap();
        assert_eq!(&checkpoint[..8], b"TORC\x01\x00\x00\x00");
    }

    #[test]
    fn natural_melee_compiled_rules_keep_explicit_riders_and_reject_positional_records() {
        let mut document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let ability = document
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        ability.strikes = tor_simulation::rules::StrikeSource::NaturalMelee {
            additional_riders: vec![tor_simulation::rules::RiderDef {
                condition: None,
                trigger: tor_simulation::rules::RiderTrigger::Success,
                check: None,
                effects: vec![tor_simulation::rules::Effect::ApplyCondition {
                    condition: tor_simulation::content_ids::ConditionId::new("rattled").unwrap(),
                    duration: 100,
                }],
                descriptors: vec![],
            }],
        };
        let encoded = encode_rules_document(&document).unwrap();
        let restored = decode_rules_document(&encoded).unwrap();
        assert_eq!(encode_rules_document(&restored).unwrap(), encoded);
        assert_eq!(
            restored
                .abilities
                .iter()
                .find(|ability| ability.basic_attack)
                .unwrap()
                .strikes,
            document
                .abilities
                .iter()
                .find(|ability| ability.basic_attack)
                .unwrap()
                .strikes,
        );
        let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        let source = value["document"]["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["basic_attack"] == true)
            .unwrap();
        source["strikes"]["additional_riders"][0] =
            serde_json::json!(["success", null, null, [], []]);
        assert!(decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
        let mut obsolete: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        obsolete["format"] = serde_json::json!(11);
        assert!(decode_rules_document(&serde_json::to_string(&obsolete).unwrap()).is_err());
    }

    #[test]
    fn phase5_stored_touch_range_rejects_surplus_fields() {
        #[derive(Deserialize)]
        struct Range {
            #[serde(with = "AllyRange")]
            range: tor_simulation::rules::AllyRange,
        }
        let touch =
            serde_json::from_value::<Range>(serde_json::json!({"range":{"kind":"touch"}})).unwrap();
        assert_eq!(touch.range, tor_simulation::rules::AllyRange::Touch);
        assert!(serde_json::from_value::<Range>(
            serde_json::json!({"range":{"kind":"touch","range":1}})
        )
        .is_err());
    }
    #[test]
    fn phase5_stored_supply_cost_requires_a_named_object() {
        use tor_simulation::rules::SupplyCost;
        assert!(serde_json::from_str::<super::Owned<SupplyCost>>(r#"{"supply":"bandage","quantity":1,"access":"carried_only","spend_at":"first_resolution"}"#).is_ok());
        assert!(serde_json::from_str::<super::Owned<SupplyCost>>(
            r#"["bandage",1,"carried_only","first_resolution"]"#
        )
        .is_err());
    }
    #[test]
    fn phase5_stored_supply_definition_requires_a_named_object() {
        use tor_simulation::rules::SupplyDef;
        assert!(serde_json::from_str::<super::Owned<SupplyDef>>(
            r#"{"id":"bandage","name":"Bandage"}"#
        )
        .is_ok());
        assert!(
            serde_json::from_str::<super::Owned<SupplyDef>>(r#"["bandage","Bandage"]"#).is_err()
        );
    }

    #[test]
    fn stored_behavior_and_interaction_have_independent_required_shapes() {
        let behavior = serde_json::json!({"disposition":"passive","idle":"wait","memory_ticks":1000,"doors":false,"care":false,"equipment":false,"loot":false,"gold":false,"flee_percent":25});
        let decode = |value: &serde_json::Value| {
            serde_json::from_slice::<super::Owned<crate::creature_authoring::BehaviorPolicy>>(
                &serde_json::to_vec(value).unwrap(),
            )
        };
        assert!(decode(&behavior).is_ok());
        for percent in [0, 100, 101, u32::MAX] {
            let mut threshold = behavior.clone();
            threshold["flee_percent"] = serde_json::json!(percent);
            assert_eq!(decode(&threshold).is_ok(), percent <= 100);
        }
        for field in [
            "disposition",
            "idle",
            "memory_ticks",
            "doors",
            "care",
            "equipment",
            "loot",
            "gold",
            "flee_percent",
        ] {
            let mut missing = behavior.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(decode(&missing).is_err());
        }
        for malformed in [
            serde_json::json!(["passive", "wait", 1000]),
            serde_json::json!({"disposition":{"passive":null},"idle":"wait","memory_ticks":1000}),
        ] {
            assert!(decode(&malformed).is_err());
        }
        for encoded in [r#""non_hostile""#, r#""hostile_to_target""#] {
            assert!(
                serde_json::from_str::<super::Owned<tor_simulation::rules::Interaction>>(encoded)
                    .is_ok()
            );
        }
        for encoded in [
            r#"{"non_hostile":null}"#,
            r#"{"kind":"hostile_to_target"}"#,
            r#""unknown""#,
        ] {
            assert!(
                serde_json::from_str::<super::Owned<tor_simulation::rules::Interaction>>(encoded)
                    .is_err()
            );
        }
        let document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let ability = document.abilities[0].clone();
        let mut stored = serde_json::to_value(super::Borrowed(&ability)).unwrap();
        stored.as_object_mut().unwrap().remove("interaction");
        assert!(
            serde_json::from_value::<super::Owned<tor_simulation::rules::AbilityDef>>(stored)
                .is_err()
        );
    }

    #[test]
    fn stored_partial_ai_profiles_require_nullable_fields_and_bounded_values() {
        let empty = serde_json::json!({"idle":null,"doors":null,"care":null,"equipment":null,"loot":null,"gold":null,"flee_percent":null});
        let decode = |value: &serde_json::Value| {
            crate::storage::codec::strict::<super::Owned<crate::scenario_package::AiProfile>>(
                &serde_json::to_vec(value).unwrap(),
            )
        };
        assert!(decode(&empty).is_ok());
        for field in [
            "idle",
            "doors",
            "care",
            "equipment",
            "loot",
            "gold",
            "flee_percent",
        ] {
            let mut missing = empty.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                decode(&missing).is_err(),
                "stored {field} must not use source defaults"
            );
        }
        let explicit = serde_json::json!({"idle":"wait","doors":false,"care":false,"equipment":false,"loot":false,"gold":false,"flee_percent":0});
        let profile = decode(&explicit).unwrap().0;
        assert_eq!(
            serde_json::to_value(super::Borrowed(&profile)).unwrap(),
            explicit
        );
        let mut invalid = explicit;
        invalid["flee_percent"] = serde_json::json!(101);
        assert!(decode(&invalid).is_err());
        let mut invalid = empty;
        invalid["extra"] = serde_json::json!(false);
        assert!(decode(&invalid).is_err());
    }

    #[test]
    fn independent_stored_supply_adapters_reject_missing_unknown_and_unbounded_fields() {
        use tor_simulation::rules::{AbilityDef, RulesetDocument, SupplyCost};
        let document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let ability = &document.abilities[0];
        let original = serde_json::to_value(super::Borrowed(ability)).unwrap();
        let decoded: super::Owned<AbilityDef> = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(&decoded.0, ability);
        assert!(original.get("cooldown").unwrap().is_null());
        for field in ["cooldown", "supply_costs"] {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<super::Owned<AbilityDef>>(value).is_err());
        }
        let row = serde_json::json!({"supply":"bandage", "quantity":1, "access":"carried_only", "spend_at":"first_resolution"});
        assert!(serde_json::from_value::<super::Owned<SupplyCost>>(row.clone()).is_ok());
        for field in ["supply", "quantity", "access", "spend_at"] {
            let mut value = row.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<super::Owned<SupplyCost>>(value).is_err());
        }
        let mut value = row.clone();
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<super::Owned<SupplyCost>>(value).is_err());
        let mut value = original;
        value["supply_costs"] = serde_json::json!(vec![row; 9]);
        assert!(serde_json::from_value::<super::Owned<AbilityDef>>(value).is_err());
    }
    #[test]
    fn outgoing_edge_policy_is_required_and_strict_in_independent_stored_rules() {
        let document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let encoded = super::encode_rules_document(&document).unwrap();
        let original: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        for kind in ["ordinary", "ignore_negative"] {
            let mut document = document.clone();
            document.abilities[0].outgoing_edge_policy =
                serde_json::from_value(serde_json::json!({"kind":kind})).unwrap();
            let encoded = super::encode_rules_document(&document).unwrap();
            let decoded = super::decode_rules_document(&encoded).unwrap();
            assert_eq!(super::encode_rules_document(&decoded).unwrap(), encoded);
        }
        let mut missing = original.clone();
        missing["document"]["abilities"][0]
            .as_object_mut()
            .unwrap()
            .remove("outgoing_edge_policy");
        assert!(super::decode_rules_document(&serde_json::to_string(&missing).unwrap()).is_err());
        assert!(
            serde_json::from_value::<super::Owned<tor_simulation::rules::AbilityDef>>(
                missing["document"]["abilities"][0].clone()
            )
            .is_err()
        );
        for invalid in [
            serde_json::json!(null),
            serde_json::json!("ordinary"),
            serde_json::json!(["ordinary"]),
            serde_json::json!({}),
            serde_json::json!({"kind":"unknown"}),
            serde_json::json!({"kind":"ordinary","extra":true}),
            serde_json::json!({"kind":"ignore_negative","extra":true}),
        ] {
            let mut value = original.clone();
            value["document"]["abilities"][0]["outgoing_edge_policy"] = invalid;
            assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
            assert!(
                serde_json::from_value::<super::Owned<tor_simulation::rules::AbilityDef>>(
                    value["document"]["abilities"][0].clone()
                )
                .is_err(),
                "the independent decoder rejects shape before canonical byte comparison"
            );
        }
    }
    #[test]
    fn compiled_supply_rules_require_explicit_owned_fields() {
        let document = toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let encoded = super::encode_rules_document(&document).unwrap();
        let restored = super::decode_rules_document(&encoded).unwrap();
        assert_eq!(super::encode_rules_document(&restored).unwrap(), encoded);
        let original: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        for field in ["cooldown", "supply_costs"] {
            let mut value = original.clone();
            value["document"]["abilities"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
        }
        let mut value = original.clone();
        value["document"]
            .as_object_mut()
            .unwrap()
            .remove("supplies");
        assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
        let mut value = original.clone();
        value["document"]["abilities"][0]["cooldown"] = serde_json::json!(0);
        assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
        let mut value = original;
        value["document"]["abilities"][0]["supply_costs"] = serde_json::json!(vec![
            serde_json::json!({"supply":"bandage", "quantity":1, "access":"carried_only", "spend_at":"first_resolution"});
            9
        ]);
        assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
    }
    #[test]
    fn stored_check_only_edge_stats_round_trip_strictly() {
        use crate::creature_authoring::Stat;
        for stat in [
            Stat::CheckEdge,
            Stat::IncomingEdge { defense: None },
            Stat::IncomingEdge {
                defense: Some("physical".into()),
            },
        ] {
            let bytes = serde_json::to_vec(&super::Borrowed(&stat)).unwrap();
            let decoded = strict::<super::Owned<Stat>>(&bytes).unwrap();
            assert_eq!(decoded.0, stat);
            let mut forged = serde_json::to_value(super::Borrowed(&stat)).unwrap();
            forged["private_owner"] = serde_json::json!(1);
            assert!(strict::<super::Owned<Stat>>(&serde_json::to_vec(&forged).unwrap()).is_err());
        }
    }

    use super::*;
    use crate::storage::codec::strict;
    use serde_json::Value;

    #[test]
    fn saved_talent_protection_uses_the_independent_named_category_schema() {
        use crate::creature_authoring::{Modifier, Op, Scope};
        let modifier = Modifier {
            scope: Scope::Always,
            op: Op::TalentReduction {
                category: "impact".into(),
                amount: 2,
            },
        };
        let bytes = serde_json::to_vec(&Borrowed(&modifier)).unwrap();
        let Owned(restored) = strict::<Owned<Modifier>>(&bytes).unwrap();
        assert_eq!(restored, modifier);
        assert!(restored.compile().is_ok());
        for malformed in [
            r#"["always",{"kind":"talent_reduction","category":"impact","amount":2}]"#,
            r#"{"scope":"always","op":{"kind":"talent_reduction","category":"impact"}}"#,
            r#"{"scope":"always","op":{"kind":"talent_reduction","category":"impact","category":"keen","amount":2}}"#,
            r#"{"scope":"always","op":{"kind":"talent_reduction","category":"impact","amount":2,"unknown":true}}"#,
            r#"{"scope":"always","op":{"kind":"talent_reduction","selector":{"kind":"descriptor","descriptor":"fire"},"amount":2}}"#,
        ] {
            assert!(
                strict::<Owned<Modifier>>(malformed.as_bytes()).is_err(),
                "{malformed}"
            );
        }
        for (category, amount) in [("unknown", 2), ("impact", 1_000_001)] {
            let invalid = Modifier {
                scope: Scope::Always,
                op: Op::TalentReduction {
                    category: category.into(),
                    amount,
                },
            };
            assert!(invalid.compile().is_err());
        }
    }

    #[test]
    fn saved_recovery_stat_modifiers_round_trip_through_the_independent_schema() {
        use crate::creature_authoring::{Modifier, Op, Scope, Stat};
        for (stat, amount, kind) in [
            (Stat::Health, 10, "health"),
            (Stat::HealthRecoveryPeriod, 40, "health_recovery_period"),
            (
                Stat::HealthRecoveryPeriodPercent,
                -25,
                "health_recovery_period_percent",
            ),
            (
                Stat::HealthRecoveryResetPercent,
                -50,
                "health_recovery_reset_percent",
            ),
        ] {
            let modifier = Modifier {
                scope: Scope::Always,
                op: Op::Add {
                    stat,
                    value: amount,
                },
            };
            let value = serde_json::to_value(Borrowed(&modifier)).unwrap();
            assert_eq!(value["op"]["stat"], serde_json::json!({"kind": kind}));
            let Owned(restored) =
                strict::<Owned<Modifier>>(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(restored, modifier);
            let text = serde_json::to_string(&value).unwrap();
            let tag = format!("\"kind\":\"{kind}\"");
            assert_eq!(text.matches(&tag).count(), 1);
            let duplicate = text.replacen(&tag, &format!("{tag},{tag}"), 1);
            assert_ne!(duplicate, text);
            assert!(strict::<Owned<Modifier>>(duplicate.as_bytes()).is_err());
            let mut unknown = value.clone();
            unknown["op"]["stat"]["private_owner"] = 999.into();
            assert!(strict::<Owned<Modifier>>(&serde_json::to_vec(&unknown).unwrap()).is_err());
            let mut missing = value;
            missing["op"]["stat"]
                .as_object_mut()
                .unwrap()
                .remove("kind");
            assert!(strict::<Owned<Modifier>>(&serde_json::to_vec(&missing).unwrap()).is_err());
        }
    }

    #[test]
    fn procedural_family_save_golden_and_strict_boundaries() {
        use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
        let expected: BTreeMap<String, Value> =
            serde_json::from_str(include_str!("../fixtures/saved-world-families.json")).unwrap();
        for (name, domain) in [
            ("finite", Domain::Finite { count: 7 }),
            ("seeded_finite", Domain::SeededFinite { min: 2, max: 9 }),
            (
                "open",
                Domain::Open {
                    max_depth: 1_000_000,
                },
            ),
        ] {
            let family = WorldFamily {
                gravity: None,
                kind: FamilyKind::GridChain,
                version: 1,
                base_region: 100,
                domain,
                grid: [2, 3],
                size: [10, 8, 2],
                recipe: "rooms".into(),
                zone: None,
            };
            let value = serde_json::to_value(Borrowed(&family)).unwrap();
            assert_eq!(value, expected[name]);
            let bytes = serde_json::to_vec(&value).unwrap();
            let Owned(restored) = strict::<Owned<WorldFamily>>(&bytes).unwrap();
            assert_eq!(restored, family);
            for pointer in ["", "/domain"] {
                let mut unknown = value.clone();
                unknown
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert("unexpected".into(), Value::Bool(true));
                assert!(
                    strict::<Owned<WorldFamily>>(&serde_json::to_vec(&unknown).unwrap()).is_err()
                );
                for field in value.pointer(pointer).unwrap().as_object().unwrap().keys() {
                    let mut missing = value.clone();
                    missing
                        .pointer_mut(pointer)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove(field);
                    assert!(
                        strict::<Owned<WorldFamily>>(&serde_json::to_vec(&missing).unwrap())
                            .is_err(),
                        "{name}: accepted missing {pointer}/{field}"
                    );
                }
            }
            let text = serde_json::to_string(&value).unwrap();
            let duplicate = text.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
            assert_ne!(duplicate, text);
            assert!(strict::<Owned<WorldFamily>>(duplicate.as_bytes()).is_err());
            let domain_kind = format!("\"kind\":\"{name}\"");
            let duplicate = text.replacen(&domain_kind, &format!("{domain_kind},{domain_kind}"), 1);
            assert_ne!(duplicate, text);
            assert!(strict::<Owned<WorldFamily>>(duplicate.as_bytes()).is_err());
        }
    }

    #[test]
    fn compiled_rules_catalog_order_is_irrelevant_but_effect_order_is_preserved() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let mut document = package.rules().unwrap().document().clone();
        let second_pool = tor_simulation::content_ids::PointPoolId::new("secondary").unwrap();
        document
            .point_pools
            .push(tor_simulation::rules::PointPoolDef {
                id: second_pool.clone(),
                name: "Secondary".into(),
                allowed: vec![tor_simulation::rules::PurchaseKind::SkillRank],
            });
        document
            .constants
            .racial_point_grants
            .push(tor_simulation::rules::PointGrant {
                pool: second_pool,
                points: 1,
            });
        let package = source::Package::from_parts(
            package.manifest.clone(),
            package.region_defs().unwrap(),
            &toml::to_string(&document).unwrap(),
        )
        .unwrap();
        let mut document = package.rules().unwrap().document().clone();
        let original = encode_rules_document(&document).unwrap();
        document.point_pools.reverse();
        document.supplies.reverse();
        document.abilities.reverse();
        document.ability_tags.reverse();
        document.talents.reverse();
        document.creature_types.reverse();
        document.subtypes.reverse();
        document.classes.reverse();
        document.conditions.reverse();
        document.descriptors.reverse();
        document.flags.reverse();
        document.resources.reverse();
        document.equipment_slots.reverse();
        assert_eq!(encode_rules_document(&document).unwrap(), original);
        let reordered = source::Package::from_parts(
            package.manifest.clone(),
            package.region_defs().unwrap(),
            &toml::to_string(&document).unwrap(),
        )
        .unwrap();
        assert_eq!(
            package.certificate.ruleset_hash,
            reordered.certificate.ruleset_hash
        );
        assert_eq!(
            package.rules().unwrap().document(),
            reordered.rules().unwrap().document()
        );
        assert_ne!(
            package.certificate.files[&package.manifest.ruleset],
            reordered.certificate.files[&package.manifest.ruleset]
        );
        let first = document
            .abilities
            .iter()
            .flat_map(|ability| match &ability.strikes {
                tor_simulation::rules::StrikeSource::Authored { strikes } => strikes.as_slice(),
                tor_simulation::rules::StrikeSource::SelectedMelee
                | tor_simulation::rules::StrikeSource::Projectile { .. }
                | tor_simulation::rules::StrikeSource::NaturalMelee { .. } => &[],
            })
            .flat_map(|strike| &strike.on_success)
            .next()
            .unwrap()
            .clone();
        let second = document
            .abilities
            .iter()
            .flat_map(|ability| match &ability.strikes {
                tor_simulation::rules::StrikeSource::Authored { strikes } => strikes.as_slice(),
                tor_simulation::rules::StrikeSource::SelectedMelee
                | tor_simulation::rules::StrikeSource::Projectile { .. }
                | tor_simulation::rules::StrikeSource::NaturalMelee { .. } => &[],
            })
            .flat_map(|strike| &strike.on_success)
            .find(|effect| **effect != first)
            .unwrap()
            .clone();
        {
            let tor_simulation::rules::StrikeSource::Authored { strikes } =
                &mut document.abilities[0].strikes
            else {
                panic!("expected authored fixture");
            };
            strikes[0].on_success = vec![first, second];
        }
        let ordered = encode_rules_document(&document).unwrap();
        {
            let tor_simulation::rules::StrikeSource::Authored { strikes } =
                &mut document.abilities[0].strikes
            else {
                panic!("expected authored fixture");
            };
            strikes[0].on_success.reverse();
        }
        assert_ne!(encode_rules_document(&document).unwrap(), ordered);
        let ordered_grants = encode_rules_document(&document).unwrap();
        document.constants.racial_point_grants.reverse();
        assert_ne!(
            encode_rules_document(&document).unwrap(),
            ordered_grants,
            "grant order selects the default spending pool"
        );
    }

    #[test]
    fn compiled_regions_normalize_explicit_defaults_and_source_comments() {
        let authored = "id = 1\nname = \"Room\"\nsize = [4, 4, 1]\n";
        let explicit = format!(
            "{authored}# authored presentation only\nlit = true\nchamber = false\nlighting = []\nwalls = []\n"
        );
        let first: source::RegionDef = toml::from_str(authored).unwrap();
        let second: source::RegionDef = toml::from_str(&explicit).unwrap();
        assert_ne!(
            source::source_digest(authored),
            source::source_digest(&explicit)
        );
        assert_eq!(
            encode_region_input(&first).unwrap(),
            encode_region_input(&second).unwrap()
        );
    }

    #[test]
    fn compiled_package_rejects_tampered_index_and_dependency_provenance() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let original: Value =
            serde_json::from_slice(&encode_content_package(&package).unwrap()).unwrap();
        for pointer in [
            "/index/regions/0/file",
            "/index/regions/0/name",
            "/package/certificate/content_hash",
        ] {
            let mut bad = original.clone();
            *bad.pointer_mut(pointer).unwrap() = Value::String("tampered".into());
            assert!(
                decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "{pointer}"
            );
        }
        let mut bad = original;
        bad["package"]["certificate"]["ruleset_dependencies"][source::RULESET] =
            Value::String("0".repeat(64));
        assert!(decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err());
    }

    #[test]
    fn compiled_metadata_capacity_fails_before_a_save_is_admitted() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut package = source::read_package(&root).unwrap();
        package.manifest.version = "x".repeat(MAX_SAVE_METADATA_BYTES);
        let error = check_metadata_capacity(&package).unwrap_err();
        assert!(error.message.contains("save metadata limit"));
        assert!(encode_content_package(&package).is_err());
    }

    fn reject_obsolete_compiled_headers(input: &str, current: u32, accepts: impl Fn(&str) -> bool) {
        assert!(accepts(input));
        assert_eq!(
            serde_json::from_str::<Value>(input).unwrap()["format"],
            current
        );
        let header = format!("\"format\":{current}");
        let mut replacements: Vec<_> = [0, current - 1, current + 1]
            .into_iter()
            .map(|version| format!("\"format\":{version}"))
            .collect();
        replacements.extend([
            format!("{header},{header}"),
            format!("{header},\"unknown\":true"),
        ]);
        for replacement in replacements {
            let invalid = input.replacen(&header, &replacement, 1);
            assert_ne!(invalid, input);
            assert!(!accepts(&invalid), "accepted {replacement}");
        }
        let missing = input.replacen(&format!("{header},"), "", 1);
        assert_ne!(missing, input);
        assert!(!accepts(&missing));
    }

    #[test]
    fn compiled_rules_current_header_rejects_previous_and_malformed_envelopes() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let input = package.rules_document;
        reject_obsolete_compiled_headers(&input, super::RULES_INPUT_FORMAT, |text| {
            decode_rules_document(text).is_ok()
        });
    }

    #[test]
    fn compiled_region_current_header_rejects_previous_and_malformed_envelopes() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let input = encode_region_input(&package.region_def(1).unwrap()).unwrap();
        reject_obsolete_compiled_headers(&input, super::REGION_INPUT_FORMAT, |text| {
            decode_region_input(text).is_ok()
        });
    }

    #[test]
    fn compiled_package_current_header_rejects_previous_and_malformed_envelopes() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let input = String::from_utf8(encode_content_package(&package).unwrap()).unwrap();
        reject_obsolete_compiled_headers(&input, super::CONTENT_PACKAGE_FORMAT, |text| {
            decode_content_package(text.as_bytes()).is_ok()
        });
    }

    #[test]
    fn compiled_rules_reject_unknown_version_duplicates_and_changed_document() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let original = &package.rules_document;
        assert_eq!(
            encode_rules_document(&decode_rules_document(original).unwrap()).unwrap(),
            *original
        );
        let header = format!("\"format\":{}", super::RULES_INPUT_FORMAT);
        for replacement in ["\"format\":0".to_owned(), format!("{header},{header}")] {
            let invalid = original.replacen(&header, &replacement, 1);
            assert_ne!(invalid, *original);
            assert!(decode_rules_document(&invalid).is_err());
        }
        let mut corrupted = package.clone();
        let mut document = decode_rules_document(original).unwrap();
        document.constants.base_defense += 1;
        corrupted.rules_document = encode_rules_document(&document).unwrap();
        assert!(corrupted.check_identity().is_err());
    }

    #[test]
    fn compiled_region_round_trip_rejects_unknown_duplicate_and_missing_fields() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/generated-filler");
        let package = source::read_package(&root).unwrap();
        let def = package.region_def(2).unwrap();
        let input = encode_region_input(&def).unwrap();
        assert_eq!(
            encode_region_input(&decode_region_input(&input).unwrap()).unwrap(),
            input
        );
        let value: Value = serde_json::from_str(&input).unwrap();
        for path in ["/region", "/region/generate"] {
            let mut corrupted = value.clone();
            corrupted
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("surprise".into(), Value::Bool(true));
            assert!(decode_region_input(&serde_json::to_string(&corrupted).unwrap()).is_err());
        }
        let mut missing = value.clone();
        missing["region"]
            .as_object_mut()
            .unwrap()
            .remove("lighting");
        assert!(decode_region_input(&serde_json::to_string(&missing).unwrap()).is_err());
        assert_eq!(value["format"], 8);
        let header = "\"format\":8";
        for replacement in ["\"format\":0".to_owned(), format!("{header},{header}")] {
            let invalid = input.replacen(header, &replacement, 1);
            assert_ne!(
                invalid, input,
                "the strict mutation must change the current input"
            );
            assert!(decode_region_input(&invalid).is_err());
        }
        assert!(decode_region_input("id = 1").is_err());
    }

    #[test]
    fn compiled_discovery_fields_round_trip_and_are_required_strictly() {
        let region: source::RegionDef = serde_json::from_value(serde_json::json!({
            "id":1,"name":"Hidden features","size":[5,5,2],"chamber":true,
            "doors":[{"id":1,"at":[3,2,0],"open":false,"height":2,"concealment":{"dc":12}}],
            "traps":[{"id":1,"at":[2,2,0],"dc":12,"kind":{"kind":"rust","slot":"body_armor","amount":1}},
                     {"id":2,"at":[1,2,0],"dc":13,"kind":{"kind":"sleeping_gas","condition":"rogue.sleep","duration":200}}]
        })).unwrap();
        let encoded = encode_region_input(&region).unwrap();
        assert_eq!(
            encode_region_input(&decode_region_input(&encoded).unwrap()).unwrap(),
            encoded
        );
        let value: Value = serde_json::from_str(&encoded).unwrap();
        for (path, field) in [
            ("/region", "traps"),
            ("/region/doors/0", "concealment"),
            ("/region/traps/0", "dc"),
            ("/region/traps/0/kind", "slot"),
            ("/region/traps/1/kind", "condition"),
        ] {
            let mut missing = value.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                decode_region_input(&serde_json::to_string(&missing).unwrap()).is_err(),
                "missing {path}/{field}"
            );
        }
        for path in [
            "/region/doors/0/concealment",
            "/region/traps/0",
            "/region/traps/1/kind",
        ] {
            let mut extra = value.clone();
            extra
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("surprise".into(), Value::Bool(true));
            assert!(
                decode_region_input(&serde_json::to_string(&extra).unwrap()).is_err(),
                "extra {path}"
            );
        }
        for (path, fields) in [
            (
                "/region",
                &[
                    "trapdoor_destination",
                    "lit",
                    "lighting",
                    "id",
                    "name",
                    "size",
                    "chamber",
                    "zone",
                    "gravity",
                    "gravity_overrides",
                    "anchors",
                    "walls",
                    "openings",
                    "places",
                    "portals",
                    "doors",
                    "traps",
                    "items",
                    "actors",
                    "generate",
                ][..],
            ),
            (
                "/region/doors/0",
                &["concealment", "id", "at", "open", "height"][..],
            ),
            ("/region/doors/0/concealment", &["dc"][..]),
            (
                "/region/traps/0",
                &["avoidance_dc", "id", "at", "dc", "kind"][..],
            ),
        ] {
            let mut positional = value.clone();
            let record = positional.pointer_mut(path).unwrap();
            *record = Value::Array(fields.iter().map(|field| record[*field].clone()).collect());
            assert!(
                decode_region_input(&serde_json::to_string(&positional).unwrap()).is_err(),
                "positional {path}"
            );
        }
        assert!(
            serde_json::from_value::<Owned<crate::generation_recipe::SecretEntrances>>(
                serde_json::json!([250, 2, 12])
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<Owned<crate::generation_recipe::HiddenTraps>>(
                serde_json::json!([[1, 2], 12, "body_armor", 1, "rogue.sleep", 200])
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<Owned<source::IndexedRegion>>(serde_json::json!([
                [],
                1,
                "regions/1.toml",
                "hash",
                "compiled_hash",
                "Hidden features",
                [5, 5, 2],
                true,
                null,
                {},
                [],
                [],
                [],
                [],
                [1],
                [1, 2],
                false
            ]))
            .is_err()
        );
    }

    #[test]
    fn current_region_input_pins_supply_placement_bytes() {
        // Supply capability belongs to the separately stored manifest archetype.
        // Placement still carries only its name. The region envelope also owns
        // actor recipes with explicit nutrition requirements.
        let region: source::RegionDef = serde_json::from_value(serde_json::json!({
            "id":1,"name":"Supply room","size":[4,4,1],
            "items":[{"id":10,"at":[1,1,0],"archetype":"clean_bandage","quantity":2}]
        }))
        .unwrap();
        let expected = concat!(
            "{\"format\":8,\"region\":{\"trapdoor_destination\":null,\"lit\":true,\"lighting\":[],\"id\":1,",
            "\"name\":\"Supply room\",\"size\":[4,4,1],\"chamber\":false,\"zone\":null,",
            "\"gravity\":null,\"gravity_overrides\":[],\"anchors\":{},\"walls\":[],",
            "\"openings\":[],\"places\":[],\"portals\":[],\"doors\":[],\"traps\":[],\"items\":[{",
            "\"equipped_slot\":null,\"class\":null,\"quantity\":2,\"stackable\":null,",
            "\"properties\":{},\"id\":10,\"at\":[1,1,0],\"archetype\":\"clean_bandage\",",
            "\"name\":null,\"carried_by\":null,\"seed_names\":[]}],\"actors\":[],\"generate\":null}}"
        );
        assert_eq!(encode_region_input(&region).unwrap(), expected);
        let restored = decode_region_input(expected).unwrap();
        assert_eq!(encode_region_input(&restored).unwrap(), expected);
    }

    #[test]
    fn current_region_recipe_requires_growth_and_nutrition_with_current_header() {
        let region: source::RegionDef = serde_json::from_value(serde_json::json!({
            "id":1,"name":"Growth room","size":[4,4,1],
            "actors":[{"id":2,"at":[1,1,0],"controller":"external",
                "creature":{"species":"human","name":"owner","faction":"neutral",
                    "binding":"intellect","restoration":"authored", "nutrition": {"kind":"exempt"},"starting_skills":["athletics","heavy_weaponry"],
                    "hit_dice":[{"source":"racial"}],"future_growth":{"kind":"selected","source":{"kind":"racial"}}}}]
        })).unwrap();
        let encoded = encode_region_input(&region).unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["format"], 8);
        assert_eq!(
            value["region"]["actors"][0]["creature"]["future_growth"],
            serde_json::json!({"kind":"selected","source":{"kind":"racial"}})
        );
        assert_eq!(
            value["region"]["actors"][0]["creature"]["nutrition"],
            serde_json::json!({"kind":"exempt"})
        );
        assert_eq!(
            encode_region_input(&decode_region_input(&encoded).unwrap()).unwrap(),
            encoded
        );
        let mut previous = value.clone();
        previous["format"] = (super::REGION_INPUT_FORMAT - 1).into();
        assert!(
            decode_region_input(&serde_json::to_string(&previous).unwrap()).is_err(),
            "current actor fields cannot upgrade the previous region envelope"
        );
        for field in ["future_growth", "nutrition"] {
            let mut missing = value.clone();
            missing["region"]["actors"][0]["creature"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(decode_region_input(&serde_json::to_string(&missing).unwrap()).is_err());
        }
    }

    #[test]
    fn compiled_room_omission_recipe_roundtrips_without_authoring_sources() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = source::read_package(&root).unwrap();
        let preset: crate::generation_recipe::GridRoomsPreset = serde_json::from_value(
            serde_json::json!({"version":1,"room_width":[4,6],"room_height":[3,5],
                "extra_connections":[0,1],"room_omissions":{"count":[0,3]}}),
        )
        .unwrap();
        let mut manifest = template.manifest.clone();
        manifest
            .generation_recipes
            .insert("optional".into(), preset.to_recipe().unwrap());
        let mut package = source::Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap();
        package.check().unwrap();
        let encoded = encode_content_package(&package).unwrap();
        let restored = decode_content_package(&encoded).unwrap();
        assert!(restored.directory.is_none());
        assert_eq!(restored.sources.files_read(), 0);
        assert_eq!(encode_content_package(&restored).unwrap(), encoded);
        let value: Value = serde_json::from_slice(&encoded).unwrap();
        let path = "/package/manifest/generation_recipes/optional/stages/6/operation/config";
        assert_eq!(
            value.pointer(path).unwrap(),
            &serde_json::json!({"count":[0,3]})
        );
        package.rules_source = "not authoring TOML".into();
        package.resolved_rules_source = Some("also not authoring TOML".into());
        assert_eq!(encode_content_package(&package).unwrap(), encoded);
        for invalid in [
            serde_json::json!({}),
            serde_json::json!([[0, 3]]),
            serde_json::json!({"count":[0,3],"surprise":true}),
        ] {
            let mut bad = value.clone();
            *bad.pointer_mut(path).unwrap() = invalid;
            assert!(decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let text = String::from_utf8(encoded).unwrap();
        assert!(text.contains("\"count\":[0,3]"));
        let duplicate = text.replacen("\"count\":[0,3]", "\"count\":[0,3],\"count\":[0,3]", 1);
        assert!(decode_content_package(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn compiled_metadata_has_no_authoring_sources_or_machine_path() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut package = source::read_package(&root).unwrap();
        let encoded = encode_content_package(&package).unwrap();
        let value: Value = serde_json::from_slice(&encoded).unwrap();
        assert!(value["package"].get("rules_source").is_none());
        assert!(value["package"].get("resolved_rules_source").is_none());
        assert_eq!(value["package"]["directory"], Value::Null);
        let restored = decode_content_package(&encoded).unwrap();
        assert_eq!(
            restored.rules().unwrap().document(),
            package.rules().unwrap().document()
        );
        package.rules_source = "this is not TOML".into();
        package.resolved_rules_source = Some("also not TOML".into());
        assert_eq!(encode_content_package(&package).unwrap(), encoded);
        let mut bad = value;
        bad["package"]["rules_document"] = Value::String("id = 'source-is-not-a-snapshot'".into());
        assert!(decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err());
    }

    fn current_scenarios() -> BTreeMap<String, Value> {
        // Captured from the current writer. Only machine-specific package
        // directories are normalized to null; this is not an old-save reader.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "fixtures/saved-scenarios-v{}.json",
            crate::engine::ARCHIVE_VERSION
        ));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "{}: {error}; record the current writer with TOR_RECORD_SCENARIO_FIXTURES",
                path.display()
            )
        });
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn current_writer_matches_golden_scenario_fixture() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
        let mut actual = BTreeMap::new();
        for (name, path) in [
            ("two-room", "two-room"),
            ("items", "tests/items"),
            ("first-dungeon", "first-dungeon"),
            ("interactions", "tests/interactions"),
            ("ai-interactions", "tests/ai-interactions"),
        ] {
            let mut scenario = source::load(&root.join(path), 42, None, false).unwrap();
            let mut package = (**scenario.package.as_ref().unwrap()).clone();
            package.directory = None;
            scenario.package = Some(Arc::new(package));
            actual.insert(
                name.to_owned(),
                serde_json::to_value(Borrowed(&scenario)).unwrap(),
            );
        }
        let diagnostic = crate::Scenario::performance(42, 8, 2).unwrap();
        actual.insert(
            "diagnostic".into(),
            serde_json::to_value(Borrowed(&diagnostic)).unwrap(),
        );
        if let Some(output) = std::env::var_os("TOR_RECORD_SCENARIO_FIXTURES") {
            let mut bytes = serde_json::to_vec_pretty(&actual).unwrap();
            bytes.push(b'\n');
            std::fs::write(output, bytes).unwrap();
            return;
        }
        assert_eq!(actual, current_scenarios());
    }

    #[test]
    fn stored_package_metadata_requires_current_owned_fields() {
        let fixtures = current_scenarios();
        let original = &fixtures["first-dungeon"];
        assert_saved_nested_fields::<crate::Scenario>(
            original,
            "/package",
            &[
                "manifest",
                "rules_document",
                "certificate",
                "validated",
                "selected",
            ],
        );
        assert_saved_nested_fields::<crate::Scenario>(
            original,
            "/package/certificate",
            &[
                "validator",
                "ruleset",
                "ruleset_hash",
                "ruleset_dependencies",
                "content_hash",
                "files",
                "regions",
                "model_hash",
                "coverage",
            ],
        );
    }

    #[test]
    fn compiled_package_requires_current_certificate_identities() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let encoded = encode_content_package(&package).unwrap();
        decode_content_package(&encoded).unwrap();
        let original: Value = serde_json::from_slice(&encoded).unwrap();
        for field in ["ruleset", "validator"] {
            let mut unsupported = original.clone();
            unsupported["package"]["certificate"][field] =
                Value::String(format!("unsupported-{field}"));
            assert!(
                decode_content_package(&serde_json::to_vec(&unsupported).unwrap()).is_err(),
                "accepted unsupported {field} identity"
            );
        }
    }

    fn assert_saved_nested_fields<T: Stored>(original: &Value, path: &str, fields: &[&str]) {
        strict::<Owned<T>>(&serde_json::to_vec(original).unwrap()).unwrap();
        let object = original
            .pointer(path)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("missing saved object {path}"));
        let mut unknown = original.clone();
        unknown
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), Value::Bool(true));
        assert!(
            strict::<Owned<T>>(&serde_json::to_vec(&unknown).unwrap()).is_err(),
            "{path} accepted an unknown field"
        );
        for field in fields {
            let expected = object
                .get(*field)
                .unwrap_or_else(|| panic!("missing saved field {path}/{field}"));
            let mut missing = original.clone();
            assert!(missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(*field)
                .is_some());
            assert!(
                strict::<Owned<T>>(&serde_json::to_vec(&missing).unwrap()).is_err(),
                "{path} accepted missing field {field}"
            );
            // Replace the selected object with a raw duplicate-member object;
            // serde_json::Value itself cannot retain duplicate fields.
            let mut marked = original.clone();
            *marked.pointer_mut(path).unwrap() = Value::String("duplicate_saved_object".into());
            let text = serde_json::to_string(&marked).unwrap();
            let marker = "\"duplicate_saved_object\"";
            assert_eq!(text.matches(marker).count(), 1);
            let encoded = serde_json::to_string(object).unwrap();
            let duplicate = format!(
                "{{{}:{},{}",
                serde_json::to_string(field).unwrap(),
                serde_json::to_string(expected).unwrap(),
                &encoded[1..]
            );
            let text = text.replacen(marker, &duplicate, 1);
            assert!(
                strict::<Owned<T>>(text.as_bytes()).is_err(),
                "{path} accepted duplicate field {field}"
            );
        }
    }

    #[test]
    fn saved_builds_require_creation_and_nested_purchase_fields() {
        let fixtures = current_scenarios();
        let original = &fixtures["first-dungeon"];
        for (build, kind, choice) in [
            (
                "/package/manifest/characters/0/creature",
                "talent",
                "talent",
            ),
            (
                "/package/manifest/archetypes/dungeon.guardian/creature",
                "skill_rank",
                "skill",
            ),
        ] {
            assert_saved_nested_fields::<crate::Scenario>(
                original,
                build,
                &["starting_skills", "restoration", "nutrition", "hit_dice"],
            );
            let advancement = format!("{build}/hit_dice/0");
            assert_saved_nested_fields::<crate::Scenario>(
                original,
                &advancement,
                &["source", "purchases"],
            );
            let purchase = format!("{advancement}/purchases/0");
            assert_saved_nested_fields::<crate::Scenario>(
                original,
                &purchase,
                &["pool", "selection"],
            );
            let selection = format!("{purchase}/selection");
            assert_eq!(original.pointer(&selection).unwrap()["kind"], kind);
            assert_saved_nested_fields::<crate::Scenario>(original, &selection, &["kind", choice]);
        }
    }

    #[test]
    fn saved_skill_rank_purchase_fields_are_required_and_strict() {
        use crate::creature_authoring::{Purchase, PurchaseSelection};
        let purchase = Purchase {
            pool: "advancement".into(),
            selection: PurchaseSelection::SkillRank {
                skill: "athletics".into(),
            },
        };
        // Exercise the independent save writer/reader, not the authoring or
        // mutable-simulation serde implementations.
        let original = serde_json::to_value(Borrowed(&purchase)).unwrap();
        assert_saved_nested_fields::<Purchase>(&original, "", &["pool", "selection"]);
        assert_saved_nested_fields::<Purchase>(&original, "/selection", &["kind", "skill"]);
    }

    #[test]
    fn current_scenarios_round_trip_through_the_explicit_saved_schema() {
        for (name, expected) in current_scenarios() {
            let bytes = serde_json::to_vec(&expected).unwrap();
            let Owned(actual) = strict::<Owned<crate::Scenario>>(&bytes).unwrap();
            assert_eq!(
                serde_json::to_value(Borrowed(&actual)).unwrap(),
                expected,
                "{name}"
            );
            if let Some(package) = actual.package {
                assert!(
                    package.index.regions.is_empty(),
                    "index attachment stays separate"
                );
                assert_eq!(
                    package.sources.files_read(),
                    0,
                    "decode must not acquire regions"
                );
            }
        }
    }

    #[test]
    fn stored_scenario_coordinates_require_numbers_and_reject_unknown_fields() {
        let original = &current_scenarios()["diagnostic"];
        for (field, bad) in [
            ("region", Value::String("1".into())),
            ("x", Value::from(i64::from(i32::MAX) + 1)),
            ("unexpected", Value::Bool(true)),
        ] {
            let mut value = original.clone();
            value["actors"][0]["position"][field] = bad;
            assert!(
                strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn stored_metadata_rejects_unknown_nested_fields_at_each_owned_boundary() {
        let fixtures = current_scenarios();
        for (fixture, paths) in [(
            "first-dungeon",
            &[
                "/streaming",
                "/package",
                "/package/certificate",
                "/package/manifest",
                "/package/manifest/archetypes/dungeon.guardian",
                "/package/manifest/archetypes/dungeon.guardian/creature",
                "/package/manifest/archetypes/dungeon.guardian/creature/hit_dice/0",
                "/package/manifest/characters/0",
                "/package/manifest/characters/0/creature/hit_dice/3",
                "/package/manifest/creatures",
                "/package/manifest/creatures/species/stone_construct",
                "/package/manifest/creatures/species/stone_construct/attributes",
                "/package/manifest/creatures/species/stone_construct/melee",
                "/package/manifest/creatures/species/stone_construct/melee/strikes/0/on_success/0/damage",
                "/package/manifest/creatures/species/stone_construct/melee/strikes/0/on_success/0/damage/components/0/amount",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack/damage",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack/damage/primary",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack/damage/components/0/amount",
            ][..],
        )] {
            for path in paths {
                let mut value = fixtures[fixture].clone();
                value
                    .pointer_mut(path)
                    .and_then(Value::as_object_mut)
                    .unwrap_or_else(|| panic!("missing {fixture} fixture boundary {path}"))
                    .insert("unexpected".into(), Value::Bool(true));
                assert!(
                    strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
                    "{fixture}: {path} accepted an unknown field"
                );
            }
        }
    }

    #[test]
    fn stored_actor_definitions_reject_removed_combat_fields() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        macro_rules! reject {
            ($definition:expr, $kind:ty) => {{
                let mut value = serde_json::to_value(Borrowed($definition)).unwrap();
                strict::<Owned<$kind>>(&serde_json::to_vec(&value).unwrap()).unwrap();
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("combat".into(), serde_json::Value::Null);
                // Storage deliberately sanitizes decoding diagnostics. Validate
                // both the strict boundary and the underlying unknown-field cause.
                assert!(strict::<Owned<$kind>>(&serde_json::to_vec(&value).unwrap()).is_err());
                let error = serde_json::from_value::<Owned<$kind>>(value)
                    .err()
                    .expect("removed combat field must fail");
                assert!(
                    error.to_string().contains("unknown field `combat`"),
                    "{error}"
                );
            }};
        }
        reject!(&package.manifest.characters[0], source::Character);
        reject!(&source::Archetype::default(), source::Archetype);
    }

    #[test]
    fn all_stored_damage_names_preserve_their_pre_refactor_spellings() {
        for (value, name) in [
            (source::DamageType::Energy, "energy"),
            (source::DamageType::Impact, "impact"),
            (source::DamageType::Keen, "keen"),
            (source::DamageType::Spirit, "spirit"),
            (source::DamageType::Vital, "vital"),
            (source::DamageType::Psychic, "psychic"),
        ] {
            let bytes = serde_json::to_vec(&Borrowed(&value)).unwrap();
            assert_eq!(bytes, format!("\"{name}\"").as_bytes());
            assert_eq!(
                strict::<Owned<source::DamageType>>(&bytes).unwrap().0,
                value
            );
        }
    }

    #[test]
    fn stored_damage_collections_reject_duplicate_and_unknown_names() {
        for bytes in [
            br#"{"impact":1,"impact":2}"#.as_slice(),
            br#"{"unknown":1}"#.as_slice(),
        ] {
            assert!(strict::<Owned<BTreeMap<source::DamageType, u32>>>(bytes).is_err());
        }
        for bytes in [
            br#"["impact","impact"]"#.as_slice(),
            br#"["unknown"]"#.as_slice(),
        ] {
            assert!(strict::<Owned<BTreeSet<source::DamageType>>>(bytes).is_err());
        }
    }

    #[test]
    fn saved_stair_pair_metadata_and_generated_names_round_trip_strictly() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/paired-stairs");
        let scenario = source::load(&root, 42, None, true).unwrap();
        let bytes = serde_json::to_vec(&Borrowed(&scenario)).unwrap();
        let restored = strict::<Owned<crate::Scenario>>(&bytes).unwrap().0;
        assert_eq!(serde_json::to_vec(&Borrowed(&restored)).unwrap(), bytes);
        let package = restored.package.unwrap();
        assert_eq!(package.manifest.stair_pairs["first"].lower, "2/up");
        // The index is stored separately from the scenario's manifest metadata.
        let index = &scenario.package.as_ref().unwrap().index;
        let restored_index = decode_index(&encode_index(index).unwrap()).unwrap();
        assert_eq!(restored_index.regions[1].stair_anchors, ["up", "down"]);
        for bad in [
            br#"{"upper":"1/stair","lower":"2/up","lower":"2/down"}"#.as_slice(),
            br#"{"upper":"1/stair","lower":"2/up","gate":true}"#.as_slice(),
        ] {
            assert!(strict::<Owned<source::StairPair>>(bad).is_err());
        }
    }

    #[test]
    fn stored_indexes_preserve_existing_bytes_and_generated_flag_omission() {
        for bytes in [
            include_bytes!("../../../scenarios/two-room/index.json").as_slice(),
            include_bytes!("../../../scenarios/tests/generated-filler/index.json").as_slice(),
            include_bytes!("../../../scenarios/first-dungeon/index.json").as_slice(),
        ] {
            let index = decode_index(bytes).unwrap();
            assert_eq!(encode_index(&index).unwrap(), bytes);
            assert_eq!(index.to_bytes().unwrap(), bytes);
        }
    }

    #[test]
    fn stored_index_rejects_nested_duplicates_and_noncanonical_optional_fields() {
        let text = include_str!("../../../scenarios/two-room/index.json");
        for bad in [
            text.replacen(
                "\"start\":[1,1,0]",
                "\"start\":[0,0,0],\"start\":[1,1,0]",
                1,
            ),
            text.replacen("\"zone\":null,", "", 1),
            text.replacen(
                "\"chamber\":true",
                "\"chamber\":true,\"generated\":false",
                1,
            ),
            text.replacen("\"items\":", "\"unexpected\":true,\"items\":", 1),
        ] {
            assert_ne!(bad, text);
            assert!(decode_index(bad.as_bytes()).is_err());
        }
    }

    #[test]
    fn stored_index_requires_unique_increasing_region_identities() {
        let bytes = include_bytes!("../../../scenarios/two-room/index.json");
        let original: Value = serde_json::from_slice(bytes).unwrap();
        let mut duplicate = original.clone();
        duplicate["regions"][1]["id"] = Value::from(1);
        assert!(decode_index(&serde_json::to_vec(&duplicate).unwrap()).is_err());
        let mut descending = original;
        descending["regions"].as_array_mut().unwrap().reverse();
        assert!(decode_index(&serde_json::to_vec(&descending).unwrap()).is_err());
    }
}

#[cfg(test)]
mod ordered_strike_save_tests {
    use super::*;
    use serde_json::{json, Value};
    fn source() -> Value {
        json!({"kind": "humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000}, "subtypes": [], "anatomy": null, "modifiers": [],
            "attributes": {"strength": 1, "speed": 1, "intellect": 1, "willpower": 1, "awareness": 1, "presence": 1},
            "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 10, "recovery": 20,
                "strikes": [{"offset": 0, "unlock": null, "check": null, "on_success": [], "on_failure": [], "descriptors": [], "riders": []}]}})
    }
    #[test]
    fn authored_species_rejects_a_complete_positional_record() {
        let author: crate::creature_authoring::Species = serde_json::from_value(source()).unwrap();
        let named = serde_json::to_value(author).unwrap();
        let fields = [
            "behavior",
            "displacement",
            "kind",
            "subtypes",
            "attributes",
            "anatomy",
            "melee",
            "modifiers",
        ];
        assert_eq!(named.as_object().unwrap().len(), fields.len());
        let positional = Value::Array(
            fields
                .into_iter()
                .map(|field| named[field].clone())
                .collect(),
        );
        assert!(serde_json::from_value::<crate::creature_authoring::Species>(positional).is_err());
    }

    #[test]
    fn authored_behavior_policy_rejects_memory_beyond_the_current_bound() {
        for memory_ticks in [
            0,
            tor_simulation::limits::NUMERIC as u64,
            tor_simulation::limits::NUMERIC as u64 + 1,
            u64::MAX,
        ] {
            let value =
                json!({"disposition":"passive", "idle":"wait", "memory_ticks":memory_ticks});
            assert_eq!(
                serde_json::from_value::<crate::creature_authoring::BehaviorPolicy>(value).is_ok(),
                memory_ticks <= tor_simulation::limits::NUMERIC as u64,
                "memory_ticks {memory_ticks}"
            );
        }
    }

    #[test]
    fn stored_behavior_policy_rejects_memory_beyond_its_independent_current_bound() {
        for memory_ticks in [
            0,
            tor_simulation::limits::NUMERIC as u64,
            tor_simulation::limits::NUMERIC as u64 + 1,
            u64::MAX,
        ] {
            let value = json!({"disposition":"passive", "idle":"wait", "memory_ticks":memory_ticks, "doors":false, "care":false, "equipment":false, "loot":false, "gold":false, "flee_percent":25});
            assert_eq!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::BehaviorPolicy>>(
                    &serde_json::to_vec(&value).unwrap()
                )
                .is_ok(),
                memory_ticks <= tor_simulation::limits::NUMERIC as u64,
                "memory_ticks {memory_ticks}"
            );
        }
    }

    #[test]
    fn ordered_strikes_stored_rider_trigger_has_an_independent_required_current_shape() {
        for trigger in ["success", "failure"] {
            let value = json!({"trigger":trigger,"condition":null,"check":null,"effects":[],"descriptors":[]});
            let decoded: Owned<crate::creature_authoring::Rider> =
                crate::storage::codec::strict(&serde_json::to_vec(&value).unwrap())
                    .expect("save-owned rider trigger must decode");
            assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), value);
            let mut missing = value;
            missing.as_object_mut().unwrap().remove("trigger");
            assert!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::Rider>>(
                    &serde_json::to_vec(&missing).unwrap()
                )
                .is_err()
            );
        }
        let author: crate::creature_authoring::Species = serde_json::from_value(source()).unwrap();
        let mut value = serde_json::to_value(Borrowed(&author)).unwrap();
        assert!(
            crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                &serde_json::to_vec(&value).unwrap()
            )
            .is_ok()
        );
        value["melee"]["strikes"][0]["riders"] = json!([{"trigger":"failure","condition":null,"check":null,"effects":[],"descriptors":[]}]);
        assert!(
            crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                &serde_json::to_vec(&value).unwrap()
            )
            .is_err(),
            "stored checkless primary cannot own a failure trigger"
        );
    }
    #[test]
    fn ordered_strike_content_stored_payload_free_check_selector_rejects_unknown_fields() {
        for kind in ["natural_attack", "selected_weapon"] {
            let valid = json!({"kind": kind});
            assert!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::SkillSelector>>(
                    &serde_json::to_vec(&valid).unwrap()
                )
                .is_ok()
            );
            let forged = json!({"kind": kind, "unrecognized": true});
            assert!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::SkillSelector>>(
                    &serde_json::to_vec(&forged).unwrap()
                )
                .is_err(),
                "{kind}"
            );
        }
    }
    #[test]
    fn ordered_strike_content_catalog_storage_round_trips_independent_natural_records() {
        let author: crate::creature_authoring::Species =
            serde_json::from_value(source()).expect("natural authoring must decode");
        let stored = serde_json::to_vec(&Borrowed(&author)).unwrap();
        let restored: Owned<crate::creature_authoring::Species> =
            crate::storage::codec::strict(&stored).unwrap();
        assert_eq!(restored.0, author);
        assert_eq!(serde_json::to_vec(&Borrowed(&restored.0)).unwrap(), stored);
    }
    #[test]
    fn ordered_strike_content_catalog_storage_rejects_nested_unknowns_missing_lists_and_overflow() {
        let author: crate::creature_authoring::Species =
            serde_json::from_value(source()).expect("natural authoring must decode");
        let original = serde_json::to_value(Borrowed(&author)).unwrap();
        for pointer in ["/melee", "/melee/strikes/0"] {
            let mut forged = original.clone();
            forged
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("derived_cache".into(), json!({}));
            assert!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                    &serde_json::to_vec(&forged).unwrap()
                )
                .is_err()
            );
        }
        for field in [
            "on_success",
            "on_failure",
            "descriptors",
            "riders",
            "unlock",
            "check",
        ] {
            let mut forged = original.clone();
            forged["melee"]["strikes"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                    &serde_json::to_vec(&forged).unwrap()
                )
                .is_err(),
                "missing {field}"
            );
        }
        let mut forged = original;
        forged["melee"]["strikes"] = json!(vec![forged["melee"]["strikes"][0].clone(); 17]);
        assert!(
            crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                &serde_json::to_vec(&forged).unwrap()
            )
            .is_err()
        );
    }
    #[test]
    fn ordered_strike_content_stored_rider_schema_preserves_order_and_rejects_recursive_checks() {
        let mut value = source();
        value["melee"]["strikes"][0]["riders"] = json!([
            {"trigger": "success", "check": {"skill": {"kind": "named", "skill": "spellcasting"}, "defense": "spiritual"}, "effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 1}}], "descriptors": []},
            {"trigger": "success", "check": null, "effects": [{"kind": "heal", "amount": {"kind": "fixed", "amount": 2}}], "descriptors": []}]);
        let author: crate::creature_authoring::Species = serde_json::from_value(value).unwrap();
        let original = serde_json::to_value(Borrowed(&author)).unwrap();
        let bytes = serde_json::to_vec(&original).unwrap();
        let restored: Owned<crate::creature_authoring::Species> =
            crate::storage::codec::strict(&bytes).unwrap();
        assert_eq!(restored.0, author);
        let mut reordered = original.clone();
        reordered["melee"]["strikes"][0]["riders"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert_ne!(
            serde_json::to_vec(&reordered).unwrap(),
            bytes,
            "authored rider order participates in source identity"
        );
        for pointer in [
            "/melee/strikes/0/riders/0",
            "/melee/strikes/0/riders/0/check",
        ] {
            let mut forged = original.clone();
            forged
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("actor".into(), json!("target"));
            assert!(
                crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                    &serde_json::to_vec(&forged).unwrap()
                )
                .is_err()
            );
        }
        let mut forged = original;
        forged["melee"]["strikes"][0]["riders"] =
            json!(vec![forged["melee"]["strikes"][0]["riders"][0].clone(); 9]);
        assert!(
            crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(
                &serde_json::to_vec(&forged).unwrap()
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod advancement_boundary_tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn advancement_boundary_saved_rules_and_policy_require_named_records() {
        let document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let control = serde_json::to_value(Borrowed(&document)).unwrap();
        let decoded: Owned<tor_simulation::rules::RulesetDocument> =
            serde_json::from_slice(&serde_json::to_vec(&control).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), control);
        let mut missing = control.clone();
        missing.as_object_mut().unwrap().remove("advancement");
        assert!(
            serde_json::from_slice::<Owned<tor_simulation::rules::RulesetDocument>>(
                &serde_json::to_vec(&missing).unwrap()
            )
            .is_err()
        );
        for field in [
            "purchase_recovery",
            "respec_prepare",
            "respec_recovery",
            "received_loss_interval",
            "respec_ration",
        ] {
            let mut missing = control.clone();
            missing["advancement"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_slice::<Owned<tor_simulation::rules::RulesetDocument>>(
                    &serde_json::to_vec(&missing).unwrap()
                )
                .is_err(),
                "missing policy {field}"
            );
        }
        let mut policy_array = control.clone();
        policy_array["advancement"] = json!([
            control["advancement"]["purchase_recovery"],
            control["advancement"]["respec_prepare"],
            control["advancement"]["respec_recovery"],
            control["advancement"]["received_loss_interval"],
            control["advancement"]["respec_ration"]
        ]);
        assert!(
            serde_json::from_slice::<Owned<tor_simulation::rules::RulesetDocument>>(
                &serde_json::to_vec(&policy_array).unwrap()
            )
            .is_err()
        );
        let fields = [
            "id",
            "constants",
            "point_pools",
            "supplies",
            "abilities",
            "forms",
            "ability_tags",
            "talents",
            "expertises",
            "creature_types",
            "subtypes",
            "classes",
            "conditions",
            "descriptors",
            "flags",
            "resources",
            "health_recovery",
            "nutrition",
            "nutrition_condition_trigger",
            "advancement",
            "equipment_slots",
            "growth",
        ];
        assert_eq!(control.as_object().unwrap().len(), fields.len());
        let array = Value::Array(
            fields
                .into_iter()
                .map(|field| control[field].clone())
                .collect(),
        );
        assert!(
            serde_json::from_slice::<Owned<tor_simulation::rules::RulesetDocument>>(
                &serde_json::to_vec(&array).unwrap()
            )
            .is_err(),
            "changed stored RulesDocument must be named"
        );
    }
}

#[cfg(test)]
mod public_plan_stored_effect_tests {
    use super::*;
    #[test]
    fn public_plan_stored_effects_are_independent_strict_named_records() {
        for raw in [
            r#"{"kind":"observe_public_plan"}"#,
            r#"{"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"ability_tag":"weapon_attack"}"#,
        ] {
            let independently_restored: Owned<source::Effect> =
                crate::storage::codec::strict(raw.as_bytes())
                    .expect("independent current save effect supports observation");
            let source: source::Effect =
                serde_json::from_slice(raw.as_bytes()).expect("author control");
            assert_eq!(independently_restored.0, source);
            let encoded = serde_json::to_vec(&Borrowed(&source)).unwrap();
            let restored: Owned<source::Effect> =
                crate::storage::codec::strict(&encoded).expect("independent current save record");
            assert_eq!(restored.0, source);
            assert_eq!(
                serde_json::to_value(Borrowed(&source)).unwrap(),
                serde_json::from_str::<serde_json::Value>(raw).unwrap()
            );
        }
        for raw in [
            r#"["observe_public_plan"]"#,
            r#"["apply_qualified_condition_to_source","readied",300,"weapon_attack"]"#,
            r#"{"kind":"observe_public_plan","location":{}}"#,
            r#"{"location":{},"kind":"observe_public_plan"}"#,
            r#"{"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"duration":301,"ability_tag":"weapon_attack"}"#,
            r#"{"condition":"readied","duration":300,"duration":301,"ability_tag":"weapon_attack","kind":"apply_qualified_condition_to_source"}"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Owned<source::Effect>>(raw.as_bytes()).is_err(),
                "{raw}"
            );
        }
    }
}
fn stored_birth_ordinal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let ordinal = u64::deserialize(d)?;
    if !(1..=tor_simulation::limits::HIT_DICE as u64).contains(&ordinal) {
        return Err(serde::de::Error::custom(
            "invalid stored birth advancement ordinal",
        ));
    }
    Ok(ordinal)
}
fn stored_armor_reduction<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let reduction = u32::deserialize(d)?;
    if reduction > tor_simulation::limits::NUMERIC as u32 {
        return Err(serde::de::Error::custom("invalid stored armor reduction"));
    }
    Ok(reduction)
}
fn stored_corrosion_amount<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let amount = u32::deserialize(d)?;
    if !(1..=tor_simulation::limits::NUMERIC as u32).contains(&amount) {
        return Err(serde::de::Error::custom("invalid stored corrosion amount"));
    }
    Ok(amount)
}
impl Effect {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<source::Effect, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Theft {
                #[serde(with = "mapped")]
                selection: source::TheftSelection,
                #[serde(with = "mapped")]
                relocation: Option<source::RelocationGate>,
            },
            AlterEquipment {
                #[serde(with = "mapped")]
                selection: source::EquippedSelection,
                #[serde(with = "mapped")]
                alteration: source::EquipmentAlteration,
            },
            CorrodeArmor {
                slot: String,
                #[serde(deserialize_with = "stored_corrosion_amount")]
                amount: u32,
            },
            ApplyLinkedHold {
                #[serde(with = "mapped")]
                policy: source::HoldPolicy,
            },
            HoldDamage {
                #[serde(with = "DamageType")]
                category: source::DamageType,
                #[serde(deserialize_with = "required_option")]
                descriptor: Option<String>,
            },
            DrainLatestHitDie {},
            RestoreHitDie {},
            RoomLight {},
            StepSelfOneCell {},
            TeleportRandom {},
            TeleportToRayOrigin {},
            ArrestDescent {},
            PushTargetOneCell {
                #[serde(with = "PushDirection")]
                direction: source::PushDirection,
            },
            Damage {
                #[serde(with = "mapped")]
                damage: source::DamageExpr,
            },
            Heal {
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            HealPerHitDie {
                sides: u16,
                bonus: i32,
                #[serde(with = "mapped")]
                overflow: source::HealingOverflow,
            },
            LoseAttribute {
                attribute: String,
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            RestoreAttribute {
                attribute: String,
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            LoseMaximumHealth {
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            RestoreMaximumHealth {
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            Nourish {
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            ApplyCondition {
                condition: String,
                duration: u64,
            },
            ApplyPersistentCondition {
                condition: String,
                #[serde(deserialize_with = "required_option")]
                cancels: Option<String>,
            },
            ApplyConditionForTurns {
                condition: String,
                #[serde(with = "mapped")]
                duration: source::AmountExpr,
            },
            RemoveCondition {
                condition: String,
            },
            Restore {
                resource: String,
                #[serde(with = "mapped")]
                amount: source::AmountExpr,
            },
            SearchAdjacentGuises {},
            ObservePublicPlan {},
            ApplyQualifiedConditionToSource {
                condition: String,
                duration: u64,
                ability_tag: String,
            },
        }
        Ok(match super::named_map(deserializer)? {
            Fields::Theft {
                selection,
                relocation,
            } => source::Effect::Theft {
                selection,
                relocation,
            },
            Fields::AlterEquipment {
                selection,
                alteration,
            } => source::Effect::AlterEquipment {
                selection,
                alteration,
            },
            Fields::CorrodeArmor { slot, amount } => source::Effect::CorrodeArmor { slot, amount },
            Fields::ApplyLinkedHold { policy } => source::Effect::ApplyLinkedHold { policy },
            Fields::HoldDamage {
                category,
                descriptor,
            } => source::Effect::HoldDamage {
                category,
                descriptor,
            },
            Fields::DrainLatestHitDie {} => source::Effect::DrainLatestHitDie,
            Fields::RestoreHitDie {} => source::Effect::RestoreHitDie,
            Fields::RoomLight {} => source::Effect::RoomLight,
            Fields::StepSelfOneCell {} => source::Effect::StepSelfOneCell,
            Fields::TeleportRandom {} => source::Effect::TeleportRandom,
            Fields::TeleportToRayOrigin {} => source::Effect::TeleportToRayOrigin,
            Fields::ArrestDescent {} => source::Effect::ArrestDescent,
            Fields::PushTargetOneCell { direction } => {
                source::Effect::PushTargetOneCell { direction }
            }
            Fields::Damage { damage } => source::Effect::Damage { damage },
            Fields::Heal { amount } => source::Effect::Heal { amount },
            Fields::HealPerHitDie {
                sides,
                bonus,
                overflow,
            } => source::Effect::HealPerHitDie {
                sides,
                bonus,
                overflow,
            },
            Fields::LoseAttribute { attribute, amount } => {
                source::Effect::LoseAttribute { attribute, amount }
            }
            Fields::RestoreAttribute { attribute, amount } => {
                source::Effect::RestoreAttribute { attribute, amount }
            }
            Fields::LoseMaximumHealth { amount } => source::Effect::LoseMaximumHealth { amount },
            Fields::RestoreMaximumHealth { amount } => {
                source::Effect::RestoreMaximumHealth { amount }
            }
            Fields::Nourish { amount } => source::Effect::Nourish { amount },
            Fields::ApplyCondition {
                condition,
                duration,
            } => source::Effect::ApplyCondition {
                condition,
                duration,
            },
            Fields::ApplyPersistentCondition { condition, cancels } => {
                source::Effect::ApplyPersistentCondition { condition, cancels }
            }
            Fields::ApplyConditionForTurns {
                condition,
                duration,
            } => source::Effect::ApplyConditionForTurns {
                condition,
                duration,
            },
            Fields::RemoveCondition { condition } => source::Effect::RemoveCondition { condition },
            Fields::Restore { resource, amount } => source::Effect::Restore { resource, amount },
            Fields::SearchAdjacentGuises {} => source::Effect::SearchAdjacentGuises,
            Fields::ObservePublicPlan {} => source::Effect::ObservePublicPlan,
            Fields::ApplyQualifiedConditionToSource {
                condition,
                duration,
                ability_tag,
            } => source::Effect::ApplyQualifiedConditionToSource {
                condition,
                duration,
                ability_tag,
            },
        })
    }
}

#[cfg(test)]
mod form_boundary_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stored_forms_and_talent_choices_require_complete_bounded_named_bytes() {
        let mut document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        document.forms = vec![tor_simulation::rules::FormDef {
            id: tor_simulation::content_ids::FormId::new("test.spark").unwrap(),
            name: "Spark".into(),
            ability: tor_simulation::content_ids::AbilityId::new("spark_lance_fire").unwrap(),
        }];
        document.talents[0].form_choices = vec![document.forms[0].id.clone()];
        let original = serde_json::to_value(Borrowed(&document)).unwrap();
        let decode = |value: &serde_json::Value| {
            serde_json::from_slice::<Owned<tor_simulation::rules::RulesetDocument>>(
                &serde_json::to_vec(value).unwrap(),
            )
        };
        assert_eq!(decode(&original).unwrap().0, document);
        for path in ["/forms/0", "/talents/0"] {
            let fields: Vec<_> = original
                .pointer(path)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            for field in fields {
                let mut missing = original.clone();
                missing
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(&field);
                assert!(decode(&missing).is_err(), "missing {path}/{field}");
            }
            let mut unknown = original.clone();
            unknown.pointer_mut(path).unwrap()["extra"] = json!(true);
            assert!(decode(&unknown).is_err());
        }
        for (path, malformed) in [
            ("/forms", json!(null)),
            (
                "/forms/0",
                json!(["test.spark", "Spark", "spark_lance_fire"]),
            ),
            ("/talents/0/form_choices", json!(null)),
            ("/talents/0/form_choices", json!(vec!["test.spark"; 17])),
            ("/forms", json!(vec![original["forms"][0].clone(); 4097])),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(path).unwrap() = malformed;
            assert!(decode(&value).is_err(), "invalid {path}");
        }
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove("forms");
        assert!(decode(&missing).is_err());
        let bytes = serde_json::to_string(&original).unwrap();
        let duplicate = bytes.replacen(
            "\"form_choices\":[\"test.spark\"]",
            "\"form_choices\":[\"test.spark\"],\"form_choices\":[]",
            1,
        );
        assert_ne!(duplicate, bytes);
        assert!(
            serde_json::from_str::<Owned<tor_simulation::rules::RulesetDocument>>(&duplicate)
                .is_err()
        );
    }

    #[test]
    fn stored_form_catalog_order_is_canonical_and_choice_order_is_significant() {
        let mut document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        document.forms = ["test.z", "test.a"]
            .into_iter()
            .map(|id| tor_simulation::rules::FormDef {
                id: tor_simulation::content_ids::FormId::new(id).unwrap(),
                name: id.into(),
                ability: tor_simulation::content_ids::AbilityId::new("spark_lance_fire").unwrap(),
            })
            .collect();
        document.talents[0].form_choices =
            document.forms.iter().map(|form| form.id.clone()).collect();
        let encoded = encode_rules_document(&document).unwrap();
        assert_eq!(
            encode_rules_document(&decode_rules_document(&encoded).unwrap()).unwrap(),
            encoded
        );
        document.forms.reverse();
        assert_eq!(encode_rules_document(&document).unwrap(), encoded);
        document.talents[0].form_choices.reverse();
        assert_ne!(encode_rules_document(&document).unwrap(), encoded);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::CreatureSize",
    rename_all = "snake_case"
)]
enum CreatureSize {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Colossal,
}
#[derive(Serialize)]
#[serde(
    remote = "crate::creature_authoring::DisplacementProfile",
    deny_unknown_fields
)]
struct DisplacementProfile {
    #[serde(with = "CreatureSize")]
    size: crate::creature_authoring::CreatureSize,
    anchored: bool,
}
impl DisplacementProfile {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<crate::creature_authoring::DisplacementProfile, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(with = "CreatureSize")]
            size: crate::creature_authoring::CreatureSize,
            anchored: bool,
        }
        let fields: Fields = super::named_map(deserializer)?;
        Ok(crate::creature_authoring::DisplacementProfile {
            size: fields.size,
            anchored: fields.anchored,
        })
    }
}
#[derive(Serialize)]
#[serde(
    remote = "tor_simulation::abilities::InvocationKind",
    rename_all = "snake_case"
)]
enum InvocationKind {
    ActorTarget,
    SelfStep,
    DirectionalRay,
    DirectionalContactRay,
    DirectionalProjectile,
}
impl InvocationKind {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::abilities::InvocationKind, D::Error> {
        use tor_simulation::abilities::InvocationKind as Runtime;
        match String::deserialize(deserializer)?.as_str() {
            "actor_target" => Ok(Runtime::ActorTarget),
            "self_step" => Ok(Runtime::SelfStep),
            "directional_ray" => Ok(Runtime::DirectionalRay),
            "directional_contact_ray" => Ok(Runtime::DirectionalContactRay),
            "directional_projectile" => Ok(Runtime::DirectionalProjectile),
            _ => Err(serde::de::Error::custom("invalid stored invocation kind")),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::PushDirection", rename_all = "snake_case")]
enum PushDirection {
    AwayFromSource,
}

#[cfg(test)]
mod growth_boundary_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn saved_growth_rules_require_complete_named_independent_records() {
        let document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let good = serde_json::to_value(Borrowed(&document)).unwrap();
        let _: Owned<tor_simulation::rules::RulesetDocument> =
            serde_json::from_value(good.clone()).unwrap();
        for path in ["/growth", "/growth/threat"] {
            for field in good.pointer(path).unwrap().as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad)
                        .is_err(),
                    "missing {path}/{field}"
                );
            }
            let mut bad = good.clone();
            *bad.pointer_mut(path).unwrap() = json!([]);
            assert!(
                serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad)
                    .is_err()
            );
            let mut bad = good.clone();
            bad.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("extra".into(), json!(1));
            assert!(
                serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad)
                    .is_err()
            );
        }
        let mut bad = good;
        bad.as_object_mut().unwrap().remove("growth");
        assert!(
            serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad).is_err()
        );
    }
    #[test]
    fn saved_recipe_requires_future_policy_without_authoring_defaults() {
        let source = json!({"species":"human","name":"owner","faction":"allies","binding":"intellect",
            "starting_skills":["athletics","lore"],"restoration":"authored", "nutrition": {"kind":"exempt"},"hit_dice":[{"source":"racial"}]});
        let spec: crate::creature_authoring::BuildSpec = serde_json::from_value(source).unwrap();
        let good = serde_json::to_value(Borrowed(&spec)).unwrap();
        assert_eq!(good["future_growth"], json!({"kind":"disabled"}));
        let _: Owned<crate::creature_authoring::BuildSpec> =
            serde_json::from_value(good.clone()).unwrap();
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove("future_growth");
        assert!(
            serde_json::from_value::<Owned<crate::creature_authoring::BuildSpec>>(bad).is_err()
        );
        for future in [
            json!({"kind":"disabled","class":"warrior"}),
            json!({"kind":"selected"}),
            json!({"kind":"selected","source":{"kind":"class"}}),
        ] {
            let mut bad = good.clone();
            bad["future_growth"] = future;
            assert!(
                serde_json::from_value::<Owned<crate::creature_authoring::BuildSpec>>(bad).is_err()
            );
        }
    }
}

#[cfg(test)]
mod condition_boundary_tests {
    use super::*;
    use serde_json::{json, Value};
    use tor_simulation::rules::{ConditionDef, RulesetDocument};

    fn condition() -> Value {
        json!({
            "id": "test.held",
            "family": "test.hold",
            "lifecycle": "timed",
            "name": "Held",
            "modifiers": [],
            "stacking": {"kind": "keep"},
            "source_scoped": false,
            "descriptors": [],
            "capacity": "reject",
            "timing": {
                "kind": "actor_active",
                "on_expiry": {"kind": "protect", "duration": 400}
            },
            "restrictions": ["voluntary_actions", "ordinary_movement", "movement_abilities"],
            "preparation": "pause"
        })
    }

    #[test]
    fn saved_condition_semantics_round_trip_without_live_serializers() {
        for lifecycle in ["timed", "next_check", "next_check_against"] {
            for stacking in [
                json!({"kind": "keep"}),
                json!({"kind": "refresh"}),
                json!({"kind": "longest"}),
                json!({"kind": "stack", "maximum": 3}),
            ] {
                for expiry in [
                    json!({"kind": "remove"}),
                    json!({"kind": "protect", "duration": 400}),
                ] {
                    for preparation in ["preserve", "pause"] {
                        let mut value = condition();
                        value["lifecycle"] = json!(lifecycle);
                        value["stacking"] = stacking.clone();
                        value["timing"]["on_expiry"] = expiry.clone();
                        value["preparation"] = json!(preparation);
                        let decoded: Owned<ConditionDef> =
                            serde_json::from_value(value.clone()).unwrap();
                        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
                    }
                }
            }
        }
    }

    #[test]
    fn saved_condition_records_require_every_field_and_reject_positional_aliases() {
        let good = condition();
        for path in ["", "/stacking", "/timing", "/timing/on_expiry"] {
            for field in good.pointer(path).unwrap().as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    serde_json::from_value::<Owned<ConditionDef>>(bad).is_err(),
                    "missing {path}/{field}"
                );
            }
            let mut bad = good.clone();
            bad.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!(true));
            assert!(serde_json::from_value::<Owned<ConditionDef>>(bad).is_err());
            let mut bad = good.clone();
            *bad.pointer_mut(path).unwrap() = json!([]);
            assert!(serde_json::from_value::<Owned<ConditionDef>>(bad).is_err());
        }
        let positional = json!([
            "test.held", "test.hold", "timed", "Held", [], {"kind": "keep"},
            false, [], "reject", {"kind": "actor_active", "on_expiry": {"kind": "remove"}},
            [], "preserve"
        ]);
        assert!(serde_json::from_value::<Owned<ConditionDef>>(positional).is_err());
    }

    #[test]
    fn saved_condition_semantics_reject_unknown_values_and_surplus_unit_fields() {
        for (path, invalid) in [
            ("/lifecycle", json!("unknown")),
            ("/lifecycle", json!([])),
            ("/lifecycle", json!({"timed": null})),
            ("/capacity", json!("unknown")),
            ("/capacity", json!({"reject": null})),
            ("/preparation", json!("unknown")),
            ("/preparation", json!({"kind": "pause"})),
            ("/preparation", json!({"pause": null})),
            ("/restrictions", json!(["unknown"])),
            ("/restrictions", json!([["voluntary_actions"]])),
            ("/restrictions", json!([{"voluntary_actions": null}])),
            ("/stacking", json!({"kind": "unknown"})),
            ("/stacking", json!({"kind": "keep", "maximum": 1})),
            ("/stacking", json!({"kind": "refresh", "maximum": 1})),
            ("/stacking", json!({"kind": "longest", "maximum": 1})),
            ("/stacking", json!({"kind": "stack"})),
            (
                "/timing",
                json!({"kind": "unknown", "on_expiry": {"kind": "remove"}}),
            ),
            ("/timing/on_expiry", json!({"kind": "unknown"})),
            (
                "/timing/on_expiry",
                json!({"kind": "remove", "duration": 400}),
            ),
            ("/timing/on_expiry", json!({"kind": "protect"})),
            ("/timing/on_expiry/duration", json!(0)),
            (
                "/timing/on_expiry/duration",
                json!(tor_simulation::limits::NUMERIC as u64 + 1),
            ),
            ("/timing/on_expiry/duration", json!(u64::MAX)),
            ("/timing/on_expiry/duration", json!(-1)),
            ("/timing/on_expiry/duration", json!(1.5)),
            ("/timing/on_expiry/duration", json!([400])),
        ] {
            let mut bad = condition();
            *bad.pointer_mut(path).unwrap() = invalid;
            assert!(
                serde_json::from_value::<Owned<ConditionDef>>(bad).is_err(),
                "invalid {path}"
            );
        }
    }

    #[test]
    fn saved_condition_records_reject_duplicate_fields_from_bytes() {
        let value = condition();
        let good = serde_json::to_string(&value).unwrap();
        for path in ["", "/stacking", "/timing", "/timing/on_expiry"] {
            let record = value.pointer(path).unwrap();
            let original = serde_json::to_string(record).unwrap();
            for (field, operand) in record.as_object().unwrap() {
                let duplicate = format!(
                    "{{{}:{},{}",
                    serde_json::to_string(field).unwrap(),
                    serde_json::to_string(operand).unwrap(),
                    &original[1..]
                );
                let bad = good.replacen(&original, &duplicate, 1);
                assert_ne!(bad, good);
                assert!(
                    serde_json::from_str::<Owned<ConditionDef>>(&bad).is_err(),
                    "duplicate {path}/{field}"
                );
            }
        }
    }

    #[test]
    fn saved_condition_collections_are_bounded_and_restrictions_are_unique() {
        let modifier = json!({
            "scope": "always",
            "op": {"kind": "add", "stat": {"kind": "edge"}, "value": 1}
        });
        for (field, values) in [
            (
                "restrictions",
                json!(["ordinary_movement", "ordinary_movement"]),
            ),
            ("restrictions", json!(vec!["ordinary_movement"; 4])),
            (
                "modifiers",
                json!(vec![
                    modifier;
                    tor_simulation::limits::MODIFIERS_PER_SOURCE + 1
                ]),
            ),
            (
                "descriptors",
                json!(vec![
                    "test.descriptor";
                    tor_simulation::limits::DESCRIPTORS_PER_EFFECT + 1
                ]),
            ),
        ] {
            let mut bad = condition();
            bad[field] = values;
            assert!(
                serde_json::from_value::<Owned<ConditionDef>>(bad).is_err(),
                "invalid {field} collection"
            );
        }
        let mut good = condition();
        good["restrictions"] = json!([]);
        good["capacity"] = json!("oldest");
        assert!(serde_json::from_value::<Owned<ConditionDef>>(good).is_ok());
    }

    #[test]
    fn saved_condition_protection_accepts_exact_duration_bounds() {
        for duration in [1, tor_simulation::limits::NUMERIC as u64] {
            let mut good = condition();
            good["timing"]["on_expiry"]["duration"] = json!(duration);
            let restored: Owned<ConditionDef> = serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), good);
        }
    }

    #[test]
    fn saved_rules_conditions_use_the_strict_bounded_independent_adapter() {
        let mut document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        document.conditions = vec![
            serde_json::from_value::<Owned<ConditionDef>>(condition())
                .unwrap()
                .0,
        ];
        let good = serde_json::to_value(Borrowed(&document)).unwrap();
        assert_eq!(good["conditions"], json!([condition()]));
        let restored: Owned<RulesetDocument> = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(restored.0, document);
        let encoded = encode_rules_document(&document).unwrap();
        assert_eq!(
            encode_rules_document(&decode_rules_document(&encoded).unwrap()).unwrap(),
            encoded
        );
        for field in ["timing", "restrictions", "preparation"] {
            let mut bad = good.clone();
            bad["conditions"][0].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Owned<RulesetDocument>>(bad).is_err());
        }
        let mut bad = good;
        bad["conditions"] = json!(vec![
            condition();
            tor_simulation::limits::CATALOG_ENTRIES + 1
        ]);
        assert!(serde_json::from_value::<Owned<RulesetDocument>>(bad).is_err());
    }
}

#[cfg(test)]
mod population_boundary_tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn carried_stock_saved_authoring_requires_explicit_nullable_and_empty_fields() {
        let activation: source::FloorActivation = serde_json::from_value(json!({
            "version":1,"protagonist":"selected_character","profiles":{}
        }))
        .unwrap();
        let encoded = serde_json::to_value(Borrowed(&activation)).unwrap();
        assert_eq!(encoded["carried_item_pools"], json!({}));
        assert_eq!(encoded["wandering"], Value::Null);
        reject_shape(&activation);
        let stage: source::PopulationStage = serde_json::from_value(json!({
            "kind":"actors","id":"inhabitants","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
                "archetype":"marker","weight":1
            }]}]
        }))
        .unwrap();
        let encoded = serde_json::to_value(Borrowed(&stage)).unwrap();
        assert_eq!(encoded["depth_growth"], json!({"kind":"fixed"}));
        let mut missing_growth = encoded.clone();
        missing_growth
            .as_object_mut()
            .unwrap()
            .remove("depth_growth");
        assert!(serde_json::from_value::<Owned<source::PopulationStage>>(missing_growth).is_err());
        let choice = &encoded["bands"][0]["choices"][0];
        assert!(choice.as_object().unwrap().contains_key("carried_loot"));
        assert_eq!(choice["carried_loot"], Value::Null);
        let mut missing = encoded;
        missing["bands"][0]["choices"][0]
            .as_object_mut()
            .unwrap()
            .remove("carried_loot");
        assert!(serde_json::from_value::<Owned<source::PopulationStage>>(missing).is_err());
    }

    #[test]
    fn carried_stock_saved_pool_and_policy_records_are_complete_named_and_bounded() {
        let activation: source::FloorActivation = serde_json::from_value(json!({
            "version":1,"protagonist":"selected_character","profiles":{},
            "carried_item_pools":{"supplies":{"version":1,"choices":[
                {"archetype":"stock","weight":2,"quantity":7}
            ]}}
        }))
        .unwrap();
        let pool = &activation.carried_item_pools["supplies"];
        reject_shape(pool);
        reject_shape(&pool.choices[0]);
        let actor: source::ActorPopulationChoice = serde_json::from_value(json!({
            "archetype":"marker","weight":1,"carried_loot":{
                "version":1,"pool":"supplies","chance":{"numerator":1,"denominator":5}
            }
        }))
        .unwrap();
        reject_shape(&actor);
        reject_shape(actor.carried_loot.as_ref().unwrap());
        reject_shape(&actor.carried_loot.as_ref().unwrap().chance);
        let good = serde_json::to_value(Borrowed(&activation)).unwrap();
        let mut too_many_choices = good.clone();
        too_many_choices["carried_item_pools"]["supplies"]["choices"] = json!(vec![
            json!({"archetype":"stock","weight":2,"quantity":7});
            65
        ]);
        assert!(
            serde_json::from_value::<Owned<source::FloorActivation>>(too_many_choices).is_err()
        );
        let mut boundary = good.clone();
        boundary["carried_item_pools"] = Value::Object(
            (0..32)
                .map(|n| {
                    (
                        format!("pool{n}"),
                        good["carried_item_pools"]["supplies"].clone(),
                    )
                })
                .collect(),
        );
        let _: Owned<source::FloorActivation> = serde_json::from_value(boundary.clone()).unwrap();
        boundary["carried_item_pools"]["overflow"] = good["carried_item_pools"]["supplies"].clone();
        assert!(serde_json::from_value::<Owned<source::FloorActivation>>(boundary).is_err());
        let raw = serde_json::to_string(&good).unwrap();
        let entry = format!("\"supplies\":{}", good["carried_item_pools"]["supplies"]);
        let duplicate = raw.replacen(&entry, &format!("{entry},{entry}"), 1);
        assert_ne!(duplicate, raw);
        assert!(serde_json::from_str::<Owned<source::FloorActivation>>(&duplicate).is_err());
        for field in ["version", "pool", "chance"] {
            let encoded = serde_json::to_value(Borrowed(&actor)).unwrap();
            let raw = serde_json::to_string(&encoded).unwrap();
            let policy =
                serde_json::to_value(Borrowed(actor.carried_loot.as_ref().unwrap())).unwrap();
            let entry = format!("\"{field}\":{}", policy[field]);
            let duplicate = raw.replacen(&entry, &format!("{entry},{entry}"), 1);
            assert_ne!(duplicate, raw);
            assert!(
                serde_json::from_str::<Owned<source::ActorPopulationChoice>>(&duplicate).is_err()
            );
        }
    }

    #[test]
    fn carried_stock_compiled_package_preserves_nonempty_pools_and_actor_policy() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/rogue-geometry");
        let mut manifest = source::resolve_source(&root).unwrap().manifest;
        manifest.archetypes.insert(
            "marker".into(),
            serde_json::from_value(json!({"name":"marker"})).unwrap(),
        );
        manifest.archetypes.insert(
            "stock".into(),
            serde_json::from_value(json!({"name":"finite stock","stackable":true})).unwrap(),
        );
        manifest.floor_activation = Some(
            serde_json::from_value(json!({
                "version":1,"protagonist":"selected_character","families":{"dungeon":"plain"},
                "carried_item_pools":{"supplies":{"version":1,"choices":[
                    {"archetype":"marker","weight":2,"quantity":1},
                    {"archetype":"stock","weight":3,"quantity":7}
                ]}},
                "profiles":{"plain":{"version":1,"stages":[{
                    "kind":"actors","id":"inhabitants","version":1,"count":[1,1],
                    "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
                        "archetype":"marker","weight":1,"carried_loot":{
                            "version":1,"pool":"supplies","chance":{"numerator":1,"denominator":5}
                        }
                    }]}]
                }]}}
            }))
            .unwrap(),
        );
        let package = source::Package::from_parts(
            manifest,
            vec![],
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap();
        let encoded = encode_content_package(&package).unwrap();
        let restored = decode_content_package(&encoded).unwrap();
        assert_eq!(encode_content_package(&restored).unwrap(), encoded);
        assert_eq!(
            restored.build(42, false).unwrap().activations_since(0),
            package.build(42, false).unwrap().activations_since(0)
        );
        let good: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            good["package"]["manifest"]["floor_activation"]["carried_item_pools"]["supplies"]
                ["choices"][1]["quantity"],
            json!({"kind":"fixed", "quantity":7})
        );
        for path in [
            "/package/manifest/floor_activation/carried_item_pools",
            "/package/manifest/floor_activation/profiles/plain/stages/0/bands/0/choices/0/carried_loot",
            "/package/manifest/floor_activation/carried_item_pools/supplies/choices/0/quantity",
        ] {
            let mut missing = good.clone();
            let (parent, field) = path.rsplit_once('/').unwrap();
            missing
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                decode_content_package(&serde_json::to_vec(&missing).unwrap()).is_err(),
                "{path}"
            );
        }
        let actor = &good["package"]["manifest"]["floor_activation"]["profiles"]["plain"]["stages"]
            [0]["bands"][0]["choices"][0];
        for invalid in [
            json!(false),
            json!("none"),
            json!({}),
            json!({"kind":"not_carried"}),
        ] {
            let mut bad = actor.clone();
            bad["carried_loot"] = invalid;
            assert!(serde_json::from_value::<Owned<source::ActorPopulationChoice>>(bad).is_err());
        }
    }

    #[test]
    fn compiled_weighted_population_requires_exact_current_band_operands() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/rogue-geometry");
        let mut manifest = source::resolve_source(&root).unwrap().manifest;
        manifest.archetypes.insert(
            "marker".into(),
            serde_json::from_value(json!({"name":"marker"})).unwrap(),
        );
        manifest.floor_activation = Some(
            serde_json::from_value(json!({
                "version":1,"protagonist":"selected_character","families":{"dungeon":"plain"},
                "profiles":{"plain":{"version":1,"stages":[{
                    "id":"stock","version":1,"kind":"items","count":[1,1],"quantity":1,
                    "bands":[{"depth":{"kind":"open","minimum":1},
                        "choices":[{"archetype":"marker","weight":1}]}]
                }]}}
            }))
            .unwrap(),
        );
        let package = source::Package::from_parts(
            manifest,
            vec![],
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap();
        let encoded = encode_content_package(&package).unwrap();
        let restored = decode_content_package(&encoded).unwrap();
        assert_eq!(encode_content_package(&restored).unwrap(), encoded);
        let good: Value = serde_json::from_slice(&encoded).unwrap();
        for case in 0..7 {
            let mut bad = good.clone();
            let stage = &mut bad["package"]["manifest"]["floor_activation"]["profiles"]["plain"]
                ["stages"][0];
            match case {
                0 => stage["bands"][0]["depth"]["kind"] = json!("future"),
                1 => stage["bands"][0]["choices"][0]["weight"] = json!(0),
                2 => stage["bands"][0]["choices"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("weight")
                    .map(|_| ())
                    .unwrap(),
                3 => stage["bands"][0]["choices"][0]["alias"] = json!("marker"),
                4 => stage["bands"][0]["choices"][0]["archetype"] = json!("unknown"),
                5 => stage["minimum_depth"] = json!(1),
                6 => stage["bands"][0]["choices"][0]["weight"] = json!(2),
                _ => unreachable!(),
            }
            assert!(
                decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "case {case}"
            );
        }
    }

    fn reject_shape<T: Stored>(value: &T) {
        let good = serde_json::to_value(Borrowed(value)).unwrap();
        let _: Owned<T> = serde_json::from_value(good.clone()).unwrap();
        for key in good.as_object().unwrap().keys() {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<Owned<T>>(missing).is_err(),
                "missing {key}"
            );
        }
        let mut extra = good;
        extra["unexpected"] = json!(true);
        assert!(serde_json::from_value::<Owned<T>>(extra).is_err());
        assert!(serde_json::from_value::<Owned<T>>(json!([])).is_err());
    }

    #[test]
    fn saved_retrieval_policy_requires_complete_independent_named_fields() {
        let policy: source::UniqueRetrieval = serde_json::from_value(json!({
            "version":1,"protagonist":"selected_character","family":"dungeon",
            "minimum_depth":26,"archetype":"amulet","exit_anchor":"dungeon/1/up"
        }))
        .unwrap();
        reject_shape(&policy);
        assert!(
            serde_json::from_value::<Owned<source::UniqueRetrieval>>(json!([
                1,
                "selected_character",
                "dungeon",
                26,
                "amulet",
                "dungeon/1/up"
            ]))
            .is_err()
        );
        let encoded = serde_json::to_value(Borrowed(&policy)).unwrap();
        let raw = serde_json::to_string(&encoded).unwrap();
        for (field, value) in encoded.as_object().unwrap() {
            let pair = format!("{}:{}", serde_json::to_string(field).unwrap(), value);
            let duplicate = raw.replacen(&pair, &format!("{pair},{pair}"), 1);
            assert_ne!(duplicate, raw);
            assert!(
                serde_json::from_str::<Owned<source::UniqueRetrieval>>(&duplicate).is_err(),
                "duplicate {field}"
            );
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/rogue-geometry");
        let manifest = source::resolve_source(&root).unwrap().manifest;
        let mut encoded = serde_json::to_value(Borrowed(&manifest)).unwrap();
        assert!(encoded.get("unique_retrieval").is_some());
        encoded.as_object_mut().unwrap().remove("unique_retrieval");
        assert!(
            serde_json::from_value::<Owned<source::Manifest>>(encoded).is_err(),
            "current saved policy cannot default a missing objective field"
        );
    }

    #[test]
    fn saved_population_and_loadout_require_complete_named_independent_records() {
        let policy: source::ScenarioCapabilities =
            serde_json::from_value(json!({"npc_stair_traversal":false})).unwrap();
        reject_shape(&policy);
        let kit: source::CharacterLoadout =
            serde_json::from_value(json!({"archetype":"marker"})).unwrap();
        reject_shape(&kit);
        for kind in ["items", "actors"] {
            let stage: source::PopulationStage = serde_json::from_value(json!({"kind":kind,
                "id":"initial","version":1,"count":[1,1],
                "bands":[{"depth":{"kind":"open","minimum":1},
                "choices":[{"archetype":"marker","weight":1}]}]}))
            .unwrap();
            reject_shape(&stage);
        }
        let band: source::PopulationBand = serde_json::from_value(json!({
            "depth":{"kind":"finite","minimum":1,"maximum":2},
            "choices":[{"archetype":"marker","weight":1}]
        }))
        .unwrap();
        reject_shape(&band);
        reject_shape(&band.depth);
        reject_shape(&source::PopulationDepthGrowth::Fixed {});
        reject_shape(&source::PopulationDepthGrowth::RacialAfter { depth: 26 });
        reject_shape(&band.choices[0]);
        let activation: source::FloorActivation = serde_json::from_value(json!({
            "version":1,"protagonist":"selected_character","families":{"dungeon":"plain"},
            "profiles":{"plain":{"version":1,"stages":[]}}}))
        .unwrap();
        reject_shape(&activation);
        reject_shape(&activation.profiles["plain"]);
    }

    #[test]
    fn saved_population_rejects_over_limit_stage_and_choice_collections() {
        let stage: source::PopulationStage = serde_json::from_value(json!({"kind":"items",
            "id":"initial","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},
            "choices":[{"archetype":"marker","weight":1}]}]}))
        .unwrap();
        let good = serde_json::to_value(Borrowed(&stage)).unwrap();
        let mut bad = good.clone();
        bad["bands"][0]["choices"] =
            Value::Array(vec![json!({"archetype":"marker","weight":1}); 65]);
        assert!(serde_json::from_value::<Owned<source::PopulationStage>>(bad).is_err());
        let mut bad = good.clone();
        bad["bands"] = Value::Array(vec![good["bands"][0].clone(); 33]);
        assert!(serde_json::from_value::<Owned<source::PopulationStage>>(bad).is_err());
        let bad = json!({"version":1,"stages":vec![good;33]});
        assert!(serde_json::from_value::<Owned<source::PopulationProfile>>(bad).is_err());
    }
}

impl NutritionRequirement {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::nutrition::NutritionRequirement, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Exempt {},
            Required {},
        }
        Ok(match super::named_map(d)? {
            Fields::Exempt {} => tor_simulation::nutrition::NutritionRequirement::Exempt,
            Fields::Required {} => tor_simulation::nutrition::NutritionRequirement::Required,
        })
    }
}
impl RecoveryNutrition {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::health_recovery::RecoveryNutrition, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Normal {},
            Slowed { multiplier: u16 },
            Stopped {},
        }
        Ok(match super::named_map(d)? {
            Fields::Normal {} => tor_simulation::health_recovery::RecoveryNutrition::Normal,
            Fields::Slowed { multiplier } => {
                tor_simulation::health_recovery::RecoveryNutrition::Slowed { multiplier }
            }
            Fields::Stopped {} => tor_simulation::health_recovery::RecoveryNutrition::Stopped,
        })
    }
}
impl NutritionConditionTrigger {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::nutrition::NutritionConditionTrigger, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Disabled {},
            OnEmptyDepletion {
                condition: tor_simulation::content_ids::ConditionId,
                #[serde(with = "mapped")]
                chance: tor_simulation::nutrition::NutritionChance,
                #[serde(with = "mapped")]
                duration: tor_simulation::rules::AmountExpr,
                ticks_per_unit: u64,
            },
        }
        Ok(match super::named_map(d)? {
            Fields::Disabled {} => tor_simulation::nutrition::NutritionConditionTrigger::Disabled,
            Fields::OnEmptyDepletion {
                condition,
                chance,
                duration,
                ticks_per_unit,
            } => tor_simulation::nutrition::NutritionConditionTrigger::OnEmptyDepletion {
                condition,
                chance,
                duration,
                ticks_per_unit,
            },
        })
    }
}
impl NutritionPolicy {
    fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<tor_simulation::nutrition::NutritionPolicy, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Disabled {},
            Enabled {
                initial: u32,
                maximum: u32,
                hungry_below: u32,
                weak_below: u32,
                starve_at: i32,
                unit_ticks: u64,
                #[serde(with = "mapped")]
                hungry_recovery: tor_simulation::health_recovery::RecoveryNutrition,
                #[serde(with = "mapped")]
                weak_recovery: tor_simulation::health_recovery::RecoveryNutrition,
                #[serde(with = "mapped")]
                empty_recovery: tor_simulation::health_recovery::RecoveryNutrition,
                #[serde(with = "mapped")]
                regeneration_cost: tor_simulation::nutrition::RegenerationCost,
            },
        }
        Ok(match super::named_map(d)? {
            Fields::Disabled {} => tor_simulation::nutrition::NutritionPolicy::Disabled,
            Fields::Enabled {
                initial,
                maximum,
                hungry_below,
                weak_below,
                starve_at,
                unit_ticks,
                hungry_recovery,
                weak_recovery,
                empty_recovery,
                regeneration_cost,
            } => tor_simulation::nutrition::NutritionPolicy::Enabled {
                initial,
                maximum,
                hungry_below,
                weak_below,
                starve_at,
                unit_ticks,
                hungry_recovery,
                weak_recovery,
                empty_recovery,
                regeneration_cost,
            },
        })
    }
}
#[cfg(test)]
mod nutrition_schema_tests {
    use super::*;

    #[test]
    fn stored_edibles_round_trip_the_independent_fixed_and_rolled_named_operands() {
        for (json, toml) in [
            (
                r#"{"nutrition":{"kind":"fixed","amount":6}}"#,
                r#"nutrition = { kind = "fixed", amount = 6 }"#,
            ),
            (
                r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":1099}}}"#,
                r#"nutrition = { kind = "rolled", dice = { count = 1, sides = 400, bonus = 1099 } }"#,
            ),
        ] {
            let expected: serde_json::Value = serde_json::from_str(json).unwrap();
            let authored: source::EdibleSpec = toml::from_str(toml).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&authored)).unwrap(), expected);
            let saved: Owned<source::EdibleSpec> = serde_json::from_str(json).unwrap();
            assert_eq!(saved.0, authored);
            assert_eq!(serde_json::to_value(saved).unwrap(), expected);
        }
    }

    #[test]
    fn stored_edibles_reject_legacy_scalars_and_every_nested_positional_alias() {
        for raw in [
            "null",
            "{}",
            "[]",
            r#"{"nutrition":6}"#,
            r#"{"nutrition":null}"#,
            r#"{"nutrition":{"kind":"fixed","amount":6},"extra":0}"#,
            r#"{"nutrition":{"kind":"fixed","amount":6},"nutrition":{"kind":"fixed","amount":6}}"#,
            r#"[{"kind":"fixed","amount":6}]"#,
            r#"{"nutrition":{}}"#,
            r#"{"nutrition":["fixed",6]}"#,
            r#"{"nutrition":{"kind":"Fixed","amount":6}}"#,
            r#"{"nutrition":{"kind":"fixed"}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":null}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":6,"extra":0}}"#,
            r#"{"nutrition":{"kind":"fixed","kind":"fixed","amount":6}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":6,"amount":6}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":-1}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":6.5}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":"6"}}"#,
            r#"{"nutrition":{"kind":"fixed","amount":4294967296}}"#,
            r#"{"nutrition":{"kind":"rolled"}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":null}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":[1,400,1099]}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":null,"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":null,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":null}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":1099,"extra":0}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"count":1,"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":1099,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":-1,"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":65536,"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":65536,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":2147483648}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":-2147483649}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":true,"sides":400,"bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":"400","bonus":1099}}}"#,
            r#"{"nutrition":{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":1099.5}}}"#,
        ] {
            assert!(
                serde_json::from_str::<Owned<source::EdibleSpec>>(raw).is_err(),
                "stored edible accepted {raw}"
            );
        }
    }

    #[test]
    fn compiled_content_and_item_bearing_region_keep_the_declared_edible_operand() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = source::read_package(&root).unwrap();
        for amount in [
            serde_json::json!({"kind":"fixed","amount":6}),
            serde_json::json!({"kind":"rolled","dice":{"count":1,"sides":400,"bonus":1099}}),
        ] {
            let mut manifest = template.manifest.clone();
            manifest.archetypes.insert(
                "test_meal".into(),
                serde_json::from_value(serde_json::json!({
                    "name":"test meal", "identity":"test_meal", "class":"food",
                    "stackable":true, "edible":{"nutrition":amount.clone()}
                }))
                .unwrap(),
            );
            let mut regions = template.region_defs().unwrap();
            regions[0].items.push(
                serde_json::from_value(serde_json::json!({
                    "id":10,"at":[1,1,0],"carried_by":1,"archetype":"test_meal","quantity":2
                }))
                .unwrap(),
            );
            let region_input = encode_region_input(&regions[0]).unwrap();
            assert_eq!(
                encode_region_input(&decode_region_input(&region_input).unwrap()).unwrap(),
                region_input
            );
            // RegionInput owns the placement; ContentPackage owns the named
            // item definition. The runtime operand is never duplicated there.
            let placement: serde_json::Value = serde_json::from_str(&region_input).unwrap();
            let meal = placement["region"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == 10)
                .unwrap();
            assert_eq!(meal["archetype"], "test_meal");
            assert_eq!(meal["quantity"], 2);
            assert!(meal.get("edible").is_none());
            let package =
                source::Package::from_parts(manifest, regions, &template.rules_source).unwrap();
            let encoded = encode_content_package(&package).unwrap();
            let restored = decode_content_package(&encoded).unwrap();
            assert_eq!(encode_content_package(&restored).unwrap(), encoded);
            let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(
                value["package"]["manifest"]["archetypes"]["test_meal"]["edible"],
                serde_json::json!({"nutrition":amount})
            );
            assert_eq!(
                serde_json::to_value(Borrowed(
                    restored.manifest.archetypes["test_meal"]
                        .edible
                        .as_ref()
                        .unwrap()
                ))
                .unwrap(),
                serde_json::json!({"nutrition":amount})
            );
        }
    }

    #[test]
    fn stored_nutrition_policy_and_requirement_reject_missing_and_extra_fields() {
        for raw in [r#"{"kind":"exempt","extra":0}"#, "[]", "{}"] {
            assert!(
                serde_json::from_str::<Owned<tor_simulation::nutrition::NutritionRequirement>>(raw)
                    .is_err()
            );
        }
        for raw in [
            r#"{"kind":"disabled","extra":0}"#,
            "[]",
            r#"{"kind":"enabled"}"#,
        ] {
            assert!(
                serde_json::from_str::<Owned<tor_simulation::nutrition::NutritionPolicy>>(raw)
                    .is_err()
            );
        }
        let document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let good = serde_json::to_value(Borrowed(&document)).unwrap();
        assert_eq!(good["nutrition"]["kind"], "disabled");
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove("nutrition");
        assert!(
            serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad).is_err()
        );
        let restored: Owned<tor_simulation::rules::RulesetDocument> =
            serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), good);
    }
}

#[cfg(test)]
mod explicit_nutrition_recovery_tests {
    use super::*;
    use serde_json::{json, Value};
    use tor_simulation::nutrition::NutritionPolicy;

    fn policy() -> Value {
        json!({"kind":"enabled", "initial":8, "maximum":12,
            "hungry_below":7, "weak_below":3, "starve_at":-3, "unit_ticks":100,
            "hungry_recovery":{"kind":"normal"},
            "weak_recovery":{"kind":"slowed", "multiplier":3},
            "regeneration_cost":{"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0},
        "empty_recovery":{"kind":"stopped"}})
    }

    #[test]
    fn saved_regeneration_cost_has_independent_strict_named_shape() {
        let mut expected = policy();
        expected["regeneration_cost"] =
            json!({"denominator":3,"health":1,"stamina":2,"focus":3,"mana":4});
        let restored: Owned<NutritionPolicy> = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), expected);
        for field in ["denominator", "health", "stamina", "focus", "mana"] {
            let mut invalid = expected.clone();
            invalid["regeneration_cost"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<Owned<NutritionPolicy>>(invalid).is_err());
            let raw = serde_json::to_string(&expected).unwrap();
            let duplicate = raw.replacen(
                &format!("\"{field}\":"),
                &format!("\"{field}\":1,\"{field}\":"),
                1,
            );
            assert!(serde_json::from_str::<Owned<NutritionPolicy>>(&duplicate).is_err());
        }
        for cost in [
            json!([]),
            json!({"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0,"extra":0}),
        ] {
            let mut invalid = expected.clone();
            invalid["regeneration_cost"] = cost;
            assert!(serde_json::from_value::<Owned<NutritionPolicy>>(invalid).is_err());
        }
        expected
            .as_object_mut()
            .unwrap()
            .remove("regeneration_cost");
        assert!(serde_json::from_value::<Owned<NutritionPolicy>>(expected).is_err());
    }

    #[test]
    fn stored_explicit_nutrition_recovery_round_trips_without_authoring_sources() {
        for recovery in [
            json!({"kind":"normal"}),
            json!({"kind":"slowed", "multiplier":100}),
            json!({"kind":"stopped"}),
        ] {
            for field in ["hungry_recovery", "weak_recovery", "empty_recovery"] {
                let mut expected = policy();
                expected[field] = recovery.clone();
                let restored: Owned<NutritionPolicy> =
                    serde_json::from_value(expected.clone()).unwrap();
                assert!(restored.0.valid());
                assert_eq!(serde_json::to_value(&restored).unwrap(), expected);
                let mut document: tor_simulation::rules::RulesetDocument =
                    toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
                document.nutrition = restored.0;
                let encoded = encode_rules_document(&document).unwrap();
                let decoded = decode_rules_document(&encoded).unwrap();
                assert_eq!(encode_rules_document(&decoded).unwrap(), encoded);
                assert_eq!(
                    serde_json::to_value(Borrowed(&decoded.nutrition)).unwrap(),
                    expected
                );
                assert!(tor_simulation::rules::Ruleset::validate(decoded).is_ok());
            }
        }
    }

    #[test]
    fn stored_explicit_nutrition_recovery_rejects_missing_duplicate_and_invalid_fields() {
        let good = policy();
        assert!(
            serde_json::from_value::<Owned<NutritionPolicy>>(good.clone())
                .unwrap()
                .0
                .valid()
        );
        for field in ["hungry_recovery", "weak_recovery", "empty_recovery"] {
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Owned<NutritionPolicy>>(missing).is_err(),
                "{field}"
            );
            for invalid in [
                Value::Null,
                json!([]),
                json!({}),
                json!({"kind":"normal", "extra":0}),
                json!({"kind":"stopped", "multiplier":2}),
                json!({"kind":"slowed"}),
                json!({"kind":"slowed", "multiplier":0}),
                json!({"kind":"slowed", "multiplier":1}),
                json!({"kind":"slowed", "multiplier":101}),
                json!({"kind":"slowed", "multiplier":u64::MAX}),
                json!({"kind":"unknown"}),
            ] {
                let mut value = good.clone();
                value[field] = invalid;
                assert!(
                    !serde_json::from_value::<Owned<NutritionPolicy>>(value)
                        .is_ok_and(|policy| policy.0.valid()),
                    "{field}"
                );
            }
            let raw = serde_json::to_string(&good).unwrap();
            let duplicate = raw.replacen(
                &format!("\"{field}\":"),
                &format!("\"{field}\":{{\"kind\":\"normal\"}},\"{field}\":"),
                1,
            );
            assert!(
                serde_json::from_str::<Owned<NutritionPolicy>>(&duplicate).is_err(),
                "{field}"
            );
            let mut value = good.clone();
            value[field] = json!({"kind":"slowed", "multiplier":2});
            let duplicate = serde_json::to_string(&value).unwrap().replacen(
                "\"multiplier\":2",
                "\"multiplier\":2,\"multiplier\":3",
                1,
            );
            assert!(
                serde_json::from_str::<Owned<NutritionPolicy>>(&duplicate).is_err(),
                "{field}"
            );
        }
        let mut obsolete = good;
        obsolete["hungry_recovery_multiplier"] = json!(2);
        assert!(serde_json::from_value::<Owned<NutritionPolicy>>(obsolete).is_err());
    }
}

#[cfg(test)]
mod nutrition_trigger_schema_tests {
    use super::Owned;
    use serde_json::json;
    use tor_simulation::rules::RulesetDocument;

    #[test]
    fn stored_trigger_rejects_disabled_extensions_and_nested_operand_aliases() {
        use tor_simulation::nutrition::NutritionConditionTrigger;
        for source in [
            r#"{"kind":"disabled","extra":0}"#,
            r#"{"kind":"disabled","chance":{"one_in":5}}"#,
            r#"{"kind":"disabled","kind":"disabled"}"#,
            r#"["disabled"]"#,
        ] {
            assert!(
                serde_json::from_str::<Owned<NutritionConditionTrigger>>(source).is_err(),
                "{source}"
            );
        }
        let good = json!({"kind":"on_empty_depletion","condition":"fixture_stupor",
            "chance":{"one_in":5},"duration":{"kind":"rolled","dice":{"count":1,"sides":8,"bonus":3}},
            "ticks_per_unit":100});
        for (path, value) in [
            ("/chance", json!([5])),
            ("/chance", json!({"one_in":5,"extra":0})),
            ("/duration", json!(["fixed", 4])),
            ("/duration", json!({"kind":"fixed","amount":4,"extra":0})),
            ("/duration/dice", json!([1, 8, 3])),
            (
                "/duration/dice",
                json!({"count":1,"sides":8,"bonus":3,"extra":0}),
            ),
        ] {
            let mut invalid = good.clone();
            *invalid.pointer_mut(path).unwrap() = value;
            assert!(
                serde_json::from_value::<Owned<NutritionConditionTrigger>>(invalid).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn stored_trigger_has_independent_closed_named_operands() {
        let rules: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let mut saved = serde_json::to_value(Owned(rules)).unwrap();
        assert_eq!(
            saved["nutrition_condition_trigger"],
            json!({"kind":"disabled"})
        );
        let trigger = json!({"kind":"on_empty_depletion","condition":"fixture_stupor",
            "chance":{"one_in":5},"duration":{"kind":"rolled","dice":{"count":1,"sides":8,"bonus":3}},
            "ticks_per_unit":100});
        saved["nutrition_condition_trigger"] = trigger.clone();
        let decoded: Owned<RulesetDocument> = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), saved);
        for value in [
            json!(null),
            json!([]),
            json!({"kind":"disabled","extra":0}),
            json!({"kind":"on_empty_depletion"}),
        ] {
            saved["nutrition_condition_trigger"] = value;
            assert!(serde_json::from_value::<Owned<RulesetDocument>>(saved.clone()).is_err());
        }
        saved
            .as_object_mut()
            .unwrap()
            .remove("nutrition_condition_trigger");
        assert!(serde_json::from_value::<Owned<RulesetDocument>>(saved).is_err());
    }
}

#[cfg(test)]
mod restorable_loss_stored_effect_tests {
    use super::*;

    #[test]
    fn hit_die_effect_save_adapters_are_independent_closed_operand_free_records() {
        for kind in ["drain_latest_hit_die", "restore_hit_die"] {
            let value = serde_json::json!({"kind":kind});
            let source: source::Effect = serde_json::from_value(value.clone()).unwrap();
            let encoded = serde_json::to_vec(&Borrowed(&source)).unwrap();
            let restored: Owned<source::Effect> = crate::storage::codec::strict(&encoded).unwrap();
            assert_eq!(restored.0, source);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&encoded).unwrap(),
                value
            );
            for invalid in [
                serde_json::Value::Null,
                serde_json::json!([kind]),
                serde_json::json!({"kind":kind,"amount":1}),
                serde_json::json!({"kind":kind,"source":"mage"}),
                serde_json::json!({"kind":kind,"health_seed":42}),
            ] {
                assert!(crate::storage::codec::strict::<Owned<source::Effect>>(
                    &serde_json::to_vec(&invalid).unwrap()
                )
                .is_err());
            }
            let duplicate = format!(r#"{{"kind":"{kind}","kind":"{kind}"}}"#);
            assert!(
                crate::storage::codec::strict::<Owned<source::Effect>>(duplicate.as_bytes())
                    .is_err()
            );
        }
    }

    #[test]
    fn restorable_effect_save_adapters_are_independent_strict_named_records() {
        for kind in [
            "lose_attribute",
            "restore_attribute",
            "lose_maximum_health",
            "restore_maximum_health",
        ] {
            let mut value = serde_json::json!({"kind":kind,"amount":{"kind":"rolled","dice":{"count":1,"sides":3,"bonus":0}}});
            if kind.ends_with("attribute") {
                value["attribute"] = "strength".into();
            }
            let source: source::Effect = serde_json::from_value(value.clone()).unwrap();
            let encoded = serde_json::to_vec(&Borrowed(&source)).unwrap();
            let restored: Owned<source::Effect> = crate::storage::codec::strict(&encoded).unwrap();
            assert_eq!(restored.0, source);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&encoded).unwrap(),
                value
            );
            for invalid in [
                serde_json::Value::Null,
                serde_json::json!([kind, value["amount"]]),
                serde_json::json!({"kind":kind}),
                serde_json::json!({"kind":kind,"amount":null}),
                serde_json::json!({"kind":kind,"amount":value["amount"],"source":"hidden"}),
            ] {
                assert!(crate::storage::codec::strict::<Owned<source::Effect>>(
                    &serde_json::to_vec(&invalid).unwrap()
                )
                .is_err());
            }
            let duplicate = format!(
                r#"{{"kind":"{kind}","attribute":"strength","amount":{{"kind":"fixed","amount":1}},"amount":{{"kind":"fixed","amount":2}}}}"#
            );
            assert!(
                crate::storage::codec::strict::<Owned<source::Effect>>(duplicate.as_bytes())
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod theft_gold_schema_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_item_traits_require_explicit_current_authority_fields() {
        let source: source::Archetype = serde_json::from_value(
            json!({"name":"coin", "class":"coin", "stackable":true, "currency":"gold"}),
        )
        .unwrap();
        let encoded = serde_json::to_value(Borrowed(&source)).unwrap();
        assert_eq!(encoded["magical"], false);
        assert_eq!(encoded["currency"], "gold");
        let restored: Owned<source::Archetype> = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&restored.0)).unwrap(),
            encoded
        );
        for field in ["magical", "currency"] {
            let mut missing = encoded.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Owned<source::Archetype>>(missing).is_err());
        }
        let mut unknown = encoded;
        unknown["currency"] = json!("future_currency");
        assert!(serde_json::from_value::<Owned<source::Archetype>>(unknown).is_err());
    }

    #[test]
    fn saved_population_quantities_are_tagged_even_when_authored_as_scalars() {
        let source: source::QuantityExpression = serde_json::from_value(json!(7)).unwrap();
        let encoded = serde_json::to_value(Borrowed(&source)).unwrap();
        assert_eq!(encoded, json!({"kind":"fixed", "quantity":7}));
        let restored: Owned<source::QuantityExpression> =
            serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&restored.0)).unwrap(),
            encoded
        );
        for malformed in [
            json!(7),
            json!(["fixed", 7]),
            json!({"kind":"fixed"}),
            json!({"kind":"uniform","minimum":2}),
            json!({"kind":"fixed","quantity":7,"depth":1}),
        ] {
            assert!(
                serde_json::from_value::<Owned<source::QuantityExpression>>(malformed).is_err()
            );
        }
    }

    #[test]
    fn saved_theft_requires_independent_selection_and_nullable_relocation() {
        for selection in [
            json!({"kind":"magical_unit"}),
            json!({"kind":"currency","currency":"gold","amount":{"kind":"uniform","minimum":2,"maximum":61}}),
        ] {
            let authored: source::Effect =
                serde_json::from_value(json!({"kind":"theft", "selection":selection})).unwrap();
            let encoded = serde_json::to_value(Borrowed(&authored)).unwrap();
            assert_eq!(encoded["relocation"], json!(null));
            let restored: Owned<source::Effect> = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(Borrowed(&restored.0)).unwrap(),
                encoded
            );
            for field in ["selection", "relocation"] {
                let mut missing = encoded.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<Owned<source::Effect>>(missing).is_err());
            }
            let mut extra = encoded.clone();
            extra["item"] = json!(1);
            assert!(serde_json::from_value::<Owned<source::Effect>>(extra).is_err());
            let mut positional = encoded;
            positional["selection"] = json!(["magical_unit"]);
            assert!(serde_json::from_value::<Owned<source::Effect>>(positional).is_err());
        }
    }
}

#[cfg(test)]
mod armor_record_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stored_armor_has_independent_named_required_current_shapes() {
        let good = json!({"slot":"body_armor", "armor":{
            "physical_reduction":2,"material":"metal","rustproof":false},
            "attack":null,"modifiers":[],"potency_modifiers":[],"periodic_relocation":null,"weapon_profile":null});
        let value: Owned<source::EquipmentSpec> = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&value.0)).unwrap(), good);
        let mut bad = vec![json!(["body_armor", null, null, []])];
        for field in [
            "armor",
            "attack",
            "modifiers",
            "potency_modifiers",
            "weapon_profile",
            "periodic_relocation",
        ] {
            let mut omitted = good.clone();
            omitted.as_object_mut().unwrap().remove(field);
            bad.push(omitted);
        }
        for armor in [
            json!([2, "metal", false]),
            json!({"physical_reduction":2,"material":"metal"}),
            json!({"physical_reduction":2,"material":"metal","rustproof":false,"extra":0}),
        ] {
            let mut value = good.clone();
            value["armor"] = armor;
            bad.push(value);
        }
        for value in bad {
            assert!(serde_json::from_value::<Owned<source::EquipmentSpec>>(value).is_err());
        }
        for value in [
            json!({"kind":"corrode_armor","amount":1}),
            json!({"kind":"corrode_armor","slot":"body_armor"}),
            json!({"kind":"corrode_armor","slot":"body_armor","amount":0}),
            json!({"kind":"corrode_armor","slot":"body_armor","amount":1_000_001}),
            json!({"kind":"corrode_armor","slot":"body_armor","amount":1,"item":42}),
            json!(["corrode_armor", "body_armor", 1]),
        ] {
            assert!(serde_json::from_value::<Owned<source::Effect>>(value).is_err());
        }
    }
}

#[cfg(test)]
mod birth_advancement_record_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn stored_future_ordinal_choices_are_named_required_and_bounded() {
        let author: source::ActorPopulationChoice = serde_json::from_value(json!({"archetype":"subject","weight":1,"advancement":[{"ordinal":3,"purchases":[{"pool":"advancement","selection":{"kind":"skill_rank","skill":"athletics"}}]}]})).unwrap();
        let good = serde_json::to_value(Borrowed(&author)).unwrap();
        assert_eq!(good["advancement"][0]["attribute"], json!(null));
        let restored: Owned<source::ActorPopulationChoice> =
            serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove("advancement");
        assert!(serde_json::from_value::<Owned<source::ActorPopulationChoice>>(missing).is_err());
        for field in ["ordinal", "purchases", "attribute"] {
            let mut missing = good.clone();
            missing["advancement"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_value::<Owned<source::ActorPopulationChoice>>(missing).is_err()
            );
        }
        for malformed in [
            json!([3, [], null]),
            json!({"ordinal":3,"purchases":[],"attribute":null,"unknown":0}),
        ] {
            let mut bad = good.clone();
            bad["advancement"][0] = malformed;
            assert!(serde_json::from_value::<Owned<source::ActorPopulationChoice>>(bad).is_err());
        }
        let mut positional = good.clone();
        positional["advancement"][0]["purchases"][0] =
            json!(["advancement",{"kind":"skill_rank","skill":"athletics"}]);
        assert!(
            serde_json::from_value::<Owned<source::ActorPopulationChoice>>(positional).is_err()
        );
        let mut oversized = good.clone();
        oversized["advancement"] = json!(vec![good["advancement"][0].clone(); 257]);
        assert!(serde_json::from_value::<Owned<source::ActorPopulationChoice>>(oversized).is_err());
    }
}

#[cfg(test)]
mod consumable_effect_stored_tests {
    use super::*;
    #[test]
    fn saved_consumable_effects_and_extend_stacking_are_independent_strict_named_records() {
        for raw in [
            r#"{"kind":"heal_per_hit_die","overflow":"none","sides":4,"bonus":0}"#,
            r#"{"kind":"heal_per_hit_die","overflow":"improve_earliest_hit_die","sides":4,"bonus":0}"#,
            r#"{"kind":"heal_per_hit_die","overflow":"improve_earliest_hit_dice_by_excess","sides":8,"bonus":0}"#,
            r#"{"kind":"remove_condition","condition":"readied"}"#,
            r#"{"kind":"apply_condition_for_turns","condition":"readied","duration":{"kind":"rolled","dice":{"count":1,"sides":2,"bonus":18}}}"#,
        ] {
            let source: source::Effect = serde_json::from_str(raw).unwrap();
            let saved: Owned<source::Effect> =
                crate::storage::codec::strict(raw.as_bytes()).unwrap();
            assert_eq!(saved.0, source);
            let bytes = serde_json::to_vec(&Borrowed(&source)).unwrap();
            assert_eq!(
                crate::storage::codec::strict::<Owned<source::Effect>>(&bytes)
                    .unwrap()
                    .0,
                source
            );
            let mut bad: serde_json::Value = serde_json::from_str(raw).unwrap();
            bad["extra"] = serde_json::json!(1);
            assert!(crate::storage::codec::strict::<Owned<source::Effect>>(
                &serde_json::to_vec(&bad).unwrap()
            )
            .is_err());
        }
        for raw in [
            r#"["remove_condition","readied"]"#,
            r#"{"kind":"remove_condition","condition":"readied","condition":"fear"}"#,
            r#"{"kind":"heal_per_hit_die","overflow":"none","sides":4}"#,
            r#"{"kind":"heal_per_hit_die","sides":4,"bonus":0}"#,
            r#"{"kind":"heal_per_hit_die","overflow":null,"sides":4,"bonus":0}"#,
            r#"{"kind":"heal_per_hit_die","overflow":"unknown","sides":4,"bonus":0}"#,
            r#"{"kind":"apply_condition_for_turns","condition":"readied","duration":{"kind":"rolled","dice":[1,2,18]}}"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Owned<source::Effect>>(raw.as_bytes()).is_err(),
                "{raw}"
            );
        }
        let stacking = tor_simulation::rules::Stacking::Extend;
        let saved = serde_json::to_vec(&Borrowed(&stacking)).unwrap();
        assert_eq!(
            crate::storage::codec::strict::<Owned<tor_simulation::rules::Stacking>>(&saved)
                .unwrap()
                .0,
            stacking
        );
        for raw in [
            r#"["extend"]"#,
            r#"{"kind":"extend","maximum":1}"#,
            r#"{"kind":"extend","kind":"extend"}"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Owned<tor_simulation::rules::Stacking>>(
                    raw.as_bytes()
                )
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod equipment_selection_stored_tests {
    use super::*;

    #[test]
    fn saved_equipment_selections_round_trip_as_strict_named_records() {
        for raw in [
            r#"{"kind":"slot","slot":"body_armor","class":"armor","last":false}"#,
            r#"{"kind":"slot","slot":"hands","class":"weapon","last":true}"#,
            r#"{"kind":"all"}"#,
        ] {
            let source: source::EquippedSelection = serde_json::from_str(raw).unwrap();
            let saved = serde_json::to_vec(&Borrowed(&source)).unwrap();
            assert_eq!(
                crate::storage::codec::strict::<Owned<source::EquippedSelection>>(&saved)
                    .unwrap()
                    .0,
                source
            );
            let effect = source::Effect::AlterEquipment {
                selection: source,
                alteration: source::EquipmentAlteration::RemoveCurse,
            };
            let saved = serde_json::to_vec(&Borrowed(&effect)).unwrap();
            assert_eq!(
                crate::storage::codec::strict::<Owned<source::Effect>>(&saved)
                    .unwrap()
                    .0,
                effect
            );
        }
        for raw in [
            r#""all""#,
            r#"["all"]"#,
            r#"["slot","body_armor","armor",false]"#,
            r#"{"kind":"all","extra":1}"#,
            r#"{"kind":"slot","slot":"body_armor","class":"armor"}"#,
            r#"{"kind":"slot","slot":"body_armor","class":"armor","last":false,"last":true}"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Owned<source::EquippedSelection>>(raw.as_bytes())
                    .is_err(),
                "{raw}"
            );
        }
    }
}

#[cfg(test)]
mod equipment_generation_stored_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_equipment_generation_requires_explicit_operands_despite_source_defaults() {
        let defaulted: source::EquipmentGeneration =
            serde_json::from_value(json!({"kind":"authored"})).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&defaulted)).unwrap(),
            json!({"kind":"authored","hit_enchantment":0,"damage_enchantment":0})
        );
        for value in [
            json!({"kind":"signed_potency","negative":1,"positive":2}),
            json!({"kind":"authored","hit_enchantment":0,"damage_enchantment":0}),
            json!({"kind":"authored","hit_enchantment":-2,"damage_enchantment":3}),
            json!({"kind":"weapon","curse_percent":10,"enchant_percent":5,"minimum":1,"maximum":3}),
            json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":1,"maximum":3}),
        ] {
            let source: source::EquipmentGeneration =
                serde_json::from_value(value.clone()).unwrap();
            let saved = serde_json::to_vec(&Borrowed(&source)).unwrap();
            let restored =
                crate::storage::codec::strict::<Owned<source::EquipmentGeneration>>(&saved)
                    .unwrap()
                    .0;
            assert_eq!(restored, source);
            assert_eq!(serde_json::to_value(Borrowed(&restored)).unwrap(), value);
            for field in value.as_object().unwrap().keys() {
                let mut missing = value.clone();
                missing.as_object_mut().unwrap().remove(field);
                let raw = serde_json::to_vec(&missing).unwrap();
                assert!(
                    serde_json::from_slice::<Owned<source::EquipmentGeneration>>(&raw).is_err(),
                    "missing {field}: {missing}"
                );
                assert!(
                    crate::storage::codec::strict::<Owned<source::EquipmentGeneration>>(&raw)
                        .is_err()
                );
            }
        }
        for raw in [
            r#""authored""#,
            r#"["authored",0,0]"#,
            r#"{"kind":"authored","hit_enchantment":0,"damage_enchantment":0,"extra":1}"#,
            r#"{"kind":"authored","hit_enchantment":0,"hit_enchantment":1,"damage_enchantment":0}"#,
        ] {
            assert!(
                crate::storage::codec::strict::<Owned<source::EquipmentGeneration>>(raw.as_bytes())
                    .is_err(),
                "{raw}"
            );
        }
    }

    #[test]
    fn saved_archetype_requires_equipment_generation_even_when_null() {
        // Source omission is intentional convenience; the independent saved
        // shape must carry the current explicit nullable authority field.
        let source: source::Archetype = serde_json::from_value(json!({"name":"ordinary"})).unwrap();
        assert!(source.equipment_generation.is_none());
        let saved = serde_json::to_value(Borrowed(&source)).unwrap();
        assert_eq!(
            saved.get("equipment_generation"),
            Some(&serde_json::Value::Null)
        );
        let restored = crate::storage::codec::strict::<Owned<source::Archetype>>(
            &serde_json::to_vec(&saved).unwrap(),
        )
        .unwrap()
        .0;
        assert_eq!(serde_json::to_value(Borrowed(&restored)).unwrap(), saved);
        let mut missing = saved.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("equipment_generation");
        let raw = serde_json::to_vec(&missing).unwrap();
        assert!(serde_json::from_slice::<Owned<source::Archetype>>(&raw).is_err());
        assert!(crate::storage::codec::strict::<Owned<source::Archetype>>(&raw).is_err());
        let mut unknown = saved;
        unknown["equipment_generation"] = json!({"kind":"future"});
        assert!(crate::storage::codec::strict::<Owned<source::Archetype>>(
            &serde_json::to_vec(&unknown).unwrap(),
        )
        .is_err());
    }
}

#[cfg(test)]
mod expertise_boundary_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_expertise_catalog_and_grants_have_independent_complete_named_schema() {
        use tor_simulation::{
            content_ids::ExpertiseId,
            expertise::{ExpertiseCategory, ExpertiseDef},
            rules::RulesetDocument,
        };
        let mut document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let id = ExpertiseId::new("fixture.knife").unwrap();
        document.expertises.push(ExpertiseDef {
            id: id.clone(),
            name: "Knife".into(),
            category: ExpertiseCategory::Equipment,
        });
        document.talents[0].expertise_grants.push(id);
        let good = serde_json::to_value(Borrowed(&document)).unwrap();
        let restored: Owned<RulesetDocument> = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(restored.0, document);
        for pointer in [
            "/expertises",
            "/talents/0/expertise_grants",
            "/growth/threat/expertise",
        ] {
            let mut bad = good.clone();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            bad.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                serde_json::from_value::<Owned<RulesetDocument>>(bad).is_err(),
                "missing {pointer}"
            );
        }
        for row in [
            json!(["fixture.knife", "Knife", "equipment"]),
            json!({"id":"fixture.knife","name":"Knife"}),
            json!({"id":"fixture.knife","name":"Knife","category":"equipment","active":true}),
            json!({"id":"fixture.knife","name":"Knife","category":{"equipment":null}}),
        ] {
            let mut bad = good.clone();
            bad["expertises"][0] = row;
            assert!(serde_json::from_value::<Owned<RulesetDocument>>(bad).is_err());
        }
        let mut bad = good.clone();
        bad["talents"][0]["expertise_grants"] = json!(vec![
            "fixture.knife";
            tor_simulation::limits::MODIFIERS_PER_SOURCE
                + 1
        ]);
        assert!(serde_json::from_value::<Owned<RulesetDocument>>(bad).is_err());
        let mut bad = good;
        bad["expertises"] = json!(vec![
            json!({"id":"fixture.knife","name":"Knife","category":"equipment"});
            tor_simulation::limits::CATALOG_ENTRIES + 1
        ]);
        assert!(serde_json::from_value::<Owned<RulesetDocument>>(bad).is_err());
    }
}

#[cfg(test)]
mod anatomy_storage_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stored_anatomy_requires_named_complete_bounded_explicit_fields() {
        let good = json!({"slots": ["weapon"], "capacities": {"grip": 2}, "primary_attack_socket": {"kind": "socket", "slot": 0}});
        let source: Owned<source::AnatomySpec> = serde_json::from_value(good.clone()).unwrap();
        let runtime: Owned<tor_simulation::AnatomySpec> =
            serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(source).unwrap(), good);
        assert_eq!(serde_json::to_value(runtime).unwrap(), good);
        for field in ["slots", "capacities", "primary_attack_socket"] {
            let mut bad = good.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Owned<source::AnatomySpec>>(bad.clone()).is_err());
            assert!(serde_json::from_value::<Owned<tor_simulation::AnatomySpec>>(bad).is_err());
        }
        for bad in [
            json!([["weapon"], {"grip": 2}, {"kind": "socket", "slot": 0}]),
            json!({"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "socket", "slot": 0}}),
            json!({"slots": [], "capacities": {"grip": 65}, "primary_attack_socket": {"kind": "none"}}),
            json!({"slots": [], "capacities": {}, "primary_attack_socket": null}),
            json!({"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "none", "slot": 0}}),
            json!({"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "none"}, "extra": 0}),
        ] {
            assert!(serde_json::from_value::<Owned<source::AnatomySpec>>(bad.clone()).is_err());
            assert!(serde_json::from_value::<Owned<tor_simulation::AnatomySpec>>(bad).is_err());
        }
        for (field, oversized) in [
            (
                "slots",
                json!(vec![
                    "weapon";
                    tor_simulation::limits::EQUIPMENT_SOCKETS + 1
                ]),
            ),
            (
                "capacities",
                serde_json::to_value(
                    (0..=tor_simulation::limits::EQUIPMENT_SOCKETS)
                        .map(|i| (format!("pool_{i}"), 1))
                        .collect::<BTreeMap<_, _>>(),
                )
                .unwrap(),
            ),
        ] {
            let mut bad = good.clone();
            bad[field] = oversized;
            assert!(serde_json::from_value::<Owned<source::AnatomySpec>>(bad.clone()).is_err());
            assert!(serde_json::from_value::<Owned<tor_simulation::AnatomySpec>>(bad).is_err());
        }
        let duplicate = r#"{"slots":[],"capacities":{"grip":1,"grip":2},"primary_attack_socket":{"kind":"none"}}"#;
        assert!(serde_json::from_str::<Owned<source::AnatomySpec>>(duplicate).is_err());
        assert!(serde_json::from_str::<Owned<tor_simulation::AnatomySpec>>(duplicate).is_err());
        let none = json!({"slots": [], "capacities": {"grip": 0}, "primary_attack_socket": {"kind": "none"}});
        assert!(serde_json::from_value::<Owned<source::AnatomySpec>>(none.clone()).is_ok());
        assert!(serde_json::from_value::<Owned<tor_simulation::AnatomySpec>>(none).is_ok());
    }

    #[test]
    fn stored_rules_own_required_equipment_resource_schema() {
        let rules: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let good = serde_json::to_value(Borrowed(&rules)).unwrap();
        assert!(
            serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(good.clone())
                .is_ok()
        );
        for field in ["capacities", "primary_attack_socket"] {
            let mut bad = good.clone();
            bad["constants"]["default_anatomy"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad)
                    .is_err()
            );
        }
        let mut missing = good.clone();
        missing["equipment_slots"][0]
            .as_object_mut()
            .unwrap()
            .remove("capacity");
        assert!(
            serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(missing)
                .is_err()
        );
        for capacity in [
            json!({"kind":"none", "units":1}),
            json!({"kind":"shared", "resource":"grip", "units":0}),
            json!({"kind":"shared", "resource":"grip", "units":65}),
            json!({"kind":"shared", "resource":"grip"}),
            json!({"kind":"shared", "resource":"bad id", "units":1}),
            json!(null),
        ] {
            let mut bad = good.clone();
            bad["equipment_slots"][0]["capacity"] = capacity;
            assert!(
                serde_json::from_value::<Owned<tor_simulation::rules::RulesetDocument>>(bad)
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod weapon_profile_record_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_weapon_profiles_are_independent_named_bounded_and_explicitly_nullable() {
        let good = json!({"slot":"weapon","armor":null,"attack":null,"modifiers":[],"potency_modifiers":[],"periodic_relocation":null,
            "weapon_profile":{"expertise":"fixture.knife","launcher":null,
                "base_traits":[{"kind":"two_handed"},{"kind":"thrown","range":{"short_feet":20,"long_feet":60}}],
                "expert_traits":{"add":[{"kind":"quickdraw"},{"kind":"offhand"}],"remove":["two_handed"]}}});
        let restored: Owned<source::EquipmentSpec> = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
        let mut bad = vec![];
        for pointer in [
            "/weapon_profile",
            "/weapon_profile/launcher",
            "/weapon_profile/expertise",
            "/weapon_profile/base_traits",
            "/weapon_profile/expert_traits",
            "/weapon_profile/expert_traits/add",
            "/weapon_profile/expert_traits/remove",
        ] {
            let mut value = good.clone();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            value
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            bad.push(value);
        }
        for (pointer, replacements) in [
            (
                "/weapon_profile",
                vec![json!(["fixture.knife",null,[],{"add":[],"remove":[]}])],
            ),
            (
                "/weapon_profile/launcher",
                vec![
                    json!([20, 60]),
                    json!({"short_feet":20}),
                    json!({"short_feet":20,"long_feet":60,"extra":0}),
                    json!({"short_feet":0,"long_feet":60}),
                    json!({"short_feet":21,"long_feet":60}),
                    json!({"short_feet":60,"long_feet":20}),
                    json!({"short_feet":20,"long_feet":1_000_005}),
                ],
            ),
            (
                "/weapon_profile/base_traits",
                vec![
                    json!([{"kind":"finesse"}]),
                    json!([{"kind":"offhand","extra":0}]),
                    json!([{"kind":"thrown"}]),
                    json!(["offhand"]),
                    json!(vec![json!({"kind":"offhand"}); 5]),
                ],
            ),
            (
                "/weapon_profile/expert_traits",
                vec![json!([[], []]), json!({"add":[],"remove":[],"extra":0})],
            ),
            (
                "/weapon_profile/expert_traits/add",
                vec![json!(vec![json!({"kind":"quickdraw"}); 5])],
            ),
            (
                "/weapon_profile/expert_traits/remove",
                vec![
                    json!(["finesse"]),
                    json!([{"offhand":null}]),
                    json!(vec!["offhand"; 5]),
                ],
            ),
        ] {
            for replacement in replacements {
                let mut value = good.clone();
                *value.pointer_mut(pointer).unwrap() = if pointer == "/weapon_profile/launcher" {
                    json!({"family":"fixture.arrow", "range":replacement})
                } else {
                    replacement
                };
                bad.push(value);
            }
        }
        for value in bad {
            assert!(
                serde_json::from_value::<Owned<source::EquipmentSpec>>(value.clone()).is_err(),
                "accepted {value}"
            );
        }
    }
}

#[cfg(test)]
mod projectile_record_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_projectile_is_named_strict_bounded_and_round_trips() {
        let good = json!({"family":"fixture.arrow","recovery":"recoverable",
            "damage":{"components":[{"category":"keen","descriptor":null,
                "amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}],
                "primary":{"category":"keen","descriptor":null,"sides":6}}});
        let restored: Owned<source::ProjectileSpec> = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
        for pointer in [
            "/family",
            "/recovery",
            "/damage",
            "/damage/components",
            "/damage/primary",
            "/damage/components/0/descriptor",
            "/damage/primary/descriptor",
            "/damage/primary/sides",
            "/damage/components/0/amount/bonus",
        ] {
            let mut bad = good.clone();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            bad.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                serde_json::from_value::<Owned<source::ProjectileSpec>>(bad).is_err(),
                "{pointer}"
            );
        }
        for (pointer, replacement) in [
            ("/family", json!("bad family")),
            ("/recovery", json!("always")),
            ("/damage", json!([[], null])),
            (
                "/damage/components",
                json!(vec![good["damage"]["components"][0].clone(); 33]),
            ),
            (
                "/damage/components/0",
                json!(["keen",null,{"type":"fixed","value":1}]),
            ),
            ("/damage/primary", json!(["keen", null, 6])),
            (
                "/damage/components/0/amount",
                json!({"type":"fixed","value":1,"extra":0}),
            ),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                serde_json::from_value::<Owned<source::ProjectileSpec>>(bad).is_err(),
                "{pointer}"
            );
        }
        for pointer in ["", "/damage", "/damage/components/0", "/damage/primary"] {
            let mut bad = good.clone();
            bad.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("extra".into(), json!(0));
            assert!(serde_json::from_value::<Owned<source::ProjectileSpec>>(bad).is_err());
        }
        let archetype = source::Archetype {
            projectile: Some(restored.0),
            stackable: true,
            class: source::ItemClass::Weapon,
            ..Default::default()
        };
        let saved = serde_json::to_value(Borrowed(&archetype)).unwrap();
        let restored: Owned<source::Archetype> = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), saved);
        let mut missing = saved;
        missing.as_object_mut().unwrap().remove("projectile");
        assert!(serde_json::from_value::<Owned<source::Archetype>>(missing).is_err());
        let empty = serde_json::to_value(Borrowed(&source::Archetype::default())).unwrap();
        assert!(empty["projectile"].is_null());
        assert!(serde_json::from_value::<Owned<source::Archetype>>(empty).is_ok());
    }

    #[test]
    fn saved_launcher_rejects_old_range_field_and_requires_family_and_range() {
        let good = json!({"expertise":"fixture.bow","launcher":{"family":"fixture.arrow",
            "range":{"short_feet":20,"long_feet":60}},"base_traits":[],"expert_traits":{"add":[],"remove":[]}});
        let restored: Owned<source::WeaponProfileSpec> =
            serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
        for key in ["family", "range"] {
            let mut bad = good.clone();
            bad["launcher"].as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<Owned<source::WeaponProfileSpec>>(bad).is_err());
        }
        let mut old = good;
        old.as_object_mut().unwrap().remove("launcher");
        old["ranged"] = json!({"short_feet":20,"long_feet":60});
        assert!(serde_json::from_value::<Owned<source::WeaponProfileSpec>>(old).is_err());
    }
}

#[cfg(test)]
mod projectile_definition_codec_tests {
    use super::*;
    use serde_json::json;
    use tor_simulation::abilities::InvocationKind as RuntimeInvocationKind;
    use tor_simulation::projectile_profiles::ProjectileDeliveryKind;
    use tor_simulation::rules::{AbilityDef, Reach, Ruleset, RulesetDocument, StrikeSource};

    fn projectile_document(delivery: ProjectileDeliveryKind) -> RulesetDocument {
        let mut document: RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        let mut ability = document
            .abilities
            .iter()
            .find(|row| row.basic_attack)
            .unwrap()
            .clone();
        ability.id = tor_simulation::content_ids::AbilityId::new("test_projectile").unwrap();
        ability.name = "Test projectile".into();
        ability.basic_attack = false;
        ability.invocation = RuntimeInvocationKind::DirectionalProjectile;
        ability.reach = Reach::Projectile;
        ability.strikes = StrikeSource::Projectile { delivery };
        document.abilities.push(ability);
        document
    }

    #[test]
    fn projectile_definitions_round_trip_through_independent_compiled_rules_codec() {
        for (delivery, name) in [
            (ProjectileDeliveryKind::Throw, "throw"),
            (ProjectileDeliveryKind::Launch, "launch"),
        ] {
            let document = projectile_document(delivery);
            let expected = Ruleset::validate(document.clone()).unwrap();
            let encoded = encode_rules_document(&document).unwrap();
            let restored = decode_rules_document(&encoded).unwrap();
            assert_eq!(encode_rules_document(&restored).unwrap(), encoded);
            let restored_rules = Ruleset::validate(restored).unwrap();
            let id = tor_simulation::content_ids::AbilityId::new("test_projectile").unwrap();
            assert_eq!(restored_rules.ability(&id), expected.ability(&id));
            let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            let ability = value["document"]["abilities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "test_projectile")
                .unwrap();
            assert_eq!(ability["invocation"], json!("directional_projectile"));
            assert_eq!(ability["reach"], json!({"kind":"projectile"}));
            assert_eq!(
                ability["strikes"],
                json!({"kind":"projectile","delivery":name})
            );
            let restored: Owned<AbilityDef> = serde_json::from_value(ability.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(Borrowed(&restored.0)).unwrap(),
                *ability
            );
        }
    }

    #[test]
    fn stored_projectile_source_requires_exact_named_delivery() {
        for (name, delivery) in [
            ("throw", ProjectileDeliveryKind::Throw),
            ("launch", ProjectileDeliveryKind::Launch),
        ] {
            let good = json!({"kind":"projectile","delivery":name});
            let decoded: Owned<StrikeSource> = serde_json::from_value(good.clone()).unwrap();
            assert_eq!(decoded.0, StrikeSource::Projectile { delivery });
            assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), good);
        }
        for bad in [
            json!({"kind":"projectile"}),
            json!({"delivery":"throw"}),
            json!({"kind":"projectile","delivery":null}),
            json!({"kind":"projectile","delivery":"lob"}),
            json!({"kind":"projectile","delivery":"Throw"}),
            json!({"kind":"projectile","delivery":"thrown"}),
            json!({"kind":"projectile","delivery":"launched"}),
            json!({"kind":"projectile","delivery":["throw"]}),
            json!({"kind":"projectile","delivery":"throw","range":6}),
            json!({"kind":"projectile","delivery":"launch","additional_riders":[]}),
            json!({"kind":"projectile","delivery":"throw","strikes":[]}),
            json!(["projectile", "throw"]),
        ] {
            assert!(
                serde_json::from_value::<Owned<StrikeSource>>(bad.clone()).is_err(),
                "{bad}"
            );
        }
        for bad in [
            r#"{"kind":"projectile","kind":"projectile","delivery":"throw"}"#,
            r#"{"kind":"projectile","delivery":"throw","delivery":"launch"}"#,
        ] {
            assert!(
                serde_json::from_str::<Owned<StrikeSource>>(bad).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn stored_projectile_definition_has_required_independent_named_fields() {
        let document = projectile_document(ProjectileDeliveryKind::Throw);
        let ability = document.abilities.last().unwrap();
        let good = serde_json::to_value(Borrowed(ability)).unwrap();
        for field in ["invocation", "reach", "strikes"] {
            let mut bad = good.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Owned<AbilityDef>>(bad).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("invocation", json!("projectile")),
            ("invocation", json!("DirectionalProjectile")),
            ("invocation", json!(null)),
            ("reach", json!({})),
            ("reach", json!({"kind":"projectile","range":6})),
            ("reach", json!({"kind":"projectile","delivery":"throw"})),
            ("reach", json!(["projectile"])),
            ("strikes", json!({"kind":"projectile"})),
            ("strikes", json!({"kind":"projectile","delivery":"lob"})),
            ("strikes", json!(["projectile", "throw"])),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(
                serde_json::from_value::<Owned<AbilityDef>>(bad).is_err(),
                "{field}"
            );
        }
        let positional =
            serde_json::Value::Array(good.as_object().unwrap().values().cloned().collect());
        assert!(serde_json::from_value::<Owned<AbilityDef>>(positional).is_err());
        let text = serde_json::to_string(&good).unwrap();
        for (before, after) in [
            (
                "\"invocation\":",
                "\"invocation\":\"actor_target\",\"invocation\":",
            ),
            (
                "\"reach\":{\"kind\":\"projectile\"}",
                "\"reach\":{\"kind\":\"projectile\",\"kind\":\"projectile\"}",
            ),
            (
                "\"delivery\":\"throw\"",
                "\"delivery\":\"throw\",\"delivery\":\"launch\"",
            ),
        ] {
            let bad = text.replacen(before, after, 1);
            assert_ne!(bad, text, "ineffective mutation: {before}");
            assert!(
                serde_json::from_str::<Owned<AbilityDef>>(&bad).is_err(),
                "{before}"
            );
        }
    }

    #[test]
    fn compiled_projectile_rules_reject_noncanonical_enum_records() {
        let document = projectile_document(ProjectileDeliveryKind::Throw);
        let encoded = encode_rules_document(&document).unwrap();
        for (before, after) in [
            (
                "\"invocation\":\"directional_projectile\"",
                "\"invocation\":{\"directional_projectile\":null}",
            ),
            ("\"delivery\":\"throw\"", "\"delivery\":{\"throw\":null}"),
            ("\"delivery\":\"throw\"", "\"delivery\":\"lob\""),
            (
                "\"kind\":\"projectile\"",
                "\"kind\":\"projectile\",\"range\":6",
            ),
        ] {
            let bad = encoded.replacen(before, after, 1);
            assert_ne!(bad, encoded, "ineffective mutation: {before}");
            assert!(decode_rules_document(&bad).is_err(), "{after}");
        }
    }

    #[test]
    fn saved_invocation_kind_preserves_existing_variants() {
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Record {
            #[serde(with = "InvocationKind")]
            invocation: RuntimeInvocationKind,
        }
        for (name, invocation) in [
            ("actor_target", RuntimeInvocationKind::ActorTarget),
            ("self_step", RuntimeInvocationKind::SelfStep),
            ("directional_ray", RuntimeInvocationKind::DirectionalRay),
            (
                "directional_contact_ray",
                RuntimeInvocationKind::DirectionalContactRay,
            ),
            (
                "directional_projectile",
                RuntimeInvocationKind::DirectionalProjectile,
            ),
        ] {
            let value = json!({"invocation":name});
            let restored: Record = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(restored.invocation, invocation);
            assert_eq!(serde_json::to_value(restored).unwrap(), value);
            for invalid in [json!({(name): null}), json!([name]), json!(null)] {
                assert!(serde_json::from_value::<Record>(json!({"invocation":invalid})).is_err());
            }
        }
    }
}

#[cfg(test)]
mod persistent_condition_stored_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_persistent_effects_require_explicit_opposition_and_unbounded_lifetime() {
        for cancels in [None, Some("other")] {
            let good = json!({"kind":"apply_persistent_condition","condition":"fixture","cancels":cancels});
            let source: source::Effect = serde_json::from_value(good.clone()).unwrap();
            let saved = serde_json::to_value(Borrowed(&source)).unwrap();
            assert_eq!(saved, good);
            assert_eq!(
                serde_json::from_value::<Owned<source::Effect>>(saved)
                    .unwrap()
                    .0,
                source
            );
            for field in good.as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.as_object_mut().unwrap().remove(field);
                assert!(
                    serde_json::from_value::<Owned<source::Effect>>(bad).is_err(),
                    "missing {field}"
                );
            }
            for bad in [
                json!(["apply_persistent_condition", "fixture", cancels]),
                json!({"kind":"apply_persistent_condition","condition":"fixture","cancels":cancels,"duration":1}),
            ] {
                assert!(serde_json::from_value::<Owned<source::Effect>>(bad).is_err());
            }
        }
        let timing = tor_simulation::rules::ConditionTiming::UntilRemoved;
        let saved = serde_json::to_value(Borrowed(&timing)).unwrap();
        assert_eq!(saved, json!({"kind":"until_removed"}));
        assert_eq!(
            serde_json::from_value::<Owned<tor_simulation::rules::ConditionTiming>>(saved)
                .unwrap()
                .0,
            timing
        );
        for bad in [
            json!({}),
            json!(["until_removed"]),
            json!("until_removed"),
            json!({"kind":"until_removed","on_expiry":{"kind":"remove"}}),
        ] {
            assert!(
                serde_json::from_value::<Owned<tor_simulation::rules::ConditionTiming>>(bad)
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod worn_equipment_schema_tests {
    use super::*;
    use crate::creature_authoring::{Modifier, Op, Scope, Selector, Stat};
    use crate::storage::codec::strict;
    #[test]
    fn current_wearable_modifiers_round_trip_the_independent_save_boundary() {
        for op in [
            Op::SignedReduction {
                selector: Selector::Category {
                    category: source::DamageType::Psychic,
                },
                amount: -1,
            },
            Op::AttributeWithoutDefense {
                attribute: "speed".into(),
                defense: "physical".into(),
                value: 2,
            },
            Op::SustainAttribute {
                attribute: "strength".into(),
            },
            Op::ProtectArmor {},
            Op::HealthRecoveryRate { multiplier: 2 },
            Op::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 2,
            },
            Op::Add {
                stat: Stat::NutritionRateThirtieths,
                value: -15,
            },
        ] {
            let modifier = Modifier {
                scope: Scope::Always,
                op,
            };
            let encoded = serde_json::to_vec(&Borrowed(&modifier)).unwrap();
            assert_eq!(strict::<Owned<Modifier>>(&encoded).unwrap().0, modifier);
            let mut extra = serde_json::to_value(Borrowed(&modifier)).unwrap();
            extra["op"]["secret_ring"] = serde_json::json!(true);
            assert!(strict::<Owned<Modifier>>(&serde_json::to_vec(&extra).unwrap()).is_err());
        }
    }
    #[test]
    fn stored_identification_set_is_explicit_sorted_unique_and_not_a_legacy_scalar() {
        let good = serde_json::json!({"control":null,"descriptors":[],"effects":[],"identify":["ring","wand"],"discovery":"on_use"});
        let decoded =
            strict::<Owned<source::ReadableSpec>>(&serde_json::to_vec(&good).unwrap()).unwrap();
        assert_eq!(
            decoded.0.identify,
            Some(vec![source::ItemClass::Ring, source::ItemClass::Wand])
        );
        for classes in [
            serde_json::json!("wand"),
            serde_json::json!([]),
            serde_json::json!(["wand", "ring"]),
            serde_json::json!(["wand", "wand"]),
            serde_json::json!([{"wand":null}]),
        ] {
            let mut bad = good.clone();
            bad["identify"] = classes;
            assert!(
                strict::<Owned<source::ReadableSpec>>(&serde_json::to_vec(&bad).unwrap()).is_err()
            );
        }
    }
}

#[cfg(test)]
mod hazards_shape_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn current_hazard_payloads_are_named_required_and_not_discovery_dc_aliases() {
        for good in [
            json!({"kind":"trap_door","destination":{"region":2,"name":"landing"},"open":false}),
            json!({"kind":"arrow","attack_bonus":0,"ammunition":"fixture.arrow"}),
            json!({"kind":"bear_trap","condition":"fixture.bear","duration":300}),
            json!({"kind":"teleport"}),
            json!({"kind":"poison_dart","attack_bonus":0,"exposure":"poison"}),
            json!({"kind":"mysterious"}),
            json!({"kind":"sleeping_gas","condition":"fixture.sleep","duration":200,"exposure":null}),
            json!({"kind":"sleeping_gas","condition":"fixture.sleep","duration":200,"exposure":"poison"}),
        ] {
            let restored: Owned<source::TrapKind> = serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
            for field in good.as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<Owned<source::TrapKind>>(bad).is_err());
            }
            let mut extra = good.clone();
            extra["dc"] = json!(12);
            assert!(serde_json::from_value::<Owned<source::TrapKind>>(extra).is_err());
            let positional =
                serde_json::Value::Array(good.as_object().unwrap().values().cloned().collect());
            assert!(serde_json::from_value::<Owned<source::TrapKind>>(positional).is_err());
        }
    }

    #[test]
    fn current_resistance_policy_is_required_nullable_and_named() {
        for resistance in [json!(null), json!({"skill":"athletics","difficulty":12})] {
            let good = json!({"id":"poison","name":"Poison","resistance":resistance});
            let restored: Owned<tor_simulation::rules::NamedDescriptor> =
                serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
            for field in ["id", "name", "resistance"] {
                let mut bad = good.clone();
                bad.as_object_mut().unwrap().remove(field);
                assert!(
                    serde_json::from_value::<Owned<tor_simulation::rules::NamedDescriptor>>(bad)
                        .is_err()
                );
            }
        }
        for bad in [
            json!({"skill":"athletics"}),
            json!({"difficulty":12}),
            json!({"skill":"athletics","difficulty":12,"defense":"physical"}),
            json!(["athletics", 12]),
        ] {
            assert!(
                serde_json::from_value::<Owned<tor_simulation::rules::NamedDescriptor>>(
                    json!({"id":"poison","name":"Poison","resistance":bad})
                )
                .is_err()
            );
        }
    }

    #[test]
    fn current_consumable_descriptors_are_explicit_even_when_empty() {
        for descriptors in [json!([]), json!(["poison"])] {
            let good = json!({"descriptors":descriptors,"effects":[],"discovery":"self_effect"});
            let restored: Owned<source::ConsumableSpec> =
                serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
            for field in ["descriptors", "effects", "discovery"] {
                let mut bad = good.clone();
                bad.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<Owned<source::ConsumableSpec>>(bad).is_err());
            }
        }
    }

    #[test]
    fn current_guise_check_and_readied_policy_have_no_legacy_fixed_dc() {
        for good in [
            json!({"kind":"object_guise","appearance":null,"skill":"stealth","edge":0}),
            json!({"kind":"readied_touch_ambush","enabled":true}),
        ] {
            let restored: Owned<crate::creature_authoring::Perception> =
                serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
            for field in good.as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.as_object_mut().unwrap().remove(field);
                assert!(
                    serde_json::from_value::<Owned<crate::creature_authoring::Perception>>(bad)
                        .is_err()
                );
            }
            let mut extra = good.clone();
            extra["dc"] = json!(15);
            assert!(
                serde_json::from_value::<Owned<crate::creature_authoring::Perception>>(extra)
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod control_scroll_schema_tests {
    use super::*;
    use crate::storage::codec::strict;
    use serde_json::json;
    #[test]
    fn stored_control_modes_and_ground_scare_have_independent_named_bounded_shapes() {
        for good in [
            json!({"kind":"hold_nearby","condition":"fixture.held","duration":1000,"radius":2}),
            json!({"kind":"arm_confusion","condition":"fixture.confused","duration":2000}),
            json!({"kind":"aggravate"}),
            json!({"kind":"create_monster"}),
            json!({"kind":"nothing"}),
        ] {
            let decoded: Owned<tor_simulation::control_scrolls::ControlSpec> =
                strict(&serde_json::to_vec(&good).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), good);
            let mut extra = good;
            extra["unexpected"] = true.into();
            assert!(
                strict::<Owned<tor_simulation::control_scrolls::ControlSpec>>(
                    &serde_json::to_vec(&extra).unwrap()
                )
                .is_err()
            );
        }
        for bad in [
            json!(["arm_confusion", "fixture.confused", 2000]),
            json!({"kind":"arm_confusion","condition":"fixture.confused","duration":0}),
            json!({"kind":"arm_confusion","condition":"fixture.confused"}),
            json!({"kind":"hold_nearby","condition":"fixture.held","duration":1000,"radius":9}),
        ] {
            assert!(
                strict::<Owned<tor_simulation::control_scrolls::ControlSpec>>(
                    &serde_json::to_vec(&bad).unwrap()
                )
                .is_err()
            );
        }
        let good = json!({"condition":"fixture.scared","duration":1000});
        let decoded: Owned<tor_simulation::control_scrolls::ConditionPayload> =
            strict(&serde_json::to_vec(&good).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), good);
        for bad in [
            json!(["fixture.scared", 1000]),
            json!({"condition":"fixture.scared","duration":0}),
            json!({"condition":"fixture.scared","duration":1000,"check":"lore"}),
        ] {
            assert!(
                strict::<Owned<tor_simulation::control_scrolls::ConditionPayload>>(
                    &serde_json::to_vec(&bad).unwrap()
                )
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod awareness_schema_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn current_awareness_capabilities_are_named_complete_and_round_trip() {
        for good in [
            json!({"kind":"continual_stealth","edge":1}),
            json!({"kind":"aggravation","interval":500,"chance_denominator":3,"stealth_disadvantage":1}),
        ] {
            let restored: Owned<crate::creature_authoring::Perception> =
                serde_json::from_value(good.clone()).unwrap();
            assert_eq!(serde_json::to_value(Borrowed(&restored.0)).unwrap(), good);
            for field in good.as_object().unwrap().keys() {
                let mut missing = good.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(
                    serde_json::from_value::<Owned<crate::creature_authoring::Perception>>(missing)
                        .is_err()
                );
            }
            let mut extra = good.clone();
            extra["unknown"] = json!(true);
            assert!(
                serde_json::from_value::<Owned<crate::creature_authoring::Perception>>(extra)
                    .is_err()
            );
            let positional = json!(good.as_object().unwrap().values().collect::<Vec<_>>());
            assert!(
                serde_json::from_value::<Owned<crate::creature_authoring::Perception>>(positional)
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod room_light_effect_tests {
    use super::*;

    #[test]
    fn room_light_source_and_independent_save_are_strict_unit_effects() {
        let raw = br#"{"kind":"room_light"}"#;
        let authored: source::Effect = serde_json::from_slice(raw).unwrap();
        assert_eq!(authored, source::Effect::RoomLight);
        let saved: Owned<source::Effect> = crate::storage::codec::strict(raw).unwrap();
        assert_eq!(saved.0, authored);
        assert_eq!(serde_json::to_vec(&Borrowed(&authored)).unwrap(), raw);
        for malformed in [
            r#"["room_light"]"#,
            r#"{"kind":"room_light","changed":true}"#,
            r#"{"kind":"room_light","room":1}"#,
            r#"{"kind":"room_light","kind":"room_light"}"#,
        ] {
            assert!(
                serde_json::from_str::<source::Effect>(malformed).is_err(),
                "{malformed}"
            );
            assert!(
                crate::storage::codec::strict::<Owned<source::Effect>>(malformed.as_bytes())
                    .is_err(),
                "{malformed}"
            );
        }
    }
}
