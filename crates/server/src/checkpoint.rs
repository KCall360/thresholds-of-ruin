//! Checkpoint capture is cheap ownership transfer to the storage worker.
//! Encoding and world/navigation deduplication run outside the engine/session lock.
use super::*;
use std::collections::BTreeSet;
use tor_simulation::checkpoint::{RestoreContext, SharedState, Snapshot};
use tor_simulation::RecordId;

#[derive(Debug)]
pub(crate) struct Checkpoint {
    current_branch: BranchId,
    game: Game,
    revisions: Revisions,
    boundaries: VecDeque<Arc<Boundary>>,
    record_count: usize,
    wizard_game: bool,
    /// Region records this checkpoint refers to that aren't on disk yet;
    /// the worker writes them in the checkpoint's transaction.
    pub(crate) records: BTreeMap<RecordId, tor_world::Shared<tor_simulation::RegionRecord>>,
    /// Records made from here on have identities at least this one.
    pub(crate) watermark: RecordId,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedBoundary {
    #[serde(with = "crate::storage::schema::optional_entry")]
    id: Option<EntryId>,
    game: Snapshot,
    revisions: Revisions,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiskCheckpoint {
    pub save_id: String,
    pub sequence: u64,
    pub record_count: usize,
    version: u32,
    ruleset: String,
    #[serde(with = "crate::storage::schema::BranchId")]
    current_branch: BranchId,
    wizard_game: bool,
    shared: SharedState,
    game: Snapshot,
    revisions: Revisions,
    boundaries: Vec<SavedBoundary>,
}

impl Checkpoint {
    #[cfg(test)]
    pub(crate) fn capture(engine: &Engine) -> Self {
        let mut checkpoint = Self::capture_candidate(
            &Candidate::capture(engine),
            engine.archive.records.len(),
            engine.archive.wizard_game,
        );
        if let Some(regions) = &engine.regions {
            checkpoint.add_records(regions, &[]);
        }
        checkpoint
    }

    pub(super) fn capture_candidate(
        state: &Candidate,
        record_count: usize,
        wizard_game: bool,
    ) -> Self {
        Self {
            current_branch: state.current_branch.clone(),
            game: state.game.clone(),
            revisions: state.revisions.clone(),
            boundaries: state.boundaries.clone(),
            record_count,
            wizard_game,
            records: BTreeMap::new(),
            watermark: state.game.next_record_id(),
        }
    }

    /// Every region record the current game and its boundaries refer to.
    pub(crate) fn referenced(&self) -> BTreeSet<RecordId> {
        std::iter::once(&self.game)
            .chain(self.boundaries.iter().map(|b| &b.game))
            .flat_map(|game| game.detached_records().map(|(_, id)| id))
            .collect()
    }

    /// Carry the referenced records that aren't on disk yet: kept ones, and
    /// those the command being admitted made.
    pub(super) fn add_records(
        &mut self,
        regions: &crate::regions::Regions,
        made: &[(RecordId, tor_world::Shared<tor_simulation::RegionRecord>)],
    ) {
        let referenced = self.referenced();
        self.records = regions.unwritten(referenced.iter().copied());
        self.records.extend(
            made.iter()
                .filter(|(id, _)| referenced.contains(id))
                .cloned(),
        );
    }

    pub(crate) fn encode(&self, save_id: &str, sequence: u64) -> DiskCheckpoint {
        let mut shared = SharedState::default();
        let game = self.game.checkpoint(&mut shared);
        let boundaries = self
            .boundaries
            .iter()
            .map(|b| SavedBoundary {
                id: b.id.clone(),
                game: b.game.checkpoint(&mut shared),
                revisions: b.revisions.clone(),
            })
            .collect();
        DiskCheckpoint {
            save_id: save_id.into(),
            sequence,
            record_count: self.record_count,
            version: ARCHIVE_VERSION,
            ruleset: RULESET.into(),
            current_branch: self.current_branch.clone(),
            wizard_game: self.wizard_game,
            shared,
            game,
            revisions: self.revisions.clone(),
            boundaries,
        }
    }
}

/// Recover private queue transitions between retained gameplay states using
/// the same simulation operations as live execution. No world effects or
/// authority decisions are invented by the checkpoint decoder.
fn replay_private_boundary(
    game: &mut Game,
    revisions: &mut Revisions,
    record: &Record,
) -> Result<(), Failure> {
    let before = game.clone();
    if record.entry.tick != before.tick() {
        return Err(invalid_archive());
    }
    match &record.entry.content {
        JournalContent::IntentionAdmitted { intention, action } => {
            let admitted = game
                .admit_intention(
                    SimActor(record.entry.actor.0),
                    adapt::action(action),
                    tor_simulation::IntentionOrigin::Human,
                )
                .map_err(|_| invalid_archive())?;
            if admitted != *intention {
                return Err(invalid_archive());
            }
        }
        JournalContent::TravelIntentionAdmitted {
            intention,
            action,
            destination,
            ..
        } => {
            let Action::Move { direction } = action else {
                return Err(invalid_archive());
            };
            if game
                .admit_travel_intention(
                    SimActor(record.entry.actor.0),
                    tor_simulation::TravelStep {
                        direction: adapt::direction(*direction),
                        destination: *destination,
                    },
                )
                .map_err(|_| invalid_archive())?
                != *intention
            {
                return Err(invalid_archive());
            }
        }
        JournalContent::AutonomousIntentionAdmitted { intention } => {
            if game
                .admit_ai_intention(SimActor(record.entry.actor.0))
                .map_err(|_| invalid_archive())?
                != *intention
            {
                return Err(invalid_archive());
            }
        }
        JournalContent::IntentionChanged {
            intention, change, ..
        } => {
            super::intention::apply_intention_change(game, record.entry.actor, *intention, *change)
                .map_err(|_| invalid_archive())?;
        }
        JournalContent::IntentionFailed { intention, .. }
        | JournalContent::IntentionContinuationFailed { intention, .. } => {
            let execution = game.execute_next_intention().ok_or_else(invalid_archive)?;
            if execution.intention.id != *intention
                || execution.intention.actor != SimActor(record.entry.actor.0)
                || execution.outcome.is_ok()
            {
                return Err(invalid_archive());
            }
        }
        JournalContent::Annotation { .. } => {}
        _ => return Err(invalid_archive()),
    }
    super::intention::preparation_revision_updates(&before, game, record.entry.actor, revisions)
        .map_err(|_| invalid_archive())?;
    if super::intention::derive_intention_suspensions(&before, game, &record.entry, true)
        != record.entry.intention_suspensions
    {
        return Err(invalid_archive());
    }
    if super::intention::derive_intention_ends(&before, game, &record.entry, true)
        != record.entry.intention_ends
    {
        return Err(invalid_archive());
    }
    Ok(())
}

impl DiskCheckpoint {
    pub(super) fn restore(self, archive: Archive) -> Result<Engine, Failure> {
        if self.version != ARCHIVE_VERSION
            || self.ruleset != RULESET
            || self.record_count != archive.records.len()
            || self.wizard_game && !archive.wizard_game
            || self.boundaries.is_empty()
            || self.boundaries.len() > MAX_RETAINED_BOUNDARIES
            // Private queue transitions do not create additional world states.
            || !self.shared.valid_world_count(REWIND_BOUNDARIES + 1)
            || Uuid::parse_str(&self.current_branch.0).is_err()
            || archive.records.last().map(|r| &r.entry.branch) != Some(&self.current_branch)
        {
            return Err(invalid_archive());
        }
        let mut receipts = BTreeMap::new();
        let mut records_by_id = BTreeMap::new();
        let mut lifecycle =
            super::intention_lifecycle::JournalLifecycle::new(archive.branch.clone());
        for (index, record) in archive.records.iter().enumerate() {
            if !record.valid_admission()
                || Uuid::parse_str(&record.entry.id.0).is_err()
                || Uuid::parse_str(&record.entry.branch.0).is_err()
                || records_by_id
                    .insert(record.entry.id.clone(), record)
                    .is_some()
            {
                return Err(invalid_archive());
            }
            lifecycle.observe(record)?;
            if let Some(receipt) = &record.receipt {
                if !valid_label(&receipt.user)
                    || !valid_label(&receipt.frontend)
                    || !valid_label(&receipt.request_id)
                    || receipt.actor != record.entry.actor
                    || Uuid::parse_str(&receipt.branch.0).is_err()
                    || receipts
                        .insert((receipt.user.clone(), receipt.request_id.clone()), index)
                        .is_some()
                {
                    return Err(invalid_archive());
                }
            } else if record.resolution().is_none()
                && record.change_source().is_none()
                && (!matches!(record.entry.author, Author::Backend { .. })
                    || !matches!(
                        record.entry.content,
                        JournalContent::Annotation { .. }
                            | JournalContent::AutonomousIntentionAdmitted { .. }
                            | JournalContent::TravelIntentionAdmitted { .. }
                    ))
            {
                return Err(invalid_archive());
            }
        }
        let mut restore = RestoreContext::new(&self.shared);
        let game = restore.restore(self.game).ok_or_else(invalid_archive)?;
        if !self.revisions.valid_for(&game)
            || !lifecycle.valid_game(
                &game,
                &self.boundaries.last().ok_or_else(invalid_archive)?.id,
            )
        {
            return Err(invalid_archive());
        }
        let mut boundaries: VecDeque<Arc<Boundary>> = VecDeque::new();
        let mut raw = VecDeque::from([(None, true, None)]);
        let mut selectable = raw.clone();
        for (index, record) in archive.records.iter().enumerate() {
            if !matches!(record.entry.content, JournalContent::Annotation { .. }) {
                let boundary = (
                    Some(&record.entry.id),
                    record.entry.content.rewindable(),
                    Some(index),
                );
                raw.push_back(boundary);
                if raw.len() > REWIND_BOUNDARIES {
                    raw.pop_front();
                }
                if boundary.1 {
                    selectable.push_back(boundary);
                    if selectable.len() > REWIND_BOUNDARIES {
                        selectable.pop_front();
                    }
                }
            }
            if !self.wizard_game && matches!(record.entry.content, JournalContent::Wizard { .. }) {
                return Err(invalid_archive());
            }
        }
        let mut expected_boundaries: Vec<_> = raw.into_iter().chain(selectable).collect();
        expected_boundaries.sort_unstable_by_key(|b| b.2);
        expected_boundaries.dedup_by_key(|b| b.2);
        if !self
            .boundaries
            .iter()
            .map(|b| b.id.as_ref())
            .eq(expected_boundaries.iter().map(|b| b.0))
        {
            return Err(invalid_archive());
        }
        let mut previous_record_index = None;
        for (boundary, (_, selectable, record_index)) in
            self.boundaries.into_iter().zip(expected_boundaries)
        {
            if boundary
                .id
                .as_ref()
                .is_some_and(|id| !records_by_id.contains_key(id))
            {
                return Err(invalid_archive());
            }
            let game = restore.restore(boundary.game).ok_or_else(invalid_archive)?;
            if !boundary.revisions.valid_for(&game) || !lifecycle.valid_game(&game, &boundary.id) {
                return Err(invalid_archive());
            }
            if let Some(record) = boundary.id.as_ref().and_then(|id| records_by_id.get(id)) {
                if let Some(previous) = boundaries.back() {
                    // Selectable states may outlive their raw audit neighbors.
                    // Replay only private metadata through that gap; skipping a
                    // gameplay transaction is invalid, not an approximate proof.
                    let start = previous_record_index.map_or(0, |index| index + 1);
                    let end = record_index.ok_or_else(invalid_archive)?;
                    let recovered = if start < end {
                        let mut preceding = previous.game.clone();
                        let mut preceding_revisions = previous.revisions.clone();
                        for hidden in &archive.records[start..end] {
                            replay_private_boundary(
                                &mut preceding,
                                &mut preceding_revisions,
                                hidden,
                            )?;
                        }
                        Some((preceding, preceding_revisions))
                    } else {
                        None
                    };
                    let (preceding, preceding_revisions) = recovered
                        .as_ref()
                        .map(|(game, revisions)| (game, revisions))
                        .unwrap_or((&previous.game, &previous.revisions));
                    if !record.entry.content.rewindable() {
                        let mut expected = preceding.clone();
                        let mut expected_revisions = preceding_revisions.clone();
                        replay_private_boundary(&mut expected, &mut expected_revisions, record)?;
                        if expected != game || expected_revisions != boundary.revisions {
                            return Err(invalid_archive());
                        }
                    }
                    let same_branch = previous
                        .id
                        .as_ref()
                        .and_then(|id| records_by_id.get(id))
                        .is_none_or(|before| before.entry.branch == record.entry.branch);
                    if super::intention::derive_intention_suspensions(
                        preceding,
                        &game,
                        &record.entry,
                        same_branch,
                    ) != record.entry.intention_suspensions
                    {
                        return Err(invalid_archive());
                    }
                    if super::intention::derive_intention_ends(
                        preceding,
                        &game,
                        &record.entry,
                        same_branch,
                    ) != record.entry.intention_ends
                    {
                        return Err(invalid_archive());
                    }
                }
                if record.resolution().is_some()
                    && game
                        .pending_intention(SimActor(record.entry.actor.0))
                        .is_some()
                {
                    return Err(invalid_archive());
                }
                if let Some((intention, work)) = record.entry.content.admission() {
                    if game
                        .pending_intention(SimActor(record.entry.actor.0))
                        .is_none_or(|queued| {
                            queued.id != intention
                                || queued.origin != work.origin()
                                || !work.matches(queued.work)
                                || queued.state != tor_simulation::IntentionState::Queued
                        })
                    {
                        return Err(invalid_archive());
                    }
                }
            }
            boundaries.push_back(Arc::new(Boundary {
                id: boundary.id,
                selectable,
                game,
                revisions: boundary.revisions,
            }));
            previous_record_index = record_index;
        }
        drop(restore);
        if boundaries
            .back()
            .is_none_or(|b| b.game != game || b.revisions != self.revisions)
        {
            return Err(invalid_archive());
        }
        let mut regions = crate::engine::streaming_regions(&archive.scenario)?;
        if let Some(regions) = &mut regions {
            // Every record the checkpoint refers to was written with it.
            let on_disk = std::iter::once(&game)
                .chain(boundaries.iter().map(|b| &b.game))
                .flat_map(|g| g.detached_records().map(|(_, id)| id))
                .collect();
            regions.written(on_disk, game.next_record_id());
        } else if std::iter::once(&game)
            .chain(boundaries.iter().map(|b| &b.game))
            .any(|g| g.detached_records().next().is_some())
        {
            return Err(invalid_archive());
        }
        Ok(Engine {
            history_index: crate::history_index::HistoryIndex::rebuild(
                archive.records.iter().map(|record| &record.entry),
            ),
            recovery: RecoveryProfile::default(),
            current_branch: self.current_branch,
            wizard_enabled: false,
            boundaries,
            game,
            archive,
            revisions: self.revisions,
            observations: ObservationCache::default(),
            receipts,
            path: None,
            lock: None,
            store: None,
            regions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explored_checkpoint_preserves_exact_navigation_at_every_rewind_boundary() {
        let trace = tor_test_support::performance::Trace::load();
        let mut engine = Engine::memory(Scenario::performance(trace.seed, 16, 1).unwrap()).unwrap();
        for cycle in 0..15 {
            for (index, step) in trace.traversal.iter().enumerate() {
                let before = engine.state(ActorId(1)).unwrap();
                engine
                    .command(
                        "test",
                        "checkpoint",
                        ActorId(1),
                        &format!("{cycle}-{index}"),
                        &engine.branch().clone(),
                        Command::Act {
                            expected_revision: before.revision,
                            action: step.resolve(&before),
                        },
                    )
                    .unwrap();
            }
        }
        let bytes = serde_json::to_vec(&Checkpoint::capture(&engine).encode("test", 165)).unwrap();
        let restored_disk: DiskCheckpoint = serde_json::from_slice(&bytes).unwrap();
        let restored = restored_disk.restore(engine.archive.clone()).unwrap();
        assert_eq!(restored.game, engine.game);
        assert_eq!(restored.boundaries.len(), engine.boundaries.len());
        for (actual, expected) in restored.boundaries.iter().zip(&engine.boundaries) {
            assert_eq!(actual.game, expected.game);
            assert_eq!(actual.revisions, expected.revisions);
            assert_eq!(actual.id, expected.id);
        }
        assert_eq!(
            serde_json::to_vec(&Checkpoint::capture(&restored).encode("test", 165)).unwrap(),
            bytes
        );
        let disk: DiskCheckpoint = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            Game::restore_checkpoint(disk.game, &disk.shared),
            Some(engine.game.clone())
        );
        assert_eq!(disk.boundaries.len(), engine.boundaries.len());
        for (saved, original) in disk.boundaries.into_iter().zip(&engine.boundaries) {
            assert_eq!(saved.id, original.id);
            assert_eq!(saved.revisions, original.revisions);
            assert_eq!(
                Game::restore_checkpoint(saved.game, &disk.shared),
                Some(original.game.clone())
            );
        }
    }

    fn walk(engine: &mut Engine, direction: tor_protocol::Direction, steps: usize) {
        for _ in 0..steps {
            let revision = engine.revision(ActorId(1)).unwrap();
            let request = format!("walk-{direction:?}-{revision}");
            engine
                .command(
                    "player",
                    "test",
                    ActorId(1),
                    &request,
                    &engine.branch().clone(),
                    Command::Act {
                        expected_revision: revision,
                        action: tor_protocol::Action::Move { direction },
                    },
                )
                .unwrap();
        }
    }

    /// Revisions a command copies are bounded by what's loaded: actors in
    /// unbuilt or detached regions are parked in a map the copy shares.
    #[test]
    fn a_command_copies_revisions_only_for_loaded_actors() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/streaming-corridor");
        let mut scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let engine = Engine::memory(scenario).unwrap();
        // The guard's hall hasn't been needed, so the game doesn't know the
        // guard yet; nothing is parked until an actor leaves the loaded world.
        assert_eq!(engine.revisions.keys().collect::<Vec<_>>(), [&ActorId(1)]);
        assert!(engine.revision(ActorId(2)).is_err());
        assert!(engine.revisions.parked.is_empty());
        let candidate = Candidate::capture(&engine);
        assert!(candidate
            .revisions
            .parked
            .shares_storage(&engine.revisions.parked));
        assert!(engine.revisions.valid_for(&engine.game));
    }

    /// A save attached to a game that started in memory (as the benchmarks
    /// do) must also serve records back once they're only on disk.
    #[test]
    fn a_save_attached_later_reads_evicted_records_back() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/streaming-corridor");
        let mut scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::memory(scenario)
            .unwrap()
            .attach_profile_save_with_policy(
                directory.path().join("attached.db"),
                crate::SavePolicy {
                    checkpoint_interval: 4,
                    ..crate::SavePolicy::default()
                },
            )
            .unwrap();
        walk(&mut engine, tor_protocol::Direction::East, 68);
        assert_eq!(engine.region_counts().unwrap().detached, 2);
        engine.flush().unwrap();
        // The next command learns what the checkpoint wrote; then memory
        // lets go of it.
        walk(&mut engine, tor_protocol::Direction::East, 1);
        engine.regions.as_mut().unwrap().evict_durable();
        walk(&mut engine, tor_protocol::Direction::West, 69);
        assert!(engine.region_counts().unwrap().records_read >= 2);
    }

    #[test]
    fn region_references_round_trip_and_reject_malformed_navigation() {
        let engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let checkpoint = Checkpoint::capture(&engine).encode("test", 0);
        let value = serde_json::to_value(&checkpoint).unwrap();
        let decoded: DiskCheckpoint = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            Game::restore_checkpoint(decoded.game, &decoded.shared),
            Some(engine.game.clone())
        );
        for damage in ["index", "duplicate", "empty", "unknown", "missing", "flat"] {
            let mut broken = value.clone();
            let navigation = &mut broken["shared"]["navigation"];
            match damage {
                "index" => navigation["instances"][0]["cells"][0] = serde_json::json!(usize::MAX),
                "duplicate" => {
                    let first = navigation["instances"][0]["cells"][0].clone();
                    navigation["instances"][0]["cells"]
                        .as_array_mut()
                        .unwrap()
                        .push(first);
                }
                "empty" => navigation["cells"][0] = serde_json::json!([]),
                "unknown" => {
                    navigation["instances"][0]["extra"] = serde_json::json!(0);
                }
                "missing" => {
                    navigation["instances"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("edges");
                }
                "flat" => *navigation = serde_json::json!([]),
                _ => unreachable!(),
            }
            assert!(
                serde_json::from_value::<DiskCheckpoint>(broken).is_err(),
                "{damage}"
            );
        }
    }
}
