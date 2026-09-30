//! Region streaming in the engine: which regions each committed command keeps
//! active and loaded, and where region records are kept. See
//! `docs/region-streaming.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tor_simulation::{
    Game, RecordId, RecordStore, RegionRecord, RegionState, RegionTransition, TransitionReport,
};
use tor_world::{RegionId, Shared};

use crate::engine::storage_failure;
use crate::preload::{Job, Preloader};
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
    /// Builds and reads a preloader had already prepared.
    pub(crate) prepared: usize,
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
    /// Records read from disk, for the recovery profile. Counts records
    /// the preloader read too, so it doesn't depend on timing.
    pub(crate) reads: usize,
    /// Prepares regions just beyond the horizon on another thread. Shared by
    /// copies of the store; a result is correct for any of them.
    preload: Option<Arc<Preloader>>,
    /// Builds and reads taken ready from the preloader.
    pub(crate) prepared: usize,
    /// Regions whose files the save holds, and those built regions (or
    /// their neighbours, whose walls a build reads) need that it doesn't
    /// yet. Replay rebuilds regions from these copies.
    saved: BTreeSet<u64>,
    unsaved: BTreeSet<u64>,
    /// Why the last build failed, to report instead of a generic failure.
    failure: Option<Failure>,
}

/// What the preloader was asked for, and the deterministic work of choosing it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreloadWork {
    pub(crate) jobs: Vec<Job>,
    pub(crate) regions_expanded: usize,
    pub(crate) links_examined: usize,
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
            preload: None,
            prepared: 0,
            saved: BTreeSet::new(),
            unsaved: BTreeSet::new(),
            failure: None,
        })
    }

    /// The region files the save needs and doesn't hold yet.
    pub(crate) fn unsaved_sources(&self) -> Result<Vec<(u64, Arc<str>)>, Failure> {
        self.unsaved
            .iter()
            .map(|region| Ok((*region, self.package.region_text(*region)?)))
            .collect()
    }

    /// These regions' files are queued with a published command.
    pub(crate) fn mark_saved(&mut self, copied: Vec<u64>) {
        for region in copied {
            self.unsaved.remove(&region);
            self.saved.insert(region);
        }
    }

    /// The regions whose files a save holds, when it's opened.
    pub(crate) fn set_saved(&mut self, saved: BTreeSet<u64>) {
        self.unsaved.retain(|region| !saved.contains(region));
        self.saved = saved;
    }

    pub(crate) fn saved_count(&self) -> usize {
        self.saved.len()
    }

    pub(crate) fn attach_disk(&mut self, disk: crate::storage::Store) {
        if let Some(preload) = &self.preload {
            preload.attach_disk(disk.clone());
        }
        self.disk = Some(disk);
    }

    /// Start preparing regions in the background. Replay and recovery run
    /// without it; the engine turns it on once a game is ready to play.
    pub(crate) fn start_preloading(&mut self) {
        if self.preload.is_some() {
            return;
        }
        let preload = Preloader::start(self.package.clone(), self.index.clone(), self.seed);
        if let Some(disk) = &self.disk {
            preload.attach_disk(disk.clone());
        }
        self.preload = Some(Arc::new(preload));
    }

    pub(crate) fn stop_preloading(&mut self) {
        self.preload = None;
    }

    /// Wait for the preloader to finish what it was asked for.
    pub(crate) fn settle_preloading(&self) {
        if let Some(preload) = &self.preload {
            preload.settle();
        }
    }

    /// The regions one portal hop beyond the loaded ones: those the next
    /// transitions are likely to need. That's beyond what pins keep loaded
    /// too, not just the reference points' radii. Work is bounded by the
    /// loaded regions, never the world.
    pub(crate) fn preload_jobs(&self, game: &Game) -> PreloadWork {
        let mut work = PreloadWork::default();
        // Regions a wizard added aren't in the package; pins still follow
        // their links.
        let loaded: BTreeSet<RegionId> = game
            .loaded_regions()
            .filter(|r| self.catalog.region(*r).is_some())
            .collect();
        let Ok(plan) = self.catalog.plan(&loaded, 1, &loaded) else {
            return work;
        };
        work.regions_expanded = plan.expanded_regions;
        work.links_examined = plan.examined_links;
        for region in plan.activate {
            match game.region_state(region) {
                Some(RegionState::Unbuilt) => work.jobs.push(Job::Build(region)),
                Some(RegionState::Detached) => {
                    if let Some(id) = game.detached_record(region) {
                        if !self.resident.contains_key(&id) {
                            work.jobs.push(Job::Read(id));
                        }
                    }
                }
                _ => {}
            }
        }
        work
    }

    /// The palette around `region`: the assets of the themes of every region
    /// within one hop beyond the default load radius. Themes come from zones,
    /// so entering a room never signals what the next one holds.
    pub(crate) fn palette(&self, package: &Package, region: RegionId) -> BTreeSet<String> {
        let roots = BTreeSet::from([region]);
        let hops = self.streaming.load_radius as usize + 1;
        let near = match self.catalog.plan(&roots, hops, &BTreeSet::new()) {
            Ok(plan) => plan.required,
            Err(_) => roots,
        };
        let themes: BTreeSet<&String> = near
            .iter()
            .filter_map(|r| self.catalog.region(*r))
            .flat_map(|m| &m.themes)
            .collect();
        package.palette(themes)
    }

    /// Ask the preloader for what the next transitions are likely to need.
    pub(crate) fn preload(&self, game: &Game) -> Option<PreloadWork> {
        let preload = self.preload.as_ref()?;
        let work = self.preload_jobs(game);
        preload.want(&work.jobs);
        Some(work)
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
            .filter(|r| game.region_state(*r).is_some() || self.catalog.region(*r).is_some())
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
        let prepared = self.prepared;
        let mut store = Pending {
            regions: self,
            made,
        };
        let result = game.transition_regions_counted(&settled, &mut store);
        let failure = self.failure.take();
        let (_, report, _) = result.map_err(|_| failure.unwrap_or_else(storage_failure))?;
        work.report = report;
        work.records_read = self.reads - reads;
        work.prepared = self.prepared - prepared;
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
        if let Some(record) = self.preload.as_ref().and_then(|p| p.take(Job::Read(id))) {
            self.reads += 1;
            self.prepared += 1;
            return Some(record);
        }
        let record = Shared::new(self.disk.as_ref()?.read_region(id).ok()??);
        self.reads += 1;
        Some(record)
    }
    fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
        let record = match self
            .preload
            .as_ref()
            .and_then(|p| p.take(Job::Build(region)))
        {
            Some(record) => {
                self.prepared += 1;
                record.into_inner()
            }
            None => match self.package.build_region(self.seed, &self.index, region.0) {
                Ok(record) => record,
                Err(failure) => {
                    self.failure = Some(failure);
                    return None;
                }
            },
        };
        // Replaying this build needs the region's file and its neighbours'.
        let neighbours = self.catalog.region(region).map(|m| m.outgoing.clone());
        for needed in std::iter::once(region).chain(neighbours.into_iter().flatten()) {
            if !self.saved.contains(&needed.0) {
                self.unsaved.insert(needed.0);
            }
        }
        Some(record)
    }
    fn unbuilt(&mut self, region: RegionId) -> Option<tor_simulation::UnbuiltRegion> {
        self.package.unbuilt_region(&self.index, region.0).ok()
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
    fn unbuilt(&mut self, region: RegionId) -> Option<tor_simulation::UnbuiltRegion> {
        self.regions.unbuilt(region)
    }
}
