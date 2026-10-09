//! Region streaming in the engine: which regions each committed command keeps
//! active and loaded, and where region records are kept. See
//! `docs/region-streaming.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    pub(crate) acquisition: RegionAcquisitionProfile,
    /// The game just before the transition, when it changed anything.
    pub(crate) before: Option<Game>,
}

/// Successful region acquisitions during a command's transitions. Timings are
/// nested inside the command's region-transition duration, not extra phases.
/// Resident cache hits do not acquire a record and are excluded. Prepared counts
/// depend on worker timing; fallback timings exclude work done by that worker.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RegionAcquisitionProfile {
    pub groups_committed: usize,
    pub prepared_groups: usize,
    pub demand_groups: usize,
    pub group_wait: Duration,
    pub group_build: Duration,
    pub fallback_reads: usize,
    pub fallback_builds: usize,
    pub prepared_reads: usize,
    pub prepared_builds: usize,
    pub fallback_read: Duration,
    pub fallback_build: Duration,
}

impl RegionAcquisitionProfile {
    pub(crate) fn add(&mut self, other: Self) {
        self.groups_committed += other.groups_committed;
        self.prepared_groups += other.prepared_groups;
        self.demand_groups += other.demand_groups;
        self.group_wait += other.group_wait;
        self.group_build += other.group_build;
        self.fallback_reads += other.fallback_reads;
        self.fallback_builds += other.fallback_builds;
        self.prepared_reads += other.prepared_reads;
        self.prepared_builds += other.prepared_builds;
        self.fallback_read += other.fallback_read;
        self.fallback_build += other.fallback_build;
    }
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
    /// Scoped to a transition; never saved or used to decide game outcomes.
    acquisition: Option<RegionAcquisitionProfile>,
    /// Regions whose files the save holds, and those built regions (or
    /// their neighbours, whose walls a build reads) need that it doesn't
    /// yet. Replay rebuilds regions from these copies.
    saved: BTreeSet<u64>,
    unsaved: BTreeSet<u64>,
    /// Why the last build or declaration failed, instead of a generic failure.
    failure: Option<Failure>,
    group_needed: Option<u64>,
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
            acquisition: None,
            saved: BTreeSet::new(),
            unsaved: BTreeSet::new(),
            failure: None,
            group_needed: None,
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

    /// Whether building a region the game has declared could need a file
    /// the save doesn't hold: each declared region's own, and its
    /// neighbours', whose walls a build reads. Bounded by the declared
    /// frontier, not the package.
    pub(crate) fn needs_package_files(&self, game: &Game) -> bool {
        game.unbuilt_regions().any(|region| {
            let neighbours = self.catalog.region(region).map(|m| m.outgoing.clone());
            std::iter::once(region)
                .chain(neighbours.into_iter().flatten())
                .any(|r| !self.saved.contains(&r.0))
        })
    }

    /// Files replay will need that the save may not hold yet.
    pub(crate) fn need_files(&mut self, files: impl IntoIterator<Item = u64>) {
        for file in files {
            if !self.saved.contains(&file) {
                self.unsaved.insert(file);
            }
        }
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
            if let Some(owner) = self.index.group_owners.get(&region.0) {
                if matches!(game.region_state(region), None | Some(RegionState::Unbuilt)) {
                    let job = Job::Group(RegionId(*owner));
                    if !work.jobs.contains(&job) {
                        work.jobs.push(job);
                    }
                    continue;
                }
            }
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
        self.transition_with_profile(game, made, extra, false)
    }

    pub(crate) fn transition_with_profile(
        &mut self,
        game: &mut Game,
        made: &mut Vec<(RecordId, Shared<RegionRecord>)>,
        extra: BTreeSet<RegionId>,
        profile: bool,
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
        let mut next = game.clone();
        let mut staged = made.clone();
        let reads = self.reads;
        let prepared = self.prepared;
        self.acquisition = profile.then(RegionAcquisitionProfile::default);
        let mut generated = Vec::new();
        let mut group_files = BTreeSet::new();
        self.group_needed = None;
        let mut groups: BTreeSet<_> = settled
            .loaded
            .iter()
            .filter(|region| {
                matches!(
                    next.region_state(**region),
                    None | Some(RegionState::Unbuilt)
                )
            })
            .filter_map(|region| self.index.group_owners.get(&region.0).copied())
            .collect();
        let result = loop {
            for owner in std::mem::take(&mut groups) {
                let started = self.acquisition.as_ref().map(|_| Instant::now());
                let prepared = match &self.preload {
                    Some(preload) => preload.demand_group(RegionId(owner))?,
                    None => None,
                };
                if let (Some(acquisition), Some(started)) = (&mut self.acquisition, started) {
                    acquisition.group_wait += started.elapsed();
                }
                let started = self.acquisition.as_ref().map(|_| Instant::now());
                let batch = match prepared {
                    Some(batch) => {
                        self.prepared += 9;
                        if let Some(acquisition) = &mut self.acquisition {
                            acquisition.prepared_groups += 1;
                        }
                        batch
                    }
                    None => {
                        let batch =
                            self.package
                                .build_group(self.seed, &self.index, RegionId(owner))?;
                        if let (Some(acquisition), Some(started)) = (&mut self.acquisition, started)
                        {
                            acquisition.demand_groups += 1;
                            acquisition.group_build += started.elapsed();
                        }
                        batch
                    }
                };
                generated.extend(
                    batch
                        .records
                        .iter()
                        .map(|(definition, _)| definition.region.id),
                );
                group_files.extend(batch.files);
                let mut store = Pending {
                    regions: self,
                    made: &mut staged,
                };
                if next
                    .register_generated_regions(batch.records, &mut store)
                    .is_err()
                {
                    return Err(storage_failure());
                }
            }
            let mut store = Pending {
                regions: self,
                made: &mut staged,
            };
            let result = next.transition_regions_counted(&settled, &mut store);
            if result.is_err() {
                if let Some(owner) = self.group_needed.take() {
                    groups.insert(owner);
                    continue;
                }
            }
            break result;
        };
        work.acquisition = self.acquisition.take().unwrap_or_default();
        let failure = self.failure.take();
        let (_, mut report, _) = match result {
            Ok(result) => result,
            Err(_) => {
                return Err(failure.unwrap_or_else(storage_failure));
            }
        };
        *game = next;
        *made = staged;
        self.need_files(group_files);
        work.acquisition.groups_committed = generated.len() / 9;
        report.built.extend(generated);
        report.built.sort();
        report.built.dedup();
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

    /// Journal-only commits can make ordinary detached records durable too.
    pub(crate) fn appended(&mut self, on_disk: BTreeSet<RecordId>) {
        self.durable.extend(on_disk);
        let mut excess = self.resident.len().saturating_sub(CACHED_RECORDS);
        let durable = &self.durable;
        self.resident.retain(|id, _| {
            if excess > 0 && durable.contains(id) {
                excess -= 1;
                false
            } else {
                true
            }
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

    pub(crate) fn encoded_resident(&self) -> Result<Vec<(u64, Vec<u8>)>, Failure> {
        self.resident
            .iter()
            .map(|(id, record)| Ok((id.0, crate::storage::encode_region(id.0, record)?)))
            .collect()
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
        if let Some((record, _)) = self.preload.as_ref().and_then(|p| p.take(Job::Read(id))) {
            self.reads += 1;
            self.prepared += 1;
            if let Some(acquisition) = &mut self.acquisition {
                acquisition.prepared_reads += 1;
            }
            return Some(record);
        }
        let started = self.acquisition.as_ref().map(|_| Instant::now());
        let record = Shared::new(self.disk.as_ref()?.read_region(id).ok()??);
        self.reads += 1;
        if let (Some(acquisition), Some(started)) = (&mut self.acquisition, started) {
            acquisition.fallback_reads += 1;
            acquisition.fallback_read += started.elapsed();
        }
        Some(record)
    }
    fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
        // A pin discovered while attaching can demand another floor. Abort
        // the tentative transition so its entire group publishes before retry.
        if let Some(owner) = self.index.group_owners.get(&region.0) {
            self.group_needed = Some(*owner);
            return None;
        }
        let (record, files) = match self
            .preload
            .as_ref()
            .and_then(|p| p.take(Job::Build(region)))
        {
            Some((record, files)) => {
                self.prepared += 1;
                if let Some(acquisition) = &mut self.acquisition {
                    acquisition.prepared_builds += 1;
                }
                (record.into_inner(), files)
            }
            None => {
                let started = self.acquisition.as_ref().map(|_| Instant::now());
                match self.package.build_region(self.seed, &self.index, region.0) {
                    Ok(built) => {
                        if let (Some(acquisition), Some(started)) = (&mut self.acquisition, started)
                        {
                            acquisition.fallback_builds += 1;
                            acquisition.fallback_build += started.elapsed();
                        }
                        built
                    }
                    Err(failure) => {
                        self.failure = Some(failure);
                        return None;
                    }
                }
            }
        };
        // Replaying this build needs exactly the files it read.
        self.need_files(files);
        Some(record)
    }
    fn unbuilt(&mut self, region: RegionId) -> Option<tor_simulation::UnbuiltRegion> {
        let unbuilt = match self.package.unbuilt_region(&self.index, region.0) {
            Ok(unbuilt) => unbuilt,
            Err(failure) => {
                self.failure = Some(failure);
                return None;
            }
        };
        // Declaring a generated region read its file; replay needs it too.
        if self.package.declaring_reads_file(region.0) {
            self.need_files([region.0]);
        }
        Some(unbuilt)
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

#[cfg(test)]
mod declaration_error_tests {
    use super::*;
    use crate::scenario_package;
    use std::path::Path;

    #[test]
    fn lazy_declaration_retains_the_generated_source_failure() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
        let source = scenario_package::load(&root, 42, None, false)
            .unwrap()
            .package
            .unwrap();
        let mut definitions = source.region_defs().unwrap();
        let region = definitions
            .iter_mut()
            .find(|region| region.id == 2)
            .unwrap();
        region.size = [24, 1, 1];
        region.anchors = BTreeMap::from([("west".into(), [0, 0, 0]), ("east".into(), [23, 0, 0])]);
        for portal in &mut region.portals {
            portal.at[1] = 0;
        }
        let recipe = region.generate.as_mut().unwrap();
        recipe.rooms = [1, 1];
        recipe.actors.as_mut().unwrap().count = [10, 10];
        recipe.items = None;
        let package = Arc::new(Package::from_parts(source.manifest.clone(), definitions).unwrap());
        let mut regions = Regions::new(package, 42, Streaming::default()).unwrap();
        let expected = regions
            .package
            .unbuilt_region(&regions.index, 2)
            .unwrap_err();
        assert!(
            expected.message.contains("capacity"),
            "{}",
            expected.message
        );
        assert!(regions.unbuilt(RegionId(2)).is_none());
        let failure = regions
            .failure
            .take()
            .expect("retain the precise source failure");
        assert_eq!(failure.code, expected.code);
        assert_eq!(failure.message, expected.message);
        assert!(
            regions.unsaved.is_empty(),
            "failed declaration must not pin source bytes"
        );
    }
}
