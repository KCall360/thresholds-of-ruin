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
scalar!(String, u32);

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
#[serde(remote = "source::Manifest", deny_unknown_fields)]
struct Manifest {
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
    RoomLighting {
        darkness_roll: u32,
        darkness_start: u32,
    },
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
    consumable: Option<source::ConsumableSpec>,
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
    memory_ticks: u64,
    flee_percent: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "source::AnatomySpec", deny_unknown_fields)]
struct AnatomySpec {
    slots: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::EquipmentSpec", deny_unknown_fields)]
struct EquipmentSpec {
    slot: String,
    #[serde(with = "mapped")]
    attack: Option<crate::creature_authoring::Melee>,
    #[serde(with = "mapped")]
    modifiers: Vec<crate::creature_authoring::Modifier>,
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
        #[serde(with = "mapped")]
        components: Vec<source::DamageComponentExpr>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "source::Effect",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Effect {
    Damage {
        #[serde(with = "mapped")]
        damage: source::DamageExpr,
    },
    Heal {
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
    ApplyCondition {
        condition: String,
        duration: u64,
    },
    Restore {
        resource: String,
        #[serde(with = "mapped")]
        amount: source::AmountExpr,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "source::ConsumableSpec", deny_unknown_fields)]
struct ConsumableSpec {
    #[serde(with = "mapped")]
    effects: Vec<source::Effect>,
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
#[serde(remote = "crate::creature_authoring::Species", deny_unknown_fields)]
struct Species {
    kind: String,
    subtypes: Vec<String>,
    attributes: tor_simulation::attributes::ByAttribute<u16>,
    #[serde(with = "mapped")]
    anatomy: Option<source::AnatomySpec>,
    #[serde(with = "mapped")]
    melee: crate::creature_authoring::Melee,
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
    #[serde(with = "mapped")]
    components: Vec<crate::creature_authoring::Component>,
    #[serde(with = "mapped")]
    primary: Option<crate::creature_authoring::DamageKey>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::Component", deny_unknown_fields)]
struct NaturalComponent {
    #[serde(with = "mapped")]
    category: source::DamageType,
    descriptor: Option<String>,
    #[serde(with = "mapped")]
    amount: crate::creature_authoring::Amount,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "crate::creature_authoring::DamageKey", deny_unknown_fields)]
struct NaturalDamageKey {
    #[serde(with = "mapped")]
    category: source::DamageType,
    descriptor: Option<String>,
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
}
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::creature_authoring::Op",
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ModifierOp {
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
    GrantAbility {
        ability: String,
    },
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
    SkillRank { skill: String },
    Talent { talent: String },
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
    starting_skills: [String; 2],
    #[serde(with = "mapped")]
    restoration: crate::creature_authoring::Restoration,
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
    id: u64,
    at: [i32; 3],
    open: bool,
    height: u8,
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

// The macro only connects an explicit schema to its runtime type. All fields,
// tags and nested mappings remain visible in the declarations above.
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
remote!(
    RegionDef => source::RegionDef,
    CellLight => source::CellLight,
    Gravity => source::Gravity,
    NamedPlace => source::NamedPlace,
    Portal => source::Portal,
    Door => source::Door,
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
    Operation => crate::generation_recipe::Operation,
    Scenario => crate::Scenario,
    ActorSetup => crate::ActorSetup,
    Position => crate::journal::Position,
    Streaming => crate::Streaming,
    Manifest => source::Manifest,
    StairPair => source::StairPair,
    Zone => source::Zone,
    TerrainAssets => source::TerrainAssets,
    AppearancePool => source::AppearancePool,
    Archetype => source::Archetype,
    ItemClass => crate::scenario_package::ItemClass,
    Character => source::Character,
    Objective => source::Objective,
    AiProfile => source::AiProfile,
    AnatomySpec => source::AnatomySpec,
    EquipmentSpec => source::EquipmentSpec,
    DiceExpr => source::DiceExpr,
    AmountExpr => source::AmountExpr,
    DamageComponentExpr => source::DamageComponentExpr,
    DamageExpr => source::DamageExpr,
    Effect => source::Effect,
    CreatureCatalog => crate::creature_authoring::Catalog,
    Species => crate::creature_authoring::Species,
    Template => crate::creature_authoring::Template,
    Melee => crate::creature_authoring::Melee,
    NaturalDamage => crate::creature_authoring::Damage,
    NaturalComponent => crate::creature_authoring::Component,
    NaturalDamageKey => crate::creature_authoring::DamageKey,
    NaturalAmount => crate::creature_authoring::Amount,
    Modifier => crate::creature_authoring::Modifier,
    ModifierScope => crate::creature_authoring::Scope,
    ModifierOp => crate::creature_authoring::Op,
    ModifierStat => crate::creature_authoring::Stat,
    ModifierSelector => crate::creature_authoring::Selector,
    Advancement => crate::creature_authoring::Advancement,
    Purchase => crate::creature_authoring::Purchase,
    PurchaseSelection => crate::creature_authoring::PurchaseSelection,
    Restoration => crate::creature_authoring::Restoration,
    BuildSpec => crate::creature_authoring::BuildSpec,
    ConsumableSpec => source::ConsumableSpec,
    BodySpec => source::BodySpec,
    DamageType => source::DamageType,
    Certificate => source::Certificate,
    RegionIndex => source::RegionIndex,
    IndexedRegion => source::IndexedRegion,
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
    document: tor_simulation::rules::RulesetDocument,
}
const RULES_INPUT_FORMAT: u32 = 2;
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
        abilities,
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
const REGION_INPUT_FORMAT: u32 = 2;
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
const CONTENT_PACKAGE_FORMAT: u32 = 2;
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
            serde_json::from_str(include_str!("../fixtures/saved-world-families-v27.json"))
                .unwrap();
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
            .flat_map(|ability| &ability.on_success)
            .next()
            .unwrap()
            .clone();
        let second = document
            .abilities
            .iter()
            .flat_map(|ability| &ability.on_success)
            .find(|effect| **effect != first)
            .unwrap()
            .clone();
        document.abilities[0].on_success = vec![first, second];
        let ordered = encode_rules_document(&document).unwrap();
        document.abilities[0].on_success.reverse();
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

    fn reject_obsolete_compiled_headers(input: &str, accepts: impl Fn(&str) -> bool) {
        assert!(accepts(input));
        assert_eq!(serde_json::from_str::<Value>(input).unwrap()["format"], 2);
        for replacement in [
            "\"format\":0",
            "\"format\":1",
            "\"format\":3",
            "\"format\":2,\"format\":2",
            "\"format\":2,\"unknown\":true",
        ] {
            let invalid = input.replacen("\"format\":2", replacement, 1);
            assert_ne!(invalid, input);
            assert!(!accepts(&invalid), "accepted {replacement}");
        }
        let missing = input.replacen("\"format\":2,", "", 1);
        assert_ne!(missing, input);
        assert!(!accepts(&missing));
    }

    #[test]
    fn compiled_rules_current_header_rejects_previous_and_malformed_envelopes() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let input = package.rules_document;
        reject_obsolete_compiled_headers(&input, |text| decode_rules_document(text).is_ok());
    }

    #[test]
    fn compiled_region_current_header_rejects_previous_and_malformed_envelopes() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let input = encode_region_input(&package.region_def(1).unwrap()).unwrap();
        reject_obsolete_compiled_headers(&input, |text| decode_region_input(text).is_ok());
    }

    #[test]
    fn compiled_package_current_header_rejects_previous_and_malformed_envelopes() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = source::read_package(&root).unwrap();
        let input = String::from_utf8(encode_content_package(&package).unwrap()).unwrap();
        reject_obsolete_compiled_headers(&input, |text| {
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
        assert!(
            decode_rules_document(&original.replacen("\"format\":2", "\"format\":0", 1)).is_err()
        );
        assert!(decode_rules_document(&original.replacen(
            "\"format\":2",
            "\"format\":2,\"format\":2",
            1
        ))
        .is_err());
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
        assert!(decode_region_input(&input.replacen(
            "\"format\":2",
            "\"format\":2,\"format\":2",
            1
        ))
        .is_err());
        assert!(decode_region_input(&input.replacen("\"format\":2", "\"format\":0", 1)).is_err());
        assert!(decode_region_input("id = 1").is_err());
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
        // Captured from the current v29 writer. Only machine-specific package
        // directories are normalized to null; this is not an old-save reader.
        serde_json::from_str(include_str!("../fixtures/saved-scenarios-v29.json")).unwrap()
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
    fn obsolete_package_metadata_is_not_read_as_the_current_schema() {
        let legacy: BTreeMap<String, Value> =
            serde_json::from_str(include_str!("../fixtures/saved-scenarios-v22.json")).unwrap();
        for (name, value) in legacy {
            if !value["package"].is_null() {
                assert!(
                    strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
                    "{name}"
                );
            }
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
        for build in [
            "/package/manifest/characters/0/creature",
            "/package/manifest/archetypes/dungeon.guardian/creature",
        ] {
            assert_saved_nested_fields::<crate::Scenario>(
                original,
                build,
                &["starting_skills", "restoration", "hit_dice"],
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
            assert_saved_nested_fields::<crate::Scenario>(
                original,
                &format!("{purchase}/selection"),
                &["kind", "talent"],
            );
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
                "/package/manifest/creatures/species/stone_construct/melee/damage",
                "/package/manifest/creatures/species/stone_construct/melee/damage/components/0/amount",
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
        json!({"kind": "humanoid", "subtypes": [], "anatomy": null, "modifiers": [],
            "attributes": {"strength": 1, "speed": 1, "intellect": 1, "willpower": 1, "awareness": 1, "presence": 1},
            "melee": {"skill": "heavy_weaponry", "bonus": 0, "wind_up": 10, "recovery": 20,
                "strikes": [{"offset": 0, "unlock": null, "check": null, "on_success": [], "on_failure": [], "descriptors": [], "riders": []}]}})
    }
    #[test]
    fn ordered_strike_content_catalog_storage_round_trips_independent_natural_records() {
        let author: crate::creature_authoring::Species = serde_json::from_value(source()).expect("natural authoring must decode");
        let stored = serde_json::to_vec(&Borrowed(&author)).unwrap();
        let restored: Owned<crate::creature_authoring::Species> = crate::storage::codec::strict(&stored).unwrap();
        assert_eq!(restored.0, author);
        assert_eq!(serde_json::to_vec(&Borrowed(&restored.0)).unwrap(), stored);
    }
    #[test]
    fn ordered_strike_content_catalog_storage_rejects_nested_unknowns_missing_lists_and_overflow() {
        let author: crate::creature_authoring::Species = serde_json::from_value(source()).expect("natural authoring must decode");
        let original = serde_json::to_value(Borrowed(&author)).unwrap();
        for pointer in ["/melee", "/melee/strikes/0"] {
            let mut forged = original.clone();
            forged.pointer_mut(pointer).unwrap().as_object_mut().unwrap().insert("derived_cache".into(), json!({}));
            assert!(crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(&serde_json::to_vec(&forged).unwrap()).is_err());
        }
        for field in ["on_success", "on_failure", "descriptors", "riders", "unlock", "check"] {
            let mut forged = original.clone();
            forged["melee"]["strikes"][0].as_object_mut().unwrap().remove(field);
            assert!(crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(&serde_json::to_vec(&forged).unwrap()).is_err(), "missing {field}");
        }
        let mut forged = original;
        forged["melee"]["strikes"] = json!(vec![forged["melee"]["strikes"][0].clone(); 17]);
        assert!(crate::storage::codec::strict::<Owned<crate::creature_authoring::Species>>(&serde_json::to_vec(&forged).unwrap()).is_err());
    }
}
