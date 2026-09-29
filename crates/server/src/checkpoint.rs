//! Checkpoint capture is cheap ownership transfer to the storage worker.
//! Encoding and world/navigation deduplication run outside the engine/session lock.
use super::*;
use std::collections::BTreeSet;
use tor_simulation::checkpoint::{SharedState, Snapshot};
use tor_simulation::RecordId;

#[derive(Debug)]
pub(crate) struct Checkpoint {
    current_branch: BranchId,
    game: Game,
    revisions: BTreeMap<ActorId, u64>,
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
    id: Option<EntryId>,
    game: Snapshot,
    revisions: BTreeMap<ActorId, u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiskCheckpoint {
    pub save_id: String,
    pub sequence: u64,
    pub record_count: usize,
    version: u32,
    ruleset: String,
    current_branch: BranchId,
    wizard_game: bool,
    shared: SharedState,
    game: Snapshot,
    revisions: BTreeMap<ActorId, u64>,
    boundaries: Vec<SavedBoundary>,
}

impl Checkpoint {
    #[cfg(test)]
    pub(crate) fn capture(engine: &Engine) -> Self {
        Self::capture_candidate(
            &Candidate::capture(engine),
            engine.archive.records.len(),
            engine.archive.wizard_game,
        )
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

impl DiskCheckpoint {
    pub(super) fn restore(self, archive: Archive) -> Result<Engine, Failure> {
        if self.version != ARCHIVE_VERSION
            || self.ruleset != RULESET
            || self.record_count != archive.records.len()
            || self.wizard_game && !archive.wizard_game
            || self.boundaries.is_empty()
            || self.boundaries.len() > REWIND_BOUNDARIES
            || !self.shared.valid_world_count(REWIND_BOUNDARIES + 1)
            || Uuid::parse_str(&self.current_branch.0).is_err()
            || archive.records.last().map(|r| &r.entry.branch) != Some(&self.current_branch)
        {
            return Err(invalid_archive());
        }
        let mut receipts = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for (index, record) in archive.records.iter().enumerate() {
            if Uuid::parse_str(&record.entry.id.0).is_err()
                || Uuid::parse_str(&record.entry.branch.0).is_err()
                || !ids.insert(record.entry.id.clone())
            {
                return Err(invalid_archive());
            }
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
            } else if !matches!(record.entry.author, Author::Backend { .. })
                || !matches!(record.entry.content, HistoryContent::Annotation { .. })
            {
                return Err(invalid_archive());
            }
        }
        let game = Game::restore_checkpoint(self.game, &self.shared).ok_or_else(invalid_archive)?;
        let valid_game = |game: &Game, revisions: &BTreeMap<ActorId, u64>| {
            game.checkpoint_actor_ids()
                .eq(revisions.keys().map(|a| a.0))
        };
        if !valid_game(&game, &self.revisions) {
            return Err(invalid_archive());
        }
        let mut boundaries = VecDeque::new();
        let mut expected_boundaries = VecDeque::from([None]);
        for record in &archive.records {
            if !matches!(record.entry.content, HistoryContent::Annotation { .. }) {
                expected_boundaries.push_back(Some(record.entry.id.clone()));
                if expected_boundaries.len() > REWIND_BOUNDARIES {
                    expected_boundaries.pop_front();
                }
            }
            if !self.wizard_game && matches!(record.entry.content, HistoryContent::Wizard { .. }) {
                return Err(invalid_archive());
            }
        }
        if !self
            .boundaries
            .iter()
            .map(|b| &b.id)
            .eq(expected_boundaries.iter())
        {
            return Err(invalid_archive());
        }
        for boundary in self.boundaries {
            if boundary.id.as_ref().is_some_and(|id| !ids.contains(id)) {
                return Err(invalid_archive());
            }
            let game = Game::restore_checkpoint(boundary.game, &self.shared)
                .ok_or_else(invalid_archive)?;
            if !valid_game(&game, &boundary.revisions) {
                return Err(invalid_archive());
            }
            boundaries.push_back(Arc::new(Boundary {
                id: boundary.id,
                game,
                revisions: boundary.revisions,
            }));
        }
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
            recovery: RecoveryProfile::default(),
            current_branch: self.current_branch,
            wizard_enabled: false,
            boundaries,
            game,
            archive,
            revisions: self.revisions,
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

    #[test]
    fn a_saved_game_referring_to_a_region_record_is_rejected() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let before = engine.state(ActorId(1)).unwrap();
        engine
            .command(
                "test",
                "checkpoint",
                ActorId(1),
                "wait",
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: before.revision,
                    action: tor_protocol::Action::Wait,
                },
            )
            .unwrap();
        let restore = |engine: &Engine| {
            Checkpoint::capture(engine)
                .encode("test", 1)
                .restore(engine.archive.clone())
        };
        assert!(restore(&engine).is_ok());

        // Nothing follows the actor, so every region can detach.
        let mut records = tor_simulation::MemoryRecords::default();
        let mut game = engine.game.clone();
        game.transition_regions(&Default::default(), &mut records)
            .unwrap();
        assert!(game.detached_records().next().is_some());
        let last = engine.boundaries.pop_back().unwrap();
        engine.boundaries.push_back(Arc::new(Boundary {
            game: game.clone(),
            ..(*last).clone()
        }));
        engine.game = game;
        assert!(restore(&engine).is_err());
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
