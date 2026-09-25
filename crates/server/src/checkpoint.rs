//! Checkpoint capture is cheap ownership transfer to the storage worker.
//! Encoding and world/navigation deduplication run outside the engine/session lock.
use super::*;
use std::collections::BTreeSet;
use tor_simulation::checkpoint::{SharedState, Snapshot};

#[derive(Debug)]
pub(crate) struct Checkpoint {
    current_branch: BranchId,
    game: Game,
    revisions: BTreeMap<ActorId, u64>,
    boundaries: VecDeque<Arc<Boundary>>,
    record_count: usize,
    wizard_game: bool,
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
    pub(crate) fn capture(engine: &Engine) -> Self {
        Self {
            current_branch: engine.current_branch.clone(),
            game: engine.game.clone(),
            revisions: engine.revisions.clone(),
            boundaries: engine.boundaries.clone(),
            record_count: engine.archive.records.len(),
            wizard_game: engine.archive.wizard_game,
        }
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
        let valid_revisions = |game: &Game, revisions: &BTreeMap<ActorId, u64>| {
            game.checkpoint_actor_ids()
                .eq(revisions.keys().map(|a| a.0))
        };
        if !valid_revisions(&game, &self.revisions) {
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
            if !valid_revisions(&game, &boundary.revisions) {
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
        })
    }
}
