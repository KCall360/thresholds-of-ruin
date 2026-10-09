//! Background preloading: building regions and reading region rows on another
//! thread, just beyond what the reference points keep loaded, so a command
//! that loads them finds them ready. Correctness never depends on it: a
//! region builds the same way on any thread, and records never change once
//! made, so a prepared result is exactly what the command would have
//! produced itself. Single-region work falls back on demand; generation groups
//! wait for their specific result to avoid running duplicate floor generation.
//! See `docs/region-streaming.md` and `docs/generation-recipes.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use tor_simulation::{RecordId, RegionRecord};
use tor_world::{RegionId, Shared};

use crate::scenario_package::{Package, PackageIndex};
use crate::storage::{RegionReader, Store};
use crate::{scenario_package::PreparedGroup, Failure};

/// One piece of preparation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Job {
    /// Build all members of a coordinated floor, identified by its owner.
    Group(RegionId),
    /// Build an unbuilt region's starting record from the package.
    Build(RegionId),
    /// Read a detached region's record from disk.
    Read(RecordId),
}

impl Job {
    fn cost(self) -> usize {
        if matches!(self, Self::Group(_)) {
            9
        } else {
            1
        }
    }
}

/// A finished job: the record, and for a build, the regions whose files it
/// read.
pub(crate) type Prepared = (Shared<RegionRecord>, BTreeSet<u64>);

/// Prepared results kept at most, so a preloader can't hold more than a
/// horizon's worth of records however the reference points move.
pub(crate) const PREPARED_LIMIT: usize = 32;

#[derive(Debug, Default)]
struct State {
    #[cfg(test)]
    paused: bool,
    #[cfg(test)]
    completed: Vec<Job>,
    /// Jobs still to do, in request order.
    queue: Vec<Job>,
    /// The job the thread is doing now.
    running: Option<Job>,
    /// Finished jobs not yet taken.
    ready: BTreeMap<Job, Prepared>,
    ready_groups: BTreeMap<Job, Result<PreparedGroup, Failure>>,
    wanted: BTreeSet<Job>,
    demanded: BTreeSet<Job>,
    /// The save to read rows from, once the game has one.
    disk: Option<Store>,
    stop: bool,
}

#[derive(Debug, Default)]
struct Inner {
    state: Mutex<State>,
    wake: Condvar,
}

/// Owns the preloading thread; dropping the last handle stops and joins it.
#[derive(Debug)]
pub(crate) struct Preloader {
    inner: Arc<Inner>,
    thread: Option<JoinHandle<()>>,
}

impl Preloader {
    pub(crate) fn start(package: Arc<Package>, index: PackageIndex, seed: u64) -> Self {
        let inner = Arc::new(Inner::default());
        let worker = inner.clone();
        let thread = std::thread::Builder::new()
            .name("region-preload".into())
            .spawn(move || run(&worker, &package, &index, seed))
            .ok();
        Self { inner, thread }
    }

    pub(crate) fn attach_disk(&self, disk: Store) {
        self.inner.state.lock().unwrap().disk = Some(disk);
    }

    /// Replace the wanted jobs. Prepared results and queued jobs nobody
    /// wants any more are dropped; jobs already prepared or running aren't
    /// queued again. At most [`PREPARED_LIMIT`] jobs are kept or queued.
    pub(crate) fn want(&self, jobs: &[Job]) {
        let mut s = self.inner.state.lock().unwrap();
        let mut wanted = s.demanded.clone();
        let mut cost = wanted.iter().map(|job| job.cost()).sum::<usize>()
            + s.running
                .filter(|job| !wanted.contains(job))
                .map_or(0, Job::cost);
        for &job in jobs {
            let extra = if s.running == Some(job) {
                0
            } else {
                job.cost()
            };
            if !wanted.contains(&job) && cost + extra <= PREPARED_LIMIT {
                wanted.insert(job);
                cost += extra;
            }
        }
        s.ready.retain(|job, _| wanted.contains(job));
        s.ready_groups.retain(|job, _| wanted.contains(job));
        let running = s.running;
        let mut queue = Vec::new();
        for job in s.demanded.iter().copied().chain(jobs.iter().copied()) {
            if wanted.contains(&job)
                && !queue.contains(&job)
                && !s.ready.contains_key(&job)
                && !s.ready_groups.contains_key(&job)
                && Some(job) != running
            {
                queue.push(job);
            }
        }
        s.wanted = wanted;
        s.queue = queue;
        // `settle` waits on the same condition, so wake everyone.
        self.inner.wake.notify_all();
    }

    /// A prepared result, taken so it's used once.
    pub(crate) fn take(&self, job: Job) -> Option<Prepared> {
        self.inner.state.lock().unwrap().ready.remove(&job)
    }

    /// Share a demanded job with speculation and wait for that job only.
    /// If no worker could start, the caller uses deterministic demand execution.
    pub(crate) fn demand_group(&self, owner: RegionId) -> Result<Option<PreparedGroup>, Failure> {
        if self.thread.is_none() {
            return Ok(None);
        }
        let job = Job::Group(owner);
        let mut s = self.inner.state.lock().unwrap();
        s.demanded.insert(job);
        s.wanted.insert(job);
        // Reserve demand without allowing queued speculation to consume more
        // than the record budget. An obsolete running job finishes privately.
        let speculative: Vec<_> = s
            .wanted
            .iter()
            .rev()
            .copied()
            .filter(|other| !s.demanded.contains(other))
            .collect();
        for other in speculative {
            let cost = s.wanted.iter().map(|job| job.cost()).sum::<usize>()
                + s.running
                    .filter(|running| !s.wanted.contains(running))
                    .map_or(0, Job::cost);
            if cost <= PREPARED_LIMIT {
                break;
            }
            s.wanted.remove(&other);
        }
        let wanted = s.wanted.clone();
        s.ready.retain(|job, _| wanted.contains(job));
        s.ready_groups.retain(|job, _| wanted.contains(job));
        s.queue.retain(|job| wanted.contains(job));
        // Demand reserves capacity by dropping speculative prepared batches.
        let stale: Vec<_> = s
            .ready_groups
            .keys()
            .chain(s.ready.keys())
            .copied()
            .filter(|other| !s.demanded.contains(other))
            .collect();
        let mut cost = s
            .ready_groups
            .keys()
            .chain(s.ready.keys())
            .map(|job| job.cost())
            .sum::<usize>();
        let reserve = if s.ready_groups.contains_key(&job) {
            0
        } else {
            job.cost()
        };
        for other in stale {
            if cost + reserve <= PREPARED_LIMIT {
                break;
            }
            s.ready.remove(&other);
            s.ready_groups.remove(&other);
            s.wanted.remove(&other);
            cost -= other.cost();
        }
        if !s.ready_groups.contains_key(&job) && s.running != Some(job) {
            s.queue.retain(|queued| *queued != job);
            s.queue.insert(0, job);
        }
        self.inner.wake.notify_all();
        loop {
            if let Some(result) = s.ready_groups.remove(&job) {
                s.demanded.remove(&job);
                s.wanted.remove(&job);
                return result.map(Some);
            }
            if s.stop {
                return Err(crate::engine::storage_failure());
            }
            s = self.inner.wake.wait(s).unwrap();
        }
    }

    /// Wait until nothing is queued or running. Tests and benchmarks use it
    /// to measure the prepared path; commands wait only for demanded groups.
    pub(crate) fn settle(&self) {
        if self.thread.is_none() {
            return;
        }
        let mut s = self.inner.state.lock().unwrap();
        while !s.queue.is_empty() || s.running.is_some() {
            s = self.inner.wake.wait(s).unwrap();
        }
    }
}

impl Drop for Preloader {
    fn drop(&mut self) {
        self.inner.state.lock().unwrap().stop = true;
        self.inner.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(inner: &Inner, package: &Package, index: &PackageIndex, seed: u64) {
    let mut reader: Option<(Store, RegionReader)> = None;
    loop {
        let (job, disk) = {
            let mut s = inner.state.lock().unwrap();
            loop {
                if s.stop {
                    return;
                }
                #[cfg(test)]
                if s.paused {
                    s = inner.wake.wait(s).unwrap();
                    continue;
                }
                if !s.queue.is_empty() {
                    break;
                }
                s = inner.wake.wait(s).unwrap();
            }
            let job = s.queue.remove(0);
            s.running = Some(job);
            (job, s.disk.clone())
        };
        let group = if let Job::Group(owner) = job {
            Some(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    package.build_group(seed, index, owner)
                }))
                .unwrap_or_else(|_| Err(crate::engine::storage_failure())),
            )
        } else {
            None
        };
        let record = match job {
            Job::Group(_) => None,
            Job::Build(region) => package.build_region(seed, index, region.0).ok(),
            Job::Read(id) => disk.and_then(|disk| {
                if !reader.as_ref().is_some_and(|(held, _)| held.same(&disk)) {
                    let fresh = disk.region_reader();
                    reader = Some((disk, fresh));
                }
                let record = reader.as_mut()?.1.read(id).ok().flatten()?;
                Some((record, BTreeSet::new()))
            }),
        };
        let mut s = inner.state.lock().unwrap();
        s.running = None;
        #[cfg(test)]
        s.completed.push(job);
        if let Some(group) = group {
            if s.wanted.contains(&job) {
                s.ready_groups.insert(job, group);
            }
        }
        // A failed job leaves nothing: the command does it itself, and
        // reports the failure if it recurs.
        if let Some((record, files)) = record {
            let cost = s
                .ready
                .keys()
                .chain(s.ready_groups.keys())
                .map(|job| job.cost())
                .sum::<usize>();
            if s.wanted.contains(&job) && cost + job.cost() <= PREPARED_LIMIT {
                s.ready.insert(job, (Shared::new(record), files));
            }
        }
        inner.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_precedes_queued_speculation_and_duplicate_requests_share_one_job() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/rogue-exploration");
        let package = crate::scenario_package::load(&root, 42, None, true)
            .unwrap()
            .package
            .unwrap();
        let index = package.index(42).unwrap();
        let inner = Arc::new(Inner::default());
        inner.state.lock().unwrap().paused = true;
        let worker = inner.clone();
        let thread = std::thread::spawn(move || run(&worker, &package, &index, 42));
        let preload = Preloader {
            inner,
            thread: Some(thread),
        };
        let demanded = Job::Group(RegionId(28));
        preload.want(&[
            Job::Group(RegionId(1)),
            Job::Group(RegionId(10)),
            Job::Group(RegionId(19)),
        ]);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| preload.demand_group(RegionId(28)).unwrap().unwrap());
            let mut state = preload.inner.state.lock().unwrap();
            while !state.demanded.contains(&demanded) {
                state = preload.inner.wake.wait(state).unwrap();
            }
            assert_eq!(state.queue.first(), Some(&demanded));
            assert!(state.wanted.iter().map(|job| job.cost()).sum::<usize>() <= PREPARED_LIMIT);
            drop(state);
            // A repeated horizon update preserves the demanded reservation.
            preload.want(&[demanded, demanded, Job::Group(RegionId(1))]);
            let mut state = preload.inner.state.lock().unwrap();
            state.paused = false;
            drop(state);
            preload.inner.wake.notify_all();
            assert_eq!(waiter.join().unwrap().records.len(), 9);
        });
        preload.settle();
        let state = preload.inner.state.lock().unwrap();
        assert_eq!(state.completed.first(), Some(&demanded));
        assert_eq!(
            state
                .completed
                .iter()
                .filter(|job| **job == demanded)
                .count(),
            1
        );
    }

    #[test]
    fn unavailable_worker_returns_to_the_deterministic_demand_path() {
        let preload = Preloader {
            inner: Arc::new(Inner::default()),
            thread: None,
        };
        preload.want(&[Job::Group(RegionId(1))]);
        assert!(preload.demand_group(RegionId(1)).unwrap().is_none());
        preload.settle();
    }
    #[test]
    fn demanded_floor_is_prepared_once_as_nine_independent_records() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/rogue-exploration");
        let package = crate::scenario_package::load(&root, 42, None, true)
            .unwrap()
            .package
            .unwrap();
        let index = package.index(42).unwrap();
        let preload = Preloader::start(package.clone(), index.clone(), 42);
        let job = Job::Group(RegionId(1));
        preload.want(&[job, job, job]);
        let prepared = preload.demand_group(RegionId(1)).unwrap().unwrap();
        assert_eq!(prepared.records.len(), 9);
        assert_eq!(prepared.files, (1..=9).collect());
        let direct = package.build_group(42, &index, RegionId(1)).unwrap();
        assert_eq!(prepared.records, direct.records);
        assert_eq!(prepared.files, direct.files);
        preload.want(&[]);
        preload.settle();
        assert!(preload.inner.state.lock().unwrap().ready_groups.is_empty());
    }

    #[test]
    fn group_budget_and_failure_are_bounded_and_do_not_deadlock_demand() {
        let (preload, _, _) = preloader();
        let jobs: Vec<_> = (1..=10).map(|id| Job::Group(RegionId(id))).collect();
        preload.want(&jobs);
        preload.settle();
        assert!(preload.inner.state.lock().unwrap().ready_groups.len() <= PREPARED_LIMIT / 9);
        assert!(preload.demand_group(RegionId(100)).is_err());
        preload.want(&[]);
        preload.settle();
        assert!(preload.inner.state.lock().unwrap().ready_groups.is_empty());
    }

    fn preloader() -> (Preloader, Arc<Package>, PackageIndex) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/streaming-corridor");
        let scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
        let package = scenario.package.unwrap();
        let index = package.index(5).unwrap();
        (
            Preloader::start(package.clone(), index.clone(), 5),
            package,
            index,
        )
    }

    #[test]
    fn a_prepared_build_equals_building_on_demand_and_is_taken_once() {
        let (preload, package, index) = preloader();
        preload.want(&[Job::Build(RegionId(3))]);
        preload.settle();
        let (prepared, files) = preload.take(Job::Build(RegionId(3))).expect("prepared");
        let (built, read) = package.build_region(5, &index, 3).unwrap();
        assert_eq!(*prepared, built);
        assert_eq!(files, read);
        assert!(preload.take(Job::Build(RegionId(3))).is_none());
    }

    #[test]
    fn results_nobody_wants_any_more_are_dropped() {
        let (preload, _, _) = preloader();
        preload.want(&[Job::Build(RegionId(2)), Job::Build(RegionId(3))]);
        preload.settle();
        preload.want(&[Job::Build(RegionId(3))]);
        assert!(preload.take(Job::Build(RegionId(2))).is_none());
        assert!(preload.take(Job::Build(RegionId(3))).is_some());
    }

    #[test]
    fn at_most_the_limit_is_prepared() {
        let (preload, _, _) = preloader();
        // Regions the package doesn't have fail to build and leave nothing;
        // the corridor's seven do, whatever else is asked for.
        let jobs: Vec<Job> = (1..=PREPARED_LIMIT as u64 + 8)
            .map(|r| Job::Build(RegionId(r)))
            .collect();
        preload.want(&jobs);
        preload.settle();
        let ready = preload.inner.state.lock().unwrap().ready.len();
        assert_eq!(ready, 7);
        assert!(ready <= PREPARED_LIMIT);
    }

    #[test]
    fn a_read_without_a_save_leaves_nothing_for_the_command_to_take() {
        let (preload, _, _) = preloader();
        preload.want(&[Job::Read(RecordId(1))]);
        preload.settle();
        assert!(preload.take(Job::Read(RecordId(1))).is_none());
    }
}
