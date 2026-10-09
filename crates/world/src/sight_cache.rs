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

/// Maximum retained scene versions, including versions shared by clones.
const CAPACITY: usize = 512;

static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

fn fresh() -> u64 {
    NEXT_VERSION.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SightMode {
    Geometry,
    Illuminated,
    Neighborhood,
}
type Key = (Location, u8, u8, SightMode);
/// Multiple immutable world versions may share one viewpoint. The serial
/// orders retained versions; geometry/light witnesses still decide validity.
type Scenes = BTreeMap<(Key, u64), Entry>;
const VERSIONS_PER_VIEW: usize = 4;

/// Opaque, process-local geometry witness for derived backend caches.
/// It is neither a world identity nor persisted simulation state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometrySnapshot {
    topology: u64,
    regions: Shared<BTreeMap<RegionId, u64>>,
}

/// Process-local witness for geometry and illumination dependent results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerceptionSnapshot {
    geometry: GeometrySnapshot,
    illumination: Shared<BTreeMap<RegionId, u64>>,
}

struct Entry {
    topology: u64,
    /// Every region the scene read, with its version when the scene was built.
    regions: Vec<(RegionId, u64)>,
    cells: Vec<SightCell>,
    /// Geometry witnesses used only for acceleration, never loading pins.
    proof: Vec<(RegionId, u64)>,
    lighting: Vec<(RegionId, u64)>,
    /// The regions of the visible cells, in order.
    visible: Vec<RegionId>,
}

/// Cells a reach search starts from, and how many steps it takes.
type ReachKey = (Vec<Location>, usize);

struct ReachEntry {
    topology: u64,
    /// Every region the search read, with its version then.
    regions: Vec<(RegionId, u64)>,
    reached: Vec<RegionId>,
}

/// A region's distances to its exits, for a number of steps.
type FieldKey = (RegionId, usize);

/// Cells of a region fewer than the key's steps from a cell whose next step
/// can leave it, with that distance. Absent cells are farther.
pub(crate) type ExitField = Arc<BTreeMap<Location, u8>>;

struct FieldEntry {
    topology: u64,
    regions: Vec<(RegionId, u64)>,
    field: ExitField,
}

/// Version tokens and the scene cache. Tokens are unique within the process:
/// every edit draws a new one, so two worlds holding the same token for a
/// region (a world and its rewound clone, say) hold the same content there.
/// That lets clones share one cache. None of this is world content: it's
/// ignored by equality and never saved, and a loaded world starts fresh.
#[derive(Clone)]
pub(crate) struct SightCache {
    /// Replaced by any change to regions, passages, rotations, physical
    /// portals or chambers, except detaching or attaching a whole region.
    topology: u64,
    /// Replaced for a region by terrain and door edits in it, and when it's
    /// detached or attached. A region absent here is unchanged since
    /// `topology` was drawn.
    regions: Shared<BTreeMap<RegionId, u64>>,
    scenes: Arc<Mutex<Scenes>>,
    illumination: Shared<BTreeMap<RegionId, u64>>,
    /// Availability witnesses for bounded height proofs; terrain and doors
    /// do not affect these metadata-only proofs.
    availability: Shared<BTreeMap<RegionId, u64>>,
    /// Reach searches, which depend on the same geometry as scenes.
    reaches: Arc<Mutex<BTreeMap<ReachKey, ReachEntry>>>,
    /// Exit distance fields, likewise.
    fields: Arc<Mutex<BTreeMap<FieldKey, FieldEntry>>>,
}

impl Default for SightCache {
    fn default() -> Self {
        Self {
            topology: fresh(),
            regions: Shared::default(),
            scenes: Arc::default(),
            illumination: Shared::default(),
            availability: Shared::default(),
            reaches: Arc::default(),
            fields: Arc::default(),
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
    fn scene<'a>(&self, scenes: &'a Scenes, key: Key) -> Option<&'a Entry> {
        scenes
            .range((key, 0)..=(key, u64::MAX))
            .rev()
            .map(|(_, entry)| entry)
            .find(|entry| self.valid(entry))
    }
    pub(crate) fn geometry_snapshot(&self) -> GeometrySnapshot {
        GeometrySnapshot {
            topology: self.topology,
            regions: self.regions.clone(),
        }
    }

    pub(crate) fn perception_snapshot(&self) -> PerceptionSnapshot {
        PerceptionSnapshot {
            geometry: self.geometry_snapshot(),
            illumination: self.illumination.clone(),
        }
    }
    /// Invalidates every scene, including in clones that share the cache.
    pub(crate) fn topology_changed(&mut self) {
        self.topology = fresh();
        self.regions = Shared::default();
        self.availability = Shared::default();
    }

    pub(crate) fn region_presence_changed(&mut self, region: RegionId) {
        self.region_changed(region);
        self.availability.insert(region, fresh());
    }

    pub(crate) fn region_changed(&mut self, region: RegionId) {
        self.regions.insert(region, fresh());
    }

    fn version(&self, region: RegionId) -> u64 {
        self.regions.get(&region).copied().unwrap_or(0)
    }

    pub(crate) fn lighting_changed(&mut self, region: RegionId) {
        self.illumination.insert(region, fresh());
    }

    fn valid(&self, entry: &Entry) -> bool {
        entry.lighting.iter().all(|(region, version)| {
            self.illumination.get(region).copied().unwrap_or(0) == *version
        }) && self.current(entry.topology, &entry.regions)
            && entry.proof.iter().all(|(region, version)| {
                self.availability.get(region).copied().unwrap_or(0) == *version
            })
    }

    fn current(&self, topology: u64, regions: &[(RegionId, u64)]) -> bool {
        topology == self.topology
            && regions
                .iter()
                .all(|&(region, version)| self.version(region) == version)
    }

    /// The regions a still-valid cached scene's visible cells are in.
    pub(crate) fn visible(&self, eye: Location, frame: u8, radius: u8) -> Option<Vec<RegionId>> {
        let scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        let entry = self.scene(&scenes, (eye, frame, radius, SightMode::Geometry))?;
        Some(entry.visible.clone())
    }

    /// A still-valid cached reach.
    pub(crate) fn reach(&self, key: &ReachKey) -> Option<Vec<RegionId>> {
        let reaches = self.reaches.lock().unwrap_or_else(|e| e.into_inner());
        let entry = reaches.get(key)?;
        self.current(entry.topology, &entry.regions)
            .then(|| entry.reached.clone())
    }

    /// A still-valid exit distance field.
    pub(crate) fn field(&self, region: RegionId, steps: usize) -> Option<ExitField> {
        let fields = self.fields.lock().unwrap_or_else(|e| e.into_inner());
        let entry = fields.get(&(region, steps))?;
        self.current(entry.topology, &entry.regions)
            .then(|| entry.field.clone())
    }

    /// `read` must name every region whose geometry the field read.
    pub(crate) fn insert_field(
        &self,
        region: RegionId,
        steps: usize,
        read: impl IntoIterator<Item = RegionId>,
        field: ExitField,
    ) {
        let entry = FieldEntry {
            topology: self.topology,
            regions: read
                .into_iter()
                .map(|region| (region, self.version(region)))
                .collect(),
            field,
        };
        let mut fields = self.fields.lock().unwrap_or_else(|e| e.into_inner());
        if fields.len() >= CAPACITY && !fields.contains_key(&(region, steps)) {
            fields.clear();
        }
        fields.insert((region, steps), entry);
    }

    /// `read` must name every region whose geometry the search read.
    pub(crate) fn insert_reach(
        &self,
        key: ReachKey,
        read: impl IntoIterator<Item = RegionId>,
        reached: Vec<RegionId>,
    ) {
        let entry = ReachEntry {
            topology: self.topology,
            regions: read
                .into_iter()
                .map(|region| (region, self.version(region)))
                .collect(),
            reached,
        };
        let mut reaches = self.reaches.lock().unwrap_or_else(|e| e.into_inner());
        if reaches.len() >= CAPACITY && !reaches.contains_key(&key) {
            reaches.clear();
        }
        reaches.insert(key, entry);
    }

    pub(crate) fn get(&self, eye: Location, frame: u8, radius: u8) -> Option<Vec<SightCell>> {
        self.get_mode(eye, frame, radius, SightMode::Geometry)
    }

    pub(crate) fn get_mode(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
        mode: SightMode,
    ) -> Option<Vec<SightCell>> {
        let scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        let entry = self.scene(&scenes, (eye, frame, radius, mode))?;
        Some(entry.cells.clone())
    }

    /// The regions a still-valid cached scene read.
    pub(crate) fn regions(&self, eye: Location, frame: u8, radius: u8) -> Option<Vec<RegionId>> {
        self.regions_mode(eye, frame, radius, SightMode::Geometry)
    }

    pub(crate) fn regions_mode(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
        mode: SightMode,
    ) -> Option<Vec<RegionId>> {
        let scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        let entry = self.scene(&scenes, (eye, frame, radius, mode))?;
        Some(entry.regions.iter().map(|&(region, _)| region).collect())
    }

    pub(crate) fn contains(&self, eye: Location, frame: u8, radius: u8) -> bool {
        let scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        self.scene(&scenes, (eye, frame, radius, SightMode::Geometry))
            .is_some()
    }

    /// `regions` must name every region whose terrain or doors the scene read.
    pub(crate) fn insert(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
        regions: impl IntoIterator<Item = RegionId>,
        cells: &[SightCell],
        proof: &[RegionId],
    ) {
        self.insert_mode(
            (eye, frame, radius, SightMode::Geometry),
            regions,
            cells,
            proof,
        );
    }

    pub(crate) fn insert_mode(
        &self,
        key: Key,
        regions: impl IntoIterator<Item = RegionId>,
        cells: &[SightCell],
        proof: &[RegionId],
    ) {
        let (_, _, _, mode) = key;
        let mut visible: Vec<_> = cells.iter().map(|c| c.location.region).collect();
        visible.sort();
        visible.dedup();
        let regions: Vec<_> = regions.into_iter().collect();
        let lighting = if mode == SightMode::Illuminated {
            regions
                .iter()
                .map(|r| (*r, self.illumination.get(r).copied().unwrap_or(0)))
                .collect()
        } else {
            Vec::new()
        };
        let entry = Entry {
            proof: proof
                .iter()
                .map(|region| (*region, self.availability.get(region).copied().unwrap_or(0)))
                .collect(),
            lighting,
            topology: self.topology,
            regions: regions
                .into_iter()
                .map(|region| (region, self.version(region)))
                .collect(),
            cells: cells.to_vec(),
            visible,
        };
        let mut scenes = self.scenes.lock().unwrap_or_else(|e| e.into_inner());
        if self.scene(&scenes, key).is_some() {
            return;
        }
        let versions: Vec<_> = scenes
            .range((key, 0)..=(key, u64::MAX))
            .map(|(key, _)| *key)
            .collect();
        if versions.len() >= VERSIONS_PER_VIEW {
            scenes.remove(&versions[0]);
        }
        // Capacity counts versions, not viewpoints, keeping the original
        // memory bound even when tentative games share a viewpoint.
        if scenes.len() >= CAPACITY {
            let oldest = *scenes
                .keys()
                .min_by_key(|(_, serial)| *serial)
                .expect("cache at capacity is nonempty");
            scenes.remove(&oldest);
        }
        scenes.insert((key, fresh()), entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    #[test]
    fn capacity_evicts_oldest_scene_without_flushing_recent_views() {
        let cache = SightCache::default();
        let eye = |x| Location {
            region: RegionId(1),
            position: Position { x, y: 0, z: 0 },
        };
        for x in 0..=CAPACITY as i32 {
            cache.insert_mode(
                (eye(x), 0, 16, SightMode::Geometry),
                [RegionId(1)],
                &[],
                &[],
            );
        }
        assert!(!cache.contains(eye(0), 0, 16));
        assert!(cache.contains(eye(1), 0, 16));
        assert!(cache.contains(eye(CAPACITY as i32), 0, 16));
        assert_eq!(cache.scenes.lock().unwrap().len(), CAPACITY);
    }

    #[test]
    fn scene_versions_preserve_clone_reuse_with_bounded_retention() {
        let eye = Location {
            region: RegionId(1),
            position: Position { x: 0, y: 0, z: 0 },
        };
        for mode in [
            SightMode::Geometry,
            SightMode::Illuminated,
            SightMode::Neighborhood,
        ] {
            let mut cache = SightCache::default();
            let mut branches = Vec::new();
            for _ in 0..6 {
                if mode == SightMode::Illuminated {
                    cache.lighting_changed(eye.region);
                } else {
                    cache.region_changed(eye.region);
                }
                cache.insert_mode((eye, 0, 16, mode), [eye.region], &[], &[]);
                branches.push(cache.clone());
            }
            assert_eq!(cache.scenes.lock().unwrap().len(), VERSIONS_PER_VIEW);
            for (i, branch) in branches.iter().enumerate() {
                assert_eq!(branch.get_mode(eye, 0, 16, mode).is_some(), i >= 2);
            }
            for x in 1..=600 {
                cache.insert_mode(
                    (
                        Location {
                            position: Position { x, ..eye.position },
                            ..eye
                        },
                        0,
                        16,
                        mode,
                    ),
                    [eye.region],
                    &[],
                    &[],
                );
                assert!(cache.scenes.lock().unwrap().len() <= CAPACITY);
            }
        }
    }
}
