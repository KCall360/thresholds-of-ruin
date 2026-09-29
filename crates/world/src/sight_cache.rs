//! Reuse of exact 3D sight scenes. A cached scene is returned only while every
//! region it depends on is unchanged, so the cache never changes results; see
//! `docs/sight-3d.md`.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crate::{Location, RegionId, Shared, SightCell};

/// Scenes kept before the cache is emptied and refilled on demand.
const CAPACITY: usize = 512;

static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

fn fresh() -> u64 {
    NEXT_VERSION.fetch_add(1, Ordering::Relaxed)
}

type Key = (Location, u8, u8);

struct Entry {
    topology: u64,
    /// Every region the scene read, with its version when the scene was built.
    regions: Vec<(RegionId, u64)>,
    cells: Vec<SightCell>,
}

/// Version tokens and the scene cache. Tokens are unique within the process:
/// every edit draws a new one, so two worlds holding the same token for a
/// region (a world and its rewound clone, say) hold the same content there.
/// That lets clones share one cache. None of this is world content: it's
/// ignored by equality and never saved, and a loaded world starts fresh.
#[derive(Clone)]
pub(crate) struct SightCache {
    /// Replaced by any change to regions, passages, rotations, physical
    /// portals or chambers.
    topology: u64,
    /// Replaced for a region by terrain and door edits in it. A region absent
    /// here is unchanged since `topology` was drawn.
    regions: Shared<BTreeMap<RegionId, u64>>,
    scenes: Arc<Mutex<BTreeMap<Key, Entry>>>,
}

impl Default for SightCache {
    fn default() -> Self {
        Self {
            topology: fresh(),
            regions: Shared::default(),
            scenes: Arc::default(),
        }
    }
}

impl PartialEq for SightCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for SightCache {}

impl std::fmt::Debug for SightCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SightCache")
    }
}

impl SightCache {
    /// Invalidates every scene, including in clones that share the cache.
    pub(crate) fn topology_changed(&mut self) {
        self.topology = fresh();
        self.regions = Shared::default();
    }

    pub(crate) fn region_changed(&mut self, region: RegionId) {
        self.regions.insert(region, fresh());
    }

    fn version(&self, region: RegionId) -> u64 {
        self.regions.get(&region).copied().unwrap_or(0)
    }

    fn valid(&self, entry: &Entry) -> bool {
        entry.topology == self.topology
            && entry
                .regions
                .iter()
                .all(|&(region, version)| self.version(region) == version)
    }

    pub(crate) fn get(&self, eye: Location, frame: u8, radius: u8) -> Option<Vec<SightCell>> {
        let scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        let entry = scenes.get(&(eye, frame, radius))?;
        self.valid(entry).then(|| entry.cells.clone())
    }

    pub(crate) fn contains(&self, eye: Location, frame: u8, radius: u8) -> bool {
        let scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        scenes
            .get(&(eye, frame, radius))
            .is_some_and(|entry| self.valid(entry))
    }

    /// `regions` must name every region whose terrain or doors the scene read.
    pub(crate) fn insert(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
        regions: impl IntoIterator<Item = RegionId>,
        cells: &[SightCell],
    ) {
        let entry = Entry {
            topology: self.topology,
            regions: regions
                .into_iter()
                .map(|region| (region, self.version(region)))
                .collect(),
            cells: cells.to_vec(),
        };
        let mut scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        if scenes.len() >= CAPACITY && !scenes.contains_key(&(eye, frame, radius)) {
            scenes.clear();
        }
        scenes.insert((eye, frame, radius), entry);
    }
}
