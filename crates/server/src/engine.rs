use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tor_protocol::*;
use tor_simulation::{ActorId as SimActor, Game};
use uuid::Uuid;

use crate::adapt;
use crate::journal::{
    Command, HistoryContent, HistoryEntry, Position, WizardItem, WizardOperation, WizardResult,
};

const ARCHIVE_VERSION: u32 = 6;
#[path = "checkpoint.rs"]
mod checkpoint;
pub(crate) use checkpoint::{Checkpoint, DiskCheckpoint};
const REWIND_BOUNDARIES: usize = 128;
const RULESET: &str = "diagonal-v11";

#[cfg(test)]
mod seed_equivalence_tests {
    use super::*;
    #[test]
    fn seeded_waits_preserve_game_navigation_scheduler_and_every_retained_boundary() {
        for actors in [1, 8] {
            let mut seeded = Engine::memory(Scenario::performance(42, 8, actors).unwrap()).unwrap();
            let mut normal = seeded.diagnostic_copy();
            seeded.seed_profile_history(3).unwrap();
            seeded.seed_profile_history(129).unwrap();
            for index in 0..132 {
                let actor = ActorId(normal.game.next_actor().unwrap().0);
                normal
                    .command(
                        "bench",
                        "headless",
                        actor,
                        &format!("seed-{index}"),
                        &normal.branch().clone(),
                        Command::Act {
                            expected_revision: normal.revision(actor).unwrap(),
                            action: Action::Wait,
                        },
                    )
                    .unwrap();
            }
            assert_eq!(seeded.game, normal.game);
            assert_eq!(seeded.revisions, normal.revisions);
            assert_eq!(seeded.boundaries.len(), normal.boundaries.len());
            for (a, b) in seeded.boundaries.iter().zip(&normal.boundaries) {
                assert_eq!(a.game, b.game);
                assert_eq!(a.revisions, b.revisions);
            }
            for (a, b) in seeded.archive.records.iter().zip(&normal.archive.records) {
                assert_eq!(a.receipt, b.receipt);
                assert_eq!(a.entry.content, b.entry.content);
                assert_eq!(a.entry.tick, b.entry.tick);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

impl Failure {
    pub(crate) fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorSetup {
    pub position: Position,
    pub turn_ticks: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub seed: u64,
    pub actors: Vec<ActorSetup>,
    pub regions: u64,
    pub workload_version: Option<u32>,
}

impl Scenario {
    pub fn two_room(seed: u64) -> Self {
        Self {
            seed,
            actors: vec![ActorSetup {
                position: Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                },
                turn_ticks: 100,
            }],
            regions: 2,
            workload_version: None,
        }
    }

    pub fn performance(seed: u64, regions: u64, actors: usize) -> Result<Self, Failure> {
        if !(1..=256).contains(&regions) || !(1..=8).contains(&actors) {
            return Err(invalid_archive());
        }
        let fixture = crate::performance_fixture::specification();
        Ok(Self {
            seed,
            regions,
            workload_version: Some(fixture.version),
            actors: fixture
                .geometry
                .actors
                .into_iter()
                .take(actors)
                .map(|[x, y, z]| ActorSetup {
                    position: Position { region: 1, x, y, z },
                    turn_ticks: 100,
                })
                .collect(),
        })
    }
}

fn scenario_game(scenario: &Scenario) -> Game {
    if scenario.regions <= 2 {
        return Game::two_room_in_stone(scenario.seed);
    }
    let rooms = (1..=scenario.regions)
        .map(|id| tor_world::Region {
            id: tor_world::RegionId(id),
            name: format!("Region {id}"),
            bounds: tor_world::Extent::new(17, 17, 2).expect("valid benchmark extent"),
        })
        .collect();
    let mut world = tor_world::World::new(rooms, vec![]).expect("valid benchmark world");
    for id in 1..scenario.regions {
        for z in 0..2 {
            let from = tor_world::Location {
                region: tor_world::RegionId(id),
                position: tor_world::Position { x: 16, y: 8, z },
            };
            let to = tor_world::Location {
                region: tor_world::RegionId(id + 1),
                position: tor_world::Position { x: 0, y: 8, z },
            };
            world
                .connect(
                    tor_world::Passage {
                        from,
                        direction: tor_world::Direction::East,
                        to,
                    },
                    0,
                )
                .expect("valid benchmark passage");
            world
                .connect(
                    tor_world::Passage {
                        from: to,
                        direction: tor_world::Direction::West,
                        to: from,
                    },
                    0,
                )
                .expect("valid benchmark passage");
        }
    }
    for id in 1..=scenario.regions {
        for z in 0..2 {
            for (x, y) in [(4, 4), (4, 12), (12, 4), (12, 12)] {
                world
                    .set_wall(
                        tor_world::Location {
                            region: tor_world::RegionId(id),
                            position: tor_world::Position { x, y, z },
                        },
                        true,
                    )
                    .expect("valid benchmark wall");
            }
        }
    }
    Game::new(world, scenario.seed)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    user: String,
    frontend: String,
    request_id: String,
    actor: ActorId,
    branch: BranchId,
    command: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub(crate) entry: HistoryEntry,
    pub(crate) receipt: Option<Receipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Archive {
    pub(crate) view_salt: String,
    pub(crate) wizard_game: bool,
    pub(crate) version: u32,
    pub(crate) ruleset: String,
    pub(crate) scenario: Scenario,
    pub(crate) branch: BranchId,
    pub(crate) records: Vec<Record>,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub entry: HistoryEntry,
    pub duplicate: bool,
}

/// Diagnostic phase measurements for the checked-in performance harness.
/// Durations are deliberately not used as correctness thresholds; the counts
/// and byte totals provide stable regression contracts.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CommandProfile {
    /// Includes nested door validation and navigation observations.
    pub perception_calls: usize,
    pub scene_calls: usize,
    pub authoritative_total: Duration,
    pub rollback_capture: Duration,
    pub checkpoint_capture: Duration,
    pub checkpoint_captures: usize,
    pub simulation_transition: Duration,
    pub navigation_refresh: Duration,
    pub perception: Duration,
    pub revision_detection: Duration,
    pub rollback_snapshot: Duration,
    pub journal_serialization: Duration,
    pub journal_write: Duration,
    pub journal_sync: Duration,
    pub journal_replace: Duration,
    pub publication: Duration,
    pub simulation_transitions: usize,
    pub navigation_refreshes: usize,
    pub candidate_captures: usize,
    pub actors_observed: usize,
    pub revision_comparisons: usize,
    pub rollback_snapshots: usize,
    pub records_serialized: usize,
    pub bytes_written: u64,
    pub file_writes: usize,
    pub file_flushes: usize,
    pub file_syncs: usize,
    pub file_replacements: usize,
}

/// Startup measurements; retained history is read but only the checkpoint tail is simulated.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RecoveryProfile {
    pub total: Duration,
    pub records_loaded: usize,
    pub records_replayed: usize,
    pub checkpoint_sequence: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BootstrapProfile {
    pub total: Duration,
    pub records_serialized: usize,
}

impl CommandProfile {
    /// Exclusive top-level phases. Nested simulation perception belongs to the
    /// simulation phase; navigation perception belongs to navigation.
    pub fn exclusive_duration(&self) -> Duration {
        self.rollback_capture
            + self.checkpoint_capture
            + self.simulation_transition
            + self.navigation_refresh
            + self.perception
            + self.revision_detection
            + self.rollback_snapshot
            + self.journal_serialization
            + self.journal_write
            + self.journal_sync
            + self.journal_replace
            + self.publication
    }
}

#[derive(Clone, Debug)]
struct Boundary {
    id: Option<EntryId>,
    game: Game,
    revisions: BTreeMap<ActorId, u64>,
}

/// Private mutable decision state. A transaction never owns retained history,
/// the receipt index, or a second storage handle.
#[derive(Clone, Debug)]
struct Candidate {
    current_branch: BranchId,
    boundaries: VecDeque<Arc<Boundary>>,
    game: Game,
    revisions: BTreeMap<ActorId, u64>,
}

impl Candidate {
    fn capture(engine: &Engine) -> Self {
        Self {
            current_branch: engine.current_branch.clone(),
            boundaries: engine.boundaries.clone(),
            game: engine.game.clone(),
            revisions: engine.revisions.clone(),
        }
    }
    fn branch(&self) -> &BranchId {
        &self.current_branch
    }
    fn actors(&self) -> Vec<ActorId> {
        self.revisions.keys().copied().collect()
    }
    fn revision_view(&self, actor: ActorId) -> Result<RevisionView, Failure> {
        revision_view(&self.game, actor)
    }
    fn publish(self, engine: &mut Engine) {
        engine.current_branch = self.current_branch;
        engine.boundaries = self.boundaries;
        engine.game = self.game;
        engine.revisions = self.revisions;
    }
}

type RevisionView = (tor_simulation::Observation, Vec<tor_world::SightCell>, bool);
fn revision_view(game: &Game, actor: ActorId) -> Result<RevisionView, Failure> {
    let (view, scene) = game
        .observe_scene(SimActor(actor.0))
        .map_err(|_| invalid_archive())?;
    Ok((view, scene, game.next_actor() == Some(SimActor(actor.0))))
}

/// Durable chronological journal, including retained futures and explicit forks.
#[derive(Debug)]
pub struct Engine {
    recovery: RecoveryProfile,
    current_branch: BranchId,
    wizard_enabled: bool,
    boundaries: VecDeque<Arc<Boundary>>,
    game: Game,
    archive: Archive,
    revisions: BTreeMap<ActorId, u64>,
    receipts: BTreeMap<(String, String), usize>,
    path: Option<PathBuf>,
    lock: Option<Arc<fs::File>>,
    store: Option<crate::storage::Store>,
}

impl Engine {
    pub fn memory(scenario: Scenario) -> Result<Self, Failure> {
        if !(1..=256).contains(&scenario.regions) {
            return Err(invalid_archive());
        }
        let mut game = match scenario.workload_version {
            None => scenario_game(&scenario),
            Some(1) => crate::performance_fixture::game(scenario.seed, scenario.regions)?,
            Some(_) => return Err(invalid_archive()),
        };
        let mut revisions = BTreeMap::new();
        if scenario.actors.is_empty() {
            return Err(invalid_archive());
        }
        for actor in &scenario.actors {
            let ticks = NonZeroU64::new(actor.turn_ticks).ok_or_else(invalid_archive)?;
            let id = game
                .spawn_actor(adapt::location(actor.position), ticks)
                .map_err(|_| invalid_archive())?;
            revisions.insert(ActorId(id.0), 0);
        }
        game.refresh_navigation();
        let branch = BranchId(Uuid::new_v4().to_string());
        let initial = Arc::new(Boundary {
            id: None,
            game: game.clone(),
            revisions: revisions.clone(),
        });
        Ok(Self {
            recovery: RecoveryProfile::default(),
            current_branch: branch.clone(),
            wizard_enabled: false,
            boundaries: VecDeque::from([initial]),
            game,
            revisions,
            receipts: BTreeMap::new(),
            path: None,
            lock: None,
            store: None,
            archive: Archive {
                view_salt: Uuid::new_v4().to_string(),
                wizard_game: false,
                version: ARCHIVE_VERSION,
                ruleset: RULESET.into(),
                scenario,
                branch,
                records: Vec::new(),
            },
        })
    }

    /// Existing saves own their scenario; `scenario` is used only for a new file.
    pub fn open(path: impl AsRef<Path>, scenario: Scenario) -> Result<Self, Failure> {
        Self::open_with_policy(path, scenario, crate::SavePolicy::default())
    }
    pub fn open_with_policy(
        path: impl AsRef<Path>,
        scenario: Scenario,
        policy: crate::SavePolicy,
    ) -> Result<Self, Failure> {
        let started = Instant::now();
        policy.validate()?;
        let (path, lock) = lock_save(path.as_ref())?;
        let (store, archive, checkpoint) = crate::storage::Store::open(
            &path,
            || Self::memory(scenario).map(|engine| engine.archive),
            policy,
            lock.clone(),
        )?;
        let records_loaded = archive.records.len();
        let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
        let records_replayed = records_loaded
            - checkpoint
                .as_ref()
                .map(|c| c.record_count)
                .unwrap_or(0)
                .min(records_loaded);
        let mut engine = Self::replay(archive, checkpoint)?;
        engine.recovery = RecoveryProfile {
            total: started.elapsed(),
            records_loaded,
            records_replayed,
            checkpoint_sequence,
        };
        engine.path = Some(path);
        engine.lock = Some(lock);
        engine.store = Some(store);
        Ok(engine)
    }
    /// Wait for the records accepted before this call to become durable.
    pub fn flush(&self) -> Result<(), Failure> {
        match &self.store {
            Some(store) => store.flush(),
            None => Ok(()),
        }
    }
    pub fn recovery_profile(&self) -> &RecoveryProfile {
        &self.recovery
    }

    pub fn save_status(&self) -> crate::SaveStatus {
        self.store
            .as_ref()
            .map(|store| store.status())
            .unwrap_or_default()
    }
    pub fn request_save(&self) -> u64 {
        self.store
            .as_ref()
            .map(|store| store.request_flush())
            .unwrap_or(0)
    }
    pub(crate) fn flush_handle(&self) -> Option<crate::storage::Store> {
        self.store.clone()
    }

    fn replay(mut archive: Archive, checkpoint: Option<DiskCheckpoint>) -> Result<Self, Failure> {
        if archive.version != ARCHIVE_VERSION
            || Uuid::parse_str(&archive.view_salt).is_err()
            || archive.ruleset != RULESET
            || Uuid::parse_str(&archive.branch.0).is_err()
        {
            return Err(invalid_archive());
        }
        let (mut engine, records) = if let Some(checkpoint) = checkpoint {
            if checkpoint.record_count > archive.records.len() {
                return Err(invalid_archive());
            }
            let records = archive.records.split_off(checkpoint.record_count);
            (checkpoint.restore(archive)?, records)
        } else {
            let records = std::mem::take(&mut archive.records);
            let mut engine = Self::memory(archive.scenario.clone())?;
            engine.current_branch = archive.branch.clone();
            engine.archive = archive;
            (engine, records)
        };
        engine.wizard_enabled = engine.archive.wizard_game;
        for record in records {
            if Uuid::parse_str(&record.entry.id.0).is_err()
                || engine
                    .archive
                    .records
                    .iter()
                    .any(|old| old.entry.id == record.entry.id)
            {
                return Err(invalid_archive());
            }
            let result = if let Some(receipt) = record.receipt {
                engine
                    .apply_command(&receipt, Some(record.entry.id.clone()))
                    .map(|result| result.entry)
            } else if let (
                Author::Backend { component },
                HistoryContent::Annotation {
                    anchor,
                    category,
                    text,
                },
            ) = (&record.entry.author, &record.entry.content)
            {
                engine.backend_note(
                    record.entry.actor,
                    component,
                    anchor.clone(),
                    *category,
                    text,
                    Some(record.entry.id.clone()),
                )
            } else {
                return Err(invalid_archive());
            };
            if result.map_err(|_| invalid_archive())? != record.entry {
                return Err(invalid_archive());
            }
        }
        engine.wizard_enabled = false;
        Ok(engine)
    }

    pub fn branch(&self) -> &BranchId {
        &self.current_branch
    }
    /// Trusted administration operation. The marker commits before authority changes.
    pub fn enable_wizard(&mut self) -> Result<(), Failure> {
        if !self.archive.wizard_game {
            if let Some(store) = &self.store {
                store.wizard()?;
            }
            // Once promotion is admitted it cannot be cleared, even if the
            // following durable barrier fails. Authority remains disabled.
            self.archive.wizard_game = true;
        }
        self.flush()?;
        self.wizard_enabled = true;
        Ok(())
    }
    pub fn wizard_enabled(&self) -> bool {
        self.wizard_enabled
    }
    pub fn actors(&self) -> Vec<ActorId> {
        self.revisions.keys().copied().collect()
    }
    pub fn revision(&self, actor: ActorId) -> Result<u64, Failure> {
        self.revisions
            .get(&actor)
            .copied()
            .ok_or_else(|| Failure::new(ErrorCode::Unauthorized, "Actor is unavailable"))
    }
    pub fn observation(&self, actor: ActorId) -> Result<Observation, Failure> {
        self.revision(actor)?;
        let (view, scene, ready) = self.revision_view(actor)?;
        Ok(adapt::observation(
            view,
            scene,
            &self.archive.view_salt,
            ready,
        ))
    }
    fn revision_view(&self, actor: ActorId) -> Result<RevisionView, Failure> {
        revision_view(&self.game, actor)
    }

    pub fn travel_route(
        &self,
        actor: ActorId,
        destination: &str,
    ) -> Result<Vec<tor_simulation::TravelStep>, Failure> {
        let unavailable = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Travel destination or known route is unavailable",
            )
        };
        let location = self
            .game
            .known_cells(SimActor(actor.0))
            .find(|&cell| adapt::cell_key(&self.archive.view_salt, actor.0, cell) == destination)
            .ok_or_else(unavailable)?;
        self.game
            .travel_route(SimActor(actor.0), location)
            .map_err(|_| unavailable())
    }

    pub fn state(&self, actor: ActorId) -> Result<StateView, Failure> {
        Ok(StateView {
            wizard_game: self.archive.wizard_game,
            revision: self.revision(actor)?,
            observation: self.observation(actor)?,
        })
    }

    pub fn history(
        &self,
        actor: ActorId,
        user: &str,
        before: Option<&EntryId>,
        limit: usize,
    ) -> Result<HistoryPage, Failure> {
        self.history_branch(actor, user, self.branch(), before, limit)
    }

    pub fn history_branch(
        &self,
        actor: ActorId,
        user: &str,
        branch: &BranchId,
        before: Option<&EntryId>,
        limit: usize,
    ) -> Result<HistoryPage, Failure> {
        self.revision(actor)?;
        if limit == 0 || limit > MAX_HISTORY_PAGE {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid history page size",
            ));
        }
        let visible: Vec<_> = self
            .archive
            .records
            .iter()
            .map(|record| &record.entry)
            .filter(|entry| &entry.branch == branch && entry.visible_to(actor, user))
            .collect();
        let end = match before {
            Some(id) => visible
                .iter()
                .position(|entry| &entry.id == id)
                .ok_or_else(invalid_anchor)?,
            None => visible.len(),
        };
        let start = end.saturating_sub(limit);
        Ok(HistoryPage {
            entries: visible[start..end]
                .iter()
                .map(|entry| entry.disclosed())
                .collect(),
            older_before: (start > 0).then(|| visible[start].id.clone()),
        })
    }

    /// Recover a durable receipt before checking ephemeral control ownership.
    pub fn retry(
        &self,
        user: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: &Command,
    ) -> Result<Option<CommandResult>, Failure> {
        let Some(&index) = self.receipts.get(&(user.into(), request_id.into())) else {
            return Ok(None);
        };
        let record = &self.archive.records[index];
        let receipt = record
            .receipt
            .as_ref()
            .expect("receipt index only contains client records");
        if receipt.actor != actor || &receipt.branch != branch || &receipt.command != command {
            return Err(Failure::new(
                ErrorCode::RequestConflict,
                "Request ID was already used for another command",
            ));
        }
        Ok(Some(CommandResult {
            entry: record.entry.clone(),
            duplicate: true,
        }))
    }

    pub fn command(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
            None,
        )
    }

    /// Execute the production command path while collecting phase diagnostics.
    pub fn command_profiled(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
    ) -> Result<(CommandResult, CommandProfile), Failure> {
        let started = Instant::now();
        let counts = tor_simulation::diagnostics::work_counts();
        let mut profile = CommandProfile::default();
        let result = self.apply_command_profiled(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
            Some(&mut profile),
        )?;
        profile.authoritative_total = started.elapsed();
        let after = tor_simulation::diagnostics::work_counts();
        profile.perception_calls = after.observations - counts.observations;
        profile.scene_calls = after.scenes - counts.scenes;
        Ok((result, profile))
    }

    /// (retained records, retained rewind boundaries), without cloning either.
    pub fn profile_counts(&self) -> (usize, usize) {
        (self.archive.records.len(), self.boundaries.len())
    }

    /// Attach a detached, seeded fixture to a new, exclusively locked save.
    /// Setup persists before any timed ordinary command can publish state.
    pub fn attach_profile_save(self, path: impl AsRef<Path>) -> Result<Self, Failure> {
        self.attach_profile_save_with_policy(path, crate::SavePolicy::default())
    }
    pub fn attach_profile_save_with_policy(
        mut self,
        path: impl AsRef<Path>,
        policy: crate::SavePolicy,
    ) -> Result<Self, Failure> {
        if self.path.is_some() {
            return Err(storage_failure());
        }
        let (path, lock) = lock_save(path.as_ref())?;
        if path.exists() {
            return Err(storage_failure());
        }
        let (store, _, _) =
            crate::storage::Store::open(&path, || Ok(self.archive.clone()), policy, lock.clone())?;
        self.path = Some(path);
        self.lock = Some(lock);
        self.store = Some(store);
        Ok(self)
    }

    /// Measure the current format-6 checkpoint JSON without allocating its encoded
    /// payload or writing a save. Includes capture, deduplication and serialization;
    /// intended for offline diagnostics, outside measured action intervals. The
    /// production 64 MiB writer limit remains enforced independently.
    pub fn profile_checkpoint_encoding(&self) -> Result<(u64, Duration), Failure> {
        struct Counter(u64);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 += bytes.len() as u64;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let start = Instant::now();
        let snapshot = Checkpoint::capture_candidate(
            &Candidate::capture(self),
            self.archive.records.len(),
            self.archive.wizard_game,
        );
        let encoded = snapshot.encode(
            "00000000-0000-0000-0000-000000000000",
            self.archive.records.len() as u64,
        );
        let mut counter = Counter(0);
        serde_json::to_writer(&mut counter, &encoded).map_err(|_| storage_failure())?;
        Ok((counter.0, start.elapsed()))
    }

    /// Persist the current archive to a disposable diagnostic target. The returned
    /// duration is complete bootstrap work, not command encoding or physical I/O.
    /// This does not attach the target to the engine or publish state.
    pub fn profile_persistence(&self, path: impl AsRef<Path>) -> Result<BootstrapProfile, Failure> {
        let started = Instant::now();
        let mut candidate = self.diagnostic_copy();
        candidate.path = None;
        candidate.lock = None;
        candidate.store = None;
        let candidate = candidate.attach_profile_save(path)?;
        candidate.flush()?;
        Ok(BootstrapProfile {
            total: started.elapsed(),
            records_serialized: self.archive.records.len(),
        })
    }

    /// Populate an in-memory diagnostic fixture in linear time. Records are
    /// ordinary deterministic wait actions and remain replayable; persistence
    /// is intentionally bypassed so a 10,000-action baseline does not spend its
    /// setup time exercising the quadratic behavior being measured.
    pub fn seed_profile_history(&mut self, count: usize) -> Result<(), Failure> {
        if self.path.is_some() || self.lock.is_some() {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Only detached fixtures may be seeded",
            ));
        }
        let start = self.archive.records.len();
        let end = start.checked_add(count).ok_or_else(invalid_archive)?;
        if count >= REWIND_BOUNDARIES {
            self.boundaries.clear();
        }
        for index in start..end {
            let actor = ActorId(self.game.next_actor().ok_or_else(invalid_archive)?.0);
            let expected_revision = self.revision(actor)?;
            let tick = self.game.tick();
            let outcome = self
                .game
                .act(SimActor(actor.0), tor_simulation::Action::Wait)
                .map_err(|_| invalid_archive())?;
            // Wait changes only time/readiness. Geometry and navigation are
            // identical; equivalence tests compare the complete Game and every
            // retained boundary against ordinary commands. Never use this
            // shortcut for measured actions or an engine attached to a save.
            for (&id, revision) in &mut self.revisions {
                if outcome.next_tick != tick || ((id == actor) != (id.0 == outcome.next_actor.0)) {
                    *revision = revision.checked_add(1).ok_or_else(invalid_archive)?;
                }
            }
            let entry = HistoryEntry {
                id: new_id(),
                branch: self.branch().clone(),
                actor,
                tick,
                author: Author::User {
                    user: "bench".into(),
                },
                audience: Audience::Actor,
                content: HistoryContent::Action {
                    action: tor_protocol::Action::Wait,
                    event: adapt::event(outcome.kind),
                },
            };
            let receipt = Receipt {
                user: "bench".into(),
                frontend: "headless".into(),
                request_id: format!("seed-{index}"),
                actor,
                branch: self.branch().clone(),
                command: Command::Act {
                    expected_revision,
                    action: tor_protocol::Action::Wait,
                },
            };
            self.receipts.insert(
                (receipt.user.clone(), receipt.request_id.clone()),
                self.archive.records.len(),
            );
            self.archive.records.push(Record {
                entry: entry.clone(),
                receipt: Some(receipt),
            });
            if end - index <= REWIND_BOUNDARIES {
                self.boundaries.push_back(Arc::new(Boundary {
                    id: Some(entry.id),
                    game: self.game.clone(),
                    revisions: self.revisions.clone(),
                }));
            }
            if self.boundaries.len() > REWIND_BOUNDARIES {
                self.boundaries.pop_front();
            }
        }
        Ok(())
    }

    fn apply_command(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled(receipt, recorded_id, None)
    }

    fn apply_command_profiled(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        if matches!(receipt.command, Command::Wizard { .. }) && !self.wizard_enabled {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard operations are disabled",
            ));
        }
        if !valid_label(&receipt.user)
            || !valid_label(&receipt.frontend)
            || !valid_label(&receipt.request_id)
        {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid identity or request ID",
            ));
        }
        if let Some(result) = self.retry(
            &receipt.user,
            receipt.actor,
            &receipt.request_id,
            &receipt.branch,
            &receipt.command,
        )? {
            return Ok(result);
        }
        if &receipt.branch != self.branch() {
            return Err(Failure::new(
                ErrorCode::WrongBranch,
                "Reconnect to the current branch",
            ));
        }
        let revision = self.revision(receipt.actor)?;
        let started = Instant::now();
        let mut candidate = Candidate::capture(self);
        if let Some(profile) = profile.as_deref_mut() {
            profile.rollback_capture += started.elapsed();
            profile.candidate_captures += 1;
        }
        // Exhaustive impact decisions: new commands/actions must explicitly
        // decide whether they can change remembered geometry.
        let navigation_changed = match &receipt.command {
            Command::Act { action, .. } => match action {
                Action::Move { .. } | Action::SetDoor { .. } => true,
                Action::Wait | Action::Take { .. } => false,
            },
            Command::Wizard { .. } => true,
            Command::Annotate { .. } | Command::Travel { .. } => false,
        };
        let mut tick = candidate.game.tick();
        let entry_id = recorded_id.unwrap_or_else(new_id);
        let (author, audience, content) = match &receipt.command {
            Command::Travel {
                expected_revision,
                destination,
            } => {
                if *expected_revision != revision {
                    return Err(Failure::new(
                        ErrorCode::StaleRevision,
                        "Refresh before travelling",
                    ));
                }
                self.travel_route(receipt.actor, destination)?;
                if self.game.next_actor() != Some(SimActor(receipt.actor.0)) {
                    return Err(Failure::new(ErrorCode::InvalidAction, "Actor is not ready"));
                }
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    HistoryContent::Travel {
                        destination: destination.clone(),
                    },
                )
            }

            Command::Wizard {
                expected_revision,
                operation,
            } => {
                if *expected_revision != revision {
                    return Err(Failure::new(
                        ErrorCode::StaleRevision,
                        "Refresh before a wizard operation",
                    ));
                }
                let result =
                    candidate.apply_wizard(receipt, operation, &entry_id, &self.archive.records)?;
                tick = candidate.game.tick();
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Private,
                    HistoryContent::Wizard {
                        operation: operation.clone(),
                        result,
                    },
                )
            }
            Command::Act {
                expected_revision,
                action,
            } => {
                if *expected_revision != revision {
                    return Err(Failure::new(
                        ErrorCode::StaleRevision,
                        "Refresh the observation before acting",
                    ));
                }
                let started = Instant::now();
                let perception_changed = match action {
                    Action::Wait => false,
                    Action::Move { .. } | Action::SetDoor { .. } | Action::Take { .. } => true,
                };
                let before: BTreeMap<_, _> = self
                    .actors()
                    .into_iter()
                    .filter(|_| perception_changed)
                    .map(|actor| Ok((actor, self.revision_view(actor)?)))
                    .collect::<Result<_, Failure>>()?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.perception += started.elapsed();
                    profile.actors_observed += before.len();
                }
                let started = Instant::now();
                let outcome = candidate
                    .game
                    .act(SimActor(receipt.actor.0), adapt::action(action))
                    .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Action is unavailable"))?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.simulation_transition += started.elapsed();
                    profile.simulation_transitions += 1;
                }
                // Wait changes only tick/readiness. Other outcomes retain full
                // comparison; new action kinds must make their impact explicit.
                if matches!(outcome.kind, tor_simulation::OutcomeKind::Waited) {
                    let started = Instant::now();
                    for (&actor, revision) in &mut candidate.revisions {
                        if outcome.next_tick != tick
                            || ((actor == receipt.actor) != (actor.0 == outcome.next_actor.0))
                        {
                            *revision = revision.checked_add(1).ok_or_else(|| {
                                Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                            })?;
                        }
                        if let Some(profile) = profile.as_deref_mut() {
                            profile.revision_comparisons += 1;
                        }
                    }
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.revision_detection += started.elapsed();
                    }
                }
                for (actor, old) in before {
                    let perception_started = Instant::now();
                    let after = candidate.revision_view(actor)?;
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.perception += perception_started.elapsed();
                        profile.actors_observed += 1;
                    }
                    if navigation_changed {
                        let started = Instant::now();
                        candidate
                            .game
                            .refresh_navigation_scene(SimActor(actor.0), &after.1);
                        if let Some(profile) = profile.as_deref_mut() {
                            profile.navigation_refresh += started.elapsed();
                        }
                    }
                    let started = Instant::now();
                    let changed = after != old;
                    if changed {
                        let revision = candidate.revisions.get_mut(&actor).expect("known actor");
                        *revision = revision.checked_add(1).ok_or_else(|| {
                            Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                        })?;
                    }
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.revision_detection += started.elapsed();
                        profile.revision_comparisons += 1;
                    }
                }
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    HistoryContent::Action {
                        action: action.clone(),
                        event: adapt::event(outcome.kind),
                    },
                )
            }
            Command::Annotate {
                anchor,
                text,
                source,
                audience,
                category,
            } => {
                self.validate_note(receipt.actor, &receipt.user, *audience, anchor, text)?;
                let author = match source {
                    ClientSource::User => Author::User {
                        user: receipt.user.clone(),
                    },
                    ClientSource::Frontend => Author::Frontend {
                        user: receipt.user.clone(),
                        component: receipt.frontend.clone(),
                    },
                };
                (
                    author,
                    *audience,
                    HistoryContent::Annotation {
                        anchor: anchor.clone(),
                        category: *category,
                        text: text.clone(),
                    },
                )
            }
        };
        let started = Instant::now();
        if matches!(receipt.command, Command::Wizard { .. }) {
            candidate.game.refresh_navigation();
            if let Some(profile) = profile.as_deref_mut() {
                profile.navigation_refresh += started.elapsed();
                profile.navigation_refreshes += 1;
            }
        }
        if navigation_changed && matches!(receipt.command, Command::Act { .. }) {
            if let Some(profile) = profile.as_deref_mut() {
                profile.navigation_refreshes += 1;
            }
        }
        let entry = HistoryEntry {
            id: entry_id,
            branch: candidate.branch().clone(),
            actor: receipt.actor,
            tick,
            author,
            audience,
            content,
        };
        let record = Record {
            entry: entry.clone(),
            receipt: Some(receipt.clone()),
        };
        if !matches!(entry.content, HistoryContent::Annotation { .. }) {
            let started = Instant::now();
            candidate.boundaries.push_back(Arc::new(Boundary {
                id: Some(entry.id.clone()),
                game: candidate.game.clone(),
                revisions: candidate.revisions.clone(),
            }));
            if candidate.boundaries.len() > REWIND_BOUNDARIES {
                candidate.boundaries.pop_front();
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.rollback_snapshot += started.elapsed();
                profile.rollback_snapshots += 1;
            }
        }
        self.admit(&record, &candidate, profile.as_deref_mut())?;
        let started = Instant::now();
        candidate.publish(self);
        self.receipts.insert(
            (receipt.user.clone(), receipt.request_id.clone()),
            self.archive.records.len(),
        );
        self.archive.records.push(record);
        if let Some(profile) = profile {
            profile.publication += started.elapsed();
        }
        Ok(CommandResult {
            entry,
            duplicate: false,
        })
    }

    pub fn annotate_backend(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
    ) -> Result<HistoryEntry, Failure> {
        self.backend_note(actor, component, anchor, category, text, None)
    }

    fn backend_note(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
        recorded_id: Option<EntryId>,
    ) -> Result<HistoryEntry, Failure> {
        if !valid_label(component) {
            return Err(Failure::new(
                ErrorCode::InvalidAnnotation,
                "Invalid backend component",
            ));
        }
        self.validate_note(actor, "", Audience::Actor, &anchor, text)?;
        let entry = HistoryEntry {
            id: recorded_id.unwrap_or_else(new_id),
            branch: self.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: component.into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::Annotation {
                anchor,
                category,
                text: text.into(),
            },
        };
        let record = Record {
            entry: entry.clone(),
            receipt: None,
        };
        self.admit(&record, &Candidate::capture(self), None)?;
        self.archive.records.push(record);
        Ok(entry)
    }

    fn validate_note(
        &self,
        actor: ActorId,
        user: &str,
        audience: Audience,
        anchor: &Anchor,
        text: &str,
    ) -> Result<(), Failure> {
        let revision = self.revision(actor)?;
        if text.trim().is_empty()
            || text.len() > MAX_NOTE_BYTES
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(Failure::new(
                ErrorCode::InvalidAnnotation,
                "Notes require 1â€“4096 bytes of plain text",
            ));
        }
        match anchor {
            Anchor::State { revision: target } if *target <= revision => Ok(()),
            Anchor::Entry { id } => {
                let entry = self
                    .archive
                    .records
                    .iter()
                    .map(|r| &r.entry)
                    .find(|entry| &entry.id == id)
                    .ok_or_else(invalid_anchor)?;
                if &entry.branch != self.branch()
                    || !entry.visible_to(actor, user)
                    || (audience == Audience::Actor && entry.audience == Audience::Private)
                {
                    return Err(invalid_anchor());
                }
                Ok(())
            }
            _ => Err(invalid_anchor()),
        }
    }

    // Full history copies are confined to detached diagnostics. Ordinary command
    // transactions use Candidate, which cannot own history or receipt indexes.
    fn diagnostic_copy(&self) -> Self {
        Self {
            recovery: self.recovery.clone(),
            current_branch: self.current_branch.clone(),
            wizard_enabled: self.wizard_enabled,
            boundaries: self.boundaries.clone(),
            game: self.game.clone(),
            archive: self.archive.clone(),
            revisions: self.revisions.clone(),
            receipts: self.receipts.clone(),
            path: self.path.clone(),
            lock: self.lock.clone(),
            store: self.store.clone(),
        }
    }

    fn admit(
        &self,
        record: &Record,
        candidate: &Candidate,
        profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        if let Some(store) = &self.store {
            let started = Instant::now();
            let mut capture_time = Duration::ZERO;
            let mut captures = 0;
            store.enqueue(record, || {
                let capture_started = Instant::now();
                let checkpoint = Checkpoint::capture_candidate(
                    candidate,
                    self.archive.records.len() + 1,
                    self.archive.wizard_game,
                );
                capture_time = capture_started.elapsed();
                captures = 1;
                checkpoint
            })?;
            if let Some(profile) = profile {
                profile.journal_serialization += started.elapsed() - capture_time;
                profile.checkpoint_capture += capture_time;
                profile.checkpoint_captures += captures;
                profile.records_serialized += 1;
            }
        }
        Ok(())
    }
}

pub(crate) fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn new_id() -> EntryId {
    EntryId(Uuid::new_v4().to_string())
}
fn invalid_anchor() -> Failure {
    Failure::new(ErrorCode::InvalidAnchor, "Annotation target is unavailable")
}
pub(crate) fn storage_failure() -> Failure {
    Failure::new(
        ErrorCode::StorageFailure,
        "Could not commit the game journal",
    )
}
pub(crate) fn invalid_archive() -> Failure {
    Failure::new(
        ErrorCode::InvalidArchive,
        "Unsupported or inconsistent game journal",
    )
}

fn lock_save(path: &Path) -> Result<(PathBuf, Arc<fs::File>), Failure> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|_| storage_failure())?;
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::canonicalize(parent)
            .map_err(|_| storage_failure())?
            .join(path.file_name().ok_or_else(storage_failure)?),
        Err(_) => return Err(storage_failure()),
    };
    let mut lock_name = canonical
        .file_name()
        .ok_or_else(storage_failure)?
        .to_os_string();
    lock_name.push(".lock");
    let file = fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(canonical.with_file_name(lock_name))
        .map_err(|_| storage_failure())?;
    file.try_lock().map_err(|_| {
        Failure::new(
            ErrorCode::StorageFailure,
            "Game journal is already in use or cannot be locked",
        )
    })?;
    Ok((canonical, Arc::new(file)))
}

impl Candidate {
    fn apply_wizard(
        &mut self,
        receipt: &Receipt,
        operation: &WizardOperation,
        entry_id: &EntryId,
        history: &[Record],
    ) -> Result<WizardResult, Failure> {
        let invalid = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Wizard target or settings are unavailable",
            )
        };
        let before: BTreeMap<_, _> = self
            .actors()
            .into_iter()
            .map(|actor| Ok((actor, self.revision_view(actor)?)))
            .collect::<Result<_, Failure>>()?;
        let result = match operation {
            WizardOperation::PlaceDoor { position, open } => {
                let door = self
                    .game
                    .place_door(adapt::location(*position), *open)
                    .map_err(|_| {
                        Failure::new(ErrorCode::InvalidAction, "Invalid door placement")
                    })?;
                WizardResult::DoorPlaced { door }
            }
            WizardOperation::ConnectArea {
                from,
                direction,
                to,
                quarter_turns,
                width,
                height,
            } => {
                self.game
                    .connect_area(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::direction(*direction),
                            to: adapt::location(*to),
                        },
                        *quarter_turns,
                        *width,
                        *height,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }
            WizardOperation::PlaceRoom { region } | WizardOperation::PlaceChamber { region } => {
                let chamber = matches!(operation, WizardOperation::PlaceChamber { .. });
                // Bound setup and disclosure work independently of wire limits.
                if region.id == 0
                    || region.name.is_empty()
                    || region.name.len() > 80
                    || region.name.chars().any(char::is_control)
                    || !(1..=32).contains(&region.width)
                    || !(1..=32).contains(&region.depth)
                    || !(1..=8).contains(&region.height)
                {
                    return Err(invalid());
                }
                let room = tor_world::Region {
                    id: tor_world::RegionId(region.id),
                    name: region.name.clone(),
                    bounds: tor_world::Extent::new(region.width, region.depth, region.height)
                        .ok_or_else(invalid)?,
                };
                if chamber {
                    self.game.add_chamber(room)
                } else {
                    self.game.add_region(room)
                }
                .map_err(|_| invalid())?;
                WizardResult::RoomPlaced { region: region.id }
            }
            WizardOperation::Connect {
                from,
                direction,
                to,
                quarter_turns,
            } => {
                self.game
                    .connect(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::direction(*direction),
                            to: adapt::location(*to),
                        },
                        *quarter_turns,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }
            WizardOperation::SetPlaceHint { position, present } => {
                self.game
                    .set_place_hint(adapt::location(*position), *present)
                    .map_err(|_| invalid())?;
                WizardResult::PlaceHintSet
            }
            WizardOperation::SetWall { position, wall } => {
                self.game
                    .set_wall(adapt::location(*position), *wall)
                    .map_err(|_| invalid())?;
                WizardResult::WallSet
            }
            WizardOperation::PlaceItem { kind, position } => {
                let name = match kind {
                    WizardItem::Token => "copper token",
                    WizardItem::Tablet => "stone tablet",
                };
                let item = self
                    .game
                    .place_item(adapt::location(*position), name.into())
                    .map_err(|_| invalid())?;
                WizardResult::ItemPlaced { item: item.0 }
            }
            WizardOperation::SpawnActor {
                position,
                turn_ticks,
            } => {
                let duration = NonZeroU64::new(*turn_ticks).ok_or_else(invalid)?;
                let actor = self
                    .game
                    .spawn_actor(adapt::location(*position), duration)
                    .map_err(|_| invalid())?;
                self.revisions.insert(ActorId(actor.0), 0);
                WizardResult::ActorSpawned {
                    actor: ActorId(actor.0),
                }
            }
            WizardOperation::Teleport { actor, position } => {
                self.game
                    .teleport(SimActor(actor.0), adapt::location(*position))
                    .map_err(|_| invalid())?;
                WizardResult::Teleported { actor: *actor }
            }
            WizardOperation::Rewind { target } => {
                if let Some(id) = target {
                    if !history.iter().any(|r| {
                        &r.entry.id == id && r.entry.visible_to(receipt.actor, &receipt.user)
                    }) {
                        return Err(invalid());
                    }
                }
                let boundary = self
                    .boundaries
                    .iter()
                    .find(|b| &b.id == target)
                    .cloned()
                    .ok_or_else(invalid)?;
                if !boundary.revisions.contains_key(&receipt.actor) {
                    return Err(invalid());
                }
                self.game = boundary.game.clone();
                self.revisions = boundary.revisions.clone();
                let from_branch = self.current_branch.clone();
                self.current_branch = BranchId(entry_id.0.clone());
                return Ok(WizardResult::Rewound {
                    from_branch,
                    branch: self.current_branch.clone(),
                    tick: self.game.tick(),
                    next_actor: ActorId(self.game.next_actor().ok_or_else(invalid)?.0),
                });
            }
        };
        for (actor, old) in before {
            if actor == receipt.actor || self.revision_view(actor)? != old {
                let revision = self.revisions.get_mut(&actor).expect("existing actor");
                *revision = revision.checked_add(1).ok_or_else(invalid)?;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod scaling_tests {
    use super::*;
    use tor_test_support::performance::Trace;

    fn checked_action(engine: &mut Engine, actor: ActorId, action: Action, request: usize) {
        let before: Vec<_> = engine
            .actors()
            .into_iter()
            .map(|id| {
                (
                    id,
                    engine.revision(id).unwrap(),
                    engine.revision_view(id).unwrap(),
                )
            })
            .collect();
        let mut reference = engine.game.clone();
        let result = engine.command(
            "oracle",
            "test",
            actor,
            &format!("action-{request}"),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: action.clone(),
            },
        );
        let expected = reference.act(SimActor(actor.0), adapt::action(&action));
        assert_eq!(result.is_ok(), expected.is_ok());
        if expected.is_ok() {
            reference.refresh_navigation();
        }
        assert_eq!(
            engine.game, reference,
            "optimized navigation must equal a full refresh"
        );
        for (id, revision, old) in before {
            let changed = engine.revision_view(id).unwrap() != old;
            assert_eq!(engine.revision(id).unwrap(), revision + u64::from(changed));
        }
    }

    #[test]
    fn optimized_decisions_match_full_views_and_navigation_for_every_actor() {
        let trace = Trace::load();
        for (regions, actors) in [(8, 1), (8, 8), (256, 8)] {
            let mut engine =
                Engine::memory(Scenario::performance(42, regions, actors).unwrap()).unwrap();
            let mut secondary = 0;
            let mut request = 0;
            for step in trace.steps(regions) {
                while engine.game.next_actor() != Some(SimActor(1)) {
                    let actor = ActorId(engine.game.next_actor().unwrap().0);
                    let action = if actor == ActorId(2) {
                        let action = trace.secondary[secondary % trace.secondary.len()]
                            .resolve(&engine.state(actor).unwrap());
                        secondary += 1;
                        action
                    } else {
                        Action::Wait
                    };
                    checked_action(&mut engine, actor, action, request);
                    request += 1;
                }
                let action = step.resolve(&engine.state(ActorId(1)).unwrap());
                checked_action(&mut engine, ActorId(1), action, request);
                request += 1;
            }
        }
    }

    #[test]
    fn rejected_transactions_preserve_receipts_history_boundaries_and_shared_state() {
        let directory = tempfile::tempdir().unwrap();
        let policy = crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(
            directory.path().join("rejected.db"),
            Scenario::two_room(42),
            policy,
        )
        .unwrap();
        let game = engine.game.clone();
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: 0,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        for _ in 0..2 {
            assert_eq!(
                engine
                    .command(
                        "p",
                        "test",
                        ActorId(1),
                        "retryable",
                        &branch,
                        command.clone()
                    )
                    .unwrap_err()
                    .code,
                ErrorCode::StorageFailure
            );
            assert_eq!(engine.game, game);
            assert!(engine.receipts.is_empty() && engine.archive.records.is_empty());
            assert_eq!(engine.boundaries.len(), 1);
            assert_eq!(engine.revision(ActorId(1)).unwrap(), 0);
        }
    }
}
