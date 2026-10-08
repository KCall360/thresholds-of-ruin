//! Ordinary authored inputs. Filesystem access stays in the server; construction
//! uses the same deterministic world operations as other simulation callers.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::num::NonZeroU64;
use std::path::{Component, Path};
use std::sync::Arc;

use crate::{Failure, Scenario};
#[path = "scenario_authoring.rs"]
mod authoring;
#[path = "scenario_compiler.rs"]
mod compiler;
#[path = "scenario_diagnostics.rs"]
mod diagnostics;
#[path = "scenario_instantiation.rs"]
mod instantiation;
pub use authoring::{
    AiProfile, AnatomySpec, AttackSpec, BodySpec, CombatSpec, ConsumableSpec, DamageType,
    EffectSpec, EquipmentSlot, EquipmentSpec, ItemClass,
};
use compiler::PreparedDefinitions;
use diagnostics::{Origin, PathSegment, ReferenceValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tor_simulation::Game;
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

pub const RULESET: &str = "interactions-v25";
const VALIDATOR: &str = "tor-scenario-10";
/// The manifest and the validator's files are bounded to this.
const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// The package layout this version reads: `scenario.toml`, one file per
/// region in `regions/`, and the validator's `index.json`.
pub const FORMAT: u32 = 2;
const REGION_DIR: &str = "regions";
/// Regions a package may have.
pub const MAX_REGIONS: usize = 65_536;
/// Each region file is bounded to this.
pub(crate) const MAX_REGION_BYTES: u64 = 1024 * 1024;
/// The index is bounded to this, which holds the largest package.
pub(crate) const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;

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
fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
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
    /// `docs/protocol.md#asset-palettes`.
    #[serde(default)]
    pub assets: BTreeMap<String, Vec<String>>,
    /// Terrain assets of regions outside any zone, or in a zone without its own.
    pub terrain: Option<TerrainAssets>,
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
#[derive(Debug, Default, Serialize, Deserialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    pub anatomy: Option<AnatomySpec>,
    pub equipment: Option<EquipmentSpec>,
    pub consumable: Option<ConsumableSpec>,
    #[serde(default)]
    pub class: crate::scenario_package::ItemClass,
    pub combat: Option<CombatSpec>,
    pub body: Option<BodySpec>,
    pub identity: Option<String>,
    pub appearance_pool: Option<String>,
    #[serde(default)]
    pub stackable: bool,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    pub name: Option<String>,
    pub turn_ticks: Option<u64>,
    /// The asset clients draw actors and items of this archetype with.
    pub asset: Option<String>,
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
            equipment: self.equipment.clone(),
            consumable: self.consumable.clone(),
            class: self.class,
            combat: self.combat.clone(),
            body: self.body.clone(),
            identity: self.identity.clone(),
            appearance_pool: self.appearance_pool.clone(),
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
    pub anatomy: Option<AnatomySpec>,
    pub combat: Option<CombatSpec>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionDef {
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
    pub id: u64,
    pub at: [i32; 3],
    pub open: bool,
    /// Cells tall; a door must exactly fill its opening.
    #[serde(default = "one_cell")]
    pub height: u8,
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
    pub anatomy: Option<AnatomySpec>,
    #[serde(default)]
    pub known_identities: Vec<String>,
    pub combat: Option<CombatSpec>,
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
    pub id: u64,
    /// The region's file, relative to the package, and its SHA-256.
    pub file: String,
    pub hash: String,
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
    fn of(def: &RegionDef, file: String, hash: String) -> Self {
        Self {
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
            generated: def.generate.is_some(),
        }
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
    /// Texts held in memory: every region's for a package read whole, and
    /// none otherwise.
    texts: BTreeMap<u64, Arc<str>>,
    /// The save's copies, read when first needed, and which regions have one.
    saved: Option<(crate::storage::SourceReader, BTreeSet<u64>)>,
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
            texts,
            saved: None,
            recent: Default::default(),
            parsed: Default::default(),
            files_read: 0,
        })))
    }
    /// Read these regions' files from the save's copies, when first needed.
    pub(crate) fn attach_saved(
        &self,
        reader: crate::storage::SourceReader,
        regions: BTreeSet<u64>,
    ) {
        self.0.lock().unwrap().saved = Some((reader, regions));
    }
    /// Region file texts held in memory, other than a few recent ones.
    pub fn texts_in_memory(&self) -> usize {
        self.0.lock().unwrap().texts.len()
    }
    /// Region files read from the package directory so far.
    pub fn files_read(&self) -> usize {
        self.0.lock().unwrap().files_read
    }
    /// Look for region files not in memory in `directory` too.
    pub(crate) fn set_directory(&self, directory: &Path) {
        self.0.lock().unwrap().directory = Some(directory.to_path_buf());
    }
    pub(crate) fn has_directory(&self) -> bool {
        self.0.lock().unwrap().directory.is_some()
    }
    /// A region file's text, checked against the index.
    pub(crate) fn text(&self, entry: &IndexedRegion) -> Result<Arc<str>, Failure> {
        let (text, directory) = {
            let mut s = self.0.lock().unwrap();
            let held = s.texts.get(&entry.id).cloned().or_else(|| {
                s.recent
                    .iter()
                    .find(|(id, _)| *id == entry.id)
                    .map(|(_, text)| text.clone())
            });
            let held = match (held, &mut s.saved) {
                (Some(text), _) => Some(text),
                (None, Some((reader, regions))) if regions.contains(&entry.id) => {
                    Some(Arc::from(reader.read(entry.id)?.ok_or_else(|| {
                        fail(format!("The save's copy of region {} is missing", entry.id))
                    })?))
                }
                _ => None,
            };
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
                )))
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
        let def: Arc<RegionDef> = Arc::new(parse(&self.text(entry)?, &entry.file)?);
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

/// A package: its manifest, its region index, and where its region files
/// come from. Saves keep the manifest and index, and a copy of each region
/// file once that region is built; resuming reads other region files from
/// the package directory, checked against the index. See
/// `docs/scenario-packages.md`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub manifest: Manifest,
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
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Package source identities use repository LF bytes. Comments and all other
/// edits still affect integrity; only CRLF/LF checkout differences are ignored.
fn source_digest(text: &str) -> String {
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
    let file = std::fs::File::open(path).map_err(|e| fail(format!("{relative}: {e}")))?;
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

fn read_manifest(root: &Path) -> Result<(Manifest, String), Failure> {
    let text = read(root, "scenario.toml")?;
    let manifest: Manifest = parse(&text, "scenario.toml")?;
    require(
        manifest.format == FORMAT,
        format!("Unsupported scenario format; expected format = {FORMAT}"),
    )?;
    require(
        manifest.ruleset == RULESET,
        "Missing exact ruleset dependency",
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
    Ok((manifest, text))
}

/// Every region file in the package, read and indexed. Reads the whole
/// package: validation and unvalidated development use it.
fn scan_regions(root: &Path) -> Result<(RegionIndex, BTreeMap<u64, Arc<str>>), Failure> {
    let directory = root.join(REGION_DIR);
    let entries =
        std::fs::read_dir(&directory).map_err(|e| fail(format!("{}: {e}", directory.display())))?;
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
        names.push(name);
    }
    require(
        !names.is_empty() && names.len() <= MAX_REGIONS,
        format!("Expected 1..{MAX_REGIONS} region files"),
    )?;
    let mut regions = Vec::new();
    let mut texts = BTreeMap::new();
    for name in names {
        let file = format!("{REGION_DIR}/{name}");
        let text = read_limited(root, &file, MAX_REGION_BYTES)?;
        let def: RegionDef = parse(&text, &file)?;
        require(
            file == region_file(def.id),
            format!(
                "{file}: holds region {}; name it {}",
                def.id,
                region_file(def.id)
            ),
        )?;
        regions.push(IndexedRegion::of(&def, file, source_digest(&text)));
        texts.insert(def.id, Arc::from(text));
    }
    regions.sort_by_key(|r| r.id);
    Ok((RegionIndex { regions }, texts))
}

fn model_hash(manifest: &Manifest, index: &RegionIndex) -> Result<String, Failure> {
    Ok(digest(
        &serde_json::to_vec(&(manifest, index)).map_err(|e| fail(e.to_string()))?,
    ))
}

impl Package {
    /// Restore only stored metadata. Storage attaches the bounded index and lazy
    /// region sources separately before validating or exposing the scenario.
    pub(crate) fn from_saved_metadata(
        manifest: Manifest,
        certificate: Certificate,
        validated: bool,
        selected: u64,
        directory: Option<String>,
    ) -> Self {
        Self {
            manifest,
            certificate,
            validated,
            selected,
            directory,
            index: Default::default(),
            sources: Default::default(),
            manifest_text: None,
        }
    }

    fn assemble(
        manifest: Manifest,
        manifest_text: &str,
        index: RegionIndex,
        sources: RegionSources,
        directory: Option<&Path>,
    ) -> Result<Self, Failure> {
        let files = BTreeMap::from([
            ("scenario.toml".into(), source_digest(manifest_text)),
            ("index.json".into(), digest(&index.to_bytes()?)),
        ]);
        let content_hash = digest(&serde_json::to_vec(&files).map_err(|e| fail(e.to_string()))?);
        let model_hash = model_hash(&manifest, &index)?;
        let certificate = Certificate {
            model_hash,
            validator: VALIDATOR.into(),
            ruleset: RULESET.into(),
            content_hash,
            files,
            regions: index.regions.len(),
            coverage: "all authored regions; generated regions deterministic with connected entries; all character starts; deterministic construction twice at seeds 0, 1, 42; no winnability proof".into(),
        };
        Ok(Self {
            selected: manifest.default_character,
            manifest,
            certificate,
            validated: false,
            directory: directory
                .and_then(|d| d.canonicalize().ok())
                .map(|d| d.display().to_string()),
            index: Arc::new(index),
            sources,
            manifest_text: Some(Arc::from(manifest_text)),
        })
    }

    /// A package from definitions in memory, for tools and tests. Unvalidated.
    pub fn from_parts(manifest: Manifest, regions: Vec<RegionDef>) -> Result<Self, Failure> {
        let manifest_text = toml::to_string(&manifest).map_err(|e| fail(e.to_string()))?;
        let mut index = Vec::new();
        let mut texts = BTreeMap::new();
        for def in &regions {
            let text = toml::to_string(def).map_err(|e| fail(e.to_string()))?;
            index.push(IndexedRegion::of(
                def,
                region_file(def.id),
                source_digest(&text),
            ));
            require(
                texts.insert(def.id, Arc::<str>::from(text)).is_none(),
                format!("Duplicate region {}", def.id),
            )?;
        }
        index.sort_by_key(|r| r.id);
        Self::assemble(
            manifest,
            &manifest_text,
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

    /// A region file's text, checked against the index, for a save to copy.
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
    let (manifest, manifest_text) = read_manifest(root)?;
    let (index, texts) = scan_regions(root)?;
    Package::assemble(
        manifest,
        &manifest_text,
        index,
        RegionSources::with(Some(root), texts),
        Some(root),
    )
}

/// The directory holding `package`'s region files: the supplied package's,
/// or the one it was loaded from, when either is still the same package.
pub(crate) fn locate(package: &Package, supplied: Option<&Package>) -> Option<std::path::PathBuf> {
    let same = |p: &Package| p.certificate.model_hash == package.certificate.model_hash;
    if let Some(directory) = supplied
        .filter(|p| same(p))
        .and_then(|p| p.directory.as_ref())
    {
        return Some(directory.into());
    }
    let directory = Path::new(package.directory.as_ref()?);
    let found = read_indexed(directory)
        .or_else(|_| read_package(directory))
        .ok()?;
    same(&found).then(|| directory.to_path_buf())
}

/// A validated package: its manifest and generated index, with region
/// files read only when their regions are built.
fn read_indexed(root: &Path) -> Result<Package, Failure> {
    let (manifest, manifest_text) = read_manifest(root)?;
    let index =
        RegionIndex::from_bytes(read_limited(root, "index.json", MAX_INDEX_BYTES)?.as_bytes())?;
    Package::assemble(
        manifest,
        &manifest_text,
        index,
        RegionSources::with(Some(root), BTreeMap::new()),
        Some(root),
    )
}

/// Write a package's files: the manifest and one file per region. No index
/// or certificate; validate it, or load it as unvalidated.
pub fn write_package(
    out: &Path,
    manifest: &Manifest,
    regions: &[RegionDef],
) -> Result<(), Failure> {
    let io = |e: std::io::Error| fail(e.to_string());
    std::fs::create_dir_all(out.join(REGION_DIR)).map_err(io)?;
    let text = toml::to_string(manifest).map_err(|e| fail(e.to_string()))?;
    std::fs::write(out.join("scenario.toml"), text).map_err(io)?;
    for def in regions {
        let text = toml::to_string(def).map_err(|e| fail(e.to_string()))?;
        std::fs::write(out.join(region_file(def.id)), text).map_err(io)?;
    }
    Ok(())
}

pub fn validate(root: &Path) -> Result<Certificate, Failure> {
    let mut package = read_package(root)?;
    package.check()?;
    for def in package.region_defs()? {
        if let Some(generate) = &def.generate {
            crate::generator::check(&def, generate)?;
        }
        package.check_region(&def)?;
        // What a region shows must be in its palette.
        let forecast = package.palette(package.region_themes(def.id).unwrap_or(&[]));
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
        for entry in package.index.regions.iter().filter(|r| r.generated) {
            let region = index.region(&package, entry.id)?;
            let again = index.region(&package, entry.id)?;
            let bytes = |r: &RegionDef| serde_json::to_vec(r).map_err(|e| fail(e.to_string()));
            require(
                bytes(&region)? == bytes(&again)?,
                format!("Region {}: nondeterministic generation", entry.id),
            )?;
            require(
                crate::generator::entries_connected(&region),
                format!("Region {}: generated entries aren't connected", entry.id),
            )?;
        }
    }
    for character in &package.manifest.characters {
        package.selected = character.id;
        for seed in [0, 1, 42] {
            require(
                package.build(seed, false)? == package.build(seed, false)?,
                "Nondeterministic scenario construction",
            )?;
        }
    }
    std::fs::write(root.join("index.json"), package.index.to_bytes()?)
        .map_err(|e| fail(e.to_string()))?;
    let data = serde_json::to_vec_pretty(&package.certificate).map_err(|e| fail(e.to_string()))?;
    std::fs::write(root.join("validation.json"), data).map_err(|e| fail(e.to_string()))?;
    Ok(package.certificate)
}

pub fn load(
    root: &Path,
    seed: u64,
    selected: Option<u64>,
    allow_unvalidated: bool,
) -> Result<Scenario, Failure> {
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
    write_package(out, &source.manifest, &regions)?;
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
        let region = self.index.region(region)?;
        Some(
            region
                .zone
                .as_ref()
                .and_then(|z| self.manifest.zones.get(z))
                .and_then(|z| z.themes.as_deref())
                .unwrap_or(&self.manifest.themes),
        )
    }
    /// A region's terrain assets: its zone's, or the world's.
    pub fn region_terrain(&self, region: u64) -> Option<&TerrainAssets> {
        let zone = self.index.region(region)?.zone.as_ref();
        zone.and_then(|z| self.manifest.zones.get(z))
            .and_then(|z| z.terrain.as_ref())
            .or(self.manifest.terrain.as_ref())
    }

    /// Assets clients may need for regions with these themes, and for the
    /// run's characters, who can be anywhere.
    pub fn palette<'a>(&self, themes: impl IntoIterator<Item = &'a String>) -> BTreeSet<String> {
        themes
            .into_iter()
            .flat_map(|theme| self.manifest.assets.get(theme).into_iter().flatten())
            .chain(
                self.manifest
                    .characters
                    .iter()
                    .filter_map(|c| c.asset.as_ref()),
            )
            .cloned()
            .collect()
    }

    /// Whether the scenario names assets at all; clients get palettes only then.
    pub fn has_assets(&self) -> bool {
        !self.manifest.assets.is_empty()
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
                assets.extend(compiler::item_asset(archetype, &self.manifest).map(str::to_owned));
            }
        }
        for key in r
            .generate
            .iter()
            .flat_map(|g| g.items.iter().flat_map(|p| &p.archetypes))
        {
            assets.extend(
                compiler::item_asset(self.author_archetype(key)?, &self.manifest)
                    .map(str::to_owned),
            );
        }
        Ok(assets)
    }

    pub(crate) fn check_identity(&self) -> Result<(), Failure> {
        self.check_identity_with_origin(Some(Origin::Manifest))
    }

    fn check_identity_with_origin(&self, origin: Option<Origin<'_>>) -> Result<(), Failure> {
        require(
            self.manifest.ruleset == RULESET
                && self.certificate.ruleset == RULESET
                && self.certificate.validator == VALIDATOR,
            "Missing exact package/validator dependency",
        )?;
        require(
            self.certificate.model_hash == model_hash(&self.manifest, &self.index)?,
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
                self.manifest_text.as_deref(),
                &[PathSegment::Field("default_character")],
                ReferenceValue::Id(self.selected),
            ),
            None => failure,
        })
    }
    fn supported(&self) -> Result<(), Failure> {
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
                        self.manifest_text.as_deref(),
                        &[PathSegment::UniqueValue(ReferenceValue::Text(enemy))],
                        ReferenceValue::Text(enemy),
                    )
                })?;
            }
        }
        for (name, profile) in &self.manifest.ai_profiles {
            require(
                label(name) && tor_simulation::ai::AiProfile::from(profile.clone()).valid(),
                "Invalid AI profile",
            )
            .map_err(|failure| {
                in_declaration(failure, format!("scenario.toml: AI profile {name:?}"))
            })?;
        }
        for character in &self.manifest.characters {
            require(
                character
                    .anatomy
                    .as_ref()
                    .is_none_or(|anatomy| anatomy.slots.len() <= 64),
                "Invalid character anatomy",
            )?;
            if let Some(spec) = &character.combat {
                self.check_combat(spec, Origin::Character(character.id), || {
                    self.manifest_text.clone()
                })?;
            }
        }
        for (name, archetype) in &self.manifest.archetypes {
            if let Some(spec) = &archetype.combat {
                self.check_combat(spec, Origin::Archetype(name), || self.manifest_text.clone())?;
            }
        }
        Ok(())
    }
    fn check_combat(
        &self,
        spec: &CombatSpec,
        origin: Origin<'_>,
        source: impl FnOnce() -> Option<Arc<str>>,
    ) -> Result<(), Failure> {
        require(
            tor_simulation::combat::CombatSpec::from(spec.clone()).valid(),
            "Invalid combat attributes or faction",
        )
        .map_err(|failure| origin.context(failure))?;
        require(
            self.manifest.factions.is_empty() || self.manifest.factions.contains_key(&spec.faction),
            "Invalid combat attributes or faction",
        )
        .map_err(|failure| {
            let source = source();
            origin.reference_path(
                failure,
                source.as_deref(),
                &[PathSegment::Field("combat"), PathSegment::Field("faction")],
                ReferenceValue::Text(&spec.faction),
            )
        })
    }
    fn anchors(&self) -> Result<BTreeMap<String, Location>, Failure> {
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
        Ok(anchors)
    }
    pub fn check(&self) -> Result<(), Failure> {
        self.check_identity()?;
        require(
            (1..=MAX_REGIONS).contains(&self.index.regions.len()),
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
                    self.manifest_text.as_deref(),
                    &[PathSegment::Field("assets"), PathSegment::Key(theme)],
                    ReferenceValue::Text(theme),
                )
            })?;
        }
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
                        self.manifest_text.as_deref(),
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
                    && (c.unselected == "ai") == c.ai.is_some()
                    && c.ai.as_ref().is_none_or(|s| label(s)),
                "Invalid unselected character controller",
            )?;
        }
        let mut region_ids = BTreeSet::new();
        let mut item_ids = BTreeSet::new();
        let mut door_ids = BTreeSet::new();
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
        let anchors = self.anchors()?;
        for c in &self.manifest.characters {
            let at = self.character_anchor(c.id, &c.anchor, &anchors)?;
            starts.insert(c.id, at.region.0);
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
                    self.manifest_text.as_deref(),
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
                            self.manifest_text.as_deref(),
                            &[PathSegment::Field("objective"), PathSegment::Field("item")],
                            ReferenceValue::Id(item),
                        )
                    },
                )?;
            }
        }
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
    fn check_region(&self, r: &RegionDef) -> Result<(), Failure> {
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
                a.anatomy
                    .as_ref()
                    .is_none_or(|anatomy| anatomy.slots.len() <= 64),
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
                    && (a.controller == "ai") == a.ai.is_some()
                    && a.ai.as_ref().is_none_or(|s| label(s)),
                "Invalid actor controller",
            )
            .map_err(contextualize)?;
            if let Some(spec) = &a.combat {
                self.check_combat(spec, origin, || self.region_text(r.id).ok())?;
            }
        }
        Ok(())
    }
    fn appearance_mapping(&self, seed: u64) -> Result<BTreeMap<String, String>, Failure> {
        let mut result = BTreeMap::new();
        let mut signatures = BTreeMap::new();
        for (key, a) in &self.manifest.archetypes {
            require(
                a.anatomy
                    .as_ref()
                    .is_none_or(|anatomy| anatomy.slots.len() <= 64),
                "Invalid anatomy",
            )?;
            require(
                a.equipment.as_ref().is_none_or(|equipment| {
                    let spec: tor_simulation::EquipmentSpec = equipment.clone().into();
                    !a.stackable && a.consumable.is_none() && spec.valid(a.class.into())
                }),
                "Invalid equipment definition",
            )?;
            require(
                a.consumable.as_ref().is_none_or(|consumable| {
                    let spec: tor_simulation::ConsumableSpec = consumable.clone().into();
                    a.class == ItemClass::Potion && spec.valid()
                }),
                "Invalid consumable definition",
            )?;
            let identity = a.identity.as_ref().unwrap_or(key);
            require(label(identity), "Invalid item identity")?;
            let signature = (
                &a.name,
                &a.appearance_pool,
                a.class,
                &a.equipment,
                &a.consumable,
            );
            if let Some(previous) = signatures.insert(identity, signature) {
                require(
                    previous == signature,
                    "One identity must have one name, appearance pool, physical class and item effects",
                )?;
            }
            if let Some(pool) = &a.appearance_pool {
                require(
                    a.name.is_some(),
                    "Missing identity name for appearance pool",
                )
                .map_err(|failure| Origin::Archetype(key).context(failure))?;
                require(
                    self.manifest.appearance_pools.contains_key(pool),
                    format!("Unknown appearance pool {pool:?}"),
                )
                .map_err(|failure| {
                    Origin::Archetype(key).reference(
                        failure,
                        self.manifest_text.as_deref(),
                        "appearance_pool",
                        pool,
                    )
                })?;
            }
        }
        for (key, pool) in &self.manifest.appearance_pools {
            let classes: BTreeSet<_> = self
                .manifest
                .archetypes
                .values()
                .filter(|a| a.appearance_pool.as_ref() == Some(key))
                .map(|a| {
                    (
                        a.class,
                        a.equipment.as_ref().map(|e| e.slot),
                        a.consumable.is_some(),
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
            let identities: BTreeSet<_> = self
                .manifest
                .archetypes
                .iter()
                .filter(|(_, a)| a.appearance_pool.as_ref() == Some(key))
                .map(|(id, a)| a.identity.as_ref().unwrap_or(id))
                .collect();
            require(
                pool.confounding || identities.len() <= pool.appearances.len(),
                "Appearance pool too small",
            )?;
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
    pub(crate) fn build(&self, seed: u64, _runtime: bool) -> Result<Game, Failure> {
        self.check()?;
        self.supported()?;
        let index = self.index(seed)?;
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
        );
        let defs = self
            .index
            .regions
            .iter()
            .map(|entry| index.region(self, entry.id))
            .collect::<Result<Vec<_>, _>>()?;
        for r in &defs {
            self.check_region(r)?;
        }
        let (actors, items, doors) = index.ceilings;
        game.reserve_identities(actors, items, doors);
        let all: Vec<&RegionDef> = defs.iter().map(|d| &**d).collect();
        let anchors = resolved_anchors(&all);
        self.add_geometry(&mut game, &all)?;
        for r in &all {
            self.add_structure(&mut game, r, &anchors)?;
            self.add_stairs(&mut game, r.id, &index, &anchors)?;
        }
        for (name, position) in &anchors {
            require(
                game.authored_cell_valid(*position),
                format!("Anchor {name}: outside traversable geometry"),
            )?;
        }
        self.add_entities(&mut game, seed, &index, &all)?;
        self.configure_run(&mut game, &index)?;
        Ok(game)
    }

    fn character_anchor(
        &self,
        id: u64,
        anchor: &str,
        anchors: &BTreeMap<String, Location>,
    ) -> Result<Location, Failure> {
        anchors.get(anchor).copied().ok_or_else(|| {
            Origin::Character(id).reference(
                fail(format!("Missing character anchor {anchor:?}")),
                self.manifest_text.as_deref(),
                "anchor",
                anchor,
            )
        })
    }

    /// Whole-package facts that building regions needs, computed once so
    /// building one region reads only that region and its neighbours.
    pub(crate) fn index(&self, seed: u64) -> Result<PackageIndex, Failure> {
        let definitions = Arc::new(PreparedDefinitions::new(&self.manifest, self.selected));
        let anchors = self.anchors()?;
        let mut stairs: BTreeMap<u64, Vec<StairExit>> = BTreeMap::new();
        for (id, pair) in &self.manifest.stair_pairs {
            require(
                label(id) && !id.contains('/'),
                format!("Invalid stair pair ID {id:?}"),
            )?;
            let endpoint = |field, name: &str| {
                self.endpoint_region(name).map_err(|failure| {
                    Origin::Manifest.reference_path(
                        failure,
                        self.manifest_text.as_deref(),
                        &[
                            PathSegment::Field("stair_pairs"),
                            PathSegment::Key(id),
                            PathSegment::Field(field),
                        ],
                        ReferenceValue::Text(name),
                    )
                })
            };
            let upper = endpoint("upper", &pair.upper)?;
            let lower = endpoint("lower", &pair.lower)?;
            for (from, direction, to) in [
                (&pair.upper, Direction::Down, &pair.lower),
                (&pair.lower, Direction::Up, &pair.upper),
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
        let mut spawns = BTreeMap::new();
        for c in &definitions.characters {
            if c.creature.control.spawned() {
                let at = self.character_anchor(c.id, &c.anchor, &anchors)?;
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
        // Identities that spawn, as building the whole package allocates them:
        // omitted characters and what they carry don't.
        let ceilings = (
            ceiling(&mut homes.keys().copied())?,
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
        );
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
        for r in &self.index.regions {
            if r.actors.iter().any(|a| a.external) {
                needed.insert(r.id);
            }
        }
        if let Some(o) = &self.manifest.objective {
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
        self.configure_run(&mut game, &index)?;
        Ok(game)
    }

    /// A region as a game declares it before building it: its metadata and
    /// the identities it will hold. Reads only the index.
    pub(crate) fn unbuilt_region(
        &self,
        index: &PackageIndex,
        region: u64,
    ) -> Result<tor_simulation::UnbuiltRegion, Failure> {
        let r = self
            .index
            .region(region)
            .ok_or_else(|| fail(format!("Unknown region {region}")))?;
        let generated = if r.generated {
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
            .filter_map(|p| self.endpoint_region(&p.to).ok())
            .chain(
                index
                    .stairs
                    .get(&region)
                    .into_iter()
                    .flatten()
                    .map(|s| s.destination),
            );
        for destination in destinations {
            if shell.iter().all(|s| s.id != destination) {
                shell.push(index.region(self, destination)?);
            }
        }
        shell.sort_by_key(|s| s.id);
        let files = shell.iter().map(|s| s.id).collect();
        let shell: Vec<&RegionDef> = shell.iter().map(|s| &**s).collect();
        let anchors = resolved_anchors(&shell);
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
        );
        self.add_geometry(&mut game, &shell)?;
        self.add_structure(&mut game, &r, &anchors)?;
        self.add_stairs(&mut game, region, index, &anchors)?;
        for (name, position) in anchors.iter().filter(|(_, at)| at.region.0 == region) {
            require(
                game.authored_cell_valid(*position),
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
    }

    fn add_stairs(
        &self,
        game: &mut Game,
        region: u64,
        index: &PackageIndex,
        anchors: &BTreeMap<String, Location>,
    ) -> Result<(), Failure> {
        for exit in index.stairs.get(&region).into_iter().flatten() {
            let endpoint = |name: &str| {
                anchors.get(name).copied().ok_or_else(|| {
                    fail(format!(
                        "Stair pair {}: unresolved endpoint {name}",
                        exit.id
                    ))
                })
            };
            game.connect(
                Passage {
                    from: endpoint(&exit.from)?,
                    direction: exit.direction,
                    to: endpoint(&exit.to)?,
                },
                0,
            )
            .map_err(|e| fail(format!("Stair pair {}: {e:?}", exit.id)))?;
        }
        Ok(())
    }

    fn endpoint_region(&self, anchor: &str) -> Result<u64, Failure> {
        let missing = || fail(format!("Missing stair/portal anchor {anchor}"));
        let (id, name) = anchor.split_once('/').ok_or_else(missing)?;
        let region: u64 = id.parse().map_err(|_| missing())?;
        if id != region.to_string()
            || !self.index.region(region).is_some_and(|r| {
                r.anchors.contains_key(name) || r.stair_anchors.iter().any(|n| n == name)
            })
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
            let connect = match p.kind.as_deref() {
                Some("portal") => Game::connect_portal_area,
                None | Some("stairs") => Game::connect_area,
                _ => return Err(fail("Invalid connection kind")),
            };
            connect(
                game,
                Passage {
                    from: loc(r.id, p.at),
                    direction,
                    to,
                },
                rotation,
                p.width,
                p.height,
            )
            .map_err(|e| fail(format!("Region {} portal to {}: {e:?}", r.id, p.to)))?;
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
                    || self.manifest_text.clone(),
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
                let prepared = definitions
                    .actor(a)
                    .map_err(|failure| origin.context(failure))?;
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
        }
        Ok(())
    }

    fn configure_run(&self, game: &mut Game, index: &PackageIndex) -> Result<(), Failure> {
        if self.manifest.characters.iter().any(|c| c.combat.is_some())
            || self.manifest.objective.is_some()
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
            && self.anchors().is_ok_and(|anchors| {
                anchors
                    .values()
                    .filter(|p| loaded(p))
                    .all(|p| game.authored_cell_valid(*p))
            })
    }
}

/// Whole-package facts for building regions (see [`Package::index`]).
#[derive(Clone, Debug)]
pub(crate) struct PackageIndex {
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
struct StairExit {
    id: String,
    from: String,
    direction: Direction,
    to: String,
    destination: u64,
}

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
    /// A region's definition: authored, or materialized by its generator.
    fn region(&self, package: &Package, id: u64) -> Result<Arc<RegionDef>, Failure> {
        self.lookups.set(self.lookups.get() + 1);
        let def = package.region_def(id)?;
        let Some(generate) = &def.generate else {
            return Ok(def);
        };
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
            write_package(temp.path(), &manifest, &definitions).unwrap();
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
    fn appearance_pools_share_affordances_while_effects_remain_distinct() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let mut package = read_package(&path).unwrap();
        package
            .manifest
            .archetypes
            .get_mut("healing")
            .unwrap()
            .consumable = Some(ConsumableSpec {
            effects: vec![EffectSpec::Heal { amount: 5 }],
        });
        assert!(package.appearance_mapping(42).is_err());
        package
            .manifest
            .archetypes
            .get_mut("poison")
            .unwrap()
            .consumable = Some(ConsumableSpec {
            effects: vec![EffectSpec::Damage {
                components: BTreeMap::from([(DamageType::Vital, 8)]),
            }],
        });
        assert!(package.appearance_mapping(42).is_ok());
    }
    use super::*;

    #[test]
    fn combat_diagnostic_source_is_acquired_only_for_catalog_failure() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
        let package = read_package(&root).unwrap();
        let spec = package.manifest.characters[0].combat.as_ref().unwrap();
        let origin = Origin::Character(1);
        package
            .check_combat(spec, origin, || {
                panic!("valid combat must not acquire source")
            })
            .unwrap();
        let mut missing = spec.clone();
        missing.faction = "missing".into();
        let mut invalid = missing.clone();
        invalid.max_hp = 0;
        let failure = package
            .check_combat(&invalid, origin, || {
                panic!("attribute failure precedes source acquisition")
            })
            .unwrap_err();
        assert!(failure.message.contains("combat attributes"));
        let reads = std::cell::Cell::new(0);
        let failure = package
            .check_combat(&missing, origin, || {
                reads.set(reads.get() + 1);
                None
            })
            .unwrap_err();
        assert_eq!(reads.get(), 1);
        assert!(failure.message.contains("scenario.toml: character 1"));
        assert!(failure.message.contains("combat attributes or faction"));
        let mut empty = package.clone();
        empty.manifest.factions.clear();
        empty
            .check_combat(&missing, origin, || {
                panic!("empty faction catalog permits valid combat")
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
        let package = Package::from_parts(manifest, template.region_defs().unwrap()).unwrap();
        let index = package.index(42).unwrap();
        let declared = package.unbuilt_region(&index, 2).unwrap();
        assert!(declared
            .identities
            .actors
            .contains(&tor_simulation::ActorId(1)));
        package.start(42).unwrap();
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
        )
        .unwrap();
        let paths: Vec<_> = std::iter::once(directory.path().join("scenario.toml"))
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
        let mut actor: Actor = serde_json::from_value(serde_json::json!({
            "id": 2, "at": [3, 1, 0], "controller": "ai", "ai": "missing",
            "combat": {"max_hp": 41},
            "body": {"cells": [[0, 0, 0]], "eye": [1, 0, 0], "mass": 91}
        }))
        .unwrap();
        regions[0].actors.push(actor.clone());
        let package = Package::from_parts(manifest.clone(), regions.clone()).unwrap();
        let failure = package.build(42, true).unwrap_err();
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
        assert!(failure.message.starts_with("regions/1.toml:"));
        assert!(failure
            .message
            .ends_with(": region 1, actor 2: Unknown actor AI profile \"missing\""));
        manifest
            .ai_profiles
            .insert("careful".into(), AiProfile::default());
        actor.ai = Some("careful".into());
        *regions[0].actors.last_mut().unwrap() = actor;
        let package = Package::from_parts(manifest, regions).unwrap();
        let failure = package.build(42, true).unwrap_err();
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
        assert_eq!(
            failure.message,
            "regions/1.toml: region 1, actor 2: Actor body eye must be one of its cells"
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
                flee_percent: 101,
                ..Default::default()
            },
        );
        package.manifest.characters[0].combat = Some(CombatSpec {
            max_hp: 0,
            ..Default::default()
        });
        package.manifest.archetypes.get_mut("token").unwrap().combat = Some(CombatSpec {
            max_hp: 0,
            ..Default::default()
        });
        for expected in [
            "scenario.toml: faction \"guard\": Invalid faction relationship",
            "scenario.toml: AI profile \"guard\": Invalid AI profile",
            "scenario.toml: character 1: Invalid combat attributes or faction",
            "scenario.toml: archetype \"token\": Invalid combat attributes or faction",
        ] {
            let failure = package.supported().unwrap_err();
            assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
            assert_eq!(failure.message, expected);
            match expected {
                s if s.contains("faction relationship") => package.manifest.factions.clear(),
                s if s.contains("AI profile") => package.manifest.ai_profiles.clear(),
                s if s.contains("character 1") => package.manifest.characters[0].combat = None,
                _ => package.manifest.archetypes.get_mut("token").unwrap().combat = None,
            }
        }
        package.supported().unwrap();
        let mut region = package.region_defs().unwrap().remove(0);
        let mut actor: Actor = serde_json::from_value(serde_json::json!({
            "id": 9, "at": [1, 1, 0], "controller": "invalid",
            "combat": {"max_hp": 0}
        }))
        .unwrap();
        region.actors.push(actor.clone());
        let failure = package.check_region(&region).unwrap_err();
        assert_eq!(
            failure.message,
            "regions/1.toml: region 1, actor 9: Invalid actor controller"
        );
        actor.controller = "external".into();
        *region.actors.last_mut().unwrap() = actor;
        let failure = package.check_region(&region).unwrap_err();
        assert_eq!(
            failure.message,
            "regions/1.toml: region 1, actor 9: Invalid combat attributes or faction"
        );
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
    }
    #[test]
    fn compiled_instances_preserve_inheritance_and_explicit_overrides() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = read_package(&root).unwrap();
        let mut manifest = template.manifest.clone();
        let mut regions = template.region_defs().unwrap();
        let inherited_combat: CombatSpec = serde_json::from_value(serde_json::json!({
            "name": "guard", "max_hp": 41, "defense": -3, "faction": "neutral",
            "attack": {"bonus": -7, "wind_up": 19, "recovery": 23,
                "damage": {"energy": 1, "impact": 2, "keen": 3, "spirit": 4, "vital": 5}},
            "immunities": ["energy", "vital"],
            "reductions": {"impact": 6, "keen": 7, "spirit": 8}
        }))
        .unwrap();
        let override_combat = CombatSpec {
            name: "override".into(),
            max_hp: 57,
            ..Default::default()
        };
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
                combat: Some(inherited_combat.clone()),
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
                memory_ticks: 73,
                flee_percent: 19,
            },
        );
        regions[0].actors.extend([
            Actor {
                anatomy: None,
                known_identities: vec![],
                id: 2,
                at: [3, 1, 0],
                archetype: Some("guard".into()),
                combat: None,
                body: None,
                turn_ticks: None,
                controller: "ai".into(),
                ai: Some("careful".into()),
                velocity: None,
            },
            Actor {
                anatomy: None,
                known_identities: vec![],
                id: 3,
                at: [4, 1, 0],
                archetype: Some("guard".into()),
                combat: Some(override_combat.clone()),
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
        let package = Package::from_parts(manifest, regions).unwrap();
        let game = package.build(42, true).unwrap();
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
        let shared = serde_json::to_value(shared).unwrap();
        for (id, combat, body, ticks) in [
            ("2", inherited_combat, inherited_body, 73),
            ("3", override_combat, override_body, 89),
        ] {
            let actor = &shared["actors"][snapshot["actors"].as_u64().unwrap() as usize][id];
            assert_eq!(
                actor["combat"]["spec"],
                serde_json::to_value(combat).unwrap()
            );
            assert_eq!(actor["body"], serde_json::to_value(body).unwrap());
            assert_eq!(actor["turn_ticks"], ticks);
            assert_eq!(actor["asset"], "creature.guard");
        }
        assert_eq!(
            snapshot["combat"]["ai"]["2"]["profile"],
            serde_json::json!({"memory_ticks": 73, "flee_percent": 19})
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
    fn authoring_combat_defaults_and_nested_requirements_are_stable() {
        let character: Character = serde_json::from_value(serde_json::json!({
            "id": 7, "anchor": "1/start", "combat": {}
        }))
        .unwrap();
        assert_eq!(
            serde_json::to_value(character.combat.unwrap()).unwrap(),
            serde_json::json!({
                "name": "figure", "max_hp": 30, "defense": 10,
                "attack": {"bonus": 2, "wind_up": 60, "recovery": 40,
                           "damage": {"impact": 4}},
                "immunities": [], "reductions": {}, "faction": "neutral"
            })
        );
        for combat in [
            serde_json::json!({"attack": {"bonus": 2}}),
            serde_json::json!({"unknown": 1}),
            serde_json::json!({"attack": {"bonus": 2, "wind_up": 60,
                "recovery": 40, "damage": {"other": 4}}}),
        ] {
            assert!(serde_json::from_value::<Character>(serde_json::json!({
                "id": 7, "anchor": "1/start", "combat": combat
            }))
            .is_err());
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
            "anatomy": {"slots": ["ring", "ring", "head_armor"]},
            "known_identities": ["healing"],
            "body": {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 91},
            "combat": {
                "name": "guard", "max_hp": 41, "defense": -3, "faction": "guards",
                "attack": {"bonus": -7, "wind_up": 19, "recovery": 23,
                    "damage": {"energy": 1, "impact": 2, "keen": 3, "spirit": 4, "vital": 5}},
                "immunities": ["energy", "vital"],
                "reductions": {"impact": 6, "keen": 7, "spirit": 8}
            }
        });
        let actor: Actor = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&actor).unwrap(), value);
        let combat = actor.combat.unwrap();
        let author_bytes = serde_json::to_vec(&combat).unwrap();
        let compiled = tor_simulation::combat::CombatSpec::from(combat);
        assert_eq!(serde_json::to_value(&compiled).unwrap(), value["combat"]);
        assert_eq!(serde_json::to_vec(&compiled).unwrap(), author_bytes);
        let body = actor.body.unwrap();
        let author_bytes = serde_json::to_vec(&body).unwrap();
        let compiled = tor_simulation::BodySpec::from(body);
        assert_eq!(serde_json::to_value(&compiled).unwrap(), value["body"]);
        assert_eq!(serde_json::to_vec(&compiled).unwrap(), author_bytes);
    }

    #[test]
    fn authoring_ai_defaults_and_conversion_preserve_values() {
        for (declaration, expected) in [
            (
                serde_json::json!({}),
                serde_json::json!({"memory_ticks": 1000, "flee_percent": 25}),
            ),
            (
                serde_json::json!({"flee_percent": 0}),
                serde_json::json!({"memory_ticks": 1000, "flee_percent": 0}),
            ),
            (
                serde_json::json!({"memory_ticks": 0, "flee_percent": 100}),
                serde_json::json!({"memory_ticks": 0, "flee_percent": 100}),
            ),
        ] {
            let author: AiProfile = serde_json::from_value(declaration).unwrap();
            assert_eq!(serde_json::to_value(&author).unwrap(), expected);
            let author_bytes = serde_json::to_vec(&author).unwrap();
            let compiled = tor_simulation::ai::AiProfile::from(author);
            assert_eq!(serde_json::to_value(&compiled).unwrap(), expected);
            assert_eq!(serde_json::to_vec(&compiled).unwrap(), author_bytes);
        }
        assert!(serde_json::from_value::<AiProfile>(serde_json::json!({"unknown": 1})).is_err());
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
            let mut expected = Game::two_room_in_stone(seed);
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
                assert_eq!(actual.act(actor, action), expected.act(actor, action));
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

    fn packages() -> Vec<std::path::PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
        let mut packages: Vec<_> = std::fs::read_dir(root.join("tests"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .chain([root.join("first-dungeon"), root.join("two-room")])
            .filter(|path| path.join("scenario.toml").is_file())
            .collect();
        packages.sort();
        packages
    }

    /// Starting with nothing built and building regions on first load, in
    /// any order, gives exactly the game that building everything at once
    /// gives: a region's content never depends on which were built before.
    #[test]
    fn every_package_built_region_by_region_equals_building_it_whole() {
        use tor_simulation::RegionState;
        for path in packages() {
            let package = Arc::new(read_package(&path).unwrap());
            let all: Vec<_> = package
                .index
                .regions
                .iter()
                .map(|r| RegionId(r.id))
                .collect();
            let mut reversed = all.clone();
            reversed.reverse();
            let mut rotated = all.clone();
            rotated.rotate_left(all.len() / 2);
            for seed in [0, 42] {
                let whole = package.build(seed, true).unwrap();
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
                    let mut records = PackageRecords::new(package.clone(), seed);
                    let mut loaded = BTreeSet::new();
                    for region in order.iter() {
                        loaded.insert(*region);
                        game.transition_regions(
                            &RegionTransition {
                                active: loaded.clone(),
                                loaded: loaded.clone(),
                            },
                            &mut records,
                        )
                        .unwrap_or_else(|e| panic!("{context}: {e:?}"));
                    }
                    assert_eq!(game, whole, "{context}");
                }
            }
        }
    }

    /// The pins' reach answers (each region's exit field, then a cached
    /// search) must agree with an uncached search in every cell of every
    /// checked-in package: rotated and physical portals, stairs, chambers and
    /// their rims. Most cells are far enough from exits for the field.
    #[test]
    fn reach_answers_agree_with_the_search_in_every_package() {
        let mut fielded = 0;
        for path in packages() {
            let game = read_package(&path).unwrap().build(0, true).unwrap();
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
        let restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared);
        assert_eq!(restored, Some(game));
    }

    /// Every checked-in package, shrunk to what its characters' reference
    /// points require and then fully reactivated, equals the original. This
    /// covers real joins, rotations, physical portals, doors and bodies.
    #[test]
    fn every_package_detaches_and_reattaches_exactly() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
        let mut packages: Vec<_> = std::fs::read_dir(root.join("tests"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .chain([root.join("first-dungeon"), root.join("two-room")])
            .filter(|path| path.join("scenario.toml").is_file())
            .collect();
        packages.sort();
        let mut detached = 0;
        for path in packages {
            let package = read_package(&path).unwrap();
            let mut game = package.build(0, true).unwrap();
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
            let mut records = tor_simulation::MemoryRecords::default();
            let roots: BTreeSet<_> = game.region_roots().iter().map(|r| r.region).collect();
            let (_, report) = game
                .transition_regions(
                    &RegionTransition {
                        active: roots.clone(),
                        loaded: roots,
                    },
                    &mut records,
                )
                .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
            assert_eq!(records.len(), report.detached.len());
            detached += report.detached.len();
            let mut shared = tor_simulation::checkpoint::SharedState::default();
            let mut restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared)
                .unwrap_or_else(|| panic!("{}: {report:?}", path.display()));
            assert_eq!(restored, game, "{}", path.display());
            let all: BTreeSet<_> = package
                .index
                .regions
                .iter()
                .map(|r| RegionId(r.id))
                .collect();
            let everything = RegionTransition {
                active: all.clone(),
                loaded: all,
            };
            game.transition_regions(&everything, &mut records).unwrap();
            restored
                .transition_regions(&everything, &mut records)
                .unwrap();
            // Only the record-identity allocator moved.
            original.continue_record_ids(&game);
            assert_eq!(game, original, "{}", path.display());
            assert_eq!(restored, original, "{}", path.display());
        }
        assert!(detached > 0, "some package has a region to detach");
    }
}
