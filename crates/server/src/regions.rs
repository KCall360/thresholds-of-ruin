//! Region streaming in the engine: which regions each committed command keeps
//! active and loaded, and where region records are kept. See
//! `docs/region-streaming.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tor_simulation::{
    Game, RecordId, RecordStore, RegionRecord, RegionTransition, TransitionReport,
};
use tor_world::{RegionId, Shared};

use crate::engine::storage_failure;
use crate::region_streaming::RegionCatalog;
use crate::scenario_package::Package;
use crate::Failure;

/// Portal hops kept active and loaded around each reference point that
/// doesn't set its own radii. Saved with the scenario, so replay applies the
/// same transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Streaming {
    pub active_radius: u32,
    pub load_radius: u32,
}

impl Default for Streaming {
    fn default() -> Self {
        Self {
            active_radius: 1,
            load_radius: 2,
        }
    }
}

/// What a transition did, and the deterministic work it took. Performance
/// contracts require none of the counts to grow with the world's size.
#[derive(Clone, Debug, Default)]
pub(crate) struct TransitionWork {
    pub(crate) report: TransitionReport,
    pub(crate) horizon_regions_expanded: usize,
    pub(crate) horizon_links_examined: usize,
    pub(crate) pinned_actors: usize,
    pub(crate) reach_lookups: usize,
    pub(crate) records_read: usize,
    /// The game just before the transition, when it changed anything.
    pub(crate) before: Option<Game>,
}

/// Durable records kept in memory besides those not yet on disk.
const CACHED_RECORDS: usize = 32;

/// Where the engine's region records are: built from the package on first
/// load, kept in memory until they're on disk, and read back on demand.
#[derive(Clone, Debug)]
pub(crate) struct Regions {
    package: Arc<Package>,
    /// Built once, so building a region costs the same in any package.
    index: crate::scenario_package::PackageIndex,
    seed: u64,
    catalog: RegionCatalog,
    streaming: Streaming,
    resident: BTreeMap<RecordId, Shared<RegionRecord>>,
    /// Records known to be on disk; only these may leave memory.
    durable: BTreeSet<RecordId>,
    disk: Option<crate::storage::Store>,
    /// Records read from disk, for the recovery profile.
    pub(crate) reads: usize,
}

impl Regions {
    pub(crate) fn new(
        package: Arc<Package>,
        seed: u64,
        streaming: Streaming,
    ) -> Result<Self, Failure> {
        Ok(Self {
            catalog: RegionCatalog::from_package(&package)?,
            index: package.index(seed)?,
            package,
            seed,
            streaming,
            resident: BTreeMap::new(),
            durable: BTreeSet::new(),
            disk: None,
            reads: 0,
        })
    }

    pub(crate) fn attach_disk(&mut self, disk: crate::storage::Store) {
        self.disk = Some(disk);
    }

    /// The region sets the game's reference points ask for: each point's
    /// region and everything within its radii, by the package's links. A
    /// region the package doesn't have (a wizard added it) counts alone;
    /// pins still follow its links.
    fn horizon(&self, game: &Game, work: &mut TransitionWork) -> RegionTransition {
        let mut t = RegionTransition::default();
        for root in game.region_roots() {
            let roots = BTreeSet::from([root.region]);
            let none = BTreeSet::new();
            let mut within = |hops: usize| match self.catalog.plan(&roots, hops, &none) {
                Ok(plan) => {
                    work.horizon_regions_expanded += plan.expanded_regions;
                    work.horizon_links_examined += plan.examined_links;
                    plan.required
                }
                Err(_) => roots.clone(),
            };
            let hops = |radius: Option<u32>, default: u32| radius.unwrap_or(default) as usize;
            let active = hops(root.active_radius, self.streaming.active_radius);
            let loaded = hops(root.load_radius, self.streaming.load_radius).max(active);
            t.active.extend(within(active));
            t.loaded.extend(within(loaded));
        }
        t
    }

    /// Move `game` to the regions its reference points ask for, growing the
    /// sets as pins require. New records go to `made`, not to this store:
    /// the caller keeps them only if the command publishes.
    pub(crate) fn transition(
        &mut self,
        game: &mut Game,
        made: &mut Vec<(RecordId, Shared<RegionRecord>)>,
    ) -> Result<TransitionWork, Failure> {
        self.transition_with(game, made, BTreeSet::new())
    }

    /// [`Regions::transition`], also keeping `extra` regions active: those a
    /// wizard operation is about to act on. Regions the game doesn't know
    /// are left for the operation to reject.
    pub(crate) fn transition_with(
        &mut self,
        game: &mut Game,
        made: &mut Vec<(RecordId, Shared<RegionRecord>)>,
        extra: BTreeSet<RegionId>,
    ) -> Result<TransitionWork, Failure> {
        let mut work = TransitionWork::default();
        let mut t = self.horizon(game, &mut work);
        let known: Vec<_> = extra
            .into_iter()
            .filter(|r| game.region_state(*r).is_some())
            .collect();
        t.active.extend(known.iter().copied());
        t.loaded.extend(known);
        let (settled, pins) = game.settle_counted(&t);
        work.pinned_actors = pins.actors;
        work.reach_lookups = pins.reaches;
        if game.regions_are(&settled) {
            return Ok(work);
        }
        // Something changes: keep the game as it was, so callers can tell
        // which observers' views changed.
        work.before = Some(game.clone());
        let reads = self.reads;
        let mut store = Pending {
            regions: self,
            made,
        };
        let (_, report, _) = game
            .transition_regions_counted(&settled, &mut store)
            .map_err(|_| storage_failure())?;
        work.report = report;
        work.records_read = self.reads - reads;
        Ok(work)
    }

    /// Keep records a published command made.
    pub(crate) fn publish(&mut self, made: Vec<(RecordId, Shared<RegionRecord>)>) {
        for (id, record) in made {
            RecordStore::put(self, id, record);
        }
    }

    /// Records a checkpoint must write: those it refers to that aren't on
    /// disk yet. They stay in memory until they are.
    pub(crate) fn unwritten(
        &self,
        ids: impl IntoIterator<Item = RecordId>,
    ) -> BTreeMap<RecordId, Shared<RegionRecord>> {
        ids.into_iter()
            .filter(|id| !self.durable.contains(id))
            .filter_map(|id| Some((id, self.resident.get(&id)?.clone())))
            .collect()
    }

    /// After a checkpoint commits: exactly the records it refers to are on
    /// disk (the rest were deleted). A record made before its capture that it
    /// doesn't refer to is never needed again, since records never change and
    /// nothing refers to one again once it's gone. Memory then keeps records
    /// not on disk, plus a few that are.
    pub(crate) fn written(&mut self, on_disk: BTreeSet<RecordId>, watermark: RecordId) {
        self.resident
            .retain(|id, _| on_disk.contains(id) || *id >= watermark);
        self.durable = on_disk;
        let mut excess = self.resident.len().saturating_sub(CACHED_RECORDS);
        let durable = &self.durable;
        self.resident.retain(|id, _| {
            let evict = excess > 0 && durable.contains(id);
            if evict {
                excess -= 1;
            }
            !evict
        });
    }

    /// Drop every record that's on disk from memory, as eviction would with
    /// enough records.
    #[cfg(test)]
    pub(crate) fn evict_durable(&mut self) {
        let durable = &self.durable;
        self.resident.retain(|id, _| !durable.contains(id));
    }

    pub(crate) fn resident_count(&self) -> usize {
        self.resident.len()
    }
}

impl RecordStore for Regions {
    fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
        self.resident.insert(id, record);
    }
    fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
        if let Some(record) = self.resident.get(&id) {
            return Some(record.clone());
        }
        let record = Shared::new(self.disk.as_ref()?.read_region(id).ok()??);
        self.reads += 1;
        Some(record)
    }
    fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
        self.package
            .build_region(self.seed, &self.index, region.0)
            .ok()
    }
}

/// A candidate command's view of the store: records it makes stay here
/// until the command publishes.
struct Pending<'a> {
    regions: &'a mut Regions,
    made: &'a mut Vec<(RecordId, Shared<RegionRecord>)>,
}

impl RecordStore for Pending<'_> {
    fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
        self.made.push((id, record));
    }
    fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
        match self.made.iter().find(|(made, _)| *made == id) {
            Some((_, record)) => Some(record.clone()),
            None => self.regions.get(id),
        }
    }
    fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
        self.regions.build(region)
    }
}
