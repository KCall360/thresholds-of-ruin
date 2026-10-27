//! Ordinary authored inputs. Filesystem access stays in the server; construction
//! uses the same deterministic world operations as other simulation callers.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::num::NonZeroU64;
use std::path::{Component, Path};
use std::sync::Arc;

use crate::{Failure, Scenario};
#[path = "scenario_artifact.rs"]
pub(crate) mod artifact;
#[path = "scenario_authoring.rs"]
mod authoring;
#[path = "scenario_compiler.rs"]
mod compiler;
#[path = "scenario_diagnostics.rs"]
mod diagnostics;
#[path = "scenario_instantiation.rs"]
mod instantiation;
#[path = "scenario_item_forms.rs"]
mod item_forms;
#[cfg(test)]
#[path = "physical_device_authoring_tests.rs"]
mod physical_device_tests;
pub use item_forms::ItemForm;
#[path = "scenario_population.rs"]
mod population;
#[path = "scenario_presentation.rs"]
mod presentation;
#[path = "scenario_retrieval.rs"]
mod retrieval;
#[path = "scenario_spatial.rs"]
pub(crate) mod spatial;
#[path = "scenario_spatial_authoring.rs"]
pub mod spatial_authoring;
#[path = "scenario_spatial_runtime.rs"]
mod spatial_runtime;
#[path = "scenario_spatial_sources.rs"]
mod spatial_sources;
pub use population::{
    ActorPopulationBand, ActorPopulationChoice, CarriedItemChoice, CarriedItemPool, CarriedLoot,
    CarryChance, CharacterLoadout, FloorActivation, PopulationAdvancement, PopulationBand,
    PopulationChoice, PopulationDepth, PopulationDepthGrowth, PopulationProfile,
    PopulationQuantityExpr, PopulationStage, Protagonist, QuantityExpression, ScenarioCapabilities,
};
pub use retrieval::UniqueRetrieval;
#[cfg(test)]
pub(crate) use spatial_runtime::tests::package as spatial_audit_test_package;
pub(crate) use spatial_runtime::{HistoricalSourceAudit, SpatialAudit};
#[path = "scenario_rules.rs"]
mod rules_source;
pub use artifact::export;
pub use rules_source::{resolve_rules_source, ResolvedRules};
#[path = "scenario_source.rs"]
mod source;
pub use authoring::{
    AiProfile, AmountExpr, AnatomySpec, ArmorMaterial, ArmorSpec, BodySpec, ChargeRange,
    ConsumableSpec, CurrencyKind, DamageComponentExpr, DamageExpr, DamageType, DeviceSpec,
    DiceExpr, Effect, EquipmentAlteration, EquipmentGeneration, EquipmentSlot, EquipmentSpec,
    EquippedSelection, HealingOverflow, HeldLightSpec, HitRecovery, HoldPolicy, ItemClass,
    ItemDiscovery, LauncherProfile, PeriodicRelocation, PrimaryAttackSocket, ProjectileSpec,
    PushDirection, ReadableSpec, RelocationGate, RelocationPolicy, TheftSelection, ThrownPayload,
    WeaponProfileSpec, WeaponRangeBands, WeaponTrait, WeaponTraitChanges, WeaponTraitKind,
};
use compiler::PreparedDefinitions;
use diagnostics::{Origin, PathSegment, ReferenceValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use source::{resolve_source, ResolvedSource};
pub use tor_simulation::EdibleSpec;
use tor_simulation::Game;
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

pub const RULESET: &str = "interactions-v56";
const VALIDATOR: &str = "tor-scenario-47";
/// The manifest and the validator's files are bounded to this.
const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// The package layout this version reads: `scenario.toml`, one file per
/// region in `regions/`, and the validator's `index.json`.
pub const FORMAT: u32 = 35;
const REGION_DIR: &str = "regions";
/// Regions a package may have.
pub const MAX_REGIONS: usize = 65_536;
/// Each region file is bounded to this.
pub(crate) const MAX_REGION_BYTES: u64 = 1024 * 1024;
/// The index is bounded to this, which holds the largest package.
pub(crate) const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_VALIDATION_CELL_WORK: u64 = 67_108_864;

fn fail(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}
fn require(ok: bool, message: impl AsRef<str>) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(fail(message))
    }
}
/// Add author-facing context only on failure, preserving the original code.
fn in_declaration(mut failure: Failure, declaration: impl AsRef<str>) -> Failure {
    failure.message = format!("{}: {}", declaration.as_ref(), failure.message);
    failure
}
/// Asset identifiers are dotted lowercase names, like `creature.rat`.
fn asset_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
// Bounded reply context, request ID, revisions, tags and both Full/Delta body
// wrappers fit inside this conservative reserve. Asset IDs are ASCII and a
// delta's disjoint added/removed union is a subset of the all-theme union.
const PALETTE_ENVELOPE_RESERVE: usize = 4096;

fn check_palette_budget(assets: &BTreeSet<String>, response_bytes: usize) -> Result<(), Failure> {
    let available = response_bytes
        .checked_sub(PALETTE_ENVELOPE_RESERVE)
        .ok_or_else(|| fail("Asset forecast exceeds the palette response budget"))?;
    tor_protocol::encode_bounded_json(assets, available)
        .map_err(|_| fail("Asset forecast exceeds the palette response budget"))?;
    Ok(())
}

fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "spatial_authoring::unique_map"
    )]
    pub spatial_scopes: BTreeMap<String, spatial_authoring::AuthoredSpatialScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_retrieval: Option<UniqueRetrieval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_activation: Option<FloorActivation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ScenarioCapabilities>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub procedural_worlds: BTreeMap<String, crate::procedural_world::WorldFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arena: Option<Arena>,
    #[serde(
        default,
        skip_serializing_if = "crate::creature_authoring::Catalog::is_empty"
    )]
    pub creatures: crate::creature_authoring::Catalog,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub generation_recipes: BTreeMap<String, crate::generation_recipe::Recipe>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub generation_groups: BTreeMap<String, crate::generation_recipe::GenerationGroup>,
    /// Stable connection identities, independent of endpoint coordinates.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stair_pairs: BTreeMap<String, StairPair>,
    #[serde(default)]
    pub factions: BTreeMap<String, std::collections::BTreeSet<String>>,
    #[serde(default)]
    pub ai_profiles: BTreeMap<String, AiProfile>,
    pub format: u32,
    pub id: String,
    pub version: String,
    pub ruleset: String,
    pub default_character: u64,
    #[serde(default)]
    pub themes: Vec<String>,
    #[serde(default)]
    pub zones: BTreeMap<String, Zone>,
    #[serde(default)]
    pub archetypes: BTreeMap<String, Archetype>,
    #[serde(default)]
    pub appearance_pools: BTreeMap<String, AppearancePool>,
    pub characters: Vec<Character>,
    pub objective: Option<Objective>,
    /// The asset identifiers each theme may need: a client near a region
    /// with that theme gets them in its palette. See
    /// `docs/reference/protocol.md#asset-palettes`.
    #[serde(default)]
    pub assets: BTreeMap<String, Vec<String>>,
    /// Terrain assets of regions outside any zone, or in a zone without its own.
    pub terrain: Option<TerrainAssets>,
}
/// Arena source settings; runtime counters and outcomes belong to simulation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arena {
    #[serde(default, skip_serializing_if = "is_false")]
    pub start_paused: bool,
    pub participants: Vec<u64>,
    pub control: ArenaControl,
    pub ticks: u64,
    pub actions: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaControl {
    Manual,
    AllAi,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Arena {
    fn limits(&self) -> tor_simulation::arena::ArenaLimits {
        tor_simulation::arena::ArenaLimits {
            ticks: self.ticks,
            actions: self.actions,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StairPair {
    pub upper: String,
    pub lower: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub themes: Option<Vec<String>>,
    pub terrain: Option<TerrainAssets>,
}
/// Assets for a region's floor, walls and doors.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainAssets {
    pub floor: Option<String>,
    pub wall: Option<String>,
    pub door: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearancePool {
    pub appearances: Vec<String>,
    #[serde(default)]
    pub confounding: bool,
    /// The asset every concealed item drawing from this pool looks like, so
    /// it never discloses which identity an item is.
    pub asset: Option<String>,
}
#[derive(Debug, Default, Serialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature: Option<crate::creature_authoring::BuildSpec>,
    pub anatomy: Option<AnatomySpec>,
    pub equipment: Option<EquipmentSpec>,
    pub projectile: Option<ProjectileSpec>,
    pub equipment_generation: Option<EquipmentGeneration>,
    pub consumable: Option<ConsumableSpec>,
    pub readable: Option<ReadableSpec>,
    pub ground_scare: Option<tor_simulation::control_scrolls::ScareSpec>,
    pub device: Option<DeviceSpec>,
    pub held_light: Option<HeldLightSpec>,
    pub edible: Option<EdibleSpec>,
    pub supply: Option<String>,
    #[serde(default)]
    pub magical: bool,
    pub currency: Option<CurrencyKind>,
    #[serde(default)]
    pub class: crate::scenario_package::ItemClass,
    pub body: Option<BodySpec>,
    pub identity: Option<String>,
    pub appearance_pool: Option<String>,
    #[serde(default)]
    pub forms: Vec<ItemForm>,
    #[serde(default)]
    pub stackable: bool,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    pub name: Option<String>,
    pub turn_ticks: Option<u64>,
    /// The asset clients draw actors and items of this archetype with.
    pub asset: Option<String>,
}
impl<'de> serde::Deserialize<'de> for Archetype {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            creature: Option<crate::creature_authoring::BuildSpec>,
            anatomy: Option<AnatomySpec>,
            equipment: Option<EquipmentSpec>,
            projectile: Option<ProjectileSpec>,
            equipment_generation: Option<EquipmentGeneration>,
            consumable: Option<ConsumableSpec>,
            readable: Option<ReadableSpec>,
            ground_scare: Option<tor_simulation::control_scrolls::ScareSpec>,
            device: Option<DeviceSpec>,
            held_light: Option<HeldLightSpec>,
            edible: Option<EdibleSpec>,
            supply: Option<String>,
            #[serde(default)]
            magical: bool,
            currency: Option<CurrencyKind>,
            #[serde(default)]
            class: crate::scenario_package::ItemClass,
            body: Option<BodySpec>,
            identity: Option<String>,
            appearance_pool: Option<String>,
            #[serde(default)]
            forms: Vec<ItemForm>,
            #[serde(default)]
            stackable: bool,
            #[serde(default)]
            properties: BTreeMap<String, String>,
            name: Option<String>,
            turn_ticks: Option<u64>,
            asset: Option<String>,
        }
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = Fields;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a named archetype record")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Fields, M::Error> {
                Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let fields: Fields = deserializer.deserialize_map(Named)?;
        Ok(Self {
            creature: fields.creature,
            anatomy: fields.anatomy,
            equipment: fields.equipment,
            projectile: fields.projectile,
            equipment_generation: fields.equipment_generation,
            consumable: fields.consumable,
            readable: fields.readable,
            ground_scare: fields.ground_scare,
            device: fields.device,
            held_light: fields.held_light,
            edible: fields.edible,
            supply: fields.supply,
            magical: fields.magical,
            currency: fields.currency,
            class: fields.class,
            body: fields.body,
            identity: fields.identity,
            appearance_pool: fields.appearance_pool,
            forms: fields.forms,
            stackable: fields.stackable,
            properties: fields.properties,
            name: fields.name,
            turn_ticks: fields.turn_ticks,
            asset: fields.asset,
        })
    }
}

#[cfg(test)]
thread_local! {
    static ARCHETYPE_DEFINITION_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
impl Clone for Archetype {
    fn clone(&self) -> Self {
        ARCHETYPE_DEFINITION_COPIES.with(|count| count.set(count.get() + 1));
        Self {
            anatomy: self.anatomy.clone(),
            creature: self.creature.clone(),
            equipment: self.equipment.clone(),
            projectile: self.projectile.clone(),
            equipment_generation: self.equipment_generation,
            consumable: self.consumable.clone(),
            readable: self.readable.clone(),
            ground_scare: self.ground_scare.clone(),
            device: self.device.clone(),
            held_light: self.held_light,
            edible: self.edible.clone(),
            supply: self.supply.clone(),
            magical: self.magical,
            currency: self.currency,
            class: self.class,
            body: self.body.clone(),
            identity: self.identity.clone(),
            appearance_pool: self.appearance_pool.clone(),
            forms: self.forms.clone(),
            stackable: self.stackable,
            properties: self.properties.clone(),
            name: self.name.clone(),
            turn_ticks: self.turn_ticks,
            asset: self.asset.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loadout: Vec<CharacterLoadout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature: Option<crate::creature_authoring::BuildSpec>,
    pub anatomy: Option<AnatomySpec>,
    pub body: Option<BodySpec>,
    pub velocity: Option<[i64; 3]>,
    #[serde(default)]
    pub known_identities: Vec<String>,
    pub id: u64,
    pub anchor: String,
    #[serde(default = "hundred")]
    pub turn_ticks: u64,
    #[serde(default = "omit")]
    pub unselected: String,
    pub ai: Option<String>,
    pub asset: Option<String>,
}
fn hundred() -> u64 {
    100
}
fn omit() -> String {
    "omit".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub anchor: String,
    pub item: Option<u64>,
    pub disclosed: bool,
    pub continue_play: bool,
}
/// A place hint: a position, or a position and the name the character
/// learns on seeing it (`{ at = [3,2,0], name = "Threshold" }`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaceDef {
    At([i32; 3]),
    Named(NamedPlace),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedPlace {
    pub at: [i32; 3],
    pub name: String,
}

fn default_lit() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellLight {
    pub at: [i32; 3],
    pub lit: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionDef {
    #[serde(default = "default_lit")]
    pub lit: bool,
    #[serde(default)]
    pub lighting: Vec<CellLight>,
    pub id: u64,
    pub name: String,
    pub size: [i32; 3],
    #[serde(default)]
    pub chamber: bool,
    pub zone: Option<String>,
    pub gravity: Option<[i32; 3]>,
    #[serde(default)]
    pub gravity_overrides: Vec<Gravity>,
    #[serde(default)]
    pub anchors: BTreeMap<String, [i32; 3]>,
    #[serde(default)]
    pub walls: Vec<[i32; 3]>,
    #[serde(default)]
    pub openings: Vec<[i32; 3]>,
    #[serde(default)]
    pub places: Vec<PlaceDef>,
    #[serde(default)]
    pub portals: Vec<Portal>,
    #[serde(default)]
    pub doors: Vec<Door>,
    #[serde(default)]
    pub traps: Vec<Trap>,
    /// Structural next-depth owner used only by the hidden-trap generation stage.
    /// This is not a traversable link until a generated physical lid opens.
    #[serde(default)]
    pub trapdoor_destination: Option<u64>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub actors: Vec<Actor>,
    /// A generated region's recipe: the generator fills the region on first
    /// build. See [`crate::generator`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate: Option<crate::generator::Generate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gravity {
    pub at: [i32; 3],
    pub vector: [i32; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Portal {
    pub rotation: Option<u8>,
    pub kind: Option<String>,
    pub at: [i32; 3],
    pub direction: String,
    pub to: String,
    #[serde(default)]
    pub turns: u8,
    #[serde(default = "one")]
    pub width: u16,
    #[serde(default = "one")]
    pub height: u16,
}
fn unit_quantity() -> u64 {
    1
}
fn one() -> u16 {
    1
}
fn one_cell() -> u8 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Door {
    #[serde(default)]
    pub concealment: Option<Concealment>,
    pub id: u64,
    pub at: [i32; 3],
    pub open: bool,
    /// Cells tall; a door must exactly fill its opening.
    #[serde(default = "one_cell")]
    pub height: u8,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Concealment {
    pub dc: i32,
}
impl Concealment {
    pub(crate) fn compiled(self) -> tor_world::Concealment {
        tor_world::Concealment { dc: self.dc }
    }
    fn valid(self) -> bool {
        self.compiled().valid()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrapKind {
    TrapDoor {
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
    Rust {
        slot: String,
        amount: u32,
    },
    SleepingGas {
        exposure: Option<String>,
        condition: String,
        duration: u64,
    },
}
impl TrapKind {
    pub(crate) fn compiled(&self) -> tor_world::TrapKind {
        match self {
            Self::TrapDoor { destination, open } => tor_world::TrapKind::TrapDoor {
                destination: destination.clone(),
                open: *open,
            },
            Self::Arrow {
                attack_bonus,
                ammunition,
            } => tor_world::TrapKind::Arrow {
                attack_bonus: *attack_bonus,
                ammunition: ammunition.clone(),
            },
            Self::BearTrap {
                condition,
                duration,
            } => tor_world::TrapKind::BearTrap {
                condition: condition.clone(),
                duration: *duration,
            },
            Self::Teleport {} => tor_world::TrapKind::Teleport {},
            Self::PoisonDart {
                attack_bonus,
                exposure,
            } => tor_world::TrapKind::PoisonDart {
                attack_bonus: *attack_bonus,
                exposure: exposure.clone(),
            },
            Self::Mysterious {} => tor_world::TrapKind::Mysterious {},
            Self::Rust { slot, amount } => tor_world::TrapKind::Rust {
                slot: slot.clone(),
                amount: *amount,
            },
            Self::SleepingGas {
                exposure,
                condition,
                duration,
            } => tor_world::TrapKind::SleepingGas {
                exposure: exposure.clone(),
                condition: condition.clone(),
                duration: *duration,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trap {
    pub avoidance_dc: Option<i32>,
    pub id: u64,
    pub at: [i32; 3],
    pub dc: i32,
    pub kind: TrapKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub equipped_slot: Option<u16>,
    pub class: Option<crate::scenario_package::ItemClass>,
    #[serde(default = "unit_quantity")]
    pub quantity: u64,
    pub stackable: Option<bool>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    pub id: u64,
    pub at: [i32; 3],
    pub archetype: Option<String>,
    pub name: Option<String>,
    pub carried_by: Option<u64>,
    #[serde(default)]
    pub seed_names: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature: Option<crate::creature_authoring::BuildSpec>,
    pub anatomy: Option<AnatomySpec>,
    #[serde(default)]
    pub known_identities: Vec<String>,
    pub body: Option<BodySpec>,
    pub velocity: Option<[i64; 3]>,
    pub id: u64,
    pub at: [i32; 3],
    pub archetype: Option<String>,
    pub turn_ticks: Option<u64>,
    /// External control supports deterministic multi-actor test drivers. AI is deferred.
    #[serde(default = "external")]
    pub controller: String,
    pub ai: Option<String>,
}
fn external() -> String {
    "external".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Certificate {
    pub validator: String,
    pub ruleset: String,
    pub ruleset_hash: String,
    pub ruleset_dependencies: BTreeMap<String, String>,
    pub content_hash: String,
    /// Hashes of the manifest and the index; the index holds each region
    /// file's hash.
    pub files: BTreeMap<String, String>,
    pub regions: usize,
    pub model_hash: String,
    pub coverage: String,
}

/// What a package's regions are without reading them: generated by the
/// validator as `index.json`, so starting, planning and streaming a game
/// read one region file only when that region is built.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegionIndex {
    /// In region id order.
    pub regions: Vec<IndexedRegion>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexedRegion {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary_anchors: Vec<String>,
    pub id: u64,
    /// The region's file, relative to the package, and its SHA-256.
    pub file: String,
    pub hash: String,
    /// Canonical save-owned compiled input identity, independent of source text.
    pub compiled_hash: String,
    pub name: String,
    pub size: [i32; 3],
    pub chamber: bool,
    pub zone: Option<String>,
    pub anchors: BTreeMap<String, [i32; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stair_anchors: Vec<String>,
    /// Each outgoing portal's destination anchor, in authored order.
    pub portals: Vec<String>,
    pub actors: Vec<IndexedActor>,
    pub items: Vec<IndexedItem>,
    pub doors: Vec<u64>,
    pub traps: Vec<u64>,
    /// Filled by a generator; its identities aren't known until then.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexedActor {
    pub id: u64,
    pub at: [i32; 3],
    pub turn_ticks: Option<u64>,
    pub archetype: Option<String>,
    /// Controlled by a client, so it gets a reference point.
    pub external: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexedItem {
    pub id: u64,
    pub carried_by: Option<u64>,
}

impl IndexedRegion {
    fn of(def: &RegionDef, file: String, hash: String) -> Result<Self, Failure> {
        Ok(Self {
            compiled_hash: digest(
                crate::storage::schema::scenario::encode_region_input(def)?.as_bytes(),
            ),
            boundary_anchors: def
                .generate
                .as_ref()
                .map_or_else(Vec::new, |g| g.boundary_anchors.clone()),
            id: def.id,
            file,
            hash,
            name: def.name.clone(),
            size: def.size,
            chamber: def.chamber,
            zone: def.zone.clone(),
            anchors: def.anchors.clone(),
            stair_anchors: def
                .generate
                .as_ref()
                .map_or_else(Vec::new, |g| g.stair_anchors.clone()),
            portals: def.portals.iter().map(|p| p.to.clone()).collect(),
            actors: def
                .actors
                .iter()
                .map(|a| IndexedActor {
                    id: a.id,
                    at: a.at,
                    turn_ticks: a.turn_ticks,
                    archetype: a.archetype.clone(),
                    external: a.controller == "external",
                })
                .collect(),
            items: def
                .items
                .iter()
                .map(|i| IndexedItem {
                    id: i.id,
                    carried_by: i.carried_by,
                })
                .collect(),
            doors: def.doors.iter().map(|d| d.id).collect(),
            traps: def.traps.iter().map(|t| t.id).collect(),
            generated: def.generate.is_some(),
        })
    }
}

impl RegionIndex {
    /// Canonical bytes: what the validator writes as `index.json`, and what
    /// the certificate hashes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Failure> {
        let mut bytes = serde_json::to_vec(self).map_err(|e| fail(e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Failure> {
        require(
            bytes.len() as u64 <= MAX_INDEX_BYTES,
            format!("index.json exceeds {} KiB", MAX_INDEX_BYTES / 1024),
        )?;
        let index: Self =
            serde_json::from_slice(bytes).map_err(|e| fail(format!("index.json: {e}")))?;
        require(
            index.regions.windows(2).all(|w| w[0].id < w[1].id),
            "index.json: regions must be in increasing id order",
        )?;
        Ok(index)
    }
    pub fn region(&self, id: u64) -> Option<&IndexedRegion> {
        let at = self.regions.binary_search_by_key(&id, |r| r.id).ok()?;
        Some(&self.regions[at])
    }
}

/// Where a package's region files come from: text already in memory (from
/// the save's copies, or read while validating), or the package directory.
/// Every text is checked against the index's hash before it's used. Shared
/// by copies of the package, and usable from the preloading thread.
#[derive(Clone, Debug, Default)]
pub struct RegionSources(Arc<std::sync::Mutex<SourceState>>);

#[derive(Debug, Default)]
struct SourceState {
    directory: Option<std::path::PathBuf>,
    compiled_directory: Option<std::path::PathBuf>,
    /// Texts held in memory: every region's for a package read whole, and
    /// none otherwise.
    texts: BTreeMap<u64, Arc<str>>,
    /// Storage owns the admission and durability state of captured inputs.
    saved: Option<crate::storage::SourceReader>,
    /// Recently read texts, so a region's file is read once for its build
    /// and the save's copy of it.
    recent: std::collections::VecDeque<(u64, Arc<str>)>,
    /// Recently parsed regions, so building a region and then its neighbour
    /// doesn't parse shared neighbours twice.
    parsed: std::collections::VecDeque<(u64, Arc<RegionDef>)>,
    /// Region files read from the directory, for scaling contracts.
    files_read: usize,
}

/// Parsed regions kept: a region and its neighbours, and a few more.
const PARSED_REGIONS: usize = 16;

impl RegionSources {
    fn with(directory: Option<&Path>, texts: BTreeMap<u64, Arc<str>>) -> Self {
        Self(Arc::new(std::sync::Mutex::new(SourceState {
            directory: directory.map(Path::to_path_buf),
            compiled_directory: None,
            texts,
            saved: None,
            recent: Default::default(),
            parsed: Default::default(),
            files_read: 0,
        })))
    }
    /// Per-engine reader/location state must not be shared when a Scenario is
    /// reused to create several saves. Immutable text/definition caches may be.
    pub(crate) fn independent(&self) -> Self {
        let state = self.0.lock().unwrap();
        Self(Arc::new(std::sync::Mutex::new(SourceState {
            directory: state.directory.clone(),
            compiled_directory: state.compiled_directory.clone(),
            texts: state.texts.clone(),
            saved: state
                .saved
                .as_ref()
                .map(crate::storage::SourceReader::independent),
            recent: state.recent.clone(),
            parsed: state.parsed.clone(),
            files_read: state.files_read,
        })))
    }

    /// Bind persistent execution to immutable inputs. The authored directory
    /// stays distinct and remains usable solely for checked diagnostics.
    fn attach_saved(
        &self,
        reader: crate::storage::SourceReader,
        compiled_directory: Option<std::path::PathBuf>,
    ) {
        let mut state = self.0.lock().unwrap();
        state.saved = Some(reader);
        state.compiled_directory = compiled_directory;
    }
    /// Region file texts held in memory, other than a few recent ones.
    pub fn texts_in_memory(&self) -> usize {
        self.0.lock().unwrap().texts.len()
    }
    /// Region files read from the package directory so far.
    pub fn files_read(&self) -> usize {
        self.0.lock().unwrap().files_read
    }
    pub(crate) fn set_compiled_directory(&self, directory: &Path) {
        self.0.lock().unwrap().compiled_directory = Some(directory.to_path_buf());
    }
    pub(crate) fn has_directory(&self) -> bool {
        let state = self.0.lock().unwrap();
        state.compiled_directory.is_some() || (state.saved.is_none() && state.directory.is_some())
    }
    /// A region file's text, checked against the index.
    pub(crate) fn text(&self, entry: &IndexedRegion) -> Result<Arc<str>, Failure> {
        let (text, directory) = {
            let s = self.0.lock().unwrap();
            let held = s.texts.get(&entry.id).cloned().or_else(|| {
                s.recent
                    .iter()
                    .find(|(id, _)| *id == entry.id)
                    .map(|(_, text)| text.clone())
            });
            (held, s.directory.clone())
        };
        let text = match (text, directory) {
            (Some(text), _) => text,
            (None, Some(directory)) => {
                let text = read_limited(&directory, &entry.file, MAX_REGION_BYTES)?;
                self.0.lock().unwrap().files_read += 1;
                Arc::from(text)
            }
            (None, None) => {
                return Err(fail(format!(
                    "Region {} isn't in the save and the scenario package isn't available",
                    entry.id
                )));
            }
        };
        {
            let mut s = self.0.lock().unwrap();
            if !s.texts.contains_key(&entry.id) && !s.recent.iter().any(|(id, _)| *id == entry.id) {
                if s.recent.len() >= PARSED_REGIONS {
                    s.recent.pop_front();
                }
                s.recent.push_back((entry.id, text.clone()));
            }
        }
        require(
            source_digest(&text) == entry.hash,
            format!(
                "{} changed since the package was validated; run tor-scenario validate",
                entry.file
            ),
        )?;
        Ok(text)
    }
    /// A region's definition, parsed from its checked text.
    fn def(&self, entry: &IndexedRegion) -> Result<Arc<RegionDef>, Failure> {
        let cached = self
            .0
            .lock()
            .unwrap()
            .parsed
            .iter()
            .find(|(id, _)| *id == entry.id)
            .map(|(_, def)| def.clone());
        if let Some(def) = cached {
            return Ok(def);
        }
        let (saved, compiled_directory, persisted) = {
            let mut state = self.0.lock().unwrap();
            // Storage distinguishes pending, durable, and never-captured IDs;
            // only the last may use the artifact when this lookup returns None.
            let saved = match &mut state.saved {
                Some(reader) => reader.read(entry.id)?,
                None => None,
            };
            (
                saved,
                state.compiled_directory.clone(),
                state.saved.is_some(),
            )
        };
        let compiled = match (saved, compiled_directory) {
            (Some(input), _) => Some(input),
            (None, Some(directory)) => {
                let input = read_limited(
                    &directory,
                    &format!("regions/{}.json", entry.id),
                    MAX_REGION_BYTES,
                )?;
                self.0.lock().unwrap().files_read += 1;
                Some(Arc::from(input))
            }
            _ => None,
        };
        let def: Arc<RegionDef> = if let Some(input) = compiled {
            require(
                digest(input.as_bytes()) == entry.compiled_hash,
                "Compiled region input hash mismatch",
            )?;
            Arc::new(crate::storage::schema::scenario::decode_region_input(
                &input,
            )?)
        } else {
            require(
                !persisted,
                format!(
                    "Compiled region {} is missing from the save and its pinned artifact",
                    entry.id
                ),
            )?;
            Arc::new(parse(&self.text(entry)?, &entry.file)?)
        };
        require(
            digest(crate::storage::schema::scenario::encode_region_input(&def)?.as_bytes())
                == entry.compiled_hash,
            "Compiled region semantics differ from the package index",
        )?;
        require(
            def.id == entry.id,
            format!("{}: must hold region {}", entry.file, entry.id),
        )?;
        let mut s = self.0.lock().unwrap();
        if s.parsed.len() >= PARSED_REGIONS {
            s.parsed.pop_front();
        }
        s.parsed.push_back((entry.id, def.clone()));
        Ok(def)
    }
}

#[derive(Debug)]
struct CachedRules {
    source_hash: String,
    rules: Arc<tor_simulation::rules::Ruleset>,
}

/// A package: its manifest, its region index, and where its region files
/// come from. Saves keep the manifest and index, and a copy of each region
/// file once that region is built; resuming reads other region files from
/// the package directory, checked against the index. See
/// `docs/reference/scenario-packages.md`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub manifest: Manifest,
    /// Authored shared import/overrides or a standalone rules document.
    pub rules_source: String,
    /// Resolved imported content, pinned once so saves need no external rules.
    pub resolved_rules_source: Option<String>,
    /// Canonical resolved JSON document; restore never parses authoring TOML.
    pub rules_document: String,
    #[serde(skip)]
    rules_cache: Arc<std::sync::Mutex<Option<CachedRules>>>,
    pub certificate: Certificate,
    pub validated: bool,
    pub selected: u64,
    /// Where the package was loaded from, for resuming a save whose region
    /// files aren't all copied into it yet.
    pub directory: Option<String>,
    /// Saved in its own table (see `storage.rs`), since it grows with the
    /// package.
    #[serde(skip)]
    pub index: Arc<RegionIndex>,
    #[serde(skip)]
    pub sources: RegionSources,
    /// Original authoring bytes, when available; never fabricate coordinates
    /// from a reconstructed manifest or persist diagnostic state as game state.
    #[serde(skip)]
    manifest_text: Option<Arc<str>>,
    /// Diagnostic-only author declaration ownership, never replay input.
    #[serde(skip)]
    source_provenance: BTreeMap<String, String>,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Package source identities use repository LF bytes. Comments and all other
/// edits still affect integrity; only CRLF/LF checkout differences are ignored.
pub(crate) fn source_digest(text: &str) -> String {
    if text.contains("\r\n") {
        digest(text.replace("\r\n", "\n").as_bytes())
    } else {
        digest(text.as_bytes())
    }
}
fn read_limited(root: &Path, relative: &str, limit: u64) -> Result<String, Failure> {
    require(
        !relative.is_empty()
            && Path::new(relative)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Package paths must be relative without '..'",
    )?;
    let base = root
        .canonicalize()
        .map_err(|e| fail(format!("{}: {e}", root.display())))?;
    let path = base
        .join(relative)
        .canonicalize()
        .map_err(|e| fail(format!("{relative}: {e}")))?;
    require(
        path.starts_with(&base),
        "Package file resolves outside package directory",
    )?;
    let metadata = std::fs::metadata(&path).map_err(|e| fail(format!("{relative}: {e}")))?;
    require(
        metadata.is_file(),
        format!("{relative}: package input must be a regular file"),
    )?;
    require(
        metadata.len() <= limit,
        format!("{relative} exceeds {} KiB", limit / 1024),
    )?;
    let file = std::fs::File::open(path).map_err(|e| fail(format!("{relative}: {e}")))?;
    require(
        file.metadata().map_err(|e| fail(e.to_string()))?.is_file(),
        format!("{relative}: package input must be a regular file"),
    )?;
    require(
        file.metadata().map_err(|e| fail(e.to_string()))?.len() <= limit,
        format!("{relative} exceeds {} KiB", limit / 1024),
    )?;
    read_text_limited(file, relative, limit)
}

fn read_text_limited(reader: impl Read, relative: &str, limit: u64) -> Result<String, Failure> {
    let mut text = String::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|e| fail(format!("{relative}: {e}")))?;
    require(
        text.len() as u64 <= limit,
        format!("{relative} exceeds {} KiB", limit / 1024),
    )?;
    Ok(text)
}
fn read(root: &Path, relative: &str) -> Result<String, Failure> {
    read_limited(root, relative, MAX_BYTES)
}
fn parse<T: serde::de::DeserializeOwned>(text: &str, name: &str) -> Result<T, Failure> {
    toml::from_str(text).map_err(|e| fail(format!("{name}: {e}")))
}
/// Where region `id`'s file is in a package.
fn region_file(id: u64) -> String {
    format!("{REGION_DIR}/{id}.toml")
}

fn valid_rules_path(relative: &str) -> bool {
    !relative.is_empty()
        && relative.ends_with(".toml")
        && !relative.contains(['\\', ':'])
        && relative != "scenario.toml"
        && !relative.starts_with("regions/")
        && Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn parse_rules(source: &str, file: &str) -> Result<Arc<tor_simulation::rules::Ruleset>, Failure> {
    require(
        source.len() as u64 <= MAX_BYTES,
        "Ruleset exceeds source limit",
    )?;
    let document = rules_source::resolve(source, file)?.document;
    let rules = tor_simulation::rules::Ruleset::validate(document)
        .map_err(|error| fail(format!("{file}: {error:?}")))?;
    require(rules.id().as_str() == RULESET, "Unsupported ruleset ID")?;
    Ok(rules)
}

fn check_manifest_header(manifest: &Manifest) -> Result<(), Failure> {
    require(
        manifest.format == FORMAT,
        format!("Unsupported scenario format; expected format = {FORMAT}"),
    )?;
    require(
        valid_rules_path(&manifest.ruleset),
        "Ruleset must name a relative TOML file in the package",
    )?;
    require(label(&manifest.id), "Invalid scenario ID")?;
    let version: Vec<_> = manifest.version.split('.').collect();
    require(
        version.len() == 2
            && version.iter().all(|v| {
                !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) && v.parse::<u32>().is_ok()
            }),
        "Version must be author-controlled major.minor",
    )?;
    Ok(())
}

/// Every region file in the package, read and indexed. Reads the whole
/// package: validation and unvalidated development use it.
fn scan_regions(root: &Path) -> Result<(RegionIndex, BTreeMap<u64, Arc<str>>), Failure> {
    let directory = root.join(REGION_DIR);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((RegionIndex { regions: vec![] }, BTreeMap::new()));
        }
        Err(error) => return Err(fail(format!("{}: {error}", directory.display()))),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| fail(e.to_string()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| fail("Non-Unicode region file name"))?;
        require(
            name.strip_suffix(".toml").is_some_and(|id| {
                id.parse::<u64>()
                    .is_ok_and(|n| n > 0 && n.to_string() == id)
            }),
            format!("{REGION_DIR}/{name}: region files are named <region id>.toml"),
        )?;
        require(
            names.len() < MAX_REGIONS,
            "Region count exceeds package limit",
        )?;
        names.push(name);
    }
    require(
        names.len() <= MAX_REGIONS,
        format!("Expected 1..{MAX_REGIONS} region files"),
    )?;
    names.sort();
    let mut regions = Vec::new();
    let mut texts = BTreeMap::new();
    let mut total_source_bytes = 0u64;
    for name in names {
        let file = format!("{REGION_DIR}/{name}");
        let text = read_limited(root, &file, MAX_REGION_BYTES)?;
        total_source_bytes = total_source_bytes
            .checked_add(text.len() as u64)
            .ok_or_else(|| fail("Package source byte count overflow"))?;
        require(
            total_source_bytes <= artifact::MAX_PACKAGE_SOURCE_BYTES,
            "Region sources exceed aggregate package byte limit",
        )?;
        let def: RegionDef = parse(&text, &file)?;
        require(
            file == region_file(def.id),
            format!(
                "{file}: holds region {}; name it {}",
                def.id,
                region_file(def.id)
            ),
        )?;
        regions.push(IndexedRegion::of(&def, file, source_digest(&text))?);
        texts.insert(def.id, Arc::from(text));
    }
    regions.sort_by_key(|r| r.id);
    Ok((RegionIndex { regions }, texts))
}

fn model_hash(
    manifest: &Manifest,
    index: &RegionIndex,
    ruleset_hash: &str,
    _dependencies: &BTreeMap<String, String>,
) -> Result<String, Failure> {
    let mut model = serde_json::to_value(manifest).map_err(|e| fail(e.to_string()))?;
    // Source schema, release label and import path are provenance. Definitions,
    // generator contracts and effective rules carry simulation semantics.
    if let Some(fields) = model.as_object_mut() {
        fields.remove("format");
        fields.remove("version");
        fields.remove("ruleset");
    }
    let mut regions = serde_json::to_value(index).map_err(|e| fail(e.to_string()))?;
    if let Some(entries) = regions
        .get_mut("regions")
        .and_then(serde_json::Value::as_array_mut)
    {
        for entry in entries {
            if let Some(fields) = entry.as_object_mut() {
                fields.remove("file");
                fields.remove("hash");
            }
        }
    }
    Ok(digest(
        &serde_json::to_vec(&("tor-compiled-model-v1", model, regions, ruleset_hash))
            .map_err(|e| fail(e.to_string()))?,
    ))
}

impl Package {
    pub(crate) fn procedural_family(
        &self,
        region: u64,
    ) -> Option<(&str, &crate::procedural_world::WorldFamily)> {
        crate::procedural_world::FamilyCatalog::new(&self.manifest.procedural_worlds, 0)
            .find(region)
    }
    fn structural_definition(&self, seed: u64, region: u64) -> Result<Arc<RegionDef>, Failure> {
        if let Some((id, family)) = self.procedural_family(region) {
            return family
                .describe(id, seed, region)?
                .map(Arc::new)
                .ok_or_else(|| fail("Procedural address outside seeded domain"));
        }
        self.region_def(region)
    }
    fn structural_metadata(&self, seed: u64, region: u64) -> Result<IndexedRegion, Failure> {
        if let Some(indexed) = self.index.region(region) {
            return Ok(indexed.clone());
        }
        let def = self.structural_definition(seed, region)?;
        IndexedRegion::of(&def, String::new(), String::new())
    }
    fn generation_group(
        &self,
        identity: &str,
        seed: u64,
    ) -> Result<crate::generation_recipe::GenerationGroup, Failure> {
        if let Some(group) = self.manifest.generation_groups.get(identity) {
            return Ok(group.clone());
        }
        let (family_id, depth) = identity
            .rsplit_once('/')
            .ok_or_else(|| fail("Unknown generation group"))?;
        let family = self
            .manifest
            .procedural_worlds
            .get(family_id)
            .ok_or_else(|| fail("Unknown procedural family"))?;
        let depth = depth
            .parse::<u32>()
            .map_err(|_| fail("Invalid procedural depth"))?;
        family.group(family_id, seed, depth)
    }
    fn check_procedural_worlds(&self) -> Result<(), Failure> {
        require(
            self.manifest.procedural_worlds.len() <= 64,
            "Too many procedural families",
        )?;
        let mut intervals = Vec::new();
        for (id, family) in &self.manifest.procedural_worlds {
            (|| -> Result<(), Failure> {
                require(
                    label(id) && id.len() <= 40 && !id.contains('/'),
                    "Invalid procedural family identity",
                )?;
                family.check()?;
                require(
                    self.manifest
                        .generation_recipes
                        .contains_key(&family.recipe),
                    "Unknown procedural generation recipe",
                )?;
                crate::generation_recipe::check_layout(
                    &self.manifest.generation_recipes[&family.recipe],
                    family.size,
                )?;
                require(
                    family
                        .zone
                        .as_ref()
                        .is_none_or(|zone| self.manifest.zones.contains_key(zone)),
                    "Unknown procedural zone",
                )?;
                let end = family.last_region().expect("checked range");
                require(
                    !self
                        .index
                        .regions
                        .iter()
                        .any(|r| (family.base_region..=end).contains(&r.id)),
                    "Procedural identity range overlaps authored region",
                )?;
                require(
                    !intervals
                        .iter()
                        .any(|&(start, stop)| family.base_region <= stop && start <= end),
                    "Procedural identity ranges overlap",
                )?;
                intervals.push((family.base_region, end));
                // Validate one bounded input closure; the chain topology is arithmetic.
                // Geometry correctness is sampled separately, never expanded by domain size.
                let group = family.group(id, 0, 1)?;
                crate::generation_recipe::check_group(&group)?;
                for region in &group.members {
                    let def = family.describe(id, 0, *region)?.expect("checked member");
                    crate::generator::check(
                        &def,
                        def.generate.as_ref().expect("generated member"),
                    )?;
                }

                Ok(())
            })()
            .map_err(|failure| {
                in_declaration(
                    failure,
                    format!(
                        "{}: procedural_worlds {id:?}",
                        source::declaration_file(&self.source_provenance, "procedural_worlds", id)
                    ),
                )
            })?;
        }
        Ok(())
    }
    /// Validate the exact rules document pinned by this package.
    pub fn rules(&self) -> Result<Arc<tor_simulation::rules::Ruleset>, Failure> {
        let source = &self.rules_document;
        require(
            source.len() as u64 <= MAX_BYTES,
            "Resolved rules document exceeds limit",
        )?;
        let hash = digest(source.as_bytes());
        let mut cache = self
            .rules_cache
            .lock()
            .map_err(|_| fail("Ruleset cache unavailable"))?;
        if let Some(cached) = &*cache {
            if cached.source_hash == hash {
                return Ok(cached.rules.clone());
            }
        }
        let document = crate::storage::schema::scenario::decode_rules_document(source)?;
        let rules = tor_simulation::rules::Ruleset::validate(document)
            .map_err(|error| fail(format!("{}: {error:?}", self.manifest.ruleset)))?;
        *cache = Some(CachedRules {
            source_hash: hash,
            rules: rules.clone(),
        });
        Ok(rules)
    }

    pub(crate) fn content(&self) -> Result<Arc<tor_simulation::creatures::Content>, Failure> {
        Ok(compiler::compile_content_decorations(
            &self.manifest,
            self.manifest
                .creatures
                .compile_with_context(self.rules()?, |kind, id, failure| {
                    source::catalog_context(&self.source_provenance, kind, id, failure)
                })?,
        )?
        .content()
        .clone())
    }

    fn check_generation_groups(&self) -> Result<(), Failure> {
        for (id, recipe) in &self.manifest.generation_recipes {
            (|| -> Result<(), Failure> {
                require(
                    label(id) && !id.contains('/'),
                    "Invalid generation recipe identity",
                )?;
                crate::generation_recipe::check(recipe)?;
                for stage in &recipe.stages {
                    if let crate::generation_recipe::Operation::HiddenTraps { config } =
                        &stage.operation
                    {
                        let rules = self.rules()?;
                        check_trap_effect(
                            &rules,
                            &TrapKind::Rust {
                                slot: config.rust_slot.clone(),
                                amount: config.rust_amount,
                            },
                        )?;
                        check_trap_effect(
                            &rules,
                            &TrapKind::SleepingGas {
                                condition: config.sleep_condition.clone(),
                                duration: config.sleep_duration,
                                exposure: config.sleep_exposure.clone(),
                            },
                        )?;
                        if let Some(hazards) = &config.hazards {
                            check_trap_effect(
                                &rules,
                                &TrapKind::BearTrap {
                                    condition: hazards.bear_condition.clone(),
                                    duration: hazards.bear_duration,
                                },
                            )?;
                            self.check_trap_ammunition(&hazards.arrow_ammunition)?;
                            check_trap_effect(
                                &rules,
                                &TrapKind::PoisonDart {
                                    attack_bonus: hazards.attack_bonus,
                                    exposure: hazards.poison_exposure.clone(),
                                },
                            )?;
                        }
                    }
                }

                Ok(())
            })()
            .map_err(|failure| {
                in_declaration(
                    failure,
                    format!(
                        "{}: generation_recipes {id:?}",
                        source::declaration_file(&self.source_provenance, "generation_recipes", id)
                    ),
                )
            })?;
        }
        let mut members = BTreeSet::new();
        for (id, group) in &self.manifest.generation_groups {
            (|| -> Result<(), Failure> {
                crate::generation_recipe::check_group(group)?;
                require(
                    label(id)
                        && !id.contains('/')
                        && group.depth > 0
                        && self.manifest.generation_recipes.contains_key(&group.recipe),
                    "Invalid generation group metadata",
                )?;
                let mut size = None;
                for &region in &group.members {
                    require(members.insert(region), "Duplicate generation group member")?;
                    let metadata = self
                        .index
                        .region(region)
                        .ok_or_else(|| fail("Missing generation group member"))?;
                    require(
                        metadata.generated
                            && metadata.actors.is_empty()
                            && metadata.items.is_empty()
                            && metadata.doors.is_empty()
                            && metadata.traps.is_empty(),
                        "Recipe members must be generated and declare no inhabitants",
                    )?;
                    if let Some(expected) = size {
                        require(
                            metadata.size == expected,
                            "Unequal generation group member bounds",
                        )?;
                    }
                    size = Some(metadata.size);
                }

                Ok(())
            })()
            .map_err(|failure| {
                in_declaration(
                    failure,
                    format!(
                        "{}: generation_groups {id:?}",
                        source::declaration_file(&self.source_provenance, "generation_groups", id)
                    ),
                )
            })?;
        }
        Ok(())
    }

    fn boundary_anchor(&self, name: &str) -> bool {
        name.split_once('/').is_some_and(|(id, name)| {
            id.parse::<u64>().ok().is_some_and(|id| {
                self.index
                    .region(id)
                    .is_some_and(|r| r.boundary_anchors.iter().any(|b| b == name))
                    || (self.procedural_family(id).is_some()
                        && matches!(name, "west" | "east" | "north" | "south"))
            })
        })
    }

    fn group_definitions(
        &self,
        identity: &str,
        seed: u64,
    ) -> Result<BTreeMap<u64, RegionDef>, Failure> {
        Ok(self.generated_group(identity, seed)?.regions)
    }

    fn feature_bases(&self) -> Result<crate::generation_recipe::FeatureBases, Failure> {
        let next = |maximum: Option<u64>| {
            maximum
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| fail("Authored feature identity overflow"))
        };
        Ok(crate::generation_recipe::FeatureBases {
            doors: next(
                self.index
                    .regions
                    .iter()
                    .flat_map(|r| r.doors.iter().copied())
                    .max(),
            )?,
            traps: next(
                self.index
                    .regions
                    .iter()
                    .flat_map(|r| r.traps.iter().copied())
                    .max(),
            )?,
        })
    }
    fn generated_feature_ceiling(&self, base: u64) -> Result<u64, Failure> {
        let has_features = |recipe: &crate::generation_recipe::Recipe| {
            recipe.stages.iter().any(|stage| {
                matches!(
                    stage.operation,
                    crate::generation_recipe::Operation::SecretEntrances { .. }
                        | crate::generation_recipe::Operation::HiddenTraps { .. }
                )
            })
        };
        let explicit = self
            .manifest
            .generation_groups
            .values()
            .filter(|group| {
                self.manifest
                    .generation_recipes
                    .get(&group.recipe)
                    .is_some_and(has_features)
            })
            .flat_map(|group| group.members.iter().copied());
        let families = self
            .manifest
            .procedural_worlds
            .values()
            .filter(|family| {
                self.manifest
                    .generation_recipes
                    .get(&family.recipe)
                    .is_some_and(has_features)
            })
            .filter_map(|family| family.last_region());
        explicit
            .chain(families)
            .max()
            .unwrap_or(0)
            .checked_mul(crate::generation_recipe::FEATURE_IDENTITY_STRIDE)
            .and_then(|n| n.checked_add(base))
            .filter(|end| *end < u64::MAX)
            .ok_or_else(|| fail("Generated feature namespace exceeds identity capacity"))
    }

    fn generated_group(
        &self,
        identity: &str,
        seed: u64,
    ) -> Result<crate::generation_recipe::GeneratedGroup, Failure> {
        let group = self.generation_group(identity, seed)?;
        let recipe = self
            .manifest
            .generation_recipes
            .get(&group.recipe)
            .ok_or_else(|| fail("Unknown generation recipe"))?;
        let mut definitions = Vec::new();
        for &region in &group.members {
            let definition = self.structural_definition(seed, region)?;
            let generate = definition
                .generate
                .as_ref()
                .ok_or_else(|| fail("Group member has no generator"))?;
            require(
                generate.group.as_deref() == Some(identity),
                "Group member names a different generation group",
            )?;
            crate::generator::check(&definition, generate)?;
            definitions.push((*definition).clone());
        }
        for (slot, definition) in definitions.iter().enumerate() {
            let [width, height, clearance] = definition.size;
            require(
                clearance == 2 && definition.chamber,
                "Grid members require a two-cell chamber interior",
            )?;
            let mut expected = BTreeMap::new();
            if slot % group.grid[0] as usize > 0 {
                expected.insert(
                    "west",
                    (
                        group.members[slot - 1],
                        [0, 0, 0],
                        [width - 1, 0, 0],
                        height,
                    ),
                );
            }
            if slot % (group.grid[0] as usize) + 1 < group.grid[0] as usize {
                expected.insert(
                    "east",
                    (
                        group.members[slot + 1],
                        [width - 1, 0, 0],
                        [0, 0, 0],
                        height,
                    ),
                );
            }
            if slot / group.grid[0] as usize > 0 {
                expected.insert(
                    "north",
                    (
                        group.members[slot - group.grid[0] as usize],
                        [0, 0, 0],
                        [0, height - 1, 0],
                        width,
                    ),
                );
            }
            if slot / (group.grid[0] as usize) + 1 < group.grid[1] as usize {
                expected.insert(
                    "south",
                    (
                        group.members[slot + group.grid[0] as usize],
                        [0, height - 1, 0],
                        [0, 0, 0],
                        width,
                    ),
                );
            }
            require(
                definition.portals.len() == expected.len(),
                "Grid faces need exactly their neighboring boundary joins",
            )?;
            for portal in &definition.portals {
                let (neighbor, from, to, span) = expected
                    .remove(portal.direction.as_str())
                    .ok_or_else(|| fail("Unexpected or duplicate grid boundary direction"))?;
                let (id, name) = portal
                    .to
                    .split_once('/')
                    .ok_or_else(|| fail("Invalid grid boundary destination"))?;
                let destination = definitions
                    .iter()
                    .find(|d| d.id == neighbor)
                    .expect("group neighbor");
                require(
                    id == neighbor.to_string()
                        && destination.anchors.get(name) == Some(&to)
                        && destination
                            .generate
                            .as_ref()
                            .is_some_and(|g| g.boundary_anchors.iter().any(|a| a == name))
                        && portal.kind.as_deref() == Some("boundary")
                        && portal.at == from
                        && portal.width == span as u16
                        && portal.height == clearance as u16
                        && portal.turns == 0
                        && portal.rotation.is_none_or(|rotation| rotation == 0),
                    "Grid boundary must cover the complete reciprocal face with identity mapping",
                )?;
            }
        }
        crate::generation_recipe::materialize_with_feature_bases(
            identity,
            &group,
            recipe,
            definitions,
            seed,
            self.feature_bases()?,
        )
    }

    pub(crate) fn build_group(
        &self,
        seed: u64,
        index: &PackageIndex,
        owner: RegionId,
    ) -> Result<PreparedGroup, Failure> {
        let identity = index
            .group_identity(owner.0)
            .ok_or_else(|| fail("Missing generation group owner"))?;
        let group = self.generation_group(&identity, seed)?;
        let generated = self.generated_group(&identity, seed)?;
        let spatial = self.canonical_generated_spatial(&identity, seed, &generated)?;
        let rooms = generated.rooms;
        let definitions = generated.regions;
        let shell: BTreeMap<u64, Arc<RegionDef>> = definitions
            .into_iter()
            .map(|(id, def)| (id, Arc::new(def)))
            .collect();
        let destinations: BTreeSet<_> = group
            .members
            .iter()
            .flat_map(|id| {
                index
                    .stair_exits(*id)
                    .into_iter()
                    .map(|stair| stair.destination)
            })
            .collect();
        let mut files: BTreeSet<_> = group
            .members
            .iter()
            .copied()
            .filter(|id| self.index.region(*id).is_some())
            .collect();
        let members: Vec<_> = group.members.iter().map(|id| &*shell[id]).collect();
        let anchors = resolved_anchors(&members);
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
            index.definitions.creature_catalog().content().clone(),
        );
        game.defer_initial_awareness()
            .map_err(|error| fail(format!("Initial awareness construction: {error:?}")))?;
        self.add_geometry(&mut game, &members)?;
        for destination in destinations {
            if !shell.contains_key(&destination) {
                let declaration = self.unbuilt_region(index, destination)?;
                if self.declaring_reads_file(destination) {
                    files.insert(destination);
                }
                game.add_unbuilt_region(
                    declaration.region,
                    declaration.chamber,
                    declaration.identities,
                )
                .map_err(|e| fail(format!("Group destination: {e:?}")))?;
            }
        }
        for r in &members {
            self.check_region(r)?;
            self.add_structure(&mut game, r, &anchors)?;
            self.add_stairs(&mut game, r.id, index, &anchors)?;
        }
        self.add_entities(&mut game, seed, index, &members)?;
        let records = group
            .members
            .iter()
            .map(|id| {
                let mut declaration = self.unbuilt_region(index, *id)?;
                declaration
                    .identities
                    .doors
                    .extend(shell[id].doors.iter().map(|d| d.id));
                declaration
                    .identities
                    .traps
                    .extend(shell[id].traps.iter().map(|t| t.id));
                let record = game
                    .clone()
                    .into_region_record(RegionId(*id))
                    .map_err(|e| fail(format!("Group {identity}: {e:?}")))?;
                Ok((declaration, record))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let activation = self.activation_scope(&identity, group.depth, &members, &rooms)?;
        Ok(PreparedGroup {
            records,
            files,
            activation,
            spatial,
        })
    }

    /// Restore only stored metadata. Storage attaches the bounded index and lazy
    /// region sources separately before validating or exposing the scenario.
    pub(crate) fn from_saved_metadata(
        manifest: Manifest,
        rules_document: String,
        certificate: Certificate,
        validated: bool,
        selected: u64,
        directory: Option<String>,
    ) -> Self {
        Self {
            manifest,
            rules_source: String::new(),
            resolved_rules_source: None,
            rules_document,
            rules_cache: Default::default(),
            certificate,
            validated,
            selected,
            directory,
            index: Default::default(),
            sources: Default::default(),
            manifest_text: None,
            source_provenance: BTreeMap::new(),
        }
    }

    fn archetype_origin<'a>(&'a self, name: &'a str) -> Origin<'a> {
        source::archetype_origin(&self.source_provenance, name)
    }

    /// Re-read only on an error path. Changed/unavailable source never earns a guessed span.
    fn diagnostic_source(&self, file: &str) -> Option<Arc<str>> {
        if file == "scenario.toml" && self.manifest_text.is_some() {
            return self.manifest_text.clone();
        }
        let expected = self.certificate.files.get(file)?;
        let text = read(Path::new(self.directory.as_deref()?), file).ok()?;
        (source_digest(&text) == *expected).then(|| Arc::from(text))
    }

    fn pin_source_closure(
        &mut self,
        hashes: BTreeMap<String, String>,
        canonical: &str,
    ) -> Result<(), Failure> {
        if hashes.get("scenario.toml") != Some(&source_digest(canonical)) {
            self.manifest_text = None;
        }
        self.certificate.files.extend(hashes);
        self.certificate.content_hash = digest(
            &serde_json::to_vec(&(
                &self.certificate.files,
                &self.certificate.ruleset_dependencies,
            ))
            .map_err(|e| fail(e.to_string()))?,
        );
        Ok(())
    }

    fn assemble(
        manifest: Manifest,
        manifest_text: &str,
        rules_source: &str,
        index: RegionIndex,
        sources: RegionSources,
        directory: Option<&Path>,
    ) -> Result<Self, Failure> {
        require(
            valid_rules_path(&manifest.ruleset),
            "Ruleset must name a relative TOML file",
        )?;
        let resolved = rules_source::resolve(rules_source, &manifest.ruleset)?;
        let rules_document =
            crate::storage::schema::scenario::encode_rules_document(&resolved.document)?;
        let rules = tor_simulation::rules::Ruleset::validate(
            crate::storage::schema::scenario::decode_rules_document(&rules_document)?,
        )
        .map_err(|error| fail(format!("{}: {error:?}", manifest.ruleset)))?;
        require(rules.id().as_str() == RULESET, "Unsupported ruleset ID")?;
        let ruleset_hash = digest(rules_document.as_bytes());
        let files = BTreeMap::from([
            (manifest.ruleset.clone(), source_digest(rules_source)),
            ("scenario.toml".into(), source_digest(manifest_text)),
            ("index.json".into(), digest(&index.to_bytes()?)),
        ]);
        let content_hash = digest(
            &serde_json::to_vec(&(&files, &resolved.dependencies))
                .map_err(|e| fail(e.to_string()))?,
        );
        let model_hash = model_hash(&manifest, &index, &ruleset_hash, &resolved.dependencies)?;
        let certificate = Certificate {
            model_hash,
            validator: VALIDATOR.into(),
            ruleset: rules.id().as_str().into(),
            ruleset_hash,
            ruleset_dependencies: resolved.dependencies,
            content_hash,
            files,
            regions: index.regions.len(),
            coverage: "all authored regions and character starts; descriptor structural bounds; deterministic sampled groups at seeds 0, 1, 42 under explicit work budget; no all-seed or winnability proof".into(),
        };
        Ok(Self {
            selected: manifest.default_character,
            rules_document,
            rules_source: rules_source.to_owned(),
            resolved_rules_source: resolved.materialized.clone(),
            rules_cache: Arc::new(std::sync::Mutex::new(Some(CachedRules {
                source_hash: certificate.ruleset_hash.clone(),
                rules,
            }))),
            manifest,
            certificate,
            validated: false,
            directory: directory
                .and_then(|d| d.canonicalize().ok())
                .map(|d| d.display().to_string()),
            index: Arc::new(index),
            sources,
            manifest_text: Some(Arc::from(manifest_text)),
            source_provenance: BTreeMap::new(),
        })
    }

    /// A package from definitions in memory, for tools and tests. Unvalidated.
    pub fn from_parts(
        manifest: Manifest,
        regions: Vec<RegionDef>,
        rules_source: &str,
    ) -> Result<Self, Failure> {
        let manifest_text = toml::to_string(&manifest).map_err(|e| fail(e.to_string()))?;
        let mut index = Vec::new();
        let mut texts = BTreeMap::new();
        for def in &regions {
            let text = toml::to_string(def).map_err(|e| fail(e.to_string()))?;
            index.push(IndexedRegion::of(
                def,
                region_file(def.id),
                source_digest(&text),
            )?);
            require(
                texts.insert(def.id, Arc::<str>::from(text)).is_none(),
                format!("Duplicate region {}", def.id),
            )?;
        }
        index.sort_by_key(|r| r.id);
        Self::assemble(
            manifest,
            &manifest_text,
            rules_source,
            RegionIndex { regions: index },
            RegionSources::with(None, texts),
            None,
        )
    }

    /// Every region's definition, in id order. Reads the whole package:
    /// for tools, tests and whole-package builds only.
    pub fn region_defs(&self) -> Result<Vec<RegionDef>, Failure> {
        self.index
            .regions
            .iter()
            .map(|entry| Ok((*self.sources.def(entry)?).clone()))
            .collect()
    }

    /// One region's definition, checked against the index.
    pub fn region_def(&self, region: u64) -> Result<Arc<RegionDef>, Failure> {
        let entry = self
            .index
            .region(region)
            .ok_or_else(|| fail(format!("Unknown region {region}")))?;
        self.sources.def(entry)
    }

    /// Canonical compiled input copied by saves, independent of authored syntax.
    pub(crate) fn compiled_region(&self, region: u64) -> Result<Arc<str>, Failure> {
        Ok(Arc::from(
            crate::storage::schema::scenario::encode_region_input(
                self.region_def(region)?.as_ref(),
            )?,
        ))
    }

    /// Attach the exact persisted input closure for both normal and profiling
    /// games. A source directory alone never enables runtime TOML fallback.
    pub(crate) fn attach_persisted_inputs(
        &self,
        reader: crate::storage::SourceReader,
        supplied: Option<&Package>,
    ) {
        let compiled_directory = locate(self, supplied);
        self.sources.attach_saved(reader, compiled_directory);
    }

    /// A development/in-memory package has no immutable artifact to recover
    /// unvisited inputs from. Pin its bounded authored inputs at initial save
    /// creation; compact procedural families need no expanded snapshots.
    pub(crate) fn initial_region_inputs(
        &self,
        needed: Vec<(u64, Arc<str>)>,
    ) -> Result<Vec<(u64, Arc<str>)>, Failure> {
        if locate(self, None).is_some() {
            return Ok(needed);
        }
        let mut bytes = 0usize;
        self.index
            .regions
            .iter()
            .map(|entry| {
                let input = self.compiled_region(entry.id)?;
                bytes = bytes
                    .checked_add(input.len())
                    .ok_or_else(|| fail("Compiled snapshot byte count overflow"))?;
                require(
                    bytes <= 128 * 1024 * 1024,
                    "Initial authored snapshots exceed save byte limit",
                )?;
                Ok((entry.id, input))
            })
            .collect()
    }

    /// Authored diagnostic text, when available; never synthesized from a save.
    pub(crate) fn region_text(&self, region: u64) -> Result<Arc<str>, Failure> {
        let entry = self
            .index
            .region(region)
            .ok_or_else(|| fail(format!("Unknown region {region}")))?;
        self.sources.text(entry)
    }
}

/// A package read from its files, indexing every region file afresh. For
/// validation and unvalidated development.
pub(crate) fn read_package(root: &Path) -> Result<Package, Failure> {
    let resolved = resolve_source(root)?;
    let rules_source = read(root, &resolved.manifest.ruleset)?;
    let (index, texts) = scan_regions(root)?;
    let mut package = Package::assemble(
        resolved.manifest,
        &resolved.manifest_text,
        &rules_source,
        index,
        RegionSources::with(Some(root), texts),
        Some(root),
    )?;
    package.source_provenance = resolved.provenance;
    package.pin_source_closure(resolved.source_hashes, &resolved.manifest_text)?;
    Ok(package)
}

/// The directory holding `package`'s region files: the supplied package's,
/// or the one it was loaded from, when either is still the same package.
pub(crate) fn locate(package: &Package, supplied: Option<&Package>) -> Option<std::path::PathBuf> {
    artifact::locate(package, supplied)
}

/// A validated package: its manifest and generated index, with region
/// files read only when their regions are built.
fn read_indexed(root: &Path) -> Result<Package, Failure> {
    let resolved = resolve_source(root)?;
    let rules_source = read(root, &resolved.manifest.ruleset)?;
    let index =
        RegionIndex::from_bytes(read_limited(root, "index.json", MAX_INDEX_BYTES)?.as_bytes())?;
    let mut package = Package::assemble(
        resolved.manifest,
        &resolved.manifest_text,
        &rules_source,
        index,
        RegionSources::with(Some(root), BTreeMap::new()),
        Some(root),
    )?;
    package.source_provenance = resolved.provenance;
    package.pin_source_closure(resolved.source_hashes, &resolved.manifest_text)?;
    Ok(package)
}

/// Write a package's files: the manifest and one file per region. No index
/// or certificate; validate it, or load it as unvalidated.
pub fn write_package(
    out: &Path,
    manifest: &Manifest,
    regions: &[RegionDef],
    rules_source: &str,
) -> Result<(), Failure> {
    require(valid_rules_path(&manifest.ruleset), "Invalid ruleset path")?;
    parse_rules(rules_source, &manifest.ruleset)?;
    let io = |e: std::io::Error| fail(e.to_string());
    std::fs::create_dir_all(out.join(REGION_DIR)).map_err(io)?;
    let text = toml::to_string(manifest).map_err(|e| fail(e.to_string()))?;
    std::fs::write(out.join("scenario.toml"), text).map_err(io)?;
    let rules_path = out.join(&manifest.ruleset);
    if let Some(parent) = rules_path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(rules_path, rules_source).map_err(io)?;
    for def in regions {
        let text = toml::to_string(def).map_err(|e| fail(e.to_string()))?;
        std::fs::write(out.join(region_file(def.id)), text).map_err(io)?;
    }
    Ok(())
}

pub(crate) fn checked_package(root: &Path) -> Result<Package, Failure> {
    let mut package = read_package(root)?;
    package.check()?;
    let volume = |size: [i32; 3], chamber: bool| -> Result<u64, Failure> {
        size.into_iter().try_fold(1u64, |product, side| {
            let side = u64::try_from(side).map_err(|_| fail("Invalid validation volume"))?;
            product
                .checked_mul(side + if chamber { 2 } else { 0 })
                .ok_or_else(|| fail("Validation cell-work overflow"))
        })
    };
    let repetitions = package.manifest.characters.len() as u64 * 6;
    let mut anchor_members = BTreeSet::new();
    for character in &package.manifest.characters {
        let (endpoint, region) = package.character_endpoint(character.id, 0, &character.anchor)?;
        let name = endpoint
            .split_once('/')
            .expect("resolved character endpoint")
            .1;
        let group = package
            .index
            .region(region)
            .filter(|r| !r.anchors.contains_key(name))
            .and_then(|_| {
                package
                    .manifest
                    .generation_groups
                    .values()
                    .find(|g| g.members.contains(&region))
            });
        if let Some(group) = group {
            anchor_members.extend(group.members.iter().copied());
        }
    }
    let mut total_work = 0u64;
    for region in &package.index.regions {
        let cost = volume(region.size, region.chamber)?;
        let anchor_repetitions = if anchor_members.contains(&region.id) {
            // Two selected indexes per seed for authored packages; procedural
            // validation also indexes start() and its separate remote samples.
            repetitions
                * if package.manifest.procedural_worlds.is_empty() {
                    1
                } else {
                    2
                }
                + 3
        } else {
            0
        };
        let repetitions = repetitions + if region.generated { 6 } else { 0 } + anchor_repetitions;
        total_work = total_work
            .checked_add(
                cost.checked_mul(repetitions)
                    .ok_or_else(|| fail("Validation cell-work overflow"))?,
            )
            .ok_or_else(|| fail("Validation cell-work overflow"))?;
        require(
            total_work <= MAX_VALIDATION_CELL_WORK,
            "Validation exceeds aggregate cell-work budget",
        )?;
    }
    // Budget declared roots, generated-anchor resolution and remote samples
    // before carving geometry. Potential domain length never enters this sum.
    let mut validation_cells = 0u64;
    for seed in [0, 1, 42] {
        let roots = package
            .manifest
            .characters
            .iter()
            .map(|c| {
                package
                    .character_endpoint(c.id, seed, &c.anchor)
                    .map(|(_, region)| region)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let groups = |selected| {
            package.initial_procedural_groups(
                package
                    .manifest
                    .characters
                    .iter()
                    .zip(&roots)
                    .filter(|(c, _)| c.id == selected || c.unselected == "ai")
                    .map(|(_, region)| *region),
            )
        };
        let initial_groups: Vec<_> = package
            .manifest
            .characters
            .iter()
            .map(|c| groups(c.id))
            .collect();
        let indexed_groups = groups(package.selected);
        for (identity, family) in &package.manifest.procedural_worlds {
            let last = family.depth_limit(identity, seed);
            let samples = BTreeSet::from([1, 2.min(last), (last / 2).max(1), last]);
            let count = |groups: &BTreeSet<String>| {
                groups
                    .iter()
                    .filter(|group| {
                        group
                            .rsplit_once('/')
                            .is_some_and(|(family, _)| family == identity)
                    })
                    .count() as u64
            };
            let initial: u64 = initial_groups.iter().map(count).sum();
            let indexed = count(&indexed_groups);
            let characters = package.manifest.characters.len() as u64;
            let cells = family.slots() * family.size.iter().map(|n| *n as u64).product::<u64>();
            // Each selected-character/seed check resolves roots in four indexes
            // and builds two initial closures, plus two copies of each sample.
            // The earlier generated-region checks also construct one index.
            validation_cells = validation_cells
                .checked_add(
                    cells * (6 * initial + 2 * samples.len() as u64 * characters + indexed),
                )
                .ok_or_else(|| fail("Procedural validation budget overflow"))?;
            require(
                validation_cells <= 4_194_304,
                "Procedural validation exceeds sampled-cell budget",
            )?;
            let shell_work = volume(family.size, true)?
                * family.slots()
                * 2
                * (initial + samples.len() as u64 * characters);
            total_work = total_work
                .checked_add(shell_work)
                .ok_or_else(|| fail("Validation cell-work overflow"))?;
            require(
                total_work <= MAX_VALIDATION_CELL_WORK,
                "Validation exceeds aggregate cell-work budget",
            )?;
        }
    }
    for def in package.region_defs()? {
        if let Some(generate) = &def.generate {
            crate::generator::check(&def, generate)?;
        }
        package.check_region(&def)?;
        // What a region shows must be in its palette.
        let forecast = package.theme_assets(package.region_themes(def.id).unwrap_or(&[]));
        for asset in package.region_assets(&def)? {
            require(
                forecast.contains(&asset),
                format!(
                    "Region {}: asset {asset} isn't among its themes' assets",
                    def.id
                ),
            )?;
        }
    }
    for seed in [0, 1, 42] {
        let index = package.index(seed)?;
        let mut checked_groups = BTreeSet::new();
        for entry in package.index.regions.iter().filter(|r| r.generated) {
            let pairs: Vec<(Arc<RegionDef>, Arc<RegionDef>)> =
                if let Some(owner) = index.group_owner(entry.id) {
                    if !checked_groups.insert(owner) {
                        continue;
                    }
                    let identity = index.group_identity(owner).expect("checked group owner");
                    let first = package.group_definitions(&identity, seed)?;
                    let mut second = package.group_definitions(&identity, seed)?;
                    first
                        .into_iter()
                        .map(|(id, def)| {
                            (
                                Arc::new(def),
                                Arc::new(second.remove(&id).expect("same group members")),
                            )
                        })
                        .collect()
                } else {
                    vec![(
                        index.region(&package, entry.id)?,
                        index.region(&package, entry.id)?,
                    )]
                };
            for (region, again) in pairs {
                let bytes = |r: &RegionDef| serde_json::to_vec(r).map_err(|e| fail(e.to_string()));
                require(
                    bytes(&region)? == bytes(&again)?,
                    format!("Region {}: nondeterministic generation", region.id),
                )?;
                require(
                    crate::generator::entries_connected(&region),
                    format!("Region {}: generated entries aren't connected", region.id),
                )?;
            }
        }
    }
    for character in &package.manifest.characters {
        package.selected = character.id;
        for seed in [0, 1, 42] {
            if package.manifest.procedural_worlds.is_empty() {
                require(
                    package.build(seed, false)? == package.build(seed, false)?,
                    "Nondeterministic scenario construction",
                )?;
            } else {
                package.start(seed)?;
                require(
                    package.build(seed, false)? == package.build(seed, false)?,
                    "Nondeterministic bounded authored/procedural construction",
                )?;
                let index = package.index(seed)?;
                let mut sampled_cells = 0u64;
                for (identity, family) in &package.manifest.procedural_worlds {
                    let limit = family.depth_limit(identity, seed);
                    let last = limit;
                    let samples = BTreeSet::from([1, 2.min(last), (last / 2).max(1), last]);
                    for depth in samples {
                        sampled_cells +=
                            family.slots() * family.size.iter().map(|n| *n as u64).product::<u64>();
                        require(
                            sampled_cells <= 4_194_304,
                            "Procedural validation exceeds sampled-cell budget",
                        )?;
                        let owner = family.base_region + u64::from(depth - 1) * family.slots();
                        let first = package.build_group(seed, &index, RegionId(owner))?;
                        let second = package.build_group(seed, &index, RegionId(owner))?;
                        require(
                            first.records == second.records,
                            "Nondeterministic procedural generation",
                        )?;
                    }
                }
            }
        }
    }
    Ok(package)
}

/// Validate without writing derived artifacts into the source package.
pub fn check(root: &Path) -> Result<Certificate, Failure> {
    Ok(checked_package(root)?.certificate)
}

pub fn validate(root: &Path) -> Result<Certificate, Failure> {
    let package = checked_package(root)?;
    artifact::publish(root, &package)?;
    Ok(package.certificate)
}

pub fn load(
    root: &Path,
    seed: u64,
    selected: Option<u64>,
    allow_unvalidated: bool,
) -> Result<Scenario, Failure> {
    if root.join("package.json").is_file() {
        let mut package = artifact::load(root)?;
        package.selected = selected.unwrap_or(package.manifest.default_character);
        package.check_identity()?;
        package.supported()?;
        return Ok(Scenario {
            seed,
            actors: vec![],
            regions: package.index.regions.len() as u64,
            workload_version: None,
            package: Some(Arc::new(package)),
            streaming: Some(crate::Streaming::default()),
        });
    }
    let certificate = read(root, "validation.json")
        .ok()
        .and_then(|s| serde_json::from_str::<Certificate>(&s).ok());
    // A validated package starts from its index; a region file edited
    // since is refused when that region is built.
    let mut package = match read_indexed(root) {
        Ok(package) if certificate.as_ref() == Some(&package.certificate) => Package {
            validated: true,
            ..package
        },
        _ => {
            require(
                allow_unvalidated,
                "Scenario is unvalidated or stale; run tor-scenario validate <directory>",
            )?;
            read_package(root)?
        }
    };
    package.selected = selected.unwrap_or(package.manifest.default_character);
    // Cheap identity/capability checks; full geometry proof belongs to the utility.
    package.check_identity_with_origin(selected.is_none().then_some(Origin::Manifest))?;
    package.supported()?;
    Ok(Scenario {
        seed,
        actors: vec![],
        regions: package.index.regions.len() as u64,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: Some(crate::regions::Streaming::default()),
    })
}

/// The streaming corridor package at `root` (`scenarios/tests/streaming-
/// corridor`), lengthened or shortened to `halls` halls in a row and written
/// as an ordinary package to `out`, for region streaming workloads and
/// scaling contracts. Hall 1 keeps its pebble and hall 6 its guard; every
/// other hall is like hall 2. The written package has no certificate, so
/// it's loaded as unvalidated.
pub fn streaming_corridor(
    root: &Path,
    out: &Path,
    halls: u64,
    seed: u64,
) -> Result<Scenario, Failure> {
    corridor(root, out, halls, seed, false)
}

/// [`streaming_corridor`], with every hall after the first generated: each
/// keeps its bounds and links, and entries on row 0 only, so a straight
/// corridor along row 0 always crosses it.
pub fn generated_corridor(
    root: &Path,
    out: &Path,
    halls: u64,
    seed: u64,
) -> Result<Scenario, Failure> {
    corridor(root, out, halls, seed, true)
}

fn corridor(
    root: &Path,
    out: &Path,
    halls: u64,
    seed: u64,
    generated: bool,
) -> Result<Scenario, Failure> {
    require(
        (2..=MAX_REGIONS as u64).contains(&halls),
        format!("A corridor has 2 to {MAX_REGIONS} halls"),
    )?;
    let source = read_package(root)?;
    let original = source.region_defs()?;
    let template = original
        .iter()
        .find(|r| r.id == 2)
        .cloned()
        .ok_or_else(|| fail("Missing hall 2"))?;
    let portal = |direction: &str| {
        template
            .portals
            .iter()
            .find(|p| p.direction == direction)
            .cloned()
            .ok_or_else(|| fail("Missing hall portal"))
    };
    let (east, west) = (portal("east")?, portal("west")?);
    let mut regions = Vec::new();
    for id in 1..=halls {
        let mut hall = original
            .iter()
            .find(|r| r.id == id && (id == 1 || id == 6))
            .cloned()
            .unwrap_or_else(|| template.clone());
        hall.id = id;
        hall.name = format!("Hall {id}");
        hall.portals.clear();
        if id < halls {
            hall.portals.push(Portal {
                to: format!("{}/west", id + 1),
                ..east.clone()
            });
        }
        if id > 1 {
            hall.portals.push(Portal {
                to: format!("{}/east", id - 1),
                ..west.clone()
            });
        }
        if generated && id > 1 {
            hall.anchors.remove("start");
            hall.actors.clear();
            hall.items.clear();
            hall.generate = Some(crate::generator::Generate {
                group: None,
                boundary_anchors: Vec::new(),
                stair_anchors: vec![],
                generator: crate::generator::ROOMS.into(),
                version: crate::generator::ROOMS_VERSION,
                salt: 0,
                rooms: [1, 3],
                actors: None,
                items: None,
            });
        }
        regions.push(hall);
    }
    write_package(out, &source.manifest, &regions, &source.rules_source)?;
    load(out, seed, None, true)
}

fn region_of(id: u64, name: &str, size: [i32; 3]) -> Result<Region, Failure> {
    Ok(Region {
        id: RegionId(id),
        name: name.into(),
        bounds: Extent::new(size[0], size[1], size[2])
            .ok_or_else(|| fail("Invalid region extent"))?,
    })
}
fn loc(region: u64, [x, y, z]: [i32; 3]) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}
impl Package {
    pub fn region_themes(&self, region: u64) -> Option<&[String]> {
        let zone = self
            .index
            .region(region)
            .map(|r| &r.zone)
            .or_else(|| self.procedural_family(region).map(|(_, f)| &f.zone))?;
        Some(
            zone.as_ref()
                .and_then(|z| self.manifest.zones.get(z))
                .and_then(|z| z.themes.as_deref())
                .unwrap_or(&self.manifest.themes),
        )
    }
    /// A region's terrain assets: its zone's, or the world's.
    pub fn region_terrain(&self, region: u64) -> Option<&TerrainAssets> {
        let zone = self
            .index
            .region(region)
            .map(|r| &r.zone)
            .or_else(|| self.procedural_family(region).map(|(_, f)| &f.zone))?
            .as_ref();
        zone.and_then(|z| self.manifest.zones.get(z))
            .and_then(|z| z.terrain.as_ref())
            .or(self.manifest.terrain.as_ref())
    }

    /// Explicit regional forecast. Keep this separate from portable defaults:
    /// globally forecast archetypes must not mask a missing local actor theme.
    fn theme_assets<'a>(&self, themes: impl IntoIterator<Item = &'a String>) -> BTreeSet<String> {
        themes
            .into_iter()
            .flat_map(|theme| self.manifest.assets.get(theme).into_iter().flatten())
            .cloned()
            .collect()
    }

    /// Assets clients may need for nearby themes, plus every declared portable
    /// archetype and character. Archetypes are canonically dual-use: conservatively
    /// forecast their item appearance even if this package only spawns them as
    /// actors. This bounded overforecast needs no region reads or live inventory.
    /// Concealed archetypes contribute only their pool asset, never identity.
    pub fn palette<'a>(&self, themes: impl IntoIterator<Item = &'a String>) -> BTreeSet<String> {
        let mut assets = self.theme_assets(themes);
        assets.extend(
            self.manifest
                .characters
                .iter()
                .filter_map(|c| c.asset.clone()),
        );
        assets.extend(
            self.manifest
                .archetypes
                .values()
                .flat_map(|archetype| compiler::item_assets(archetype, &self.manifest))
                .map(str::to_owned),
        );
        assets
    }

    /// A palette is needed even when only portable or character assets exist.
    pub fn has_assets(&self) -> bool {
        !self.manifest.assets.is_empty()
            || self.manifest.characters.iter().any(|c| c.asset.is_some())
            || self
                .manifest
                .archetypes
                .values()
                .any(|a| compiler::item_assets(a, &self.manifest).next().is_some())
    }

    /// Assets a region's content may show: its terrain, and its actors' and
    /// items' archetypes (a concealed item's pool asset instead). The
    /// validator requires them in the region's themes' assets, so a
    /// palette forecasts them.
    fn region_assets(&self, r: &RegionDef) -> Result<BTreeSet<String>, Failure> {
        let mut assets = BTreeSet::new();
        if let Some(t) = self.region_terrain(r.id) {
            assets.extend([&t.floor, &t.wall, &t.door].into_iter().flatten().cloned());
        }
        // Each asset exactly as building the region assigns it: an actor
        // its archetype's, an item its item asset.
        let file = self
            .index
            .region(r.id)
            .map(|entry| entry.file.as_str())
            .unwrap_or("generated region");
        for actor in &r.actors {
            if let Some(key) = &actor.archetype {
                let archetype = self.author_archetype(key).map_err(|failure| {
                    Origin::Actor {
                        file,
                        region: r.id,
                        id: actor.id,
                    }
                    .reference(
                        failure,
                        self.region_text(r.id).ok().as_deref(),
                        "archetype",
                        key,
                    )
                })?;
                assets.extend(archetype.asset.iter().cloned());
            }
        }
        for key in r
            .generate
            .iter()
            .flat_map(|g| g.actors.iter().flat_map(|p| &p.archetypes))
        {
            assets.extend(self.author_archetype(key)?.asset.iter().cloned());
        }
        for item in &r.items {
            if let Some(key) = &item.archetype {
                let archetype = self.author_archetype(key).map_err(|failure| {
                    Origin::Item {
                        file,
                        region: r.id,
                        id: item.id,
                    }
                    .reference(
                        failure,
                        self.region_text(r.id).ok().as_deref(),
                        "archetype",
                        key,
                    )
                })?;
                assets.extend(compiler::item_assets(archetype, &self.manifest).map(str::to_owned));
            }
        }
        for key in r
            .generate
            .iter()
            .flat_map(|g| g.items.iter().flat_map(|p| &p.archetypes))
        {
            assets.extend(
                compiler::item_assets(self.author_archetype(key)?, &self.manifest)
                    .map(str::to_owned),
            );
        }
        Ok(assets)
    }

    pub(crate) fn check_identity(&self) -> Result<(), Failure> {
        self.check_identity_with_origin(Some(Origin::Manifest))
    }

    fn check_identity_with_origin(&self, origin: Option<Origin<'_>>) -> Result<(), Failure> {
        crate::storage::schema::scenario::check_metadata_capacity(self)?;
        check_manifest_header(&self.manifest)?;
        require(
            self.certificate.files.get("index.json") == Some(&digest(&self.index.to_bytes()?))
                && self.certificate.content_hash
                    == digest(
                        &serde_json::to_vec(&(
                            &self.certificate.files,
                            &self.certificate.ruleset_dependencies,
                        ))
                        .map_err(|e| fail(e.to_string()))?,
                    )
                && self.certificate.ruleset_dependencies.len() <= 1
                && self
                    .certificate
                    .ruleset_dependencies
                    .iter()
                    .all(|(id, hash)| {
                        id == RULESET
                            && hash.len() == 64
                            && hash.bytes().all(|b| b.is_ascii_hexdigit())
                    }),
            "Pinned source provenance or index integrity mismatch",
        )?;
        require(
            self.certificate.ruleset == RULESET
                && self.rules()?.id().as_str() == self.certificate.ruleset
                && self.certificate.ruleset_hash == digest(self.rules_document.as_bytes())
                && self.certificate.validator == VALIDATOR,
            "Missing exact package/validator dependency",
        )?;
        require(
            self.certificate.model_hash
                == model_hash(
                    &self.manifest,
                    &self.index,
                    &self.certificate.ruleset_hash,
                    &self.certificate.ruleset_dependencies,
                )?,
            "Pinned scenario content hash mismatch",
        )?;
        require(
            self.manifest
                .characters
                .iter()
                .any(|c| c.id == self.selected),
            "Unknown selected character ID",
        )
        .map_err(|failure| match origin {
            Some(origin) => origin.reference_path(
                failure,
                self.diagnostic_source("scenario.toml").as_deref(),
                &[PathSegment::Field("default_character")],
                ReferenceValue::Id(self.selected),
            ),
            None => failure,
        })?;
        self.check_item_identities()?;
        self.check_population()?;
        self.check_unique_retrieval()
    }
    fn supported(&self) -> Result<(), Failure> {
        if let Some(arena) = &self.manifest.arena {
            let participants: BTreeSet<_> = arena.participants.iter().copied().collect();
            require(
                arena.limits().valid()
                    && !participants.is_empty()
                    && participants.len() <= 256
                    && participants.len() == arena.participants.len()
                    && participants.contains(&self.selected)
                    && self.manifest.objective.is_none(),
                "Invalid arena limits, participants or objective",
            )?;
        }
        for (faction, enemies) in &self.manifest.factions {
            let origin = Origin::Faction(faction);
            require(label(faction), "Invalid faction relationship")
                .map_err(|failure| origin.context(failure))?;
            for enemy in enemies {
                require(
                    self.manifest.factions.contains_key(enemy),
                    "Invalid faction relationship",
                )
                .map_err(|failure| {
                    origin.reference_path(
                        failure,
                        self.diagnostic_source("scenario.toml").as_deref(),
                        &[PathSegment::UniqueValue(ReferenceValue::Text(enemy))],
                        ReferenceValue::Text(enemy),
                    )
                })?;
            }
        }
        for (name, profile) in &self.manifest.ai_profiles {
            require(label(name) && profile.valid(), "Invalid AI profile").map_err(|failure| {
                in_declaration(
                    failure,
                    format!(
                        "{}: AI profile {name:?}",
                        source::declaration_file(&self.source_provenance, "ai_profiles", name)
                    ),
                )
            })?;
        }
        for character in &self.manifest.characters {
            require(
                character.anatomy.as_ref().is_none_or(|anatomy| {
                    anatomy.slots.len() <= tor_simulation::limits::EQUIPMENT_SOCKETS
                }),
                "Invalid character anatomy",
            )?;
        }
        Ok(())
    }
    fn anchors(&self, seed: u64) -> Result<BTreeMap<String, Location>, Failure> {
        let mut anchors = BTreeMap::new();
        for r in &self.index.regions {
            for (name, p) in &r.anchors {
                require(label(name) && !name.contains('/'), "Invalid anchor ID")?;
                require(
                    anchors
                        .insert(format!("{}/{}", r.id, name), loc(r.id, *p))
                        .is_none(),
                    "Duplicate anchor",
                )?;
            }
        }
        for (id, family) in &self.manifest.procedural_worlds {
            let endpoint = family.resolve_port(id, seed, "1", "start")?;
            let (owner, _) = endpoint.split_once('/').expect("resolved entry endpoint");
            let owner = owner.parse().expect("resolved entry owner");
            let def = family
                .describe(id, seed, owner)?
                .ok_or_else(|| fail("Missing procedural start"))?;
            anchors.insert(endpoint, loc(owner, def.anchors["start"]));
        }
        Ok(anchors)
    }
    pub fn check(&self) -> Result<(), Failure> {
        self.check_identity()?;
        self.check_generation_groups()?;
        self.check_procedural_worlds()?;
        require(
            self.index.regions.len() <= MAX_REGIONS
                && (!self.index.regions.is_empty() || !self.manifest.procedural_worlds.is_empty()),
            format!("Expected 1..{MAX_REGIONS} authored regions"),
        )?;
        require(
            !self.manifest.characters.is_empty() && self.manifest.characters.len() <= 64,
            "Expected 1..64 starting characters",
        )?;
        require(
            self.manifest.themes.iter().all(|s| label(s))
                && self.manifest.zones.iter().all(|(id, z)| {
                    label(id) && z.themes.as_ref().is_none_or(|p| p.iter().all(|s| label(s)))
                }),
            "Invalid zone/theme identifier",
        )?;
        self.appearance_mapping(0)?;
        let known_themes: BTreeSet<&String> = self
            .manifest
            .themes
            .iter()
            .chain(
                self.manifest
                    .zones
                    .values()
                    .flat_map(|z| z.themes.iter().flatten()),
            )
            .collect();
        let terrain = self
            .manifest
            .zones
            .values()
            .filter_map(|z| z.terrain.as_ref())
            .chain(self.manifest.terrain.as_ref())
            .flat_map(|t| [&t.floor, &t.wall, &t.door].into_iter().flatten());
        let named = self
            .manifest
            .assets
            .values()
            .flatten()
            .chain(terrain)
            .chain(
                self.manifest
                    .archetypes
                    .values()
                    .filter_map(|a| a.asset.as_ref()),
            )
            .chain(
                self.manifest
                    .appearance_pools
                    .values()
                    .filter_map(|p| p.asset.as_ref()),
            )
            .chain(
                self.manifest
                    .characters
                    .iter()
                    .filter_map(|c| c.asset.as_ref()),
            );
        for asset in named {
            require(asset_id(asset), format!("Invalid asset identifier {asset}"))?;
        }
        for theme in self.manifest.assets.keys() {
            require(
                known_themes.contains(theme),
                format!("Assets for unknown theme {theme}"),
            )
            .map_err(|failure| {
                Origin::Manifest.reference_path(
                    failure,
                    self.diagnostic_source("scenario.toml").as_deref(),
                    &[PathSegment::Field("assets"), PathSegment::Key(theme)],
                    ReferenceValue::Text(theme),
                )
            })?;
        }
        // Full palettes and disjoint added/removed deltas cannot exceed this
        // all-theme union. Reserve room for the ordinary reply envelope rather
        // than allowing a valid package to disconnect clients on attachment.
        let all_assets = self.palette(self.manifest.assets.keys());
        check_palette_budget(&all_assets, tor_protocol::MAX_RESPONSE_BYTES)?;
        let identities: BTreeSet<_> = self
            .manifest
            .archetypes
            .iter()
            .map(|(id, a)| a.identity.as_ref().unwrap_or(id))
            .collect();
        for character in &self.manifest.characters {
            for (index, identity) in character.known_identities.iter().enumerate() {
                require(
                    identities.contains(identity),
                    "Unknown initial item identity",
                )
                .map_err(|failure| {
                    Origin::Character(character.id).reference_path(
                        failure,
                        self.diagnostic_source("scenario.toml").as_deref(),
                        &[
                            PathSegment::Field("known_identities"),
                            PathSegment::Index(index),
                        ],
                        ReferenceValue::Text(identity),
                    )
                })?;
            }
        }
        let mut actor_ids = BTreeSet::new();
        for c in &self.manifest.characters {
            require(
                c.id > 0 && c.id < u64::MAX && actor_ids.insert(c.id) && c.turn_ticks > 0,
                "Duplicate/invalid character ID or duration",
            )?;
            require(
                matches!(c.unselected.as_str(), "omit" | "ai")
                    && (c.unselected == "ai" || c.ai.is_none())
                    && c.ai.as_ref().is_none_or(|s| label(s)),
                "Invalid unselected character controller",
            )?;
        }
        let mut region_ids = BTreeSet::new();
        let mut item_ids = BTreeSet::new();
        let mut door_ids = BTreeSet::new();
        let mut trap_ids = BTreeSet::new();
        let mut starts = BTreeMap::new();
        for r in &self.index.regions {
            require(
                r.id > 0
                    && region_ids.insert(r.id)
                    && label(&r.name)
                    && (1..=32).contains(&r.size[0])
                    && (1..=32).contains(&r.size[1])
                    && (1..=8).contains(&r.size[2]),
                format!("Region {}: invalid ID/name/bounds", r.id),
            )?;
            for (index, target) in r.portals.iter().enumerate() {
                let contextualize = |failure| {
                    self.region_reference(
                        r.id,
                        Origin::Region {
                            file: &r.file,
                            id: r.id,
                        },
                        failure,
                        &[
                            PathSegment::Field("portals"),
                            PathSegment::Index(index),
                            PathSegment::Field("to"),
                        ],
                        ReferenceValue::Text(target),
                    )
                };
                let resolved = self.resolve_endpoint(0, target).map_err(contextualize)?;
                let destination = resolved.split_once('/').and_then(|(id, _)| id.parse().ok());
                require(
                    destination.is_none_or(|id| self.procedural_family(id).is_none()),
                    "Portal declarations cannot target procedural families; use stair_pairs",
                )
                .map_err(contextualize)?;
            }
            if let Some(zone) = &r.zone {
                require(
                    self.manifest.zones.contains_key(zone),
                    "Unknown zone reference",
                )
                .map_err(|failure| {
                    self.region_reference(
                        r.id,
                        Origin::Region {
                            file: &r.file,
                            id: r.id,
                        },
                        failure,
                        &[PathSegment::Field("zone")],
                        ReferenceValue::Text(zone),
                    )
                })?;
            }
            for a in &r.actors {
                require(
                    a.id > 0 && a.id < u64::MAX && actor_ids.insert(a.id),
                    "Duplicate/invalid actor ID",
                )?;
                starts.insert(a.id, r.id);
            }
            for i in &r.items {
                require(
                    i.id > 0 && i.id < u64::MAX && item_ids.insert(i.id),
                    "Duplicate/invalid item ID",
                )?;
            }
            for d in &r.doors {
                require(
                    *d > 0 && *d < u64::MAX && door_ids.insert(*d),
                    "Duplicate/invalid door ID",
                )?;
            }
            for t in &r.traps {
                require(
                    *t > 0 && *t < u64::MAX && trap_ids.insert(*t),
                    "Duplicate/invalid trap ID",
                )?;
            }
        }
        for (id, a) in &self.manifest.archetypes {
            require(
                label(id)
                    && a.name.as_ref().is_none_or(|s| label(s))
                    && a.turn_ticks != Some(0)
                    && a.properties.len() <= 32
                    && a.properties.iter().all(|(k, v)| {
                        label(k) && v.len() <= 80 && !v.chars().any(char::is_control)
                    }),
                "Invalid archetype",
            )?;
        }
        let anchors = self.anchors(0)?;
        for c in &self.manifest.characters {
            // Validate the root structurally before any generated coordinates
            // are needed, so compiler work budgets precede carving.
            let (_, region) = self.character_endpoint(c.id, 0, &c.anchor)?;
            starts.insert(c.id, region);
        }
        // A carried item is authored in the region its carrier starts in,
        // so building a region reads only that region's file.
        for r in &self.index.regions {
            for i in &r.items {
                if let Some(owner) = i.carried_by {
                    let start = starts.get(&owner).ok_or_else(|| {
                        self.region_reference(
                            r.id,
                            Origin::Item {
                                file: &r.file,
                                region: r.id,
                                id: i.id,
                            },
                            fail("Unknown inventory owner"),
                            &[PathSegment::Field("carried_by")],
                            ReferenceValue::Id(owner),
                        )
                    })?;
                    require(
                        *start == r.id,
                        format!(
                            "Item {}: author it in region {}, where its carrier starts",
                            i.id, start
                        ),
                    )?;
                }
            }
        }
        if let Some(objective) = &self.manifest.objective {
            require(
                anchors.contains_key(&objective.anchor),
                "Invalid objective anchor reference",
            )
            .map_err(|failure| {
                Origin::Objective.reference_path(
                    failure,
                    self.diagnostic_source("scenario.toml").as_deref(),
                    &[
                        PathSegment::Field("objective"),
                        PathSegment::Field("anchor"),
                    ],
                    ReferenceValue::Text(&objective.anchor),
                )
            })?;
            if let Some(item) = objective.item {
                require(item_ids.contains(&item), "Invalid objective item reference").map_err(
                    |failure| {
                        Origin::Objective.reference_path(
                            failure,
                            self.diagnostic_source("scenario.toml").as_deref(),
                            &[PathSegment::Field("objective"), PathSegment::Field("item")],
                            ReferenceValue::Id(item),
                        )
                    },
                )?;
            }
        }
        self.validate_spatial_sources()?;
        Ok(())
    }
    /// Acquire diagnostic text only after a reference check fails. Unavailable
    /// source must never replace the semantic failure or invent coordinates.
    fn region_reference(
        &self,
        region: u64,
        origin: Origin<'_>,
        failure: Failure,
        path: &[PathSegment<'_>],
        expected: ReferenceValue<'_>,
    ) -> Failure {
        let source = self.region_text(region).ok();
        origin.reference_path(failure, source.as_deref(), path, expected)
    }

    /// What only a region's own file can show: its actors' controllers and
    /// combat. The validator checks every region; building one checks it.
    fn check_trap_ammunition(&self, key: &str) -> Result<(), Failure> {
        require(
            self.manifest.archetypes.get(key).is_some_and(|item| {
                item.stackable
                    && item.projectile.is_some()
                    && !item.magical
                    && item.appearance_pool.is_none()
                    && item.equipment_generation.is_none()
            }),
            "Arrow trap ammunition must name an ordinary stackable projectile archetype",
        )
    }
    fn check_trapdoor_destination(
        &self,
        source: &RegionDef,
        destination: &tor_world::NamedAnchor,
    ) -> Result<(), Failure> {
        require(
            destination.region.0 != source.id,
            "Trapdoor destination must be a different physical region",
        )?;
        if let Some((_, family)) = self.procedural_family(destination.region.0) {
            let ordinal = destination
                .name
                .strip_prefix("trapdoor_landing_")
                .and_then(|value| value.parse::<u32>().ok());
            let capacity = self
                .manifest
                .generation_recipes
                .get(&family.recipe)
                .and_then(|recipe| {
                    recipe
                        .stages
                        .iter()
                        .find_map(|stage| match &stage.operation {
                            crate::generation_recipe::Operation::HiddenTraps { config }
                                if config.hazards.is_some() =>
                            {
                                Some(config.count[1])
                            }
                            _ => None,
                        })
                });
            return require(
                ordinal
                    .zip(capacity)
                    .is_some_and(|(ordinal, capacity)| ordinal < capacity)
                    && family.size[2] >= 2
                    && family
                        .address(destination.region.0)
                        .is_some_and(|(depth, _)| depth <= family.max_depth()),
                "Unknown generated trapdoor ceiling endpoint",
            );
        }
        let target = self.region_def(destination.region.0)?;
        let position = target
            .anchors
            .get(&destination.name)
            .ok_or_else(|| fail("Unknown trapdoor destination anchor"))?;
        require(
            target.chamber
                && target.size[2] >= 2
                && position[2] == target.size[2] - 1
                && (0..target.size[0]).contains(&position[0])
                && (0..target.size[1]).contains(&position[1]),
            "Trapdoor destination must be a supported chamber ceiling endpoint",
        )?;
        require((0..target.size[2]).all(|z| !target.walls.contains(&[position[0], position[1], z])) && !target.openings.contains(&[position[0], position[1], -1]), "Trapdoor destination column must have clear upright-body clearance and real floor support")
    }
    fn check_region(&self, r: &RegionDef) -> Result<(), Failure> {
        self.check_retrieval_region(r)?;
        for door in &r.doors {
            require(
                door.concealment.is_none_or(Concealment::valid),
                "Invalid secret entrance DC",
            )?;
        }
        if !r.traps.is_empty() {
            let rules = self.rules()?;
            for trap in &r.traps {
                let value = tor_world::Trap {
                    id: trap.id,
                    dc: trap.dc,
                    avoidance_dc: trap.avoidance_dc,
                    kind: trap.kind.compiled(),
                };
                require(value.valid(), "Invalid trap definition")?;
                check_trap_effect(&rules, &trap.kind)?;
                match &trap.kind {
                    TrapKind::Arrow { ammunition, .. } => self.check_trap_ammunition(ammunition)?,
                    TrapKind::TrapDoor { destination, open } => {
                        require(
                            !open && trap.at[2] == 0 && r.chamber,
                            "Authored trapdoors start closed on a chamber floor",
                        )?;
                        self.check_trapdoor_destination(r, destination)?;
                    }
                    _ => {}
                }
            }
        }
        if let Some(generate) = &r.generate {
            let pools = generate
                .actors
                .iter()
                .map(|pool| ("actors", pool.archetypes.as_slice()))
                .chain(
                    generate
                        .items
                        .iter()
                        .map(|pool| ("items", pool.archetypes.as_slice())),
                );
            let file = self
                .index
                .region(r.id)
                .map(|entry| entry.file.as_str())
                .unwrap_or("generated region");
            let origin = Origin::Region { file, id: r.id };
            for (kind, archetypes) in pools {
                for (index, archetype) in archetypes.iter().enumerate() {
                    require(
                        self.manifest.archetypes.contains_key(archetype),
                        format!("Region {}: unknown archetype {archetype}", r.id),
                    )
                    .map_err(|failure| {
                        self.region_reference(
                            r.id,
                            origin,
                            failure,
                            &[
                                PathSegment::Field("generate"),
                                PathSegment::Field(kind),
                                PathSegment::Field("archetypes"),
                                PathSegment::Index(index),
                            ],
                            ReferenceValue::Text(archetype),
                        )
                    })?;
                }
            }
            if let Some(pool) = &generate.actors {
                require(
                    self.manifest.ai_profiles.contains_key(&pool.ai),
                    format!("Region {}: unknown AI profile {}", r.id, pool.ai),
                )
                .map_err(|failure| {
                    self.region_reference(
                        r.id,
                        origin,
                        failure,
                        &[
                            PathSegment::Field("generate"),
                            PathSegment::Field("actors"),
                            PathSegment::Field("ai"),
                        ],
                        ReferenceValue::Text(&pool.ai),
                    )
                })?;
            }
        }
        let actor_file = self
            .index
            .region(r.id)
            .map(|entry| entry.file.as_str())
            .unwrap_or("generated region");
        for a in &r.actors {
            let origin = Origin::Actor {
                file: actor_file,
                region: r.id,
                id: a.id,
            };
            let contextualize = |failure| origin.context(failure);
            require(
                a.anatomy.as_ref().is_none_or(|anatomy| {
                    anatomy.slots.len() <= tor_simulation::limits::EQUIPMENT_SOCKETS
                }),
                "Invalid actor anatomy",
            )
            .map_err(contextualize)?;
            for identity in &a.known_identities {
                require(
                    self.manifest.archetypes.iter().any(|(key, archetype)| {
                        archetype.identity.as_ref().unwrap_or(key) == identity
                    }),
                    "Unknown initial item identity",
                )
                .map_err(contextualize)?;
            }
            require(
                matches!(a.controller.as_str(), "external" | "ai")
                    && (a.controller == "ai" || a.ai.is_none())
                    && a.ai.as_ref().is_none_or(|s| label(s)),
                "Invalid actor controller",
            )
            .map_err(contextualize)?;
        }
        Ok(())
    }
    /// Type knowledge is shared by identity. Revalidate the same invariant for
    /// source-free metadata, not only when preparing authored appearances.
    fn check_item_identities(&self) -> Result<(), Failure> {
        let mut signatures = BTreeMap::new();
        for (key, item) in &self.manifest.archetypes {
            item_forms::validate(item)
                .map_err(|failure| self.archetype_origin(key).context(failure))?;
            let identity = item.identity.as_ref().unwrap_or(key);
            require(label(identity), "Invalid item identity")
                .map_err(|failure| self.archetype_origin(key).context(failure))?;
            let signature = (
                &item.name,
                &item.appearance_pool,
                item.class,
                &item.equipment,
                &item.projectile,
                &item.consumable,
                &item.readable,
                &item.device,
                &item.held_light,
                &item.edible,
                &item.supply,
                &item.forms,
            );
            if let Some(previous) = signatures.insert(identity, signature) {
                require(previous == signature,
                    "One identity must have one name, appearance pool, physical class, item effects, projectile profile, supply and forms")
                    .map_err(|failure| self.archetype_origin(key).context(failure))?;
            }
        }
        Ok(())
    }
    fn appearance_mapping(&self, seed: u64) -> Result<BTreeMap<String, String>, Failure> {
        self.check_item_identities()?;
        let rules = self.rules()?;
        let mut result = BTreeMap::new();
        // Validate every alternative, even when this seed does not select it.
        let alternatives = item_forms::alternatives(&self.manifest);
        for (key, a) in &alternatives {
            (|| -> Result<(), Failure> {
                require(
                    a.anatomy.as_ref().is_none_or(|anatomy| {
                        anatomy.slots.len() <= tor_simulation::limits::EQUIPMENT_SOCKETS
                    }),
                    "Invalid anatomy",
                )?;
                let projectile = a
                    .projectile
                    .as_ref()
                    .map(|value| value.compile(&rules))
                    .transpose()?;
                if projectile.is_some() {
                    require(
                        a.class == ItemClass::Weapon
                            && a.consumable.is_none()
                            && a.readable.is_none()
                            && a.device.is_none()
                            && a.edible.is_none()
                            && a.currency.is_none(),
                        "Invalid projectile capability combination",
                    )?;
                }
                if let Some(policy) = a.equipment_generation {
                    let equipment = a
                        .equipment
                        .as_ref()
                        .map(|gear| gear.compile(&rules))
                        .transpose()?;
                    require(
                        policy.compile()?.valid_for(
                            a.class.into(),
                            equipment.as_ref(),
                            projectile.as_ref(),
                        ),
                        "Invalid equipment generation definition",
                    )?;
                }
                if let Some(policy) = a.held_light {
                    policy.compile()?;
                    require(
                        a.class == ItemClass::Tool
                            && !a.stackable
                            && a.equipment.as_ref().is_some_and(|equipment| {
                                equipment.slot == "held_light" && equipment.attack.is_none()
                            })
                            && a.device.is_none()
                            && a.consumable.is_none()
                            && a.readable.is_none()
                            && a.edible.is_none()
                            && a.projectile.is_none()
                            && a.supply.is_none(),
                        "Invalid held light capability combination",
                    )?;
                }
                if let Some(equipment) = &a.equipment {
                    let spec = equipment.compile(&rules)?;
                    require(
                        (!a.stackable
                            || (a.projectile.is_none()
                                && a.ground_scare.is_none()
                                && a.currency.is_none()
                                && spec.supports_stacking(a.class.into())))
                            && a.consumable.is_none()
                            && a.readable.is_none()
                            && (a.device.is_none() || spec.supports_device(a.class.into()))
                            && a.edible.is_none()
                            && a.supply.is_none()
                            && spec.validate_rules(a.class.into(), &rules).is_ok(),
                        "Invalid equipment definition",
                    )?;
                }
                if let Some(consumable) = &a.consumable {
                    let spec = consumable.compile(&rules)?;
                    require(
                        a.class == ItemClass::Potion
                            && a.edible.is_none()
                            && a.readable.is_none()
                            && a.device.is_none()
                            && spec.valid(),
                        "Invalid consumable definition",
                    )?;
                }
                if let Some(readable) = &a.readable {
                    readable.compile(&rules)?;
                    require(
                        a.equipment.is_none()
                            && a.consumable.is_none()
                            && a.edible.is_none()
                            && a.device.is_none()
                            && a.supply.is_none()
                            && a.currency.is_none(),
                        "Invalid readable definition",
                    )?;
                }
                if let Some(device) = &a.device {
                    device.compile(&rules)?;
                    let physical_device = a
                        .equipment
                        .as_ref()
                        .map(|equipment| equipment.compile(&rules))
                        .transpose()?
                        .is_none_or(|equipment| equipment.supports_device(a.class.into()));
                    require(
                        !a.stackable
                            && physical_device
                            && a.projectile.is_none()
                            && a.held_light.is_none()
                            && a.ground_scare.is_none()
                            && a.consumable.is_none()
                            && a.edible.is_none()
                            && a.readable.is_none()
                            && a.supply.is_none()
                            && a.currency.is_none(),
                        "Invalid device definition",
                    )?;
                }
                if let Some(edible) = &a.edible {
                    require(
                        edible.valid()
                            && a.equipment.is_none()
                            && a.consumable.is_none()
                            && a.readable.is_none()
                            && a.device.is_none(),
                        "Invalid edible definition",
                    )?;
                }
                Ok(())
            })()
            .map_err(|failure| self.archetype_origin(key).context(failure))?;
            if let Some(pool) = &a.appearance_pool {
                require(
                    a.name.is_some(),
                    "Missing identity name for appearance pool",
                )
                .map_err(|failure| self.archetype_origin(key).context(failure))?;
                require(
                    self.manifest.appearance_pools.contains_key(pool),
                    format!("Unknown appearance pool {pool:?}"),
                )
                .map_err(|failure| {
                    self.archetype_origin(key).reference(
                        failure,
                        self.diagnostic_source(source::declaration_file(
                            &self.source_provenance,
                            "archetypes",
                            key,
                        ))
                        .as_deref(),
                        "appearance_pool",
                        pool,
                    )
                })?;
            }
        }
        for (key, pool) in &self.manifest.appearance_pools {
            let classes: BTreeSet<_> = alternatives
                .iter()
                .map(|(_, a)| a)
                .filter(|a| a.appearance_pool.as_ref() == Some(key))
                .map(|a| {
                    (
                        a.class,
                        a.equipment.as_ref().map(|e| e.slot.clone()),
                        a.consumable.is_some(),
                        a.readable.is_some(),
                        a.device.is_some(),
                        a.edible.is_some(),
                    )
                })
                .collect();
            require(
                classes.len() <= 1,
                "Appearance pool must have one physical class and interaction affordances",
            )?;
            require(
                label(key)
                    && !pool.appearances.is_empty()
                    && pool.appearances.len() <= 4096
                    && pool.appearances.iter().all(|s| label(s)),
                "Invalid appearance pool",
            )?;
            let unique: BTreeSet<_> = pool.appearances.iter().collect();
            require(
                pool.confounding || unique.len() == pool.appearances.len(),
                "Duplicate appearances require confounding",
            )?;
            let possible_identities: BTreeSet<_> = alternatives
                .iter()
                .filter(|(_, a)| a.appearance_pool.as_ref() == Some(key))
                .map(|(id, a)| a.identity.as_deref().unwrap_or(id))
                .collect();
            require(
                pool.confounding || possible_identities.len() <= pool.appearances.len(),
                "Appearance pool too small",
            )?;
            let identities: BTreeSet<_> = self
                .manifest
                .archetypes
                .iter()
                .filter(|(id, a)| item_forms::selected_pool(a, id, seed) == Some(key.as_str()))
                .map(|(id, a)| a.identity.as_ref().unwrap_or(id))
                .collect();
            let mut appearances: Vec<_> = pool.appearances.iter().enumerate().collect();
            appearances.sort_by_key(|(index, _)| {
                let mut hash = Sha256::new();
                hash.update(seed.to_le_bytes());
                hash.update(key.as_bytes());
                hash.update((*index as u64).to_le_bytes());
                <[u8; 32]>::from(hash.finalize())
            });
            for (index, identity) in identities.into_iter().enumerate() {
                result.insert(
                    identity.clone(),
                    appearances[index % appearances.len()].1.clone(),
                );
            }
        }
        Ok(result)
    }
    fn author_archetype(&self, key: &str) -> Result<&Archetype, Failure> {
        self.manifest
            .archetypes
            .get(key)
            .ok_or_else(|| fail(format!("Unknown archetype {key}")))
    }
    pub(crate) fn build(&self, seed: u64, runtime: bool) -> Result<Game, Failure> {
        require(
            !runtime || self.manifest.procedural_worlds.is_empty(),
            "Procedural worlds require bounded region streaming",
        )?;
        self.check()?;
        self.supported()?;
        let index = self.index(seed)?;
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
            index.definitions.creature_catalog().content().clone(),
        );
        game.defer_initial_awareness()
            .map_err(|error| fail(format!("Initial awareness construction: {error:?}")))?;
        // Full authoring validation materializes each floor once. Drain this
        // temporary batch into definitions; it is never a terrain baseline.
        let mut generated = BTreeMap::new();
        let mut generated_rooms = BTreeMap::new();
        let mut generated_spatial = tor_simulation::spatial_scope::registry::Registry::default();
        let mut defs = Vec::new();
        for entry in &self.index.regions {
            if let Some(owner) = index.group_owners.get(&entry.id) {
                if !generated.contains_key(&entry.id) {
                    let identity = &index.groups[owner];
                    let group = self.generated_group(identity, seed)?;
                    generated_spatial
                        .register_archive_checked(
                            self.canonical_generated_spatial(identity, seed, &group)?,
                        )
                        .map_err(|e| fail(format!("Generated spatial capacity: {e:?}")))?;
                    generated_rooms.insert(identity.clone(), group.rooms);
                    generated.extend(group.regions);
                }
                defs.push(Arc::new(
                    generated.remove(&entry.id).expect("validated group member"),
                ));
            } else {
                defs.push(index.region(self, entry.id)?);
            }
        }
        // Include the actual spawned roots, not every preceding floor. Families
        // without a spawned root retain one representative validation group.
        for identity in self.initial_procedural_groups(index.homes.values().map(|at| at.region.0)) {
            let group = self.generated_group(&identity, seed)?;
            generated_spatial
                .register_archive_checked(
                    self.canonical_generated_spatial(&identity, seed, &group)?,
                )
                .map_err(|e| fail(format!("Generated spatial capacity: {e:?}")))?;
            generated_rooms.insert(identity, group.rooms);
            defs.extend(group.regions.into_values().map(Arc::new));
        }
        for r in &defs {
            self.check_region(r)?;
        }
        let (actors, items, doors) = index.ceilings;
        game.reserve_identities(actors, items, doors);
        let all: Vec<&RegionDef> = defs.iter().map(|d| &**d).collect();
        let anchors = resolved_anchors(&all);
        self.add_geometry(&mut game, &all)?;
        let included: BTreeSet<_> = all.iter().map(|r| r.id).collect();
        let deferred: BTreeSet<_> = all
            .iter()
            .flat_map(|r| index.stair_exits(r.id))
            .map(|exit| exit.destination)
            .filter(|id| !included.contains(id))
            .collect();
        for destination in deferred {
            let declaration = self.unbuilt_region(&index, destination)?;
            game.add_unbuilt_region(
                declaration.region,
                declaration.chamber,
                declaration.identities,
            )
            .map_err(|error| fail(format!("Validation destination: {error:?}")))?;
        }
        for r in &all {
            self.add_structure(&mut game, r, &anchors)?;
            self.add_stairs(&mut game, r.id, &index, &anchors)?;
        }
        for (name, position) in &anchors {
            require(
                self.boundary_anchor(name) || game.authored_cell_valid(*position),
                format!("Anchor {name}: outside traversable geometry"),
            )?;
        }
        self.install_authored_spatial(&mut game)?;
        for scope in generated_spatial.scopes() {
            self.install_spatial_scope(&mut game, scope.clone())?;
        }
        self.add_entities(&mut game, seed, &index, &all)?;
        self.configure_population(&mut game, &index)?;
        let scope_owners: BTreeSet<_> = all
            .iter()
            .filter_map(|region| index.group_owner(region.id))
            .collect();
        for owner in scope_owners {
            let identity = index
                .group_identity(owner)
                .ok_or_else(|| fail("Missing activation group"))?;
            let group = self.generation_group(&identity, seed)?;
            let members: Vec<_> = all
                .iter()
                .copied()
                .filter(|region| group.members.contains(&region.id))
                .collect();
            let rooms = generated_rooms
                .get(&identity)
                .ok_or_else(|| fail("Missing generated room metadata"))?;
            if let Some(scope) = self.activation_scope(&identity, group.depth, &members, rooms)? {
                game.register_activation_scope(scope)
                    .map_err(|error| fail(format!("Initial activation scope: {error:?}")))?;
            }
        }
        self.initialize_selected_character_with_index(&mut game, &index)?;
        self.configure_run(&mut game, &index)?;
        game.initialize_discovery()
            .map_err(|error| fail(format!("Initial discovery publication: {error:?}")))?;
        Ok(game)
    }

    fn character_endpoint(
        &self,
        id: u64,
        seed: u64,
        anchor: &str,
    ) -> Result<(String, u64), Failure> {
        (|| {
            let resolved = self.resolve_endpoint(seed, anchor)?;
            require(
                !self.boundary_anchor(&resolved),
                "Character starts cannot use structural boundary anchors",
            )?;
            let region = self.endpoint_region_seed(seed, &resolved)?;
            require(
                self.procedural_family(region)
                    .is_none_or(|(identity, family)| family.materializable(identity, seed, region)),
                "Character start exceeds procedural domain or operational limit",
            )?;
            Ok((resolved, region))
        })()
        .map_err(|failure| {
            Origin::Character(id).reference(
                failure,
                self.diagnostic_source("scenario.toml").as_deref(),
                "anchor",
                anchor,
            )
        })
    }

    fn character_anchor(
        &self,
        id: u64,
        seed: u64,
        anchor: &str,
        anchors: &mut BTreeMap<String, Location>,
    ) -> Result<Location, Failure> {
        let (resolved, region) = self.character_endpoint(id, seed, anchor)?;
        if !anchors.contains_key(&resolved) {
            let definition = self.structural_definition(seed, region)?;
            if let Some(group) = definition
                .generate
                .as_ref()
                .and_then(|g| g.group.as_deref())
            {
                // Stair cells are chosen by the ordinary group generator. Cache
                // only this bounded group's anchors for other co-located roots.
                let definitions = self.group_definitions(group, seed)?;
                anchors.extend(resolved_anchors(&definitions.values().collect::<Vec<_>>()));
            }
        }
        anchors.get(&resolved).copied().ok_or_else(|| {
            Origin::Character(id).reference(
                fail(format!("Missing character anchor {anchor:?}")),
                self.diagnostic_source("scenario.toml").as_deref(),
                "anchor",
                anchor,
            )
        })
    }

    fn initial_procedural_groups(&self, roots: impl Iterator<Item = u64>) -> BTreeSet<String> {
        let mut groups = BTreeSet::new();
        let mut rooted_families = BTreeSet::new();
        for region in roots {
            if let Some((identity, family)) = self.procedural_family(region) {
                let (depth, _) = family.address(region).expect("resolved family address");
                groups.insert(format!("{identity}/{depth}"));
                rooted_families.insert(identity);
            }
        }
        for identity in self.manifest.procedural_worlds.keys() {
            if !rooted_families.contains(identity.as_str()) {
                groups.insert(format!("{identity}/1"));
            }
        }
        groups
    }

    /// Whole-package facts that building regions needs, computed once so
    /// building one region reads only that region and its neighbours.
    pub(crate) fn index(&self, seed: u64) -> Result<PackageIndex, Failure> {
        let mut definitions = PreparedDefinitions::new_with_sources(
            &self.manifest,
            self.selected,
            self.rules()?,
            &self.source_provenance,
            |file| self.diagnostic_source(file),
        )?;
        definitions.select_item_forms(&self.manifest, seed, &self.source_provenance)?;
        let definitions = Arc::new(definitions);
        let mut anchors = self.anchors(seed)?;
        let mut stairs: BTreeMap<u64, Vec<StairExit>> = BTreeMap::new();
        for (id, pair) in &self.manifest.stair_pairs {
            require(
                label(id) && !id.contains('/'),
                format!("Invalid stair pair ID {id:?}"),
            )?;
            let endpoint = |field, name: &str| {
                self.endpoint_region_seed(seed, name).map_err(|failure| {
                    let file = self
                        .source_provenance
                        .get(&format!("stair_pairs.{id}"))
                        .map(String::as_str)
                        .unwrap_or("scenario.toml");
                    Origin::StairPair { file, id }.reference_path(
                        failure,
                        self.diagnostic_source(file).as_deref(),
                        &[PathSegment::Field(field)],
                        ReferenceValue::Text(name),
                    )
                })
            };
            let upper = endpoint("upper", &pair.upper)?;
            let lower = endpoint("lower", &pair.lower)?;
            let upper_anchor = self.resolve_endpoint(seed, &pair.upper)?;
            let lower_anchor = self.resolve_endpoint(seed, &pair.lower)?;
            for (from, direction, to) in [
                (&upper_anchor, Direction::Down, &lower_anchor),
                (&lower_anchor, Direction::Up, &upper_anchor),
            ] {
                let region = if direction == Direction::Down {
                    upper
                } else {
                    lower
                };
                stairs.entry(region).or_default().push(StairExit {
                    id: id.clone(),
                    from: from.clone(),
                    direction,
                    to: to.clone(),
                    destination: if direction == Direction::Down {
                        lower
                    } else {
                        upper
                    },
                });
            }
        }
        for (region, exits) in &stairs {
            let mut directions = BTreeSet::new();
            for exit in exits {
                require(
                    directions.insert((exit.from.clone(), exit.direction as u8)),
                    "Duplicate stair exit direction",
                )?;
            }
            if let Some((identity, family)) = self.procedural_family(*region) {
                for exit in family.connections(identity, seed, *region) {
                    require(
                        directions.insert((exit.from, exit.direction as u8)),
                        "Authored stair conflicts with procedural chain exit",
                    )?;
                }
            }
        }
        let mut spawns = BTreeMap::new();
        for c in &definitions.characters {
            if c.creature.control.spawned() {
                let at = self.character_anchor(c.id, seed, &c.anchor, &mut anchors)?;
                spawns.insert(c.id, (at, c.turn_ticks));
            }
        }
        for r in &self.index.regions {
            for a in &r.actors {
                let ticks = a
                    .turn_ticks
                    .or(definitions
                        .archetype(&a.archetype)
                        .map_err(|reference| {
                            Origin::Actor {
                                file: &r.file,
                                region: r.id,
                                id: a.id,
                            }
                            .reference(
                                fail(format!("Unknown archetype {}", reference.name)),
                                self.region_text(r.id).ok().as_deref(),
                                "archetype",
                                reference.name,
                            )
                        })?
                        .turn_ticks)
                    .unwrap_or(100);
                require(
                    spawns.insert(a.id, (loc(r.id, a.at), ticks)).is_none(),
                    "Duplicate actor ID",
                )?;
            }
        }
        let homes: BTreeMap<u64, Location> =
            spawns.iter().map(|(id, (at, _))| (*id, *at)).collect();
        if let Some(arena) = &self.manifest.arena {
            require(
                arena.participants.iter().all(|id| homes.contains_key(id)),
                "Unknown or omitted arena participant",
            )?;
        }
        let mut spawns_by_region: BTreeMap<u64, Vec<(u64, Location, u64)>> = BTreeMap::new();
        for (id, (at, ticks)) in &spawns {
            spawns_by_region
                .entry(at.region.0)
                .or_default()
                .push((*id, *at, *ticks));
        }
        let ceiling = |ids: &mut dyn Iterator<Item = u64>| {
            ids.max()
                .map_or(Some(1), |max| max.checked_add(1))
                .ok_or_else(|| fail("Authored identity too large"))
        };
        let regions = &self.index.regions;
        // Reserve every declared character, including omitted alternate presets.
        // Dynamic population must never reuse an authored identity.
        let ceilings = (
            ceiling(
                &mut homes
                    .keys()
                    .copied()
                    .chain(self.manifest.characters.iter().map(|c| c.id)),
            )?,
            ceiling(
                &mut regions
                    .iter()
                    .flat_map(|r| r.items.iter())
                    .filter(|i| !definitions.omitted_carrier(i.carried_by))
                    .map(|i| i.id),
            )?,
            ceiling(&mut regions.iter().flat_map(|r| r.doors.iter().copied()))?,
        );
        // Generated regions take identities above every authored one (even
        // omitted characters', so the ranges don't depend on the selection),
        // each region in its own range; the ceilings cover them all.
        let generated_base = (
            ceiling(
                &mut self
                    .manifest
                    .characters
                    .iter()
                    .map(|c| c.id)
                    .chain(regions.iter().flat_map(|r| r.actors.iter().map(|a| a.id))),
            )?,
            ceiling(&mut regions.iter().flat_map(|r| r.items.iter().map(|i| i.id)))?,
        );
        // Reserved up to a fixed end, so the ceilings (which every game
        // state records) don't grow with the package.
        let mut ceilings = ceilings;
        let feature_bases = self.feature_bases()?;
        ceilings.2 = ceilings
            .2
            .max(self.generated_feature_ceiling(feature_bases.doors)?);
        self.generated_feature_ceiling(feature_bases.traps)?;
        if let Some(last) = regions.iter().filter(|r| r.generated).map(|r| r.id).max() {
            require(
                last <= MAX_REGIONS as u64,
                format!("Generated region ids must be at most {MAX_REGIONS}"),
            )?;
            let end = |base: u64| {
                (MAX_REGIONS as u64)
                    .checked_mul(crate::generator::IDENTITY_STRIDE)
                    .and_then(|n| n.checked_add(base))
                    .ok_or_else(|| fail("Authored identity too large"))
            };
            ceilings.0 = ceilings.0.max(end(generated_base.0)?);
            ceilings.1 = ceilings.1.max(end(generated_base.1)?);
        }
        Ok(PackageIndex {
            procedural_worlds: self.manifest.procedural_worlds.clone(),
            group_owners: self
                .manifest
                .generation_groups
                .values()
                .flat_map(|group| {
                    group
                        .members
                        .iter()
                        .map(|region| (*region, group.members[0]))
                })
                .collect(),
            groups: self
                .manifest
                .generation_groups
                .iter()
                .map(|(id, group)| (group.members[0], id.clone()))
                .collect(),
            definitions,
            anchors,
            stairs,
            homes,
            spawns: spawns_by_region,
            ceilings,
            seed,
            generated_base,
            appearances: self.appearance_mapping(seed)?,
            lookups: std::cell::Cell::new(0),
        })
    }

    /// A game with every region known but none built: each is built from
    /// this package when it's first loaded (see [`Package::build_region`]).
    /// Building every region gives exactly [`Package::build`]'s game.
    pub(crate) fn start(&self, seed: u64) -> Result<Game, Failure> {
        self.check()?;
        self.supported()?;
        let index = self.index(seed)?;
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
            index.definitions.creature_catalog().content().clone(),
        );
        game.defer_initial_awareness()
            .map_err(|error| fail(format!("Initial awareness construction: {error:?}")))?;
        let (actors, items, doors) = index.ceilings;
        game.reserve_identities(actors, items, doors);
        // Only regions the run refers to are declared now: where its
        // characters and client-controlled actors start, and its objective.
        // Transitions declare the rest as they're needed.
        let mut needed = BTreeSet::new();
        for c in &self.manifest.characters {
            if let Some(home) = index.homes.get(&c.id) {
                needed.insert(home.region.0);
            }
        }
        if let Some(arena) = &self.manifest.arena {
            for actor in &arena.participants {
                needed.insert(index.homes[actor].region.0);
            }
        }
        for r in &self.index.regions {
            if r.actors.iter().any(|a| a.external) {
                needed.insert(r.id);
            }
        }
        if let Some(o) = &self.manifest.objective {
            require(
                !self.boundary_anchor(&o.anchor),
                "Objectives cannot use structural boundary anchors",
            )?;
            let at = index
                .anchors
                .get(&o.anchor)
                .ok_or_else(|| fail("Missing objective anchor"))?;
            needed.insert(at.region.0);
            if let Some(item) = o.item {
                let holder = self
                    .index
                    .regions
                    .iter()
                    .find(|r| r.items.iter().any(|i| i.id == item))
                    .ok_or_else(|| fail("Missing objective item"))?;
                needed.insert(holder.id);
            }
        }
        for region in needed {
            let unbuilt = self.unbuilt_region(&index, region)?;
            game.add_unbuilt_region(unbuilt.region, unbuilt.chamber, unbuilt.identities)
                .map_err(|e| fail(format!("Region {region}: {e:?}")))?;
        }
        self.configure_population(&mut game, &index)?;
        // Arena setup validates living, loaded participants after the engine's
        // ordinary transition materializes their pinned regions.
        if self.manifest.arena.is_none() {
            self.configure_run(&mut game, &index)?;
        }
        Ok(game)
    }

    /// A region as a game declares it before building it: its metadata and
    /// the identities it will hold. Reads only the index.
    pub(crate) fn unbuilt_region(
        &self,
        index: &PackageIndex,
        region: u64,
    ) -> Result<tor_simulation::UnbuiltRegion, Failure> {
        let r = self.structural_metadata(index.seed, region)?;
        let generated = if r.generated && index.group_owner(region).is_none() {
            Some(index.region(self, region)?)
        } else {
            None
        };
        // Character starts are indexed independently of the region's source.
        // Add generated inhabitants to those declarations instead of replacing
        // them, so lazy startup knows every actor before configuring the run.
        Ok(tor_simulation::UnbuiltRegion {
            region: region_of(r.id, &r.name, r.size)?,
            chamber: r.chamber,
            identities: tor_simulation::RegionIdentities {
                actors: index
                    .spawns
                    .get(&region)
                    .into_iter()
                    .flatten()
                    .map(|(id, _, _)| tor_simulation::ActorId(*id))
                    .chain(
                        generated
                            .iter()
                            .flat_map(|def| &def.actors)
                            .map(|a| tor_simulation::ActorId(a.id)),
                    )
                    .collect(),
                items: r
                    .items
                    .iter()
                    .map(|i| (i.id, i.carried_by))
                    .chain(
                        generated
                            .iter()
                            .flat_map(|def| &def.items)
                            .map(|i| (i.id, i.carried_by)),
                    )
                    .filter(|(_, owner)| !index.definitions.omitted_carrier(*owner))
                    .map(|(id, _)| tor_simulation::ItemId(id))
                    .collect(),
                traps: r
                    .traps
                    .iter()
                    .copied()
                    .chain(generated.iter().flat_map(|def| &def.traps).map(|t| t.id))
                    .collect(),
                doors: r
                    .doors
                    .iter()
                    .copied()
                    .chain(generated.iter().flat_map(|def| &def.doors).map(|d| d.id))
                    .collect(),
            },
        })
    }

    /// One region's starting record, built in a scratch game holding it and
    /// its neighbours' geometry, so its links and entities are checked
    /// exactly as [`Package::build`] checks them. Reads only that region and
    /// its neighbours (through `index`), so the result doesn't depend on
    /// which regions were built before, and the cost doesn't depend on the
    /// package's size.
    ///
    /// Also returns the regions whose files the build read: a save copies
    /// exactly these, so replaying the build never needs the package.
    pub(crate) fn build_region(
        &self,
        seed: u64,
        index: &PackageIndex,
        region: u64,
    ) -> Result<(tor_simulation::RegionRecord, BTreeSet<u64>), Failure> {
        let r = index.region(self, region)?;
        self.check_region(&r)?;
        let mut shell: Vec<Arc<RegionDef>> = vec![r.clone()];
        let destinations = r
            .portals
            .iter()
            .filter_map(|p| self.endpoint_region_seed(seed, &p.to).ok())
            .chain(
                index
                    .stairs
                    .get(&region)
                    .into_iter()
                    .flatten()
                    .filter(|s| index.group_owner(s.destination).is_none())
                    .map(|s| s.destination),
            );
        for destination in destinations {
            if shell.iter().all(|s| s.id != destination) {
                shell.push(index.region(self, destination)?);
            }
        }
        shell.sort_by_key(|s| s.id);
        let files = shell
            .iter()
            .map(|s| s.id)
            .filter(|id| self.index.region(*id).is_some())
            .collect();
        let shell: Vec<&RegionDef> = shell.iter().map(|s| &**s).collect();
        let anchors = resolved_anchors(&shell);
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
            index.definitions.creature_catalog().content().clone(),
        );
        game.defer_initial_awareness()
            .map_err(|error| fail(format!("Initial awareness construction: {error:?}")))?;
        self.add_geometry(&mut game, &shell)?;
        // Deferred stair destinations need only their structural declarations.
        for destination in index
            .stairs
            .get(&region)
            .into_iter()
            .flatten()
            .map(|exit| exit.destination)
            .filter(|id| index.group_owner(*id).is_some())
            .collect::<BTreeSet<_>>()
        {
            if shell.iter().all(|r| r.id != destination) {
                let declaration = self.unbuilt_region(index, destination)?;
                game.add_unbuilt_region(
                    declaration.region,
                    declaration.chamber,
                    declaration.identities,
                )
                .map_err(|e| fail(format!("Stair destination: {e:?}")))?;
            }
        }
        self.add_structure(&mut game, &r, &anchors)?;
        self.add_stairs(&mut game, region, index, &anchors)?;
        for (name, position) in anchors.iter().filter(|(_, at)| at.region.0 == region) {
            require(
                self.boundary_anchor(name) || game.authored_cell_valid(*position),
                format!("Anchor {name}: outside traversable geometry"),
            )?;
        }
        self.add_entities(&mut game, seed, index, &[&r])?;
        let record = game
            .into_region_record(RegionId(region))
            .map_err(|e| fail(format!("Region {region}: {e:?}")))?;
        Ok((record, files))
    }

    /// Whether declaring `region` reads its file: a generated region's
    /// generator runs to learn its identities.
    pub(crate) fn declaring_reads_file(&self, region: u64) -> bool {
        self.index.region(region).is_some_and(|r| r.generated)
            && !self
                .manifest
                .generation_groups
                .values()
                .any(|g| g.members.contains(&region))
    }

    fn add_stairs(
        &self,
        game: &mut Game,
        region: u64,
        index: &PackageIndex,
        anchors: &BTreeMap<String, Location>,
    ) -> Result<(), Failure> {
        let deferred = |exit: &StairExit| {
            index.group_owner(region).is_some() || index.group_owner(exit.destination).is_some()
        };
        let mut names: BTreeSet<_> = if index.group_owner(region).is_some() {
            self.structural_metadata(index.seed, region)?
                .stair_anchors
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        // Trapdoor ceiling endpoints are structural coordinates too. Publish the
        // same complete immutable directory regardless of whether a stair uses it.
        names.extend(
            anchors
                .iter()
                .filter(|(_, at)| at.region.0 == region)
                .filter_map(|(name, _)| name.split_once('/').map(|(_, name)| name.to_owned())),
        );
        let exits = index.stair_exits(region);
        for exit in exits.iter().filter(|exit| deferred(exit)) {
            let (_, name) = exit
                .from
                .split_once('/')
                .ok_or_else(|| fail("Invalid named stair source"))?;
            names.insert(name.to_owned());
        }
        if !names.is_empty() {
            let coordinates = names
                .into_iter()
                .map(|name| {
                    let position = anchors
                        .get(&format!("{region}/{name}"))
                        .ok_or_else(|| {
                            fail(format!("Region {region}: unresolved stair anchor {name}"))
                        })?
                        .position;
                    Ok((name, position))
                })
                .collect::<Result<BTreeMap<_, _>, Failure>>()?;
            game.register_named_anchors(RegionId(region), coordinates)
                .map_err(|e| fail(format!("Region {region} stair anchors: {e:?}")))?;
        }
        for exit in &exits {
            let endpoint = |name: &str| {
                anchors.get(name).copied().ok_or_else(|| {
                    fail(format!(
                        "Stair pair {}: unresolved endpoint {name}",
                        exit.id
                    ))
                })
            };
            let result = if deferred(exit) {
                let (_, name) = exit
                    .to
                    .split_once('/')
                    .ok_or_else(|| fail("Invalid named stair destination"))?;
                game.connect_named_stair(
                    endpoint(&exit.from)?,
                    exit.direction,
                    tor_world::NamedAnchor {
                        region: RegionId(exit.destination),
                        name: name.into(),
                    },
                )
            } else {
                game.connect(
                    Passage {
                        from: endpoint(&exit.from)?,
                        direction: exit.direction,
                        to: endpoint(&exit.to)?,
                    },
                    0,
                )
            };
            result.map_err(|e| fail(format!("Stair pair {}: {e:?}", exit.id)))?;
        }
        Ok(())
    }

    fn resolve_endpoint(&self, seed: u64, anchor: &str) -> Result<String, Failure> {
        crate::procedural_world::FamilyCatalog::new(&self.manifest.procedural_worlds, seed)
            .resolve_endpoint(anchor)
    }
    fn endpoint_region_seed(&self, seed: u64, anchor: &str) -> Result<u64, Failure> {
        let resolved = self.resolve_endpoint(seed, anchor)?;
        let missing = || fail(format!("Missing stair/portal anchor {anchor}"));
        let (id, name) = resolved.split_once('/').ok_or_else(missing)?;
        let region: u64 = id.parse().map_err(|_| missing())?;
        let metadata = self.structural_metadata(seed, region)?;
        if id != region.to_string()
            || !(metadata.anchors.contains_key(name)
                || metadata.stair_anchors.iter().any(|n| n == name))
        {
            return Err(missing());
        }
        Ok(region)
    }

    /// These regions, in package order, with their walls and openings.
    fn add_geometry(&self, game: &mut Game, regions: &[&RegionDef]) -> Result<(), Failure> {
        for r in regions {
            let region = region_of(r.id, &r.name, r.size)?;
            (if r.chamber {
                game.add_chamber(region)
            } else {
                game.add_region(region)
            })
            .map_err(|e| fail(format!("Region {}: {e:?}", r.id)))?;
        }
        for r in regions {
            game.set_region_light(RegionId(r.id), r.lit)
                .map_err(|e| fail(format!("Region {} lighting: {e:?}", r.id)))?;
            let mut seen = BTreeSet::new();
            for light in &r.lighting {
                if !seen.insert(light.at) {
                    return Err(fail(format!("Region {} duplicate lighting cell", r.id)));
                }
                game.set_cell_light(loc(r.id, light.at), light.lit)
                    .map_err(|e| fail(format!("Region {} lighting: {e:?}", r.id)))?;
            }
            for p in &r.walls {
                game.set_wall(loc(r.id, *p), true)
                    .map_err(|e| fail(format!("Region {} wall: {e:?}", r.id)))?;
            }
            for p in &r.openings {
                game.set_wall(loc(r.id, *p), false)
                    .map_err(|e| fail(format!("Region {} opening: {e:?}", r.id)))?;
            }
        }
        Ok(())
    }

    fn spatial_joins(
        &self,
        r: &RegionDef,
        anchors: &BTreeMap<String, Location>,
    ) -> Result<Vec<spatial_runtime::SourceJoin>, Failure> {
        let mut joins = Vec::new();
        for (index, p) in r.portals.iter().enumerate() {
            let direction = match p.direction.as_str() {
                "north" => Direction::North,
                "east" => Direction::East,
                "south" => Direction::South,
                "west" => Direction::West,
                "up" => Direction::Up,
                "down" => Direction::Down,
                _ => return Err(fail("Invalid portal direction")),
            };
            let to = *anchors.get(&p.to).ok_or_else(|| {
                let file = self
                    .index
                    .region(r.id)
                    .map(|entry| entry.file.as_str())
                    .unwrap_or("generated region");
                self.region_reference(
                    r.id,
                    Origin::Region { file, id: r.id },
                    fail(format!("Missing portal anchor {}", p.to)),
                    &[
                        PathSegment::Field("portals"),
                        PathSegment::Index(index),
                        PathSegment::Field("to"),
                    ],
                    ReferenceValue::Text(&p.to),
                )
            })?;
            require(
                p.turns < 4 && !(p.rotation.is_some() && p.turns != 0),
                "Use rotation for a cube transform, or turns for planar rotation",
            )?;
            let rotation = p.rotation.unwrap_or(p.turns);
            require(
                p.kind.as_deref() != Some("stairs")
                    || matches!(direction, Direction::Up | Direction::Down),
                "Stairs require an up/down direction",
            )?;
            let kind = match p.kind.as_deref() {
                Some("boundary") => spatial_runtime::JoinKind::Boundary,
                Some("portal") => spatial_runtime::JoinKind::Portal,
                None | Some("stairs") => spatial_runtime::JoinKind::Transport,
                _ => return Err(fail("Invalid connection kind")),
            };
            joins.push(spatial_runtime::SourceJoin {
                passage: Passage {
                    from: loc(r.id, p.at),
                    direction,
                    to,
                },
                rotation,
                width: p.width,
                height: p.height,
                kind,
            });
        }
        Ok(joins)
    }

    /// A region's gravity, outgoing links, place hints and gravity overrides.
    fn add_structure(
        &self,
        game: &mut Game,
        r: &RegionDef,
        anchors: &BTreeMap<String, Location>,
    ) -> Result<(), Failure> {
        if let Some(gravity) = r.gravity {
            game.set_gravity(RegionId(r.id), gravity)
                .map_err(|_| fail("Invalid region gravity"))?;
        }
        for join in self.spatial_joins(r, anchors)? {
            join.apply_game(game)
                .map_err(|e| fail(format!("Region {} portal: {e:?}", r.id)))?;
        }
        for p in &r.places {
            match p {
                PlaceDef::At(at) => game.set_place_hint(loc(r.id, *at), true),
                PlaceDef::Named(place) => {
                    game.set_named_place_hint(loc(r.id, place.at), &place.name)
                }
            }
            .map_err(|e| fail(format!("Region {} place: {e:?}", r.id)))?;
        }
        let mut gravity_cells = BTreeSet::new();
        for g in &r.gravity_overrides {
            require(
                gravity_cells.insert(g.at),
                "Duplicate gravity override cell",
            )?;
            game.set_cell_gravity(loc(r.id, g.at), g.vector)
                .map_err(|_| fail("Invalid gravity override"))?;
            require(
                game.authored_cell_valid(loc(r.id, g.at)),
                "Gravity override outside traversable geometry",
            )?;
        }
        Ok(())
    }

    /// Actors, items, identity knowledge and doors in these regions (given in
    /// package order), in the same order whichever regions they are.
    fn add_entities(
        &self,
        game: &mut Game,
        seed: u64,
        index: &PackageIndex,
        regions: &[&RegionDef],
    ) -> Result<(), Failure> {
        let chosen: BTreeSet<u64> = regions.iter().map(|r| r.id).collect();
        let keep = |id: u64| chosen.contains(&id);
        let homes = &index.homes;
        let definitions = &index.definitions;
        let mut spawns: Vec<_> = chosen
            .iter()
            .flat_map(|r| index.spawns.get(r).into_iter().flatten())
            .copied()
            .collect();
        // A generated region's actors come from its materialized definition.
        for r in regions.iter().filter(|r| r.generate.is_some()) {
            for a in &r.actors {
                let ticks = a
                    .turn_ticks
                    .or(definitions.archetype(&a.archetype)?.turn_ticks)
                    .unwrap_or(100);
                spawns.push((a.id, loc(r.id, a.at), ticks));
            }
        }
        spawns.sort_by_key(|(id, _, _)| *id);
        for (id, at, ticks) in spawns {
            game.spawn_authored_actor(
                id,
                at,
                NonZeroU64::new(ticks).ok_or_else(|| fail("Zero actor duration"))?,
            )
            .map_err(|e| fail(format!("Actor {id}: {e:?}")))?;
        }
        for c in &definitions.characters {
            if c.creature.control.spawned() && homes.get(&c.id).is_some_and(|h| keep(h.region.0)) {
                instantiation::configure(
                    game,
                    c.id,
                    c.creature.borrowed(),
                    Origin::Character(c.id),
                    || self.diagnostic_source("scenario.toml"),
                )?;
            }
        }
        for r in regions {
            let file = self
                .index
                .region(r.id)
                .map(|entry| entry.file.as_str())
                .unwrap_or("generated region");
            for a in &r.actors {
                let origin = Origin::Actor {
                    file,
                    region: r.id,
                    id: a.id,
                };
                let prepared = definitions.actor(a).map_err(|failure| {
                    failure.into_failure(origin, || self.region_text(r.id).ok())
                })?;
                instantiation::configure(game, a.id, prepared, origin, || {
                    // Diagnostic acquisition must never replace the original
                    // construction failure or add work to successful installs.
                    self.region_text(r.id).ok()
                })?;
            }
        }
        let appearances = &index.appearances;
        // A carried item is authored where its carrier starts.
        let mut items: Vec<_> = regions
            .iter()
            .flat_map(|r| r.items.iter().map(move |i| (r.id, i)))
            .collect();
        items.sort_by_key(|(_, i)| i.id);
        for (region, i) in items {
            let contextualize = |failure| {
                Origin::Item {
                    file: self
                        .index
                        .region(region)
                        .map(|entry| entry.file.as_str())
                        .unwrap_or("generated region"),
                    region,
                    id: i.id,
                }
                .context(failure)
            };
            let spec = definitions
                .item(i, seed, appearances)
                .map_err(contextualize)?;
            // Inventory of omitted characters is omitted with its owner.
            if definitions.omitted_carrier(i.carried_by) {
                continue;
            }
            // A carried item starts with its carrier, which may be authored
            // in another region.
            let at = match i.carried_by.and_then(|id| homes.get(&id)) {
                Some(home) if home.region.0 != region => *home,
                _ => loc(region, i.at),
            };
            game.place_item_stack(
                i.id,
                at,
                i.carried_by.map(tor_simulation::ActorId),
                i.quantity,
                spec,
            )
            .map_err(|e| contextualize(fail(format!("Item {}: {e:?}", i.id))))?;
            if let Some(slot) = i.equipped_slot {
                let owner = i
                    .carried_by
                    .ok_or_else(|| contextualize(fail("Starting equipment must be carried")))?;
                game.equip_authored(
                    tor_simulation::ActorId(owner),
                    tor_simulation::ItemId(i.id),
                    tor_simulation::EquipmentSlotId(slot),
                )
                .map_err(|_| {
                    contextualize(fail(
                        "Starting equipment does not fit anatomy or occupied slot",
                    ))
                })?;
            }
        }
        for c in &definitions.characters {
            if c.creature.control.spawned() && homes.get(&c.id).is_some_and(|h| keep(h.region.0)) {
                for identity in &c.known_identities {
                    game.learn_identity(tor_simulation::ActorId(c.id), identity)
                        .map_err(|_| fail("Unknown initial item identity"))?;
                }
            }
        }
        let mut doors: Vec<_> = regions
            .iter()
            .flat_map(|r| r.doors.iter().map(move |d| (r.id, d)))
            .collect();
        doors.sort_by_key(|(_, d)| d.id);
        for (region, d) in doors {
            let at = loc(region, d.at);
            let clearance = game.door_clearance(at);
            if d.height > clearance {
                return Err(fail(format!(
                    "Door {} doesn't fit: at most {} cells tall here",
                    d.id, clearance
                )));
            }
            // A door shorter than its walled doorway can be seen over.
            if game.doorway_open_above(at, d.height) {
                return Err(fail(format!(
                    "Door {} is shorter than its doorway, which continues above it",
                    d.id
                )));
            }
            game.place_authored_door(d.id, at, d.open, d.height)
                .map_err(|e| fail(format!("Door {}: {e:?}", d.id)))?;
            if let Some(concealment) = d.concealment {
                game.conceal_door(at, concealment.compiled())
                    .map_err(|e| fail(format!("Secret door {}: {e:?}", d.id)))?;
            }
        }
        for region in regions {
            for trap in &region.traps {
                game.place_trap(
                    loc(region.id, trap.at),
                    tor_world::Trap {
                        id: trap.id,
                        dc: trap.dc,
                        avoidance_dc: trap.avoidance_dc,
                        kind: trap.kind.compiled(),
                    },
                )
                .map_err(|e| fail(format!("Trap {}: {e:?}", trap.id)))?;
            }
        }
        Ok(())
    }

    fn configure_run(&self, game: &mut Game, index: &PackageIndex) -> Result<(), Failure> {
        if self.manifest.arena.is_some() {
            return self.configure_loaded_arena(game);
        }
        if self
            .manifest
            .characters
            .iter()
            .any(|c| c.creature.is_some())
            || self.manifest.objective.is_some()
            || self.manifest.unique_retrieval.is_some()
        {
            let objective =
                self.manifest
                    .objective
                    .as_ref()
                    .map(|o| tor_simulation::combat::Objective {
                        anchor: index.anchors[&o.anchor],
                        item: o.item.map(tor_simulation::ItemId),
                        disclosed: o.disclosed,
                        continue_play: o.continue_play,
                    });
            let characters = index
                .definitions
                .characters
                .iter()
                .filter(|c| c.creature.control.spawned())
                .map(|c| tor_simulation::ActorId(c.id))
                .collect();
            game.configure_run(
                tor_simulation::ActorId(self.selected),
                characters,
                objective,
                self.manifest.factions.clone(),
            )
            .map_err(|_| fail("Invalid run configuration"))?;
        }
        Ok(())
    }
    pub(crate) fn configure_loaded_arena(&self, game: &mut Game) -> Result<(), Failure> {
        let Some(arena) = &self.manifest.arena else {
            return Ok(());
        };
        require(
            arena.participants.iter().all(|id| {
                game.is_ai(tor_simulation::ActorId(*id))
                    == (arena.control == ArenaControl::AllAi || *id != self.selected)
            }),
            "Arena participants require the declared manual/AI control",
        )?;
        game.configure_arena_run_with_limits(
            tor_simulation::ActorId(self.selected),
            arena
                .participants
                .iter()
                .copied()
                .map(tor_simulation::ActorId)
                .collect(),
            self.manifest.factions.clone(),
            arena.limits(),
        )
        .map_err(|_| fail("Invalid arena run configuration"))?;
        if arena.start_paused {
            game.control_arena(true, 0)
                .map_err(|_| fail("Invalid initial arena control"))?;
        }
        Ok(())
    }
    /// Only structural edits that invalidate an authored anchor break validation.
    /// World mutation APIs independently enforce topology/entity consistency.
    /// Regions that aren't loaded can't have been edited, so only loaded
    /// anchors are checked.
    pub(crate) fn state_valid(&self, game: &Game) -> bool {
        let loaded = |at: &Location| {
            matches!(
                game.region_state(at.region),
                Some(tor_simulation::RegionState::Active | tor_simulation::RegionState::Frozen)
            )
        };
        game.authored_links_clear()
            && self.anchors(game.seed()).is_ok_and(|anchors| {
                anchors
                    .iter()
                    .filter(|(name, p)| !self.boundary_anchor(name) && loaded(p))
                    .all(|(_, p)| game.authored_cell_valid(*p))
            })
    }
}

/// Whole-package facts for building regions (see [`Package::index`]).
#[derive(Clone, Debug)]
pub(crate) struct PackageIndex {
    procedural_worlds: BTreeMap<String, crate::procedural_world::WorldFamily>,
    pub(crate) group_owners: BTreeMap<u64, u64>,
    pub(crate) groups: BTreeMap<u64, String>,
    definitions: Arc<PreparedDefinitions>,
    anchors: BTreeMap<String, Location>,
    stairs: BTreeMap<u64, Vec<StairExit>>,
    /// Where each spawned actor starts.
    homes: BTreeMap<u64, Location>,
    /// Actors by the region they start in: identity, start, turn length.
    spawns: BTreeMap<u64, Vec<(u64, Location, u64)>>,
    /// One more than the largest actor, item and door identity the package
    /// can make, generated ones included.
    ceilings: (u64, u64, u64),
    /// The game's seed; semantic generator inputs derive independent streams.
    seed: u64,
    /// Where generated actor and item identity ranges start.
    generated_base: (u64, u64),
    appearances: BTreeMap<String, String>,
    /// Region definitions handed out, for scaling contracts.
    lookups: std::cell::Cell<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedGroup {
    pub(crate) spatial: tor_simulation::spatial_scope::registry::Scope,
    /// Immutable geometry-only input; preparing it never chooses population.
    pub(crate) activation: Option<tor_simulation::floor_activation::PreparedScope>,
    pub(crate) records: Vec<(tor_simulation::UnbuiltRegion, tor_simulation::RegionRecord)>,
    pub(crate) files: BTreeSet<u64>,
}

type StairExit = crate::procedural_world::StairConnection;

fn resolved_anchors(regions: &[&RegionDef]) -> BTreeMap<String, Location> {
    regions
        .iter()
        .flat_map(|r| {
            r.anchors
                .iter()
                .map(move |(name, p)| (format!("{}/{name}", r.id), loc(r.id, *p)))
        })
        .collect()
}

impl PackageIndex {
    fn families(&self) -> crate::procedural_world::FamilyCatalog<'_> {
        crate::procedural_world::FamilyCatalog::new(&self.procedural_worlds, self.seed)
    }
    pub(crate) fn group_owner(&self, region: u64) -> Option<u64> {
        self.group_owners
            .get(&region)
            .copied()
            .or_else(|| self.families().owner(region))
    }
    fn group_identity(&self, owner: u64) -> Option<String> {
        self.groups
            .get(&owner)
            .cloned()
            .or_else(|| self.families().group_identity(owner))
    }
    fn stair_exits(&self, region: u64) -> Vec<StairExit> {
        let mut exits = self.stairs.get(&region).cloned().unwrap_or_default();
        exits.extend(self.families().connections(region));
        exits
    }
    pub(crate) fn creature_catalog(&self) -> &crate::creature_authoring::CompiledCatalog {
        self.definitions.creature_catalog()
    }

    pub(crate) fn named_birth_floors(&self, package: &Package) -> Option<(u64, u64)> {
        let kit = package
            .manifest
            .characters
            .iter()
            .find(|actor| actor.id == package.selected)?
            .loadout
            .len();
        Some((self.ceilings.0, self.ceilings.1.checked_add(kit as u64)?))
    }

    pub(crate) fn creature_archetypes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.definitions.creature_archetypes()
    }

    pub(crate) fn standalone_creature(
        &self,
        id: &str,
    ) -> Result<tor_simulation::floor_activation::PreparedActorChoice, Failure> {
        let definition = self.definitions.population_actor(id, 1, None)?;
        require(
            definition.actor.build.is_some(),
            "Archetype has no creature build",
        )?;
        Ok(definition)
    }

    /// Inspect immutable descriptors only; listing never materializes a region.
    pub(crate) fn authored_depth(&self, package: &Package, region: u64) -> Option<u32> {
        if let Some(owner) = self.group_owners.get(&region) {
            let name = self.groups.get(owner)?;
            return package
                .manifest
                .generation_groups
                .get(name)
                .map(|group| group.depth);
        }
        let (_, family) = self.families().find(region)?;
        family.address(region).map(|(depth, _)| depth)
    }

    /// A region's definition: authored, or materialized by its generator.
    fn region(&self, package: &Package, id: u64) -> Result<Arc<RegionDef>, Failure> {
        self.lookups.set(self.lookups.get() + 1);
        let mut def = package.structural_definition(self.seed, id)?;
        if def
            .portals
            .iter()
            .any(|portal| portal.to.matches('/').count() == 2)
        {
            let mut resolved = (*def).clone();
            for portal in &mut resolved.portals {
                portal.to = package.resolve_endpoint(self.seed, &portal.to)?;
            }
            def = Arc::new(resolved);
        }
        let Some(generate) = &def.generate else {
            return Ok(def);
        };
        if let Some(group) = &generate.group {
            return package
                .group_definitions(group, self.seed)?
                .remove(&id)
                .map(Arc::new)
                .ok_or_else(|| fail("Region missing from generated group"));
        }
        let first = |base: u64| {
            (id - 1)
                .checked_mul(crate::generator::IDENTITY_STRIDE)
                .and_then(|n| n.checked_add(base))
                .ok_or_else(|| fail("Generated region id too large"))
        };
        Ok(Arc::new(crate::generator::materialize(
            &def,
            generate,
            self.seed,
            first(self.generated_base.0)?,
            first(self.generated_base.1)?,
        )?))
    }

    /// Region definitions handed out so far.
    #[cfg(test)]
    pub(crate) fn lookups(&self) -> usize {
        self.lookups.get()
    }
}

/// Records for a game started with [`Package::start`]: detached records in
/// memory, and unbuilt regions built from the package on first load.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "tests build package games without an engine")
)]
pub(crate) struct PackageRecords {
    package: Arc<Package>,
    index: PackageIndex,
    seed: u64,
    records: tor_simulation::MemoryRecords,
}

impl PackageRecords {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "tests build package games without an engine")
    )]
    pub(crate) fn new(package: Arc<Package>, seed: u64) -> Self {
        Self {
            index: package.index(seed).expect("a valid package"),
            package,
            seed,
            records: Default::default(),
        }
    }
}

impl tor_simulation::RecordStore for PackageRecords {
    fn put(
        &mut self,
        id: tor_simulation::RecordId,
        record: tor_world::Shared<tor_simulation::RegionRecord>,
    ) {
        self.records.put(id, record);
    }
    fn get(
        &mut self,
        id: tor_simulation::RecordId,
    ) -> Option<tor_world::Shared<tor_simulation::RegionRecord>> {
        self.records.get(id)
    }
    fn build(&mut self, region: RegionId) -> Option<tor_simulation::RegionRecord> {
        self.package
            .build_region(self.seed, &self.index, region.0)
            .ok()
            .map(|(record, _)| record)
    }
    fn unbuilt(&mut self, region: RegionId) -> Option<tor_simulation::UnbuiltRegion> {
        self.package.unbuilt_region(&self.index, region.0).ok()
    }
}

#[cfg(test)]
mod tests {
    fn care_alias_package(replacement: &str) -> Result<Package, Failure> {
        let manifest: Manifest =
            toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
        let regions: Vec<RegionDef> = [
            include_str!("../../../scenarios/two-room/regions/1.toml"),
            include_str!("../../../scenarios/two-room/regions/2.toml"),
        ]
        .into_iter()
        .map(|source| toml::from_str(source).unwrap())
        .collect();
        let original = tor_simulation::rules::DEFAULT_RULESET_TOML;
        let named = "reach = { kind = \"self_or_ally\", range = { kind = \"touch\" } }";
        assert_eq!(original.matches(named).count(), 1);
        Package::from_parts(manifest.clone(), regions.clone(), original).unwrap();
        let source = original.replacen(named, replacement, 1);
        assert!(toml::from_str::<toml::Value>(&source).is_ok());
        Package::from_parts(manifest, regions, &source)
    }

    #[test]
    fn palette_budget_reserves_full_and_delta_envelopes_at_the_boundary() {
        let assets = BTreeSet::from(["creature.equine.quagga".into(), "item.potion".into()]);
        let encoded = serde_json::to_vec(&assets).unwrap().len();
        assert!(check_palette_budget(&assets, PALETTE_ENVELOPE_RESERVE + encoded).is_ok());
        assert!(check_palette_budget(&assets, PALETTE_ENVELOPE_RESERVE + encoded - 1).is_err());
        assert!(check_palette_budget(&assets, PALETTE_ENVELOPE_RESERVE - 1).is_err());
        // Exercise real envelopes too, including escaped context/request IDs
        // and the longest numeric revisions. Delta sets partition the same
        // package-wide union; they never double its serialized asset payload.
        let context: tor_protocol::ReplyContext = serde_json::from_value(serde_json::json!({
            "input": {"stream": {"stream": "\\".repeat(128), "epoch": u64::MAX.to_string()},
                      "readiness_revision": u64::MAX.to_string()},
            "actor": u64::MAX.to_string(), "branch": "\\".repeat(128),
            "cursor": {"sequence": u64::MAX.to_string(), "tick": u64::MAX.to_string()},
            "revision": u64::MAX.to_string(),
        }))
        .unwrap();
        for body in [
            tor_protocol::PaletteBody::Full {
                assets: assets.clone(),
            },
            tor_protocol::PaletteBody::Delta {
                base: u64::MAX - 1,
                added: assets.iter().take(1).cloned().collect(),
                removed: assets.iter().skip(1).cloned().collect(),
            },
        ] {
            let message = tor_protocol::ServerMessage::Palette {
                context: context.clone(),
                request_id: Some("\\".repeat(128)),
                palette: tor_protocol::PaletteUpdate {
                    revision: u64::MAX,
                    body,
                },
            };
            assert!(tor_protocol::encode_bounded_json(
                &message,
                PALETTE_ENVELOPE_RESERVE + encoded
            )
            .is_ok());
        }
    }

    #[test]
    fn phase5_authoring_package_rejects_positional_reach_alias() {
        assert!(care_alias_package("reach = [\"self_or_ally\", { kind = \"touch\" }]").is_err());
    }

    #[test]
    fn phase5_authoring_package_rejects_positional_ally_range_alias() {
        assert!(
            care_alias_package("reach = { kind = \"self_or_ally\", range = [\"touch\"] }").is_err()
        );
    }

    #[test]
    fn phase5_container_archetype_rejects_positional_fields() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = read_package(&root).unwrap();
        let mut manifest = package.manifest.clone();
        manifest.archetypes.insert(
            "bandage".into(),
            Archetype {
                supply: Some("bandage".into()),
                name: Some("Bandage".into()),
                stackable: true,
                ..Default::default()
            },
        );
        // Ordinary compiler validation proves this is a legal supply archetype.
        PreparedDefinitions::new(&manifest, 1).unwrap();
        let mut value = serde_json::to_value(&manifest.archetypes["bandage"]).unwrap();
        let decoded: Archetype = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        // Creature is the only omitted optional serialized field.
        value
            .as_object_mut()
            .unwrap()
            .entry("creature")
            .or_insert(serde_json::Value::Null);
        let object = value.as_object().unwrap();
        let fields = [
            "creature",
            "anatomy",
            "equipment",
            "projectile",
            "equipment_generation",
            "consumable",
            "readable",
            "ground_scare",
            "device",
            "held_light",
            "edible",
            "supply",
            "magical",
            "currency",
            "class",
            "body",
            "identity",
            "appearance_pool",
            "forms",
            "stackable",
            "properties",
            "name",
            "turn_ticks",
            "asset",
        ];
        assert_eq!(object.len(), fields.len());
        let positional: Vec<_> = fields
            .iter()
            .map(|field| object.get(*field).unwrap().clone())
            .collect();
        assert!(serde_json::from_value::<Archetype>(serde_json::json!(positional)).is_err());
    }

    #[test]
    fn compiled_inputs_reject_unsupported_source_headers_even_with_current_identity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let original = read_package(&root).unwrap();
        let encode = crate::storage::schema::scenario::encode_content_package;
        let decode = crate::storage::schema::scenario::decode_content_package;
        decode(&encode(&original).unwrap()).unwrap();
        for format in [0, FORMAT - 1, FORMAT + 1] {
            let mut package = original.clone();
            package.manifest.format = format;
            // Source-format metadata is deliberately outside semantic identity.
            // Current hashes and validator evidence must not bypass admission.
            assert_eq!(
                package.certificate.model_hash,
                model_hash(
                    &package.manifest,
                    &package.index,
                    &package.certificate.ruleset_hash,
                    &package.certificate.ruleset_dependencies,
                )
                .unwrap()
            );
            assert!(
                decode(&encode(&package).unwrap()).is_err(),
                "format {format}"
            );
        }
        let mut package = original;
        package.manifest.version = "not-a-version".into();
        assert!(decode(&encode(&package).unwrap()).is_err());
    }

    #[test]
    fn previous_draft_generator_validator_cannot_reuse_compiled_inputs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut package = read_package(&root).unwrap();
        package.check_identity().unwrap();
        let current_artifact = artifact::relative_root(&package);
        package.certificate.validator = "tor-scenario-13".into();
        let encoded = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
        assert!(
            crate::storage::schema::scenario::decode_content_package(&encoded).is_err(),
            "Old generator evidence must not silently admit changed entry placement"
        );
        assert!(package.check_identity().is_err());
        assert_ne!(artifact::relative_root(&package), current_artifact);
    }

    #[test]
    fn previous_campaign_validator_cannot_admit_changed_health_and_ledger_inputs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut package = read_package(&root).unwrap();
        package.check_identity().unwrap();
        for validator in ["tor-scenario-15", "tor-scenario-16"] {
            package.certificate.validator = validator.into();
            let encoded =
                crate::storage::schema::scenario::encode_content_package(&package).unwrap();
            assert!(crate::storage::schema::scenario::decode_content_package(&encoded).is_err());
            assert!(package.check_identity().is_err());
        }
    }

    #[test]
    fn current_package_rejects_previous_validator_and_rules_identity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let original = read_package(&root).unwrap();
        original.check_identity().unwrap();
        assert_eq!(original.certificate.validator, "tor-scenario-47");
        assert_eq!(original.certificate.ruleset, "interactions-v56");
        let current_artifact = artifact::relative_root(&original);
        for (boundary, identity) in [
            ("validator", "tor-scenario-27"),
            ("validator", "tor-scenario-28"),
            ("validator", "tor-scenario-29"),
            ("validator", "tor-scenario-30"),
            ("validator", "tor-scenario-31"),
            ("validator", "tor-scenario-32"),
            ("ruleset", "interactions-v38"),
            ("ruleset", "interactions-v39"),
            ("ruleset", "interactions-v40"),
            ("ruleset", "interactions-v41"),
            ("ruleset", "interactions-v42"),
        ] {
            let mut package = original.clone();
            if boundary == "validator" {
                package.certificate.validator = identity.into();
                assert_ne!(artifact::relative_root(&package), current_artifact);
            } else {
                package.certificate.ruleset = identity.into();
            }
            let encoded =
                crate::storage::schema::scenario::encode_content_package(&package).unwrap();
            assert!(crate::storage::schema::scenario::decode_content_package(&encoded).is_err());
            assert!(package.check_identity().is_err(), "{boundary}");
        }
        assert!(resolve_rules_source("import = \"interactions-v35\"\n", "rules.toml").is_err());
    }

    fn compact_package(domain: crate::procedural_world::Domain) -> Package {
        let template = read_package(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scenarios/tests/explicit-grid-groups"),
        )
        .unwrap();
        let mut manifest = template.manifest.clone();
        manifest.generation_groups.clear();
        manifest.stair_pairs.clear();
        manifest.characters[0].anchor = "caves/1/start".into();
        manifest.procedural_worlds.clear();
        manifest.procedural_worlds.insert(
            "caves".into(),
            crate::procedural_world::WorldFamily {
                kind: crate::procedural_world::FamilyKind::GridChain,
                version: 1,
                base_region: 1,
                domain,
                grid: [2, 2],
                size: [26, 7, 2],
                recipe: "exploration".into(),
                gravity: None,
                zone: None,
            },
        );
        Package::from_parts(manifest, vec![], &template.rules_source).unwrap()
    }

    #[test]
    fn rogue_magic_catalog_uses_explicit_capabilities_and_appends_finite_stock() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let package = read_package(&root).unwrap();
        let healing = &package.manifest.archetypes["rogue.healing"];
        assert!(healing.magical && healing.stackable);
        assert_eq!(healing.class, ItemClass::Potion);
        assert_eq!(healing.appearance_pool.as_deref(), Some("rogue.potions"));
        let dose = healing.consumable.as_ref().unwrap();
        assert_eq!(dose.discovery, ItemDiscovery::OnUse);
        assert_eq!(
            dose.effects,
            vec![
                Effect::HealPerHitDie {
                    sides: 4,
                    bonus: 0,
                    overflow: HealingOverflow::ImproveEarliestHitDie
                },
                Effect::RemoveCondition {
                    condition: "rogue.blindness".into()
                },
            ]
        );
        assert!(dose.compile(&package.rules().unwrap()).is_ok());
        for (id, condition, sides, bonus) in [
            ("rogue.confusion", "rogue.potion_confusion", 2, 18),
            ("rogue.blindness", "rogue.blindness", 85, 807),
        ] {
            let item = &package.manifest.archetypes[id];
            assert!(item.magical && item.stackable);
            assert_eq!(item.appearance_pool.as_deref(), Some("rogue.potions"));
            let dose = item.consumable.as_ref().unwrap();
            assert_eq!(dose.discovery, ItemDiscovery::OnUse);
            assert_eq!(
                dose.effects,
                vec![Effect::ApplyConditionForTurns {
                    condition: condition.into(),
                    duration: AmountExpr::Rolled {
                        dice: DiceExpr {
                            count: 1,
                            sides,
                            bonus
                        }
                    },
                }]
            );
        }
        for (id, class) in [
            ("rogue.identify_potion", ItemClass::Potion),
            ("rogue.identify_scroll", ItemClass::Scroll),
            ("rogue.identify_wand", ItemClass::Wand),
        ] {
            let item = &package.manifest.archetypes[id];
            let readable = item.readable.as_ref().unwrap();
            assert_eq!(
                readable.identify,
                Some(if class == ItemClass::Wand {
                    vec![ItemClass::Ring, ItemClass::Wand]
                } else {
                    vec![class]
                })
            );
            assert!(readable.effects.is_empty());
            assert_eq!(readable.discovery, ItemDiscovery::OnUse);
        }
        let sleep = package.manifest.archetypes["rogue.sleep"]
            .readable
            .as_ref()
            .unwrap();
        assert_eq!(sleep.discovery, ItemDiscovery::OnUse);
        assert_eq!(
            sleep.effects,
            vec![Effect::ApplyConditionForTurns {
                condition: "rogue.sleep".into(),
                duration: AmountExpr::Rolled {
                    dice: DiceExpr {
                        count: 1,
                        sides: 5,
                        bonus: 3
                    }
                },
            }]
        );
        assert_eq!(
            package.manifest.archetypes["rogue.restore_strength"]
                .consumable
                .as_ref()
                .unwrap()
                .discovery,
            ItemDiscovery::SelfEffect
        );
        let activation =
            serde_json::to_value(package.manifest.floor_activation.as_ref().unwrap()).unwrap();
        let stages = activation["profiles"]["rogue.low_depth"]["stages"]
            .as_array()
            .unwrap();
        let ids: Vec<_> = stages
            .iter()
            .map(|stage| stage["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            &ids[..5],
            &[
                "rogue.eight_species_foes",
                "rogue.bandages",
                "rogue.rations",
                "rogue.restore_strength",
                "rogue.room_gold"
            ]
        );
        assert_eq!(ids[5], "rogue.magic_potions");
        assert_eq!(
            stages[5]["bands"][0]["choices"],
            serde_json::json!([
                {"archetype":"rogue.healing","weight":1},
                {"archetype":"rogue.confusion","weight":1},
                {"archetype":"rogue.blindness","weight":1},
                {"archetype":"rogue.poison","weight":1},
                {"archetype":"rogue.see_invisible","weight":1},
                {"archetype":"rogue.extra_healing","weight":1},
                {"archetype":"rogue.levitation","weight":1},
            ])
        );
        assert_eq!(ids[6], "rogue.magic_scrolls");
        assert_eq!(ids[7], "rogue.magic_devices");
        for stage in &stages[5..] {
            assert_eq!(stage["kind"], "items");
            assert_eq!(stage["count"], serde_json::json!([1, 1]));
        }
        assert_eq!(
            activation["carried_item_pools"].as_object().unwrap().len(),
            2
        );
    }

    #[test]
    fn rogue_devices_compile_lore_rays_without_grants_or_non_charge_costs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let package = read_package(&root).unwrap();
        let rules = package.rules().unwrap();
        for element in ["fire", "cold", "lightning", "invisibility", "haste", "slow"] {
            let utility = matches!(element, "invisibility" | "haste" | "slow");
            let id = format!("rogue.wand_{element}");
            let item = &package.manifest.archetypes[&id];
            assert!(item.magical && !item.stackable);
            assert_eq!(item.class, ItemClass::Wand);
            assert_eq!(item.appearance_pool.as_deref(), Some("rogue.wands"));
            let device = item.device.as_ref().unwrap();
            assert_eq!(
                device.initial_charges,
                ChargeRange {
                    minimum: 3,
                    maximum: 7
                }
            );
            assert_eq!(
                device.discovery,
                if utility {
                    ItemDiscovery::Never
                } else {
                    ItemDiscovery::OnUse
                }
            );
            let compiled = device.compile(&rules).unwrap();
            let ability = rules.ability(&compiled.ability).unwrap();
            assert!(
                ability.cost.is_none()
                    && ability.cooldown.is_none()
                    && ability.supply_costs.is_empty()
            );
            assert_eq!(
                ability.invocation,
                if utility {
                    tor_simulation::abilities::InvocationKind::DirectionalContactRay
                } else {
                    tor_simulation::abilities::InvocationKind::DirectionalRay
                }
            );
            let value = serde_json::to_value(ability).unwrap();
            assert_eq!(
                value["reach"],
                serde_json::json!({"kind":"sight","range":6})
            );
            let strike = &value["strikes"]["strikes"][0];
            assert_eq!(
                strike["check"]["skill"],
                serde_json::json!({"kind":"named","skill":"agility"})
            );
            assert_eq!(
                strike["check"]["against"]["skill"],
                serde_json::json!({"kind":"named","skill":"lore"})
            );
            if utility {
                let (condition, cancels) = match element {
                    "invisibility" => ("rogue.invisible", None),
                    "haste" => ("rogue.hasted", Some("rogue.slowed")),
                    "slow" => ("rogue.slowed", Some("rogue.hasted")),
                    _ => unreachable!("utility variants are selected explicitly above"),
                };
                assert_eq!(
                    strike["on_failure"],
                    serde_json::json!([{"kind":"apply_persistent_condition", "condition":condition, "cancels":cancels}])
                );
            } else {
                let damage = &strike["on_failure"][0]["damage"]["components"][0];
                assert_eq!(damage["descriptor"], element);
                assert_eq!(
                    damage["amount"]["dice"],
                    serde_json::json!({"count":6,"sides":6,"bonus":0})
                );
            }
            // The catalog authors an invocation reference, never a character or
            // talent purchase that permanently grants the device's ability.
            assert!(!rules
                .document()
                .constants
                .default_abilities
                .contains(&compiled.ability));
            assert!(!rules.document().talents.iter().any(|talent| {
                talent.modifiers.iter().any(|modifier| matches!(
                    &modifier.op,
                    tor_simulation::rules::ModifierOp::GrantAbility { ability } if ability == &compiled.ability
                ))
            }));
        }
    }

    #[test]
    fn device_catalog_is_class_independent_but_rejects_stackable_or_mixed_capabilities() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let mut package = read_package(&root).unwrap();
        // Device capability is class-independent, but every concealed alternative
        // in one appearance group must retain the same physical class.
        let ids = [
            "rogue.wand_fire",
            "rogue.wand_cold",
            "rogue.wand_lightning",
            "rogue.wand_invisibility",
            "rogue.wand_haste",
            "rogue.wand_slow",
            "rogue.wand_teleport_away",
            "rogue.wand_teleport_to",
            "rogue.wand_nothing",
            "rogue.wand_light",
            "rogue.wand_drain_life",
            "rogue.wand_cancellation",
            "rogue.wand_magic_missile",
            "rogue.wand_polymorph",
        ];
        assert_eq!(ids.len(), 14);
        let pool = package.manifest.archetypes["rogue.wand_fire"]
            .appearance_pool
            .clone();
        assert!(pool.is_some());
        let actual: BTreeSet<_> = package
            .manifest
            .archetypes
            .iter()
            .filter(|(_, item)| item.appearance_pool == pool)
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(
            actual,
            ids.iter().copied().collect(),
            "class conversion must cover the exact complete appearance group"
        );
        for id in ids {
            let item = package.manifest.archetypes.get_mut(id).unwrap();
            // This owner isolates class-independent charged use, not the new
            // Wand-class physical equipment overlap; keep the former pure-device shape.
            item.forms.clear();
            item.class = ItemClass::Tool;
        }
        package.appearance_mapping(42).unwrap();
        let mut stackable = package.clone();
        stackable
            .manifest
            .archetypes
            .get_mut("rogue.wand_fire")
            .unwrap()
            .stackable = true;
        assert!(stackable.appearance_mapping(42).is_err());
        let mut mixed = package.clone();
        let device = mixed
            .manifest
            .archetypes
            .get_mut("rogue.wand_fire")
            .unwrap();
        device.readable = Some(ReadableSpec {
            control: None,
            descriptors: vec![],
            effects: vec![],
            identify: Some(vec![ItemClass::Wand]),
            discovery: ItemDiscovery::OnUse,
        });
        assert!(mixed.appearance_mapping(42).is_err());
    }

    #[test]
    fn bundled_rogue_uses_compact_open_family_and_preserves_room_recipe() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
        let package = read_package(&root.join("rogue-exploration")).unwrap();
        let geometry = read_package(&root.join("tests/rogue-geometry")).unwrap();
        assert!(geometry.manifest.floor_activation.is_none());
        assert!(geometry
            .manifest
            .characters
            .iter()
            .all(|character| character.creature.is_none() && character.loadout.is_empty()));
        assert_eq!(
            package.manifest.procedural_worlds["dungeon"].gravity,
            Some([0, 0, -1])
        );
        assert_eq!(
            geometry.manifest.procedural_worlds["dungeon"].gravity,
            Some([0, 0, -1])
        );
        assert_eq!(
            serde_json::to_value(&package.manifest.procedural_worlds).unwrap(),
            serde_json::to_value(&geometry.manifest.procedural_worlds).unwrap()
        );
        let production_recipe = &package.manifest.generation_recipes["exploration"];
        let geometry_recipe = &geometry.manifest.generation_recipes["exploration"];
        assert_eq!(production_recipe.version, 3);
        assert_eq!(geometry_recipe.version, 2);
        assert_eq!(geometry_recipe.stages.len(), 7);
        assert_eq!(production_recipe.stages.len(), 9);
        assert_eq!(
            serde_json::to_value(&production_recipe.stages[..7]).unwrap(),
            serde_json::to_value(&geometry_recipe.stages).unwrap(),
            "discovery appends stages without changing any existing stream identity or operand"
        );
        assert_eq!(production_recipe.stages[7].id, "secret_entrances");
        assert_eq!(production_recipe.stages[8].id, "hidden_traps");
        assert!(matches!(
            production_recipe.stages[7].operation,
            crate::generation_recipe::Operation::SecretEntrances { .. }
        ));
        assert!(matches!(
            production_recipe.stages[8].operation,
            crate::generation_recipe::Operation::HiddenTraps { .. }
        ));
        let explicit = read_package(&root.join("tests/explicit-grid-groups")).unwrap();
        assert!(package.index.regions.is_empty());
        assert!(package.manifest.generation_groups.is_empty());
        assert!(package.manifest.stair_pairs.is_empty());
        assert_eq!(package.manifest.characters[0].anchor, "dungeon/1/start");
        assert_eq!(package.manifest.procedural_worlds.len(), 1);
        let family = &package.manifest.procedural_worlds["dungeon"];
        assert_eq!(family.grid, [3, 3]);
        assert_eq!(family.size, [26, 7, 2]);
        assert_eq!(
            family.domain,
            crate::procedural_world::Domain::Open {
                max_depth: 1_000_000
            }
        );
        assert_eq!(
            serde_json::to_value(geometry_recipe).unwrap(),
            serde_json::to_value(&explicit.manifest.generation_recipes["exploration"]).unwrap()
        );
        let mut entrances = 0;
        for seed in [0, 1, 42] {
            for depth in [26, 27, 101, 1_000_000] {
                let group = family.group("dungeon", seed, depth).unwrap();
                let identity = format!("dungeon/{depth}");
                let mut discovered = package.group_definitions(&identity, seed).unwrap();
                let baseline = geometry.group_definitions(&identity, seed).unwrap();
                for region in baseline.values() {
                    assert_eq!(region.gravity, Some([0, 0, -1]));
                }
                assert!(baseline
                    .values()
                    .all(|region| region.doors.is_empty() && region.traps.is_empty()));
                let trap_count: usize = discovered.values().map(|region| region.traps.len()).sum();
                assert!(
                    (1..=2).contains(&trap_count),
                    "production discovery must place its finite traps"
                );
                for region in discovered.values_mut() {
                    assert!(region
                        .doors
                        .iter()
                        .all(|door| !door.open && door.height == 2 && door.concealment.is_some()));
                    entrances += region.doors.len();
                    // New ceiling anchors are independent physical endpoints, not
                    // rerolled carving/stairs. Assert their clearance before removing
                    // only these deliberate additions from the geometry comparison.
                    let landings: Vec<_> = region
                        .anchors
                        .iter()
                        .filter(|(name, _)| name.starts_with("trapdoor_landing_"))
                        .collect();
                    assert_eq!(landings.len(), 2);
                    for (_, landing) in landings {
                        assert_eq!(landing[2], region.size[2] - 1);
                        assert!((0..region.size[2])
                            .all(|z| !region.walls.contains(&[landing[0], landing[1], z])));
                        assert!(!region.traps.iter().any(|trap| trap.at[..2] == landing[..2]));
                    }
                    region
                        .anchors
                        .retain(|name, _| !name.starts_with("trapdoor_landing_"));
                    assert_eq!(region.gravity, Some([0, 0, -1]));
                    region.doors.clear();
                    region.traps.clear();
                }
                assert_eq!(
                    serde_json::to_value(discovered).unwrap(),
                    serde_json::to_value(baseline).unwrap(),
                    "population and discovery must not reroll geometry at seed {seed}, depth {depth}"
                );
                assert_eq!(group.members.len(), 9);
                assert!(group
                    .members
                    .iter()
                    .flat_map(|region| family.connections("dungeon", seed, *region))
                    .any(|exit| exit.direction == Direction::Down));
            }
        }
        assert!(
            entrances > 0,
            "the production seed matrix must exercise actual secret closures"
        );
    }

    #[test]
    fn rogue_spatial_restraint_uses_valid_protected_condition_lifecycle() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let package = read_package(&root).unwrap();
        let rules = package
            .rules()
            .expect("the actual authored rules are a valid baseline");
        let id = tor_simulation::content_ids::ConditionId::new("rogue.bear_trap").unwrap();
        let definition = rules.condition(&id).unwrap();
        assert_eq!(definition.stacking, tor_simulation::rules::Stacking::Extend);
        assert_eq!(
            definition.preparation,
            tor_simulation::rules::ConditionPreparation::Preserve
        );
        assert!(!definition
            .restrictions
            .contains(&tor_simulation::rules::ConditionRestriction::VoluntaryActions));
        let mut invalid = rules.document().clone();
        invalid
            .conditions
            .iter_mut()
            .find(|condition| condition.id == id)
            .unwrap()
            .stacking = tor_simulation::rules::Stacking::Refresh;
        assert!(matches!(tor_simulation::rules::Ruleset::validate(invalid),
            Err(tor_simulation::rules::RulesetError::Bound { field }) if field == "protected_condition_shape"));
    }

    #[test]
    fn generated_trap_effects_require_declared_slots_and_sleep_conditions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let template = read_package(&root).unwrap();
        Package::from_parts(template.manifest.clone(), vec![], &template.rules_source)
            .unwrap()
            .check()
            .expect("valid production discovery fixture");
        for missing_slot in [true, false] {
            let mut manifest = template.manifest.clone();
            let recipe = manifest.generation_recipes.get_mut("exploration").unwrap();
            let config = recipe
                .stages
                .iter_mut()
                .find_map(|stage| match &mut stage.operation {
                    crate::generation_recipe::Operation::HiddenTraps { config } => Some(config),
                    _ => None,
                })
                .expect("production fixture must exercise configured hidden traps");
            if missing_slot {
                config.rust_slot = "missing_trap_slot".into();
            } else {
                config.sleep_condition = "missing_trap_sleep".into();
            }
            // Rebuild the authored input and its identity together: rejection
            // must reach effect validation, not a stale certificate mismatch.
            let package = Package::from_parts(manifest, vec![], &template.rules_source).unwrap();
            let error = package.check().unwrap_err();
            assert!(
                error.message.contains(
                    "Trap effect refers to an invalid equipment slot, condition, or descriptor"
                ),
                "{error:?}"
            );
        }
    }

    #[test]
    fn compact_initial_entries_are_safe_distinct_and_match_seeded_character_spawns() {
        let package = compact_package(crate::procedural_world::Domain::Open { max_depth: 100 });
        for seed in [0, 1, 2, 42, u64::MAX] {
            let endpoint = package.resolve_endpoint(seed, "caves/1/start").unwrap();
            let owner: u64 = endpoint.split_once('/').unwrap().0.parse().unwrap();
            let definitions = package.group_definitions("caves/1", seed).unwrap();
            let definition = &definitions[&owner];
            let at = definition.anchors["start"];
            assert!(!definition.walls.contains(&at));
            assert!(!definition.walls.contains(&[at[0], at[1], 1]));
            assert!(definition
                .anchors
                .iter()
                .filter(|(name, _)| name.as_str() == "up" || name.as_str() == "down")
                .all(|(_, stair)| stair != &at));
            let group = package.generation_group("caves/1", seed).unwrap();
            let chart = |region: u64, point: [i32; 3]| {
                let slot = group.members.iter().position(|id| *id == region).unwrap() as i32;
                (
                    point[0] + slot % group.grid[0] as i32 * 26,
                    point[1] + slot / group.grid[0] as i32 * 7,
                )
            };
            let mut open = BTreeSet::new();
            let mut targets = BTreeSet::new();
            for (id, definition) in &definitions {
                for x in 0..definition.size[0] {
                    for y in 0..definition.size[1] {
                        if !definition.walls.contains(&[x, y, 0]) {
                            open.insert(chart(*id, [x, y, 0]));
                        }
                    }
                }
                for (name, point) in &definition.anchors {
                    if matches!(name.as_str(), "up" | "down") {
                        targets.insert(chart(*id, *point));
                    }
                }
            }
            let mut pending = vec![chart(owner, at)];
            let mut seen = BTreeSet::new();
            while let Some(point) = pending.pop() {
                if !seen.insert(point) {
                    continue;
                }
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let next = (point.0 + dx, point.1 + dy);
                    if open.contains(&next) && !seen.contains(&next) {
                        pending.push(next);
                    }
                }
            }
            assert!(
                targets.is_subset(&seen),
                "all stair endpoints reachable from initial entry"
            );
            let index = package.index(seed).unwrap();
            assert_eq!(
                index.spawns[&owner]
                    .iter()
                    .find(|(actor, _, _)| *actor == 1)
                    .unwrap()
                    .1,
                loc(owner, at)
            );
            let game = package.build(seed, false).unwrap();
            assert_eq!(
                game.observe(tor_simulation::ActorId(1)).unwrap().location,
                loc(owner, at)
            );
        }
    }

    #[test]
    fn declared_deep_starts_resolve_generated_ports_without_intermediate_floors() {
        for depth in [26, 999_999] {
            let template = compact_package(crate::procedural_world::Domain::Open {
                max_depth: 1_000_000,
            });
            let mut manifest = template.manifest.clone();
            manifest.characters[0].anchor = format!("caves/{depth}/up");
            let package = Package::from_parts(manifest, vec![], &template.rules_source).unwrap();
            package.check().unwrap();
            for seed in [0, 42] {
                let index = package.index(seed).unwrap();
                let endpoint = package
                    .resolve_endpoint(seed, &package.manifest.characters[0].anchor)
                    .unwrap();
                let definitions = package
                    .group_definitions(&format!("caves/{depth}"), seed)
                    .unwrap();
                let expected =
                    resolved_anchors(&definitions.values().collect::<Vec<_>>())[&endpoint];
                assert_eq!(index.homes[&1], expected);
                assert!(index.groups.is_empty());
                assert!(index.group_owners.is_empty());
                assert!(index.anchors.len() <= 32);
                let game = package.build(seed, false).unwrap();
                assert_eq!(
                    game.observe(tor_simulation::ActorId(1)).unwrap().location,
                    expected
                );
                assert_eq!(game.loaded_regions().count(), 4);
                assert!(game.loaded_regions().all(|id| {
                    let (floor, _) = package.manifest.procedural_worlds["caves"]
                        .address(id.0)
                        .unwrap();
                    floor == depth
                }));
            }
        }
    }

    #[test]
    fn declared_starts_reject_invalid_ports_boundaries_and_out_of_domain_roots() {
        let template = compact_package(crate::procedural_world::Domain::Open {
            max_depth: 1_000_000,
        });
        let mut beyond = template.manifest.procedural_worlds["caves"].clone();
        beyond.domain = crate::procedural_world::Domain::Open {
            max_depth: 1_000_001,
        };
        let numeric_beyond = beyond.resolve_port("caves", 0, "1000001", "up").unwrap();
        for anchor in [
            "caves/0/up",
            "caves/1000001/up",
            "caves/26/start",
            "caves/last/up",
            "missing/26/up",
            "caves/26/west",
            "1/west",
            "01/start",
            "1/missing",
            &numeric_beyond,
        ] {
            let mut manifest = template.manifest.clone();
            manifest.characters[0].anchor = anchor.into();
            let package = Package::from_parts(manifest, vec![], &template.rules_source).unwrap();
            let error = package.check().unwrap_err();
            assert!(
                error.message.contains("character 1"),
                "{anchor}: {}",
                error.message
            );
            assert!(package.index(0).is_err(), "{anchor}");
        }
        let mut manifest = template.manifest.clone();
        manifest.procedural_worlds.get_mut("caves").unwrap().domain =
            crate::procedural_world::Domain::Finite { count: 26 };
        manifest.characters[0].anchor = "caves/last/down".into();
        let finite = Package::from_parts(manifest.clone(), vec![], &template.rules_source).unwrap();
        finite.check().unwrap();
        assert_eq!(finite.build(42, false).unwrap().loaded_regions().count(), 4);
        manifest.characters[0].anchor = "caves/27/up".into();
        let outside = Package::from_parts(manifest, vec![], &template.rules_source).unwrap();
        assert!(outside.check().is_err());
    }

    fn give_compact_character_owned_build(manifest: &mut Manifest) {
        let definitions: Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        manifest.creatures = definitions.creatures;
        manifest.characters[0].creature = definitions.characters[0].creature.clone();
    }

    #[test]
    fn declared_start_closures_are_included_in_preflight_validation_budgets() {
        let template = compact_package(crate::procedural_world::Domain::Open {
            max_depth: 1_000_000,
        });
        let mut manifest = template.manifest.clone();
        give_compact_character_owned_build(&mut manifest);
        manifest.ai_profiles.insert(
            "careful".into(),
            AiProfile {
                care: Some(true),
                equipment: Some(true),
                loot: Some(true),
                flee_percent: Some(19),
                ..Default::default()
            },
        );
        manifest.characters = (1..=16)
            .map(|id| {
                let mut character = manifest.characters[0].clone();
                character.id = id;
                character.anchor = format!("caves/{id}/up");
                character.unselected = "ai".into();
                character.ai = Some("careful".into());
                character
            })
            .collect();
        let directory = tempfile::tempdir().unwrap();
        write_package(directory.path(), &manifest, &[], &template.rules_source).unwrap();
        let error = checked_package(directory.path()).unwrap_err();
        assert!(
            error.message.contains("sampled-cell budget"),
            "{}",
            error.message
        );
        assert!(!directory.path().join("validation.json").exists());
        assert!(!directory.path().join("index.json").exists());
    }

    #[test]
    fn declared_spawned_roots_deduplicate_groups_and_omit_unselected_floors() {
        let template = compact_package(crate::procedural_world::Domain::Open {
            max_depth: 1_000_000,
        });
        for companion_depth in [26, 27] {
            let mut manifest = template.manifest.clone();
            give_compact_character_owned_build(&mut manifest);
            manifest.characters[0].anchor = "caves/26/up".into();
            manifest.ai_profiles.insert(
                "careful".into(),
                AiProfile {
                    care: Some(true),
                    equipment: Some(true),
                    loot: Some(true),
                    flee_percent: Some(19),
                    ..Default::default()
                },
            );
            let mut companion = manifest.characters[0].clone();
            companion.id = 2;
            companion.anchor = format!("caves/{companion_depth}/down");
            companion.unselected = "ai".into();
            companion.ai = Some("careful".into());
            manifest.characters.push(companion);
            let mut omitted = manifest.characters[0].clone();
            omitted.id = 3;
            omitted.anchor = "caves/999999/up".into();
            manifest.characters.push(omitted);
            let package = Package::from_parts(manifest, vec![], &template.rules_source).unwrap();
            package.check().unwrap();
            let index = package.index(42).unwrap();
            assert_eq!(index.homes.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
            let floors = BTreeSet::from([26, companion_depth]);
            assert_eq!(
                package
                    .initial_procedural_groups(index.homes.values().map(|at| at.region.0))
                    .len(),
                floors.len()
            );
            let family = &package.manifest.procedural_worlds["caves"];
            assert!(index.anchors.values().all(|at| {
                let (depth, _) = family.address(at.region.0).unwrap();
                depth == 1 || floors.contains(&depth)
            }));
            assert!(index.anchors.len() <= 1 + 18 * floors.len());
            let game = package.build(42, false).unwrap();
            assert_eq!(game.loaded_regions().count(), 4 * floors.len());
            assert_eq!(
                game.loaded_regions()
                    .map(|id| family.address(id.0).unwrap().0)
                    .collect::<BTreeSet<_>>(),
                floors
            );
            for actor in [1, 2] {
                assert_eq!(
                    game.observe(tor_simulation::ActorId(actor))
                        .unwrap()
                        .location,
                    index.homes[&actor]
                );
            }
            assert!(game.observe(tor_simulation::ActorId(3)).is_err());
        }
    }

    #[test]
    fn compact_procedural_world_keeps_descriptor_and_queries_bounded_horizon() {
        let package = compact_package(crate::procedural_world::Domain::Open {
            max_depth: 1_000_000,
        });
        package.check().unwrap();
        assert!(package.index.regions.is_empty());
        let index = package.index(42).unwrap();
        assert!(index.group_owners.is_empty());
        assert!(index.groups.is_empty());
        assert!(index.stairs.is_empty());
        let catalog =
            crate::region_streaming::RegionCatalog::from_package_seed(&package, 42).unwrap();
        let plan = catalog
            .plan(&BTreeSet::from([RegionId(2_000_001)]), 2, &BTreeSet::new())
            .unwrap();
        assert!(plan.required.len() < 32);
        assert!(plan.expanded_regions < 16);
        let first = package
            .build_group(42, &index, RegionId(2_000_001))
            .unwrap();
        let second = package
            .build_group(42, &index, RegionId(2_000_001))
            .unwrap();
        assert_eq!(first.records, second.records);
        assert_eq!(first.records.len(), 4);
        assert!(first.files.is_empty());
    }

    #[test]
    fn authored_portals_to_procedural_families_require_named_stair_pairs() {
        let template = compact_package(crate::procedural_world::Domain::Finite { count: 2 });
        for target in ["caves/2/up", "1/start"] {
            let mut room = (*template.structural_definition(0, 1).unwrap()).clone();
            room.id = 1000;
            room.generate = None;
            room.portals = vec![Portal {
                rotation: None,
                kind: None,
                at: [2, 2, 0],
                direction: "down".into(),
                to: target.into(),
                turns: 0,
                width: 1,
                height: 1,
            }];
            let package = Package::from_parts(
                template.manifest.clone(),
                vec![room],
                &template.rules_source,
            )
            .unwrap();
            let error = package.check().unwrap_err();
            assert!(
                error.message.contains("use stair_pairs"),
                "{}",
                error.message
            );
        }
    }

    #[test]
    fn authored_hub_and_chamber_compose_with_seeded_procedural_ports() {
        let template = compact_package(crate::procedural_world::Domain::Finite { count: 1 });
        let mut manifest = template.manifest.clone();
        manifest.characters[0].anchor = "1000/stair".into();
        manifest.stair_pairs.insert(
            "entry".into(),
            StairPair {
                upper: "1000/stair".into(),
                lower: "caves/1/up".into(),
            },
        );
        manifest.stair_pairs.insert(
            "exit".into(),
            StairPair {
                upper: "caves/last/down".into(),
                lower: "1001/stair".into(),
            },
        );
        let mut room = (*template.structural_definition(0, 1).unwrap()).clone();
        room.id = 1000;
        room.name = "Hub".into();
        room.size = [5, 5, 2];
        room.generate = None;
        room.portals.clear();
        room.anchors = BTreeMap::from([("stair".into(), [2, 2, 0])]);
        let mut chamber = room.clone();
        chamber.id = 1001;
        chamber.name = "Chamber".into();
        let package =
            Package::from_parts(manifest, vec![room, chamber], &template.rules_source).unwrap();
        for seed in [0, 1, 42] {
            let catalog =
                crate::region_streaming::RegionCatalog::from_package_seed(&package, seed).unwrap();
            let entry = catalog.anchor_region("caves/1/up").unwrap();
            assert!(catalog
                .region(RegionId(1000))
                .unwrap()
                .outgoing
                .contains(&entry));
            let mut game = package.build(seed, false).unwrap();
            let actor = tor_simulation::ActorId(1);
            game.act(actor, tor_simulation::Action::Move(Direction::Down))
                .unwrap();
            assert_eq!(game.observe(actor).unwrap().location.region, entry);
            let defs = package.group_definitions("caves/1", seed).unwrap();
            let down = package.resolve_endpoint(seed, "caves/last/down").unwrap();
            let anchors = resolved_anchors(&defs.values().collect::<Vec<_>>());
            game.teleport(actor, anchors[&down]).unwrap();
            game.act(actor, tor_simulation::Action::Move(Direction::Down))
                .unwrap();
            assert_eq!(game.observe(actor).unwrap().location, loc(1001, [2, 2, 0]));
            game.act(actor, tor_simulation::Action::Move(Direction::Up))
                .unwrap();
            assert_eq!(game.observe(actor).unwrap().location, anchors[&down]);
        }
    }

    #[test]
    fn open_world_cap_keeps_structural_continuation_and_bounds_loading() {
        let package = compact_package(crate::procedural_world::Domain::Open { max_depth: 2 });
        let index = package.index(42).unwrap();
        assert!(package.build_group(42, &index, RegionId(5)).is_ok());
        assert!(package.build_group(42, &index, RegionId(9)).is_err());
        let catalog =
            crate::region_streaming::RegionCatalog::from_package_seed(&package, 42).unwrap();
        for radius in [0, 1, 2, 10] {
            let plan = catalog
                .plan(&BTreeSet::from([RegionId(5)]), radius, &BTreeSet::new())
                .unwrap();
            assert!(plan.required.iter().all(|region| region.0 <= 8));
        }
        let outgoing: BTreeSet<_> = (5..=8)
            .flat_map(|id| catalog.region(RegionId(id)).unwrap().outgoing.clone())
            .collect();
        assert!(outgoing.iter().any(|id| id.0 >= 9));
        assert!(catalog.region(RegionId(9)).is_some());
    }

    #[test]
    fn streaming_engine_respects_open_cap_without_prefetch_failure() {
        for max_depth in [1, 2] {
            for radius in [0, 1, 2] {
                let package = compact_package(crate::procedural_world::Domain::Open { max_depth });
                let scenario = Scenario {
                    seed: 42,
                    actors: vec![],
                    regions: 0,
                    workload_version: None,
                    package: Some(Arc::new(package)),
                    streaming: Some(crate::regions::Streaming {
                        active_radius: radius,
                        load_radius: radius,
                    }),
                };
                let engine = crate::Engine::memory(scenario).unwrap();
                assert!(engine.region_counts().unwrap().active <= (max_depth * 4) as usize);
                assert!(engine.observation(tor_protocol::ActorId(1)).is_ok());
            }
        }
    }

    #[test]
    fn procedural_records_are_independent_of_depth_and_member_visit_order() {
        let package =
            compact_package(crate::procedural_world::Domain::SeededFinite { min: 4, max: 7 });
        for seed in [0, 1, 42] {
            let index = package.index(seed).unwrap();
            let count = package.manifest.procedural_worlds["caves"].depth_limit("caves", seed);
            let forward: Vec<_> = (1..=count).collect();
            let reverse: Vec<_> = forward.iter().copied().rev().collect();
            let mut rotated = forward.clone();
            rotated.rotate_left(2);
            let build = |order: &[u32]| -> BTreeMap<RegionId, tor_simulation::RegionRecord> {
                order
                    .iter()
                    .flat_map(|depth| {
                        let owner = 1 + u64::from(*depth - 1) * 4;
                        let mut records = package
                            .build_group(seed, &index, RegionId(owner))
                            .unwrap()
                            .records;
                        if depth % 2 == 0 {
                            records.reverse();
                        }
                        records
                            .into_iter()
                            .map(|(declaration, record)| (declaration.region.id, record))
                    })
                    .collect()
            };
            assert_eq!(build(&forward), build(&reverse));
            assert_eq!(build(&forward), build(&rotated));
        }
    }

    #[test]
    fn seeded_finite_boundaries_and_multiple_families_remain_independent() {
        let mut package =
            compact_package(crate::procedural_world::Domain::SeededFinite { min: 2, max: 7 });
        let mut second = package.manifest.procedural_worlds["caves"].clone();
        second.base_region = 1000;
        package
            .manifest
            .procedural_worlds
            .insert("tower".into(), second);
        let package =
            Package::from_parts(package.manifest.clone(), vec![], &package.rules_source).unwrap();
        package.check().unwrap();
        for seed in [0, 1, 42] {
            for (id, family) in &package.manifest.procedural_worlds {
                let limit = family.depth_limit(id, seed);
                let first: Vec<_> = family
                    .group(id, seed, 1)
                    .unwrap()
                    .members
                    .iter()
                    .flat_map(|region| family.connections(id, seed, *region))
                    .collect();
                assert!(first.iter().all(|exit| exit.direction == Direction::Down));
                let last = family.group(id, seed, limit).unwrap();
                let exits: Vec<_> = last
                    .members
                    .iter()
                    .flat_map(|region| family.connections(id, seed, *region))
                    .collect();
                assert_eq!(exits.len(), 1);
                assert_eq!(exits[0].direction, Direction::Up);
                let down = family.resolve_port(id, seed, "last", "down").unwrap();
                assert!(last
                    .members
                    .iter()
                    .any(|region| down.starts_with(&format!("{region}/"))));
                assert!(family.group(id, seed, limit + 1).is_err());
            }
        }
    }

    #[test]
    fn attempted_traversal_beyond_open_budget_keeps_state_unchanged() {
        let package = compact_package(crate::procedural_world::Domain::Open { max_depth: 1 });
        let mut game = package.build(42, false).unwrap();
        let defs = package.group_definitions("caves/1", 42).unwrap();
        let anchors = resolved_anchors(&defs.values().collect::<Vec<_>>());
        let down = package.resolve_endpoint(42, "caves/1/down").unwrap();
        let actor = tor_simulation::ActorId(1);
        game.teleport(actor, anchors[&down]).unwrap();
        let before = game.clone();
        assert!(game
            .act(actor, tor_simulation::Action::Move(Direction::Down))
            .is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn explicit_connections_cannot_replace_a_procedural_direction() {
        let mut package = compact_package(crate::procedural_world::Domain::Finite { count: 3 });
        package.manifest.stair_pairs.insert(
            "conflict".into(),
            StairPair {
                upper: "caves/1/down".into(),
                lower: "caves/3/up".into(),
            },
        );
        assert!(package.index(42).is_err());
    }

    #[test]
    fn validation_preflights_repeated_authored_volume_before_construction() {
        let template = compact_package(crate::procedural_world::Domain::Finite { count: 1 });
        let mut manifest = template.manifest.clone();
        manifest.characters = (1..=64)
            .map(|id| {
                let mut c = manifest.characters[0].clone();
                c.id = id;
                c
            })
            .collect();
        let mut room = (*template.structural_definition(0, 1).unwrap()).clone();
        room.generate = None;
        room.portals.clear();
        room.anchors.clear();
        room.size = [32, 32, 8];
        let definitions: Vec<_> = (1000..1032)
            .map(|id| {
                let mut def = room.clone();
                def.id = id;
                def
            })
            .collect();
        let temp = tempfile::tempdir().unwrap();
        write_package(temp.path(), &manifest, &definitions, &template.rules_source).unwrap();
        let error = checked_package(temp.path()).unwrap_err();
        assert!(
            error.message.contains("aggregate cell-work budget"),
            "{}",
            error.message
        );
        assert!(!temp.path().join("validation.json").exists());
    }

    #[test]
    fn compact_validation_still_checks_authored_geometry() {
        let template = compact_package(crate::procedural_world::Domain::Finite { count: 3 });
        let mut authored = (*template.structural_definition(0, 1).unwrap()).clone();
        authored.id = 1000;
        authored.generate = None;
        authored.portals.clear();
        authored.anchors.clear();
        authored.walls = vec![[i32::MAX, 0, 0]];
        let temp = tempfile::tempdir().unwrap();
        write_package(
            temp.path(),
            &template.manifest,
            &[authored],
            &template.rules_source,
        )
        .unwrap();
        assert!(checked_package(temp.path()).is_err());
        assert!(!temp.path().join("validation.json").exists());
    }

    #[test]
    fn compact_world_starts_streaming_and_persists_without_source_files() {
        let package =
            compact_package(crate::procedural_world::Domain::SeededFinite { min: 50, max: 200 });
        let scenario = Scenario {
            seed: 42,
            actors: vec![],
            regions: 0,
            workload_version: None,
            package: Some(Arc::new(package)),
            streaming: Some(crate::regions::Streaming {
                active_radius: 0,
                load_radius: 0,
            }),
        };
        let temp = tempfile::tempdir().unwrap();
        let save = temp.path().join("compact.db");
        let engine = crate::Engine::open(&save, scenario).unwrap();
        let counts = engine.region_counts().unwrap();
        assert!(counts.active <= 4);
        assert!(counts.active + counts.frozen <= 4);
        drop(engine);
        let restored = crate::Engine::open(&save, Scenario::two_room(0)).unwrap();
        assert_eq!(restored.region_counts().unwrap(), counts);
    }

    fn stairs_package() -> Package {
        read_package(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/paired-stairs"),
        )
        .unwrap()
    }

    #[test]
    fn paired_stairs_resolve_identically_in_either_build_order() {
        let package = stairs_package();
        for seed in [0, 1, 42, u64::MAX] {
            let forward = package.index(seed).unwrap();
            let (upper, files) = package.build_region(seed, &forward, 1).unwrap();
            assert_eq!(files, BTreeSet::from([1, 2]));
            let (lower, _) = package.build_region(seed, &forward, 2).unwrap();
            let reverse = package.index(seed).unwrap();
            assert_eq!(package.build_region(seed, &reverse, 2).unwrap().0, lower);
            assert_eq!(package.build_region(seed, &reverse, 1).unwrap().0, upper);
        }
    }

    #[test]
    fn paired_stairs_allow_same_region_and_both_directions_on_one_cell() {
        let package = stairs_package();
        let mut game = package.build(42, false).unwrap();
        let actor = tor_simulation::ActorId(1);
        game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
        let before = game.observe(actor).unwrap();
        assert!(before.exits.iter().any(|e| e.direction == Direction::Up));
        assert!(before.exits.iter().any(|e| e.direction == Direction::Down));
        game.act(actor, tor_simulation::Action::Move(Direction::Down))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 3, 0]));
        game.act(actor, tor_simulation::Action::Move(Direction::Up))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, before.location);
        assert_eq!(game.tick(), 200);
    }

    #[test]
    fn paired_stairs_preserve_rotated_facing_and_existing_momentum() {
        let mut game = stairs_package().build(42, false).unwrap();
        let actor = tor_simulation::ActorId(1);
        // Enter the stair from an existing rotated link. The pair must keep
        // this facing rather than reset it to the destination region's axes.
        game.connect(
            Passage {
                from: loc(1, [1, 1, 0]),
                direction: Direction::Up,
                to: loc(3, [1, 1, 0]),
            },
            1,
        )
        .unwrap();
        game.act(actor, tor_simulation::Action::Move(Direction::Up))
            .unwrap();
        let frame = |game: &Game| {
            let observation = game.observe(actor).unwrap();
            observation
                .visible_cells
                .iter()
                .find(|cell| cell.location == observation.location)
                .unwrap()
                .frame
        };
        assert_eq!(frame(&game), 1);
        game.set_actor_velocity(actor, [1, 2, 0]).unwrap();
        game.act(actor, tor_simulation::Action::Move(Direction::Down))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 3, 0]));
        assert_eq!(frame(&game), 1);
        assert_eq!(game.actor_motion(actor).unwrap().velocity, [1, 2, 0]);
        game.act(actor, tor_simulation::Action::Move(Direction::East))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 4, 0]));
    }

    #[test]
    fn paired_stair_blocked_arrival_checks_entire_body_without_consuming_time() {
        let mut game = stairs_package().build(42, false).unwrap();
        let actor = tor_simulation::ActorId(1);
        game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
        game.set_body(
            actor,
            tor_simulation::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                eye: [0; 3],
                mass: 80,
            },
        )
        .unwrap();
        game.set_wall(loc(3, [4, 3, 0]), true).unwrap();
        let before = game.clone();
        assert!(game
            .act(actor, tor_simulation::Action::Move(Direction::Down))
            .is_err());
        assert_eq!(game, before);
        game.set_wall(loc(3, [4, 3, 0]), false).unwrap();
        game.act(actor, tor_simulation::Action::Move(Direction::Down))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 3, 0]));
    }

    #[test]
    fn stair_pairs_reject_conflicting_exits_at_either_endpoint() {
        let template = stairs_package();
        for (upper, lower) in [("1/stair", "3/return"), ("3/return", "2/up")] {
            let mut manifest = template.manifest.clone();
            manifest.stair_pairs.insert(
                "conflict".into(),
                StairPair {
                    upper: upper.into(),
                    lower: lower.into(),
                },
            );
            let temp = tempfile::tempdir().unwrap();
            let definitions = template.region_defs().unwrap();
            write_package(temp.path(), &manifest, &definitions, &template.rules_source).unwrap();
            let package = read_package(temp.path()).unwrap();
            assert!(package.build(42, false).is_err());
        }
    }

    #[test]
    fn paired_stair_occupied_arrival_is_atomic() {
        let mut game = stairs_package().build(42, false).unwrap();
        let actor = tor_simulation::ActorId(1);
        game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
        game.spawn_actor(loc(3, [3, 3, 0]), NonZeroU64::new(100).unwrap())
            .unwrap();
        let before = game.clone();
        assert!(game
            .act(actor, tor_simulation::Action::Move(Direction::Down))
            .is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn stair_pairs_reject_missing_endpoints_with_source_coordinates() {
        let mut package = stairs_package();
        package.manifest.stair_pairs.get_mut("first").unwrap().lower = "2/missing".into();
        let error = package.index(42).unwrap_err();
        assert!(error.message.contains("2/missing"), "{error}");
        // Changed source must not invent coordinates for the replacement name.
        assert!(error.message.contains("scenario.toml"), "{error}");
        assert!(error.message.contains("stair"), "{error}");
    }

    #[test]
    fn stair_pair_horizon_uses_generated_names_without_coordinates() {
        let package = stairs_package();
        let catalog = crate::region_streaming::RegionCatalog::from_package(&package).unwrap();
        assert_eq!(catalog.anchor_region("2/up").unwrap(), RegionId(2));
        assert!(catalog.resolve_anchor("2/up").is_err());
        let ids = |values: &[u64]| values.iter().copied().map(RegionId).collect();
        assert_eq!(
            catalog.plan(&ids(&[1]), 1, &ids(&[])).unwrap().required,
            ids(&[1, 2])
        );
        assert_eq!(
            catalog.plan(&ids(&[3]), 1, &ids(&[])).unwrap().required,
            ids(&[2, 3])
        );
    }
    #[test]
    fn shared_item_identity_requires_one_projectile_profile_in_source_and_restore() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let projectile = serde_json::json!({
            "family":"fixture.bow", "recovery":"recoverable",
            "damage":{"components":[{"category":"keen","descriptor":null,"amount":{"type":"fixed","value":3}}],"primary":{"category":"keen","descriptor":null}}
        });
        let archetype = serde_json::json!({
            "name":"shared arrow", "identity":"fixture.shared_arrow", "class":"weapon", "stackable":true,
            "projectile":projectile
        });
        let mut manifest = template.manifest.clone();
        for key in ["fixture.arrow_a", "fixture.arrow_b"] {
            manifest.archetypes.insert(
                key.into(),
                serde_json::from_value(archetype.clone()).unwrap(),
            );
        }
        let make = |manifest| {
            Package::from_parts(
                manifest,
                template.region_defs().unwrap(),
                &template.rules_source,
            )
            .unwrap()
        };
        let valid = make(manifest.clone());
        valid.appearance_mapping(42).unwrap();
        valid.check_identity().unwrap();
        let encode = crate::storage::schema::scenario::encode_content_package;
        let decode = crate::storage::schema::scenario::decode_content_package;
        let restored = decode(&encode(&valid).unwrap()).unwrap();
        assert!(restored.directory.is_none());
        restored.check_identity().unwrap();
        for changed in ["family", "damage", "recovery", "presence"] {
            let mut manifest = manifest.clone();
            let item = manifest.archetypes.get_mut("fixture.arrow_b").unwrap();
            let mut profile = projectile.clone();
            match changed {
                "family" => profile["family"] = serde_json::json!("fixture.sling"),
                "damage" => {
                    profile["damage"]["components"][0]["amount"]["value"] = serde_json::json!(4)
                }
                "recovery" => profile["recovery"] = serde_json::json!("consumed_on_hit"),
                "presence" => {}
                _ => unreachable!(),
            }
            item.projectile =
                (changed != "presence").then(|| serde_json::from_value(profile).unwrap());
            // Rebuild the honest hash/certificate around the mismatch. Rejection
            // must come from the shared semantic invariant, not stale hashes.
            let invalid = make(manifest);
            for failure in [
                invalid.appearance_mapping(42).unwrap_err(),
                invalid.check_identity().unwrap_err(),
                decode(&encode(&invalid).unwrap()).unwrap_err(),
            ] {
                assert!(
                    failure.message.contains("One identity"),
                    "{changed}: {failure}"
                );
                assert!(
                    failure.message.contains("projectile profile"),
                    "{changed}: {failure}"
                );
            }
        }
    }

    #[test]
    fn appearance_pools_cannot_encode_hidden_identity_through_physical_class() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let mut package = super::read_package(&path).unwrap();
        package.manifest.archetypes.get_mut("poison").unwrap().class = super::ItemClass::Weapon;
        assert!(package.appearance_mapping(42).is_err());
        package.manifest.archetypes.get_mut("poison").unwrap().class = super::ItemClass::Potion;
        assert!(package.appearance_mapping(42).is_ok());
    }

    #[test]
    fn readable_capability_is_class_independent_and_concealed_affordances_must_match() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let mut package = read_package(&path).unwrap();
        let readable = ReadableSpec {
            control: None,
            descriptors: vec![],
            effects: vec![],
            identify: Some(vec![ItemClass::Potion]),
            discovery: ItemDiscovery::OnUse,
        };
        package
            .manifest
            .archetypes
            .get_mut("healing")
            .unwrap()
            .readable = Some(readable.clone());
        // Both identities look alike, so one must not expose a unique Read action.
        assert!(package.appearance_mapping(42).is_err());
        package
            .manifest
            .archetypes
            .get_mut("poison")
            .unwrap()
            .readable = Some(readable);
        assert!(package.appearance_mapping(42).is_ok());
        // Potion is a physical label here; the explicit capability grants Read.
        package.manifest.archetypes.get_mut("poison").unwrap().class = ItemClass::Tool;
        package
            .manifest
            .archetypes
            .get_mut("healing")
            .unwrap()
            .class = ItemClass::Tool;
        assert!(package.appearance_mapping(42).is_ok());
        package
            .manifest
            .archetypes
            .get_mut("healing")
            .unwrap()
            .readable
            .as_mut()
            .unwrap()
            .identify = None;
        assert!(package.appearance_mapping(42).is_err());
    }

    #[test]
    fn appearance_pools_share_affordances_while_effects_remain_distinct() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let mut package = read_package(&path).unwrap();
        package
            .manifest
            .archetypes
            .get_mut("healing")
            .unwrap()
            .consumable = Some(ConsumableSpec {
            descriptors: vec![],
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::Heal {
                amount: AmountExpr::Fixed { amount: 5 },
            }],
        });
        assert!(package.appearance_mapping(42).is_err());
        package
            .manifest
            .archetypes
            .get_mut("poison")
            .unwrap()
            .consumable = Some(ConsumableSpec {
            descriptors: vec![],
            discovery: ItemDiscovery::SelfEffect,
            effects: vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: DamageType::Vital,
                        descriptor: None,
                        amount: AmountExpr::Fixed { amount: 8 },
                    }],
                },
            }],
        });
        assert!(package.appearance_mapping(42).is_ok());
    }
    use super::*;

    #[test]
    fn creature_diagnostic_source_is_acquired_only_for_catalog_failure() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
        let package = read_package(&root).unwrap();
        let mut manifest = package.manifest.clone();
        PreparedDefinitions::new_with_source(&manifest, 1, package.rules().unwrap(), || {
            panic!("valid build must not acquire source")
        })
        .unwrap();
        manifest.characters[0].creature.as_mut().unwrap().faction = "missing".into();
        let mut invalid = manifest.clone();
        invalid.characters[0]
            .creature
            .as_mut()
            .unwrap()
            .hit_dice
            .clear();
        let failure =
            PreparedDefinitions::new_with_source(&invalid, 1, package.rules().unwrap(), || {
                panic!("choice failure precedes source acquisition")
            })
            .unwrap_err();
        assert!(failure.message.contains("require 1 to 256 hit dice"));
        let reads = std::cell::Cell::new(0);
        let failure =
            PreparedDefinitions::new_with_source(&manifest, 1, package.rules().unwrap(), || {
                reads.set(reads.get() + 1);
                None
            })
            .unwrap_err();
        assert_eq!(reads.get(), 1);
        assert!(failure.message.contains("scenario.toml: character 1"));
        assert!(failure.message.contains("Unknown creature faction"));
        manifest.factions.clear();
        PreparedDefinitions::new_with_source(&manifest, 1, package.rules().unwrap(), || {
            panic!("empty faction catalog permits valid builds")
        })
        .unwrap();
    }

    #[test]
    fn stored_scenario_encoding_borrows_definitions_without_region_acquisition() {
        #[derive(Serialize)]
        struct StoredScenario<'a>(#[serde(with = "crate::storage::schema::scenario")] &'a Scenario);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon");
        let scenario = load(&root, 42, None, false).unwrap();
        let package = scenario.package.as_ref().unwrap();
        let reads = package.sources.files_read();
        ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.set(0));
        let saved = serde_json::to_value(StoredScenario(&scenario)).unwrap();
        assert_eq!(ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.get()), 0);
        assert_eq!(package.sources.files_read(), reads);
        let metadata = saved["package"].as_object().unwrap();
        assert_eq!(
            metadata.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "certificate",
                "directory",
                "manifest",
                "rules_document",
                "selected",
                "validated"
            ]
        );
    }

    #[test]
    fn missing_identity_name_does_not_blame_a_valid_appearance_pool_reference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let mut package = read_package(&root).unwrap();
        package.manifest.archetypes.get_mut("healing").unwrap().name = None;
        let error = package.appearance_mapping(0).unwrap_err();
        assert_eq!(
            error.message,
            "scenario.toml: archetype \"healing\": Missing identity name for appearance pool"
        );
    }
    #[test]
    fn generated_regions_declare_authored_character_identities_before_lazy_start() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
        let template = read_package(&root).unwrap();
        let mut manifest = template.manifest.clone();
        manifest
            .characters
            .iter_mut()
            .find(|c| c.id == 1)
            .unwrap()
            .anchor = "2/west".into();
        let package = Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap();
        let index = package.index(42).unwrap();
        let declared = package.unbuilt_region(&index, 2).unwrap();
        assert!(declared
            .identities
            .actors
            .contains(&tor_simulation::ActorId(1)));
        package.start(42).unwrap();
    }

    #[test]
    fn streamed_save_uses_pinned_rules_when_general_import_changes() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
        let mut scenario = load(&root, 42, None, true).unwrap();
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let package = Arc::make_mut(scenario.package.as_mut().unwrap());
        // Simulate a saved prior general-rules revision, while authored imports,
        // manifest and lazy region sources still match the directory exactly.
        let mut prior = package.rules().unwrap().document().clone();
        prior.constants.base_defense += 1;
        package.rules_document =
            crate::storage::schema::scenario::encode_rules_document(&prior).unwrap();
        package.certificate.ruleset_hash = digest(package.rules_document.as_bytes());
        package
            .certificate
            .ruleset_dependencies
            .insert(RULESET.into(), source_digest("prior shared rules revision"));
        package.certificate.model_hash = model_hash(
            &package.manifest,
            &package.index,
            &package.certificate.ruleset_hash,
            &package.certificate.ruleset_dependencies,
        )
        .unwrap();
        package.certificate.content_hash = digest(
            &serde_json::to_vec(&(
                &package.certificate.files,
                &package.certificate.ruleset_dependencies,
            ))
            .unwrap(),
        );
        let directory = tempfile::tempdir().unwrap();
        artifact::publish(directory.path(), package).unwrap();
        package.directory = Some(directory.path().display().to_string());
        let save = directory.path().join("streamed.db");
        let engine = crate::Engine::open(&save, scenario).unwrap();
        assert!(engine.region_counts().unwrap().unbuilt > 0);
        engine.flush().unwrap();
        drop(engine);
        let mut restored = crate::Engine::open(&save, Scenario::two_room(0)).unwrap();
        // Continue far enough to demand a source that had never been copied.
        for step in 0..76 {
            let actor = tor_protocol::ActorId(1);
            let revision = restored.revision(actor).unwrap();
            let branch = restored.branch().clone();
            restored
                .command(
                    "player",
                    "test",
                    actor,
                    &format!("pinned-{step}"),
                    &branch,
                    crate::journal::Command::Act {
                        expected_revision: revision,
                        action: crate::journal::Action::Move {
                            direction: crate::journal::Direction::East,
                        },
                    },
                )
                .unwrap();
        }
        assert!(restored.region_counts().unwrap().detached > 0);
    }

    #[test]
    fn shared_rules_import_accepts_validated_scenario_override() {
        let source = r#"import = "interactions-v56"
[[overrides]]
op = "set"
path = ["constants", "base_defense"]
value = 13
"#;
        let rules = parse_rules(source, "rules.toml").unwrap();
        assert_eq!(rules.constants().base_defense, 13);
    }

    #[test]
    fn shared_rules_and_authored_overrides_are_pinned_by_the_certificate() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let directory = tempfile::tempdir().unwrap();
        write_package(
            directory.path(),
            &template.manifest,
            &template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap();
        let certificate = validate(directory.path()).unwrap();
        assert_eq!(certificate.ruleset, RULESET);
        assert_eq!(certificate.ruleset_dependencies.len(), 1);
        assert_eq!(
            certificate.ruleset_dependencies[RULESET],
            source_digest(tor_simulation::rules::DEFAULT_RULESET_TOML)
        );
        assert_ne!(
            certificate.files[&template.manifest.ruleset],
            certificate.ruleset_hash
        );
        let path = directory.path().join(&template.manifest.ruleset);
        std::fs::write(
            &path,
            format!("{}\n# tuning source changed\n", template.rules_source),
        )
        .unwrap();
        assert!(load(directory.path(), 42, None, false).is_err());
        let changed = read_package(directory.path()).unwrap();
        // Comments alter authored content identity, not the effective rules.
        assert_eq!(changed.certificate.ruleset_hash, certificate.ruleset_hash);
        assert_eq!(changed.certificate.model_hash, certificate.model_hash);
        assert_ne!(changed.certificate.content_hash, certificate.content_hash);
        std::fs::remove_file(path).unwrap();
        assert!(read_package(directory.path()).is_err());
    }

    #[test]
    fn bundled_rules_paths_cannot_escape_or_replace_scenario_inputs() {
        for path in [
            "../rules.toml",
            "/rules.toml",
            "scenario.toml",
            "regions/1.toml",
            "rules.json",
        ] {
            assert!(!valid_rules_path(path), "{path}");
        }
        assert!(valid_rules_path("rules.toml"));
        assert!(valid_rules_path("rules/combat.toml"));
    }

    #[test]
    fn validation_and_loading_ignore_crlf_source_line_endings() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let directory = tempfile::tempdir().unwrap();
        write_package(
            directory.path(),
            &template.manifest,
            &template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap();
        let paths: Vec<_> = std::iter::once(directory.path().join("scenario.toml"))
            .chain(std::iter::once(
                directory.path().join(&template.manifest.ruleset),
            ))
            .chain(
                template
                    .index
                    .regions
                    .iter()
                    .map(|region| directory.path().join(&region.file)),
            )
            .collect();
        let lf: Vec<_> = paths
            .iter()
            .map(|path| {
                let text = std::fs::read_to_string(path).unwrap();
                assert!(!text.contains('\r'), "generated package source must use LF");
                text
            })
            .collect();
        for (path, text) in paths.iter().zip(&lf) {
            std::fs::write(path, text.replace('\n', "\r\n")).unwrap();
        }
        let crlf_certificate = validate(directory.path()).unwrap();
        assert_eq!(
            crlf_certificate.files["scenario.toml"],
            digest(lf[0].as_bytes())
        );
        for (path, text) in paths.iter().zip(&lf) {
            std::fs::write(path, text).unwrap();
        }
        let lf_certificate = validate(directory.path()).unwrap();
        assert_eq!(crlf_certificate, lf_certificate);
        let expected = load(directory.path(), 42, None, false)
            .unwrap()
            .package
            .unwrap()
            .build(42, false)
            .unwrap();
        for (path, text) in paths.iter().zip(&lf) {
            std::fs::write(path, text.replace('\n', "\r\n")).unwrap();
        }
        assert_eq!(
            load(directory.path(), 42, None, false)
                .unwrap()
                .package
                .unwrap()
                .build(42, false)
                .unwrap(),
            expected
        );
    }

    #[test]
    fn generated_region_comments_change_integrity_hash_without_changing_content() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
        let template = read_package(&root).unwrap();
        let directory = tempfile::tempdir().unwrap();
        write_package(
            directory.path(),
            &template.manifest,
            &template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap();
        let before = read_package(directory.path()).unwrap();
        let file = directory.path().join("regions/2.toml");
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(
            file,
            format!("# Author notes must not reroll the cave.\n{text}\n"),
        )
        .unwrap();
        let after = read_package(directory.path()).unwrap();
        assert_ne!(
            before.index.region(2).unwrap().hash,
            after.index.region(2).unwrap().hash
        );
        for seed in [0, 1, 42, u64::MAX] {
            let original = before.index(seed).unwrap().region(&before, 2).unwrap();
            let annotated = after.index(seed).unwrap().region(&after, 2).unwrap();
            assert_eq!(
                serde_json::to_value(&*original).unwrap(),
                serde_json::to_value(&*annotated).unwrap(),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn construction_reference_errors_identify_source_without_reordering_failures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let mut manifest = template.manifest.clone();
        let mut regions = template.region_defs().unwrap();
        let definitions: Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        manifest.creatures = definitions.creatures;
        manifest
            .creatures
            .species
            .get_mut("figure")
            .unwrap()
            .modifiers = vec![crate::creature_authoring::Modifier {
            cancellable: false,
            scope: crate::creature_authoring::Scope::Always,
            op: crate::creature_authoring::Op::Add {
                stat: crate::creature_authoring::Stat::Health,
                value: 33,
            },
        }];
        let creature = definitions.characters[0].creature.clone().unwrap();
        let mut actor: Actor = serde_json::from_value(serde_json::json!({
            "id": 2, "at": [3, 1, 0], "controller": "ai", "ai": "missing",
            "creature": creature,
            "body": {"cells": [[0, 0, 0]], "eye": [1, 0, 0], "mass": 91}
        }))
        .unwrap();
        regions[0].actors.push(actor.clone());
        let package =
            Package::from_parts(manifest.clone(), regions.clone(), &template.rules_source).unwrap();
        let failure = package.build(42, true).unwrap_err();
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
        assert!(failure.message.starts_with("regions/1.toml:"));
        assert!(failure
            .message
            .ends_with(": region 1, actor 2: Unknown actor AI profile \"missing\""));
        manifest
            .ai_profiles
            .insert("careful".into(), AiProfile::general());
        actor.ai = Some("careful".into());
        *regions[0].actors.last_mut().unwrap() = actor;
        let package =
            Package::from_parts(manifest.clone(), regions.clone(), &template.rules_source).unwrap();
        let failure = package.build(42, true).unwrap_err();
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
        assert_eq!(
            failure.message,
            "regions/1.toml: region 1, actor 2: Actor body eye must be one of its cells"
        );

        // A valid non-default body must precede capability acquisition, not
        // retire the actor's one-lifetime readied preparation during setup.
        let body = BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 91,
        };
        regions[0].actors.last_mut().unwrap().body = Some(body.clone());
        for value in [
            crate::creature_authoring::Perception::ObjectGuise {
                appearance: Some(crate::creature_authoring::GuiseAppearance {
                    name: "fixture object".into(),
                    description: String::new(),
                    class: ItemClass::Tool,
                    asset: None,
                }),
                skill: "stealth".into(),
                edge: 0,
            },
            crate::creature_authoring::Perception::ReadiedTouchAmbush { enabled: true },
        ] {
            manifest
                .creatures
                .species
                .get_mut("figure")
                .unwrap()
                .modifiers
                .push(crate::creature_authoring::Modifier {
                    cancellable: false,
                    scope: crate::creature_authoring::Scope::Always,
                    op: crate::creature_authoring::Op::Perception { value },
                });
        }
        let package = Package::from_parts(manifest, regions, &template.rules_source).unwrap();
        let game = package.build(42, true).unwrap();
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let pool = serde_json::to_value(&snapshot).unwrap()["actors"]
            .as_u64()
            .unwrap() as usize;
        let recorded = serde_json::to_value(&shared).unwrap();
        let ready = &recorded["actors"][pool]["2"]["readied_ambush"];
        assert_eq!(ready["state"], "ready");
        assert_eq!(ready["body"], serde_json::to_value(body).unwrap());
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
            game
        );
    }

    #[test]
    fn declaration_diagnostics_identify_source_and_preserve_precedence() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut package = read_package(&root).unwrap();
        package
            .manifest
            .factions
            .insert("guard".into(), BTreeSet::from(["missing".into()]));
        package.manifest.ai_profiles.insert(
            "guard".into(),
            AiProfile {
                flee_percent: Some(101),
                ..Default::default()
            },
        );
        for expected in [
            r#"scenario.toml: faction "guard": Invalid faction relationship"#,
            r#"scenario.toml: AI profile "guard": Invalid AI profile"#,
        ] {
            let failure = package.supported().unwrap_err();
            assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
            assert_eq!(failure.message, expected);
            if expected.contains("faction relationship") {
                package.manifest.factions.clear();
            } else {
                package.manifest.ai_profiles.clear();
            }
        }
        package.supported().unwrap();
        let definitions: Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        package.manifest.creatures = definitions.creatures;
        let mut invalid = definitions.characters[0].creature.clone().unwrap();
        invalid.hit_dice.clear();
        package.manifest.characters[0].creature = Some(invalid.clone());
        package
            .manifest
            .archetypes
            .get_mut("token")
            .unwrap()
            .creature = Some(invalid.clone());
        for origin in ["character 1", r#"archetype "token""#] {
            let failure = package.index(42).unwrap_err();
            assert!(failure.message.contains(origin), "{}", failure.message);
            assert!(failure.message.contains("require 1 to 256 hit dice"));
            assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
            if origin == "character 1" {
                package.manifest.characters[0].creature = None;
            } else {
                package
                    .manifest
                    .archetypes
                    .get_mut("token")
                    .unwrap()
                    .creature = None;
            }
        }
        let mut regions = package.region_defs().unwrap();
        let mut actor: Actor = serde_json::from_value(serde_json::json!({
            "id": 9, "at": [2, 1, 0], "controller": "invalid", "creature": invalid
        }))
        .unwrap();
        regions[0].actors.push(actor.clone());
        let failure = package.check_region(&regions[0]).unwrap_err();
        assert_eq!(
            failure.message,
            "regions/1.toml: region 1, actor 9: Invalid actor controller"
        );
        actor.controller = "external".into();
        *regions[0].actors.last_mut().unwrap() = actor;
        let failure = Package::from_parts(package.manifest.clone(), regions, &package.rules_source)
            .and_then(|package| package.build(42, true))
            .unwrap_err();
        assert!(
            failure
                .message
                .contains("regions/1.toml: region 1, actor 9"),
            "{}",
            failure.message
        );
        assert!(failure.message.contains("require 1 to 256 hit dice"));
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
    }

    #[test]
    fn compiled_instances_preserve_inheritance_and_explicit_overrides() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let mut manifest = template.manifest.clone();
        let mut regions = template.region_defs().unwrap();
        let definitions: Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        let mut guard_species = definitions.creatures.species["figure"].clone();
        guard_species.melee = serde_json::from_value(serde_json::json!({"skill":"heavy_weaponry","bonus":-7,"wind_up":19,"recovery":23,"strikes":[{"offset":0,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","amount":{"kind":"fixed","amount":2}},{"category":"energy","amount":{"kind":"fixed","amount":1}},{"category":"keen","amount":{"kind":"fixed","amount":3}},{"category":"spirit","amount":{"kind":"fixed","amount":4}},{"category":"vital","amount":{"kind":"fixed","amount":5}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]}))
        .unwrap();
        guard_species.modifiers = serde_json::from_value(serde_json::json!([
            {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 33}}, {"scope": "always", "op": {"kind": "add", "stat": {"kind": "defense", "defense": "physical"}, "value": -13}},
            {"scope": "always", "op": {"kind": "immunity", "selector": {"kind": "category", "category": "energy"}}},
            {"scope": "always", "op": {"kind": "immunity", "selector": {"kind": "category", "category": "vital"}}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "impact"}, "amount": 6}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "keen"}, "amount": 7}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "spirit"}, "amount": 8}}
        ])).unwrap();
        let mut override_species = definitions.creatures.species["figure"].clone();
        override_species.modifiers = vec![crate::creature_authoring::Modifier {
            cancellable: false,
            scope: crate::creature_authoring::Scope::Always,
            op: crate::creature_authoring::Op::Add {
                stat: crate::creature_authoring::Stat::Health,
                value: 49,
            },
        }];
        manifest
            .creatures
            .species
            .insert("guard".into(), guard_species);
        manifest
            .creatures
            .species
            .insert("override".into(), override_species);
        let inherited_creature: crate::creature_authoring::BuildSpec = serde_json::from_value(serde_json::json!({
            "species": "guard", "name": "guard", "faction": "neutral", "binding": "intellect", "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"}]
        })).unwrap();
        let override_creature: crate::creature_authoring::BuildSpec = serde_json::from_value(serde_json::json!({
            "species": "override", "name": "override", "faction": "neutral", "binding": "intellect", "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"}]
        })).unwrap();
        let inherited_body = BodySpec {
            cells: vec![[0, 0, 0]],
            eye: [0, 0, 0],
            mass: 91,
        };
        let override_body = BodySpec {
            mass: 137,
            ..inherited_body.clone()
        };
        manifest.archetypes.insert(
            "guard".into(),
            Archetype {
                creature: Some(inherited_creature.clone()),
                body: Some(inherited_body.clone()),
                identity: Some("guard-item".into()),
                name: Some("guard token".into()),
                stackable: true,
                properties: BTreeMap::from([
                    ("quality".into(), "fine".into()),
                    ("origin".into(), "authored".into()),
                ]),
                turn_ticks: Some(73),
                asset: Some("creature.guard".into()),
                ..Default::default()
            },
        );
        manifest
            .assets
            .insert("stone".into(), vec!["creature.guard".into()]);
        manifest.ai_profiles.insert(
            "careful".into(),
            AiProfile {
                care: Some(true),
                equipment: Some(true),
                loot: Some(true),
                flee_percent: Some(19),
                ..Default::default()
            },
        );
        regions[0].actors.extend([
            Actor {
                creature: None,
                anatomy: None,
                known_identities: vec![],
                id: 2,
                at: [3, 1, 0],
                archetype: Some("guard".into()),
                body: None,
                turn_ticks: None,
                controller: "ai".into(),
                ai: Some("careful".into()),
                velocity: None,
            },
            Actor {
                creature: Some(override_creature.clone()),
                anatomy: None,
                known_identities: vec![],
                id: 3,
                at: [4, 1, 0],
                archetype: Some("guard".into()),
                body: Some(override_body.clone()),
                turn_ticks: Some(89),
                controller: "external".into(),
                ai: None,
                velocity: Some([1, 0, 0]),
            },
        ]);
        regions[0].items.extend([
            Item {
                equipped_slot: None,
                class: None,
                id: 3,
                at: [1, 1, 0],
                archetype: Some("guard".into()),
                name: None,
                quantity: 3,
                stackable: None,
                properties: BTreeMap::new(),
                carried_by: None,
                seed_names: vec![],
            },
            Item {
                equipped_slot: None,
                class: None,
                id: 4,
                at: [1, 1, 0],
                archetype: Some("guard".into()),
                name: Some("named gift".into()),
                quantity: 1,
                stackable: Some(false),
                properties: BTreeMap::from([("quality".into(), "ordinary".into())]),
                carried_by: Some(1),
                seed_names: vec![],
            },
        ]);
        let catalog = manifest
            .creatures
            .compile(template.rules().unwrap())
            .unwrap();
        let package = Package::from_parts(manifest, regions, &template.rules_source).unwrap();
        let game = package.build(42, true).unwrap();
        // Expected explicit recipes share the actual immutable package palette.
        // Keep the full build comparison, including Content; a native neutral
        // catalog is not this authored package's presentation catalog.
        let catalog =
            catalog.with_presentation_catalog(game.content().presentation_catalog().clone());
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
        let shared = serde_json::to_value(shared).unwrap();
        for (id, recipe, body, ticks) in [
            ("2", inherited_creature, inherited_body, 73),
            ("3", override_creature, override_body, 89),
        ] {
            let actor = &shared["actors"][snapshot["actors"].as_u64().unwrap() as usize][id];
            let id_number = id.parse::<u64>().unwrap();
            let expected = catalog
                .prepare(&recipe)
                .unwrap()
                .instantiate(crate::creature_authoring::actor_health_seed(42, id_number))
                .unwrap();
            let actual = game.creature(tor_simulation::ActorId(id_number)).unwrap();
            assert_eq!(actual.build(), &expected);
            assert_eq!(actual.derived(), &expected.derive().unwrap());
            assert_eq!(
                game.inspect_creature(tor_simulation::ActorId(id_number))
                    .unwrap()
                    .identity
                    .name,
                recipe.name
            );
            assert_eq!(actor["body"], serde_json::to_value(body).unwrap());
            assert_eq!(actor["turn_ticks"], ticks);
            assert_eq!(actor["asset"], "creature.guard");
        }
        assert_eq!(
            snapshot["combat"]["ai"]["2"]["profile"],
            serde_json::json!({"idle": "explore", "doors": false,
                "care": true, "equipment": true, "loot": true, "gold": false, "flee_percent": 19})
        );
        assert_eq!(
            shared["actors"][snapshot["actors"].as_u64().unwrap() as usize]["3"]["motion"]
                ["velocity"],
            serde_json::json!([1, 0, 0])
        );
        let items = &shared["items"][snapshot["items"].as_u64().unwrap() as usize];
        assert_eq!(items["3"]["quantity"], 3);
        assert_eq!(items["3"]["spec"]["name"], "guard token");
        assert_eq!(items["3"]["spec"]["stackable"], true);
        assert_eq!(items["4"]["spec"]["name"], "named gift");
        assert_eq!(items["4"]["spec"]["stackable"], false);
        assert_eq!(
            items["4"]["spec"]["properties"],
            serde_json::json!({"quality": "ordinary", "origin": "authored"})
        );
        for id in ["3", "4"] {
            assert_eq!(items[id]["spec"]["identity"], "guard-item");
            assert_eq!(items[id]["spec"]["asset"], "creature.guard");
        }
    }

    #[test]
    fn prepared_definitions_are_shared_and_isolated_from_author_edits() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut package = read_package(&root).unwrap();
        let prepared = package.index(42).unwrap();
        let copy = prepared.clone();
        assert!(Arc::ptr_eq(&prepared.definitions, &copy.definitions));
        let before: Vec<_> = [1, 2]
            .into_iter()
            .map(|id| package.build_region(42, &prepared, id).unwrap())
            .collect();
        package.manifest.characters[0].body.as_mut().unwrap().mass = 91;
        for archetype in package.manifest.archetypes.values_mut() {
            archetype.name = Some("renamed author definition".into());
        }
        for (offset, id) in [1, 2].into_iter().enumerate() {
            assert_eq!(package.build_region(42, &copy, id).unwrap(), before[offset]);
        }
        let updated = package.index(42).unwrap();
        assert!(!Arc::ptr_eq(&prepared.definitions, &updated.definitions));
        for (offset, id) in [1, 2].into_iter().enumerate() {
            assert_ne!(
                package.build_region(42, &updated, id).unwrap(),
                before[offset]
            );
        }
    }

    #[test]
    fn prepared_region_avoids_recopying_archetypes_at_every_scale() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut results = Vec::new();
        for count in [16, 256, 4096] {
            let mut package = read_package(&root).unwrap();
            for id in 0..count {
                package.manifest.archetypes.insert(
                    format!("unrelated-{id}"),
                    Archetype {
                        name: Some(format!("unrelated-{id}")),
                        body: Some(BodySpec {
                            cells: vec![[0, 0, 0]],
                            eye: [0, 0, 0],
                            mass: 80,
                        }),
                        ..Default::default()
                    },
                );
            }
            let index = package.index(42).unwrap();
            let expected = package.build_region(42, &index, 2).unwrap();
            ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.set(0));
            let actual = package.build_region(42, &index, 2).unwrap();
            assert_eq!(actual, expected);
            results.push((
                count,
                ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.get()),
            ));
        }
        assert_eq!(results, vec![(16, 0), (256, 0), (4096, 0)]);
    }

    #[test]
    fn authoring_rejects_removed_profiles_and_preserves_nested_body_requirements() {
        let baseline = serde_json::json!({"id": 7, "anchor": "1/start"});
        assert!(serde_json::from_value::<Character>(baseline.clone()).is_ok());
        for profile in [
            serde_json::json!({}),
            serde_json::Value::Null,
            serde_json::json!({"max_hp": 30}),
        ] {
            let mut character = baseline.clone();
            character["combat"] = profile.clone();
            assert!(serde_json::from_value::<Character>(character)
                .unwrap_err()
                .to_string()
                .contains("unknown field `combat`"));
            assert!(
                serde_json::from_value::<Archetype>(serde_json::json!({"combat": profile}))
                    .unwrap_err()
                    .to_string()
                    .contains("unknown field `combat`")
            );
            assert!(serde_json::from_value::<Actor>(
                serde_json::json!({"id": 9, "at": [1, 2, 3], "combat": profile})
            )
            .unwrap_err()
            .to_string()
            .contains("unknown field `combat`"));
        }
        for body in [
            serde_json::json!({"cells": [[0, 0, 0]], "mass": 80}),
            serde_json::json!({"cells": [[0, 0, 0]], "eye": [0, 0, 0]}),
            serde_json::json!({"eye": [0, 0, 0], "mass": 80}),
        ] {
            assert!(serde_json::from_value::<Character>(serde_json::json!({
                "id": 7, "anchor": "1/start", "body": body
            }))
            .is_err());
        }
    }

    #[test]
    fn authoring_explicit_actor_values_preserve_canonical_shape() {
        let value = serde_json::json!({
            "id": 9, "at": [1, 2, 3], "archetype": "guard", "turn_ticks": 73,
            "controller": "ai", "ai": "cautious", "velocity": [-1, 0, 2],
            "anatomy": null,
            "known_identities": ["healing"],
            "body": {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 91},
            "creature": {
                "species": "guard", "name": "guard", "faction": "guards", "binding": "intellect",
                "attributes": null, "templates": [], "future_growth": {"kind": "disabled"},
                "body_package": null, "personal_body_grants": [],
                "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial", "purchases": [], "attribute": null}]
            }
        });
        let actor: Actor = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&actor).unwrap(), value);
        assert_eq!(actor.creature.as_ref().unwrap().species, "guard");
        let body = actor.body.unwrap();
        let author_bytes = serde_json::to_vec(&body).unwrap();
        let compiled = tor_simulation::BodySpec::from(body);
        assert_eq!(serde_json::to_value(&compiled).unwrap(), value["body"]);
        assert_eq!(serde_json::to_vec(&compiled).unwrap(), author_bytes);
    }

    #[test]
    fn authoring_ai_overrides_inherit_without_losing_explicit_false_or_zero() {
        let species = tor_simulation::behavior::BehaviorPolicy {
            doors: true,
            care: true,
            equipment: true,
            loot: true,
            gold: true,
            ..Default::default()
        };
        let author: AiProfile = serde_json::from_value(serde_json::json!({
            "idle":"wait","doors":false,"care":false,"gold":false,"flee_percent":0
        }))
        .unwrap();
        let resolved = author.resolve(species);
        assert_eq!(resolved.idle, tor_simulation::behavior::IdleBehavior::Wait);
        assert!(!resolved.doors && !resolved.care && !resolved.gold);
        assert!(resolved.equipment && resolved.loot);
        assert_eq!(resolved.flee_percent, 0);
        assert_eq!(AiProfile::default().resolve(species), species.into());
        assert!(serde_json::from_value::<AiProfile>(serde_json::json!({"unknown":true})).is_err());
        assert!(
            !serde_json::from_value::<AiProfile>(serde_json::json!({"flee_percent":101}))
                .unwrap()
                .valid()
        );
    }

    #[test]
    fn acquisition_file_growth_cannot_bypass_the_read_limit() {
        struct Counted<'a> {
            input: &'a [u8],
            consumed: &'a std::cell::Cell<usize>,
        }
        impl Read for Counted<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let size = self.input.read(buffer)?;
                self.consumed.set(self.consumed.get() + size);
                Ok(size)
            }
        }
        // The opened stream can grow after its metadata was checked.
        let consumed = std::cell::Cell::new(0);
        let result = read_text_limited(
            Counted {
                input: b"12345678901234567890",
                consumed: &consumed,
            },
            "growing.toml",
            8,
        );
        assert_eq!((result.is_err(), consumed.get()), (true, 9));
        assert_eq!(
            read_text_limited(&b"12345678"[..], "exact.toml", 8).unwrap(),
            "12345678"
        );
        assert_eq!(
            read_text_limited("éé".as_bytes(), "utf8.toml", 4).unwrap(),
            "éé"
        );
        assert!(read_text_limited(&[0xff][..], "invalid.toml", 4).is_err());
    }

    #[test]
    fn saved_index_obeys_the_same_byte_limit_as_package_files() {
        let expected = RegionIndex::default();
        let mut bytes = expected.to_bytes().unwrap();
        bytes.resize(MAX_INDEX_BYTES as usize, b' ');
        assert_eq!(RegionIndex::from_bytes(&bytes).unwrap(), expected);
        bytes.push(b' ');
        assert!(
            RegionIndex::from_bytes(&bytes).is_err(),
            "saved indexes must reject bytes beyond the package-file limit"
        );
    }

    #[test]
    fn authored_default_matches_existing_fixture_at_every_seed_variant() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = read_package(&root).unwrap();
        for seed in [0, 1, 2, 42] {
            let mut actual = package.build(seed, true).unwrap();
            let mut expected = Game::two_room_in_stone(seed, package.content().unwrap());
            expected
                .spawn_actor(loc(1, [1, 1, 0]), NonZeroU64::new(100).unwrap())
                .unwrap();
            // Packages now retain archetype identity; the legacy diagnostic
            // fixture has only ordinary names. Verify the complete disclosed
            // state and deterministic play rather than erasing that identity.
            let actor = tor_simulation::ActorId(1);
            expected
                .set_body(
                    actor,
                    tor_simulation::BodySpec {
                        cells: vec![[0, 0, 0], [0, 0, 1]],
                        eye: [0, 0, 1],
                        mass: 80,
                    },
                )
                .unwrap();
            expected.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
            expected.set_gravity(RegionId(2), [0, 0, -1]).unwrap();
            assert_eq!(
                actual.observe(actor).unwrap(),
                expected.observe(actor).unwrap()
            );
            for action in [
                tor_simulation::Action::Take {
                    item: tor_simulation::ItemId(1),
                    quantity: None,
                },
                tor_simulation::Action::Drop {
                    item: tor_simulation::ItemId(1),
                    quantity: None,
                },
                tor_simulation::Action::Move(Direction::East),
                tor_simulation::Action::Wait,
            ] {
                assert_eq!(
                    actual.act(actor, action.clone()),
                    expected.act(actor, action)
                );
                assert_eq!(
                    actual.observe(actor).unwrap(),
                    expected.observe(actor).unwrap()
                );
            }
        }
    }
}

#[cfg(test)]
mod region_lifecycle_tests {
    use super::*;
    use tor_simulation::RegionTransition;

    use crate::test_packages::all_packages as packages;

    /// Every authored region plus at most four complete groups per family.
    /// Numeric address size never controls how many regions this test visits.
    fn selected_regions(package: &Package, seed: u64) -> Vec<RegionId> {
        let mut selected: BTreeSet<_> = package
            .index
            .regions
            .iter()
            .map(|r| RegionId(r.id))
            .collect();
        let mut procedural = 0;
        for (identity, family) in &package.manifest.procedural_worlds {
            let last = family.depth_limit(identity, seed);
            let depths = BTreeSet::from([1, 2.min(last), 27.min(last), last]);
            let before = selected.len();
            for depth in depths {
                selected.extend(
                    family
                        .group(identity, seed, depth)
                        .unwrap()
                        .members
                        .into_iter()
                        .map(RegionId),
                );
            }
            let count = selected.len() - before;
            assert!(count > 0 && count <= 4 * family.slots() as usize);
            procedural += count;
        }
        assert!(!selected.is_empty());
        assert_eq!(selected.len(), package.index.regions.len() + procedural);
        selected.into_iter().collect()
    }

    fn records(package: &Arc<Package>, seed: u64) -> crate::regions::Regions {
        crate::regions::Regions::new(
            package.clone(),
            seed,
            crate::Streaming {
                active_radius: 0,
                load_radius: 0,
            },
        )
        .unwrap()
    }

    fn publish_selection(
        game: &mut Game,
        records: &mut crate::regions::Regions,
        selected: &BTreeSet<RegionId>,
    ) {
        let mut made = Vec::new();
        records
            .transition_with(game, &mut made, selected.clone())
            .unwrap();
        records.publish(made);
    }

    fn selection_reference(
        package: &Arc<Package>,
        seed: u64,
        selected: &[RegionId],
    ) -> (Game, crate::regions::Regions) {
        let mut records = records(package, seed);
        if package.manifest.procedural_worlds.is_empty() {
            return (package.build(seed, true).unwrap(), records);
        }
        let mut game = package.start(seed).unwrap();
        publish_selection(&mut game, &mut records, &selected.iter().copied().collect());
        // Required neighboring groups publish through the production atomic
        // group path. Their count is bounded independently of address depth.
        let extra_bound: usize = package
            .manifest
            .procedural_worlds
            .values()
            .map(|family| 8 * family.slots() as usize)
            .sum();
        assert!(game.loaded_regions().count() <= selected.len() + extra_bound);
        assert!(selected.iter().all(|id| matches!(
            game.region_state(*id),
            Some(tor_simulation::RegionState::Active | tor_simulation::RegionState::Frozen)
        )));
        package.initialize_selected_character(&mut game).unwrap();
        package.configure_loaded_arena(&mut game).unwrap();
        game.initialize_discovery().unwrap();
        (game, records)
    }

    /// Reads only records already published by the production group path.
    /// Structural declarations are allowed; missing terrain cannot regenerate.
    struct SavedRecords<'a>(&'a mut crate::regions::Regions);
    impl tor_simulation::RecordStore for SavedRecords<'_> {
        fn can_prepare(&self, region: RegionId) -> bool {
            self.0.can_prepare(region)
        }
        fn put(
            &mut self,
            id: tor_simulation::RecordId,
            record: tor_world::Shared<tor_simulation::RegionRecord>,
        ) {
            self.0.put(id, record);
        }
        fn get(
            &mut self,
            id: tor_simulation::RecordId,
        ) -> Option<tor_world::Shared<tor_simulation::RegionRecord>> {
            self.0.get(id)
        }
        fn unbuilt(&mut self, region: RegionId) -> Option<tor_simulation::UnbuiltRegion> {
            self.0.unbuilt(region)
        }
        fn build(&mut self, region: RegionId) -> Option<tor_simulation::RegionRecord> {
            panic!("Saved-only lifecycle test attempted terrain generation for {region:?}")
        }
    }

    /// Visit order legitimately changes allocation IDs of published detached
    /// records. Compare their complete contents by stable region identity, then
    /// load every already-published record frozen to compare the entire game.
    fn assert_same_procedural_game(
        actual: &Game,
        actual_records: &mut crate::regions::Regions,
        expected: &Game,
        expected_records: &mut crate::regions::Regions,
        context: &str,
    ) {
        use tor_simulation::{RecordStore, RegionState};
        let membership = |game: &Game| {
            game.loaded_regions()
                .map(|id| (id, game.region_state(id).unwrap()))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(membership(actual), membership(expected), "{context}");
        assert_eq!(
            actual.unbuilt_regions().collect::<BTreeSet<_>>(),
            expected.unbuilt_regions().collect::<BTreeSet<_>>(),
            "{context}"
        );
        let detached = |game: &Game, records: &mut crate::regions::Regions| {
            game.detached_records()
                .map(|(region, record)| {
                    (
                        region,
                        records.get(record).expect("published detached record"),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            detached(actual, actual_records),
            detached(expected, expected_records),
            "{context}"
        );
        let expand = |game: &Game, records: &mut crate::regions::Regions| {
            let active = game
                .loaded_regions()
                .filter(|id| game.region_state(*id) == Some(RegionState::Active))
                .collect();
            let loaded = game
                .loaded_regions()
                .chain(game.detached_records().map(|(region, _)| region))
                .collect();
            let mut game = game.clone();
            game.transition_regions(
                &RegionTransition { active, loaded },
                &mut SavedRecords(records),
            )
            .unwrap();
            assert_eq!(game.detached_records().count(), 0);
            game
        };
        let mut actual = expand(actual, actual_records);
        let mut expected = expand(expected, expected_records);
        actual.continue_record_ids(&expected);
        expected.continue_record_ids(&actual);
        assert_same_game(&actual, &expected, context);
    }

    fn assert_same_game(actual: &Game, expected: &Game, context: &str) {
        if actual == expected {
            return;
        }
        fn snapshot(game: &Game) -> serde_json::Value {
            let mut shared = tor_simulation::checkpoint::SharedState::default();
            let state = game.checkpoint(&mut shared);
            serde_json::to_value((state, shared)).unwrap()
        }
        fn first_difference(
            a: &serde_json::Value,
            b: &serde_json::Value,
            path: &str,
        ) -> Option<String> {
            if a == b {
                return None;
            }
            match (a, b) {
                (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                    for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                        let path = format!("{path}.{key}");
                        match (a.get(key), b.get(key)) {
                            (Some(a), Some(b)) => {
                                if let Some(diff) = first_difference(a, b, &path) {
                                    return Some(diff);
                                }
                            }
                            _ => return Some(format!("{path}: field presence differs")),
                        }
                    }
                }
                (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                    if a.len() != b.len() {
                        return Some(format!("{path}: lengths {} != {}", a.len(), b.len()));
                    }
                    for (index, (a, b)) in a.iter().zip(b).enumerate() {
                        if let Some(diff) = first_difference(a, b, &format!("{path}[{index}]")) {
                            return Some(diff);
                        }
                    }
                }
                _ => {
                    let short = |value: &serde_json::Value| {
                        value.to_string().chars().take(160).collect::<String>()
                    };
                    return Some(format!("{path}: {} != {}", short(a), short(b)));
                }
            }
            None
        }
        panic!(
            "{context}: {}",
            first_difference(&snapshot(actual), &snapshot(expected), "game")
                .unwrap_or_else(|| "non-checkpoint game fields differ".into())
        );
    }

    /// Starting with nothing built and building regions on first load, in
    /// any order, equals the complete authored build or one bounded procedural
    /// selection loaded at once. Content never depends on visit order.
    #[test]
    fn every_package_selected_regions_match_their_reference_in_any_order() {
        use tor_simulation::RegionState;
        for path in packages() {
            let package = Arc::new(read_package(&path).unwrap());
            for seed in [0, 42] {
                let all = selected_regions(&package, seed);
                let mut reversed = all.clone();
                reversed.reverse();
                let mut rotated = all.clone();
                rotated.rotate_left(all.len() / 2);
                let (whole, mut whole_records) = selection_reference(&package, seed, &all);
                for order in [&all, &reversed, &rotated] {
                    let context = format!("{} seed {seed} order {order:?}", path.display());
                    let mut game = package.start(seed).unwrap();
                    // Regions are declared only when needed, and none is built.
                    assert!(
                        all.iter().all(|r| matches!(
                            game.region_state(*r),
                            Some(RegionState::Unbuilt) | None
                        )),
                        "{context}"
                    );
                    let mut individual = PackageRecords::new(package.clone(), seed);
                    let mut groups = records(&package, seed);
                    let mut loaded = BTreeSet::new();
                    for region in order.iter() {
                        loaded.insert(*region);
                        if package.manifest.procedural_worlds.is_empty()
                            && package.manifest.generation_groups.is_empty()
                        {
                            game.transition_regions(
                                &RegionTransition {
                                    active: loaded.clone(),
                                    loaded: loaded.clone(),
                                },
                                &mut individual,
                            )
                            .unwrap_or_else(|error| panic!("{context}: {error:?}"));
                            package.install_authored_spatial(&mut game).unwrap();
                        } else {
                            publish_selection(&mut game, &mut groups, &loaded);
                        }
                    }
                    // Package::start deliberately defers arena setup until its
                    // participants are loaded, as Engine::start_streaming does.
                    // Compare published games, not a complete reference with
                    // an intermediate reconstruction before loadout/encounters.
                    package.initialize_selected_character(&mut game).unwrap();
                    package.configure_loaded_arena(&mut game).unwrap();
                    game.initialize_discovery().unwrap();
                    if package.manifest.procedural_worlds.is_empty()
                        && package.manifest.generation_groups.is_empty()
                    {
                        assert_same_game(&game, &whole, &context);
                    } else {
                        assert_same_procedural_game(
                            &game,
                            &mut groups,
                            &whole,
                            &mut whole_records,
                            &context,
                        );
                    }
                }
            }
        }
    }

    /// The pins' reach answers (each region's exit field, then a cached
    /// search) must agree with an uncached search in every loaded cell of each
    /// complete authored package or bounded family sample: portals, stairs, chambers and
    /// their rims. Most cells are far enough from exits for the field.
    #[test]
    fn reach_answers_agree_in_authored_packages_and_bounded_family_samples() {
        let mut fielded = 0;
        for path in packages() {
            let package = Arc::new(read_package(&path).unwrap());
            let selected = selected_regions(&package, 0);
            let (game, _) = selection_reference(&package, 0, &selected);
            fielded += game
                .check_reach()
                .unwrap_or_else(|cell| panic!("{}: {cell:?}", path.display()));
        }
        assert!(
            fielded > 1000,
            "only {fielded} cells were answered by exit fields"
        );
    }

    /// Building one region reads only it and its neighbours, however many
    /// regions the package has.
    #[test]
    fn copying_a_built_regions_file_doesnt_read_it_again() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
        let package = load(&root, 5, None, false).unwrap().package.unwrap();
        let index = package.index(5).unwrap();
        let (_, files) = package.build_region(5, &index, 3).unwrap();
        let read = package.sources.files_read();
        assert_eq!(read, files.len());
        for region in files {
            package.region_text(region).unwrap();
        }
        assert_eq!(package.sources.files_read(), read);
    }

    #[test]
    fn building_a_region_reads_only_it_and_its_neighbours() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
        let directory = tempfile::tempdir().unwrap();
        let scenario = streaming_corridor(&root, directory.path(), 256, 5).unwrap();
        let package = scenario.package.unwrap();
        let index = package.index(5).unwrap();
        for region in 1..=256 {
            let before = index.lookups();
            let (_, files) = package.build_region(5, &index, region).unwrap();
            // Itself and the halls on either side, whose walls it reads.
            let expected: BTreeSet<u64> = [region - 1, region, region + 1]
                .into_iter()
                .filter(|r| (1..=256).contains(r))
                .collect();
            assert_eq!(files, expected, "region {region}");
            let read = index.lookups() - before;
            assert!(read <= 3, "region {region} read {read} region definitions");
        }
    }

    #[test]
    fn a_game_with_unbuilt_regions_round_trips_through_a_checkpoint() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
        let package = read_package(&root.join("first-dungeon")).unwrap();
        let game = package.start(7).unwrap();
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let restored = Game::restore_checkpoint(
            game.checkpoint(&mut shared),
            &shared,
            game.content().clone(),
        );
        assert_eq!(restored, Ok(game));
    }

    /// Every authored package and bounded family selection, shrunk to character
    /// reference requirements and then reactivated, equals its original state. This
    /// covers real joins, rotations, physical portals, doors and bodies.
    #[test]
    fn every_package_selected_regions_detach_and_reattach_exactly() {
        let mut detached = 0;
        for path in packages() {
            let package = Arc::new(read_package(&path).unwrap());
            let selected = selected_regions(&package, 0);
            let (mut game, mut records) = selection_reference(&package, 0, &selected);
            // Packages without combat configure no run characters, so follow
            // the manifest's characters, as the engine will.
            for character in &package.manifest.characters {
                let actor = tor_simulation::ActorId(character.id);
                if game.observe(actor).is_err() {
                    continue; // Not spawned in this build.
                }
                game.add_reference_point(tor_simulation::ReferencePoint {
                    target: tor_simulation::ReferenceTarget::Actor(actor),
                    active_radius: None,
                    load_radius: None,
                    observes: true,
                })
                .unwrap();
            }
            let mut original = game.clone();
            let records_before = records.resident_count();
            let roots: BTreeSet<_> = game.region_roots().iter().map(|r| r.region).collect();
            let (_, report) = game
                .transition_regions(
                    &RegionTransition {
                        active: roots.clone(),
                        loaded: roots,
                    },
                    &mut SavedRecords(&mut records),
                )
                .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
            assert_eq!(
                records.resident_count(),
                records_before + report.detached.len()
            );
            detached += report.detached.len();
            let mut shared = tor_simulation::checkpoint::SharedState::default();
            let mut restored = Game::restore_checkpoint(
                game.checkpoint(&mut shared),
                &shared,
                game.content().clone(),
            )
            .unwrap_or_else(|error| panic!("{}: {report:?}: {error}", path.display()));
            assert_eq!(restored, game, "{}", path.display());
            let all: BTreeSet<_> = selected.into_iter().collect();
            let everything = RegionTransition {
                active: all.clone(),
                loaded: all,
            };
            game.transition_regions(&everything, &mut SavedRecords(&mut records))
                .unwrap();
            restored
                .transition_regions(&everything, &mut SavedRecords(&mut records))
                .unwrap();
            // Only the record-identity allocator moved.
            original.continue_record_ids(&game);
            assert_eq!(game, original, "{}", path.display());
            assert_eq!(restored, original, "{}", path.display());
        }
        assert!(detached > 0, "some package has a region to detach");
    }
}

fn check_trap_effect(
    rules: &tor_simulation::rules::Ruleset,
    kind: &TrapKind,
) -> Result<(), Failure> {
    require(
        tor_simulation::trap_kind_valid(rules, &kind.compiled()),
        "Trap effect refers to an invalid equipment slot, condition, or descriptor",
    )
}

#[cfg(test)]
#[path = "initial_awareness_authoring_tests.rs"]
mod initial_awareness_authoring_tests;
