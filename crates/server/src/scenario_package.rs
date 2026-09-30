//! Ordinary authored inputs. Filesystem access stays in the server; construction
//! uses the same deterministic world operations as other simulation callers.
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::path::{Component, Path};
use std::sync::Arc;

use crate::{Failure, Scenario};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tor_simulation::Game;
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

pub const RULESET: &str = "dungeon-v17";
const VALIDATOR: &str = "tor-scenario-6";
/// The manifest and the validator's files are bounded to this.
const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// The package layout this version reads: `scenario.toml`, one file per
/// region in `regions/`, and the validator's `index.json`.
pub const FORMAT: u32 = 2;
const REGION_DIR: &str = "regions";
/// Regions a package may have.
pub const MAX_REGIONS: usize = 65_536;
/// Each region file is bounded to this.
const MAX_REGION_BYTES: u64 = 1024 * 1024;
/// The index is bounded to this, which holds the largest package.
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;

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
fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub factions: BTreeMap<String, std::collections::BTreeSet<String>>,
    #[serde(default)]
    pub ai_profiles: BTreeMap<String, tor_simulation::ai::AiProfile>,
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
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub themes: Option<Vec<String>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearancePool {
    pub appearances: Vec<String>,
    #[serde(default)]
    pub confounding: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    pub combat: Option<tor_simulation::combat::CombatSpec>,
    pub body: Option<tor_simulation::BodySpec>,
    pub identity: Option<String>,
    pub appearance_pool: Option<String>,
    #[serde(default)]
    pub stackable: bool,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    pub name: Option<String>,
    pub turn_ticks: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub combat: Option<tor_simulation::combat::CombatSpec>,
    pub body: Option<tor_simulation::BodySpec>,
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
    pub places: Vec<[i32; 3]>,
    #[serde(default)]
    pub portals: Vec<Portal>,
    #[serde(default)]
    pub doors: Vec<Door>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub actors: Vec<Actor>,
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
    pub combat: Option<tor_simulation::combat::CombatSpec>,
    pub body: Option<tor_simulation::BodySpec>,
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
    /// Each outgoing portal's destination anchor, in authored order.
    pub portals: Vec<String>,
    pub actors: Vec<IndexedActor>,
    pub items: Vec<IndexedItem>,
    pub doors: Vec<u64>,
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
    texts: BTreeMap<u64, Arc<str>>,
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
            parsed: Default::default(),
            files_read: 0,
        })))
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
    /// Keep a region's text in memory, as the save's copy.
    pub(crate) fn insert(&self, region: u64, text: Arc<str>) {
        self.0.lock().unwrap().texts.insert(region, text);
    }
    /// A region file's text, checked against the index.
    pub(crate) fn text(&self, entry: &IndexedRegion) -> Result<Arc<str>, Failure> {
        let (text, directory) = {
            let s = self.0.lock().unwrap();
            (s.texts.get(&entry.id).cloned(), s.directory.clone())
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
        require(
            digest(text.as_bytes()) == entry.hash,
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
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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
    require(
        path.metadata().map_err(|e| fail(e.to_string()))?.len() <= limit,
        format!("{relative} exceeds {} KiB", limit / 1024),
    )?;
    std::fs::read_to_string(path).map_err(|e| fail(format!("{relative}: {e}")))
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
        regions.push(IndexedRegion::of(&def, file, digest(text.as_bytes())));
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
    fn assemble(
        manifest: Manifest,
        manifest_text: &str,
        index: RegionIndex,
        sources: RegionSources,
        directory: Option<&Path>,
    ) -> Result<Self, Failure> {
        let files = BTreeMap::from([
            ("scenario.toml".into(), digest(manifest_text.as_bytes())),
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
            coverage: "all authored regions; all character starts; deterministic construction twice at seeds 0, 1, 42; no generation or winnability proof".into(),
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
                digest(text.as_bytes()),
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
        package.check_region(&def)?;
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
    package.check_identity()?;
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
    pub(crate) fn check_identity(&self) -> Result<(), Failure> {
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
    }
    fn supported(&self) -> Result<(), Failure> {
        for (faction, enemies) in &self.manifest.factions {
            require(
                label(faction)
                    && enemies
                        .iter()
                        .all(|e| self.manifest.factions.contains_key(e)),
                "Invalid faction relationship",
            )?;
        }
        for (name, profile) in &self.manifest.ai_profiles {
            require(label(name) && profile.valid(), "Invalid AI profile")?;
        }
        for spec in self
            .manifest
            .characters
            .iter()
            .filter_map(|c| c.combat.as_ref())
            .chain(
                self.manifest
                    .archetypes
                    .values()
                    .filter_map(|a| a.combat.as_ref()),
            )
        {
            self.check_combat(spec)?;
        }
        Ok(())
    }
    fn check_combat(&self, spec: &tor_simulation::combat::CombatSpec) -> Result<(), Failure> {
        require(
            spec.valid()
                && (self.manifest.factions.is_empty()
                    || self.manifest.factions.contains_key(&spec.faction)),
            "Invalid combat attributes or faction",
        )
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
        let identities: BTreeSet<_> = self
            .manifest
            .archetypes
            .iter()
            .map(|(id, a)| a.identity.as_ref().unwrap_or(id))
            .collect();
        require(
            self.manifest
                .characters
                .iter()
                .all(|c| c.known_identities.iter().all(|id| identities.contains(id))),
            "Unknown initial item identity",
        )?;
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
            require(
                r.zone
                    .as_ref()
                    .is_none_or(|z| self.manifest.zones.contains_key(z)),
                "Unknown zone reference",
            )?;
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
            let at = anchors
                .get(&c.anchor)
                .ok_or_else(|| fail(format!("Character {}: missing anchor {}", c.id, c.anchor)))?;
            starts.insert(c.id, at.region.0);
        }
        // A carried item is authored in the region its carrier starts in,
        // so building a region reads only that region's file.
        for r in &self.index.regions {
            for i in &r.items {
                if let Some(owner) = i.carried_by {
                    let start = starts
                        .get(&owner)
                        .ok_or_else(|| fail("Unknown inventory owner"))?;
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
        if let Some(o) = &self.manifest.objective {
            require(
                anchors.contains_key(&o.anchor) && o.item.is_none_or(|id| item_ids.contains(&id)),
                "Invalid objective anchor/item reference",
            )?;
        }
        Ok(())
    }
    /// What only a region's own file can show: its actors' controllers and
    /// combat. The validator checks every region; building one checks it.
    fn check_region(&self, r: &RegionDef) -> Result<(), Failure> {
        for a in &r.actors {
            require(
                matches!(a.controller.as_str(), "external" | "ai")
                    && (a.controller == "ai") == a.ai.is_some()
                    && a.ai.as_ref().is_none_or(|s| label(s)),
                "Invalid actor controller",
            )?;
            if let Some(spec) = &a.combat {
                self.check_combat(spec)?;
            }
        }
        Ok(())
    }
    fn appearance_mapping(&self, seed: u64) -> Result<BTreeMap<String, String>, Failure> {
        let mut result = BTreeMap::new();
        let mut signatures = BTreeMap::new();
        for (key, a) in &self.manifest.archetypes {
            let identity = a.identity.as_ref().unwrap_or(key);
            require(label(identity), "Invalid item identity")?;
            let signature = (&a.name, &a.appearance_pool);
            if let Some(previous) = signatures.insert(identity, signature) {
                require(
                    previous == signature,
                    "One identity must have one name and appearance pool",
                )?;
            }
            if let Some(pool) = &a.appearance_pool {
                require(
                    a.name.is_some() && self.manifest.appearance_pools.contains_key(pool),
                    "Unknown appearance pool or missing identity name",
                )?;
            }
        }
        for (key, pool) in &self.manifest.appearance_pools {
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
    fn archetype(&self, key: &Option<String>) -> Result<Archetype, Failure> {
        key.as_ref()
            .map(|key| {
                self.manifest
                    .archetypes
                    .get(key)
                    .cloned()
                    .ok_or_else(|| fail(format!("Unknown archetype {key}")))
            })
            .unwrap_or(Ok(Archetype::default()))
    }
    pub(crate) fn build(&self, seed: u64, _runtime: bool) -> Result<Game, Failure> {
        self.check()?;
        self.supported()?;
        let index = self.index(seed)?;
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
        );
        let defs = self.region_defs()?;
        for r in &defs {
            self.check_region(r)?;
        }
        let all: Vec<&RegionDef> = defs.iter().collect();
        self.add_geometry(&mut game, &all)?;
        for r in &defs {
            self.add_structure(&mut game, r, &index.anchors)?;
        }
        for (name, position) in &index.anchors {
            require(
                game.authored_cell_valid(*position),
                format!("Anchor {name}: outside traversable geometry"),
            )?;
        }
        self.add_entities(&mut game, seed, &index, &all)?;
        self.configure_run(&mut game, &index.anchors)?;
        Ok(game)
    }

    /// Whole-package facts that building regions needs, computed once so
    /// building one region reads only that region and its neighbours.
    pub(crate) fn index(&self, seed: u64) -> Result<PackageIndex, Failure> {
        let anchors = self.anchors()?;
        let mut anchors_by_region: BTreeMap<u64, Vec<(String, Location)>> = BTreeMap::new();
        for (name, at) in &anchors {
            anchors_by_region
                .entry(at.region.0)
                .or_default()
                .push((name.clone(), *at));
        }
        let mut spawns = BTreeMap::new();
        for c in &self.manifest.characters {
            if c.id == self.selected || c.unselected == "ai" {
                let at = *anchors
                    .get(&c.anchor)
                    .ok_or_else(|| fail("Missing character anchor"))?;
                spawns.insert(c.id, (at, c.turn_ticks));
            }
        }
        for r in &self.index.regions {
            for a in &r.actors {
                let ticks = a
                    .turn_ticks
                    .or(self.archetype(&a.archetype)?.turn_ticks)
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
        Ok(PackageIndex {
            anchors,
            anchors_by_region,
            homes,
            spawns: spawns_by_region,
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
        let mut identities: BTreeMap<u64, tor_simulation::RegionIdentities> = BTreeMap::new();
        for (id, home) in &index.homes {
            identities
                .entry(home.region.0)
                .or_default()
                .actors
                .insert(tor_simulation::ActorId(*id));
        }
        for r in &self.index.regions {
            for i in r
                .items
                .iter()
                .filter(|i| !self.omitted_carrier(i.carried_by))
            {
                identities
                    .entry(r.id)
                    .or_default()
                    .items
                    .insert(tor_simulation::ItemId(i.id));
            }
            identities
                .entry(r.id)
                .or_default()
                .doors
                .extend(r.doors.iter().copied());
        }
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
        );
        for r in &self.index.regions {
            game.add_unbuilt_region(
                region_of(r.id, &r.name, r.size)?,
                r.chamber,
                identities.remove(&r.id).unwrap_or_default(),
            )
            .map_err(|e| fail(format!("Region {}: {e:?}", r.id)))?;
        }
        self.configure_run(&mut game, &index.anchors)?;
        Ok(game)
    }

    /// One region's starting record, built in a scratch game holding it and
    /// its neighbours' geometry, so its links and entities are checked
    /// exactly as [`Package::build`] checks them. Reads only that region and
    /// its neighbours (through `index`), so the result doesn't depend on
    /// which regions were built before, and the cost doesn't depend on the
    /// package's size.
    pub(crate) fn build_region(
        &self,
        seed: u64,
        index: &PackageIndex,
        region: u64,
    ) -> Result<tor_simulation::RegionRecord, Failure> {
        let r = index.region(self, region)?;
        self.check_region(&r)?;
        let mut shell: Vec<Arc<RegionDef>> = vec![r.clone()];
        for to in r.portals.iter().filter_map(|p| index.anchors.get(&p.to)) {
            if shell.iter().all(|s| s.id != to.region.0) {
                shell.push(index.region(self, to.region.0)?);
            }
        }
        shell.sort_by_key(|s| s.id);
        let shell: Vec<&RegionDef> = shell.iter().map(|s| &**s).collect();
        let mut game = Game::new(
            World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?,
            seed,
        );
        self.add_geometry(&mut game, &shell)?;
        self.add_structure(&mut game, &r, &index.anchors)?;
        for (name, position) in index.anchors_by_region.get(&region).into_iter().flatten() {
            require(
                game.authored_cell_valid(*position),
                format!("Anchor {name}: outside traversable geometry"),
            )?;
        }
        self.add_entities(&mut game, seed, index, &[&r])?;
        game.into_region_record(RegionId(region))
            .map_err(|e| fail(format!("Region {region}: {e:?}")))
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
        for p in &r.portals {
            let direction = match p.direction.as_str() {
                "north" => Direction::North,
                "east" => Direction::East,
                "south" => Direction::South,
                "west" => Direction::West,
                "up" => Direction::Up,
                "down" => Direction::Down,
                _ => return Err(fail("Invalid portal direction")),
            };
            let to = *anchors
                .get(&p.to)
                .ok_or_else(|| fail(format!("Missing portal anchor {}", p.to)))?;
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
            game.set_place_hint(loc(r.id, *p), true)
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

    /// Items carried by an omitted character are omitted with it.
    fn omitted_carrier(&self, carried_by: Option<u64>) -> bool {
        carried_by.is_some_and(|id| {
            id != self.selected
                && self
                    .manifest
                    .characters
                    .iter()
                    .any(|c| c.id == id && c.unselected == "omit")
        })
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
        let mut spawns: Vec<_> = chosen
            .iter()
            .flat_map(|r| index.spawns.get(r).into_iter().flatten())
            .copied()
            .collect();
        spawns.sort_by_key(|(id, _, _)| *id);
        for (id, at, ticks) in spawns {
            game.spawn_authored_actor(
                id,
                at,
                NonZeroU64::new(ticks).ok_or_else(|| fail("Zero actor duration"))?,
            )
            .map_err(|e| fail(format!("Actor {id}: {e:?}")))?;
        }
        for c in &self.manifest.characters {
            if (c.id == self.selected || c.unselected == "ai")
                && homes.get(&c.id).is_some_and(|h| keep(h.region.0))
            {
                if let Some(spec) = c.combat.clone().or_else(|| {
                    self.manifest
                        .objective
                        .as_ref()
                        .map(|_| tor_simulation::combat::CombatSpec::default())
                }) {
                    game.configure_combat(tor_simulation::ActorId(c.id), spec)
                        .map_err(|_| fail("Invalid character combat specification"))?;
                }
                if c.id != self.selected && c.unselected == "ai" {
                    let profile =
                        c.ai.as_ref()
                            .and_then(|name| self.manifest.ai_profiles.get(name))
                            .ok_or_else(|| fail("Unknown character AI profile"))?;
                    game.configure_ai(tor_simulation::ActorId(c.id), profile.clone())
                        .map_err(|_| fail("AI requires combat attributes"))?;
                }
                if let Some(body) = &c.body {
                    if !body.cells.contains(&body.eye) {
                        return Err(fail("Character body eye must be one of its cells"));
                    }
                    game.set_body(tor_simulation::ActorId(c.id), body.clone())
                        .map_err(|_| fail("Character body does not fit"))?;
                }
                if let Some(v) = c.velocity {
                    game.set_actor_velocity(tor_simulation::ActorId(c.id), v)
                        .map_err(|_| fail("Invalid character velocity"))?;
                }
            }
        }
        for r in regions {
            for a in &r.actors {
                if let Some(spec) = a
                    .combat
                    .as_ref()
                    .or(self.archetype(&a.archetype)?.combat.as_ref())
                {
                    game.configure_combat(tor_simulation::ActorId(a.id), spec.clone())
                        .map_err(|_| fail("Invalid actor combat specification"))?;
                }
                if a.controller == "ai" {
                    let profile =
                        a.ai.as_ref()
                            .and_then(|name| self.manifest.ai_profiles.get(name))
                            .ok_or_else(|| fail("Unknown actor AI profile"))?;
                    game.configure_ai(tor_simulation::ActorId(a.id), profile.clone())
                        .map_err(|_| fail("AI requires combat attributes"))?;
                }
                if let Some(body) = a
                    .body
                    .as_ref()
                    .or(self.archetype(&a.archetype)?.body.as_ref())
                {
                    if !body.cells.contains(&body.eye) {
                        return Err(fail("Actor body eye must be one of its cells"));
                    }
                    game.set_body(tor_simulation::ActorId(a.id), body.clone())
                        .map_err(|_| fail("Actor body does not fit"))?;
                }
                if let Some(v) = a.velocity {
                    game.set_actor_velocity(tor_simulation::ActorId(a.id), v)
                        .map_err(|_| fail("Invalid actor velocity"))?;
                }
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
            let archetype = self.archetype(&i.archetype)?;
            let name = if i.seed_names.is_empty() {
                i.name
                    .clone()
                    .or(archetype.name.clone())
                    .ok_or_else(|| fail("Item needs a name or archetype"))?
            } else {
                require(
                    i.name.is_none(),
                    "Item cannot have both name and seed_names",
                )?;
                i.seed_names[(seed % i.seed_names.len() as u64) as usize].clone()
            };
            require(
                label(&name) && i.seed_names.iter().all(|s| label(s)),
                "Invalid item name",
            )?;
            let key = i.archetype.clone().unwrap_or_else(|| name.clone());
            let identity = archetype.identity.clone().unwrap_or_else(|| key.clone());
            let concealed = archetype.appearance_pool.is_some();
            require(
                !concealed || (i.name.is_none() && i.seed_names.is_empty()),
                "Concealed items cannot override their identity name",
            )?;
            let appearance = appearances
                .get(&identity)
                .cloned()
                .unwrap_or_else(|| name.clone());
            let stackable = i.stackable.unwrap_or(archetype.stackable);
            require(
                self.manifest.objective.as_ref().and_then(|o| o.item) != Some(i.id) || !stackable,
                "Objective item instances must be non-stackable",
            )?;
            let mut properties = archetype.properties.clone();
            properties.extend(i.properties.clone());
            let spec = tor_simulation::ItemSpec {
                archetype: key,
                identity,
                name,
                appearance,
                concealed,
                stackable,
                properties,
            };
            require(
                i.quantity > 0 && (stackable || i.quantity == 1),
                "Invalid item quantity or non-stackable count",
            )?;
            require(
                spec.properties.len() <= 32
                    && spec.properties.iter().all(|(k, v)| {
                        label(k) && v.len() <= 80 && !v.chars().any(char::is_control)
                    }),
                "Invalid item properties",
            )?;
            // Inventory of omitted characters is omitted with its owner.
            if self.omitted_carrier(i.carried_by) {
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
            .map_err(|e| fail(format!("Item {}: {e:?}", i.id)))?;
        }
        for c in &self.manifest.characters {
            if (c.id == self.selected || c.unselected != "omit")
                && homes.get(&c.id).is_some_and(|h| keep(h.region.0))
            {
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

    fn configure_run(
        &self,
        game: &mut Game,
        anchors: &BTreeMap<String, Location>,
    ) -> Result<(), Failure> {
        if self.manifest.characters.iter().any(|c| c.combat.is_some())
            || self.manifest.objective.is_some()
        {
            let objective =
                self.manifest
                    .objective
                    .as_ref()
                    .map(|o| tor_simulation::combat::Objective {
                        anchor: anchors[&o.anchor],
                        item: o.item.map(tor_simulation::ItemId),
                        disclosed: o.disclosed,
                        continue_play: o.continue_play,
                    });
            let characters = self
                .manifest
                .characters
                .iter()
                .filter(|c| c.id == self.selected || c.unselected == "ai")
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
    anchors: BTreeMap<String, Location>,
    anchors_by_region: BTreeMap<u64, Vec<(String, Location)>>,
    /// Where each spawned actor starts.
    homes: BTreeMap<u64, Location>,
    /// Actors by the region they start in: identity, start, turn length.
    spawns: BTreeMap<u64, Vec<(u64, Location, u64)>>,
    appearances: BTreeMap<String, String>,
    /// Region definitions handed out, for scaling contracts.
    lookups: std::cell::Cell<usize>,
}

impl PackageIndex {
    fn region(&self, package: &Package, id: u64) -> Result<Arc<RegionDef>, Failure> {
        self.lookups.set(self.lookups.get() + 1);
        package.region_def(id)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    assert!(
                        all.iter()
                            .all(|r| game.region_state(*r) == Some(RegionState::Unbuilt)),
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
    fn building_a_region_reads_only_it_and_its_neighbours() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
        let directory = tempfile::tempdir().unwrap();
        let scenario = streaming_corridor(&root, directory.path(), 256, 5).unwrap();
        let package = scenario.package.unwrap();
        let index = package.index(5).unwrap();
        for region in 1..=256 {
            let before = index.lookups();
            package.build_region(5, &index, region).unwrap();
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
