use crate::{ObservationStream, StreamError};
use tor_protocol::*;

/// Shared presentation state for all frontends. Older history pages can be
/// requested separately; this model retains at most the latest 100 entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientState {
    snapshot: Snapshot,
    stream: ObservationStream,
}

impl ClientState {
    pub fn from_snapshot(snapshot: Snapshot) -> Result<Self, StreamError> {
        if snapshot.actor != snapshot.state.observation.actor
            || snapshot.cursor.tick != snapshot.state.observation.tick
        {
            return Err(StreamError::InconsistentState);
        }
        for entry in &snapshot.history.entries {
            validate_entry(
                entry,
                snapshot.actor,
                &snapshot.branch,
                snapshot.cursor.tick,
            )?;
        }
        Ok(Self {
            stream: ObservationStream::from_snapshot(snapshot.actor, snapshot.cursor),
            snapshot,
        })
    }

    pub fn state(&self) -> &StateView {
        &self.snapshot.state
    }
    pub fn history(&self) -> &[HistoryEntry] {
        &self.snapshot.history.entries
    }
    pub fn older_before(&self) -> Option<&EntryId> {
        self.snapshot.history.older_before.as_ref()
    }
    pub fn has_control(&self) -> bool {
        self.snapshot.has_control
    }
    pub fn branch(&self) -> &BranchId {
        &self.snapshot.branch
    }
    pub fn cursor(&self) -> StreamCursor {
        self.stream.cursor()
    }

    pub fn apply(&mut self, update: StreamUpdate) -> Result<(), StreamError> {
        if update.branch != self.snapshot.branch {
            return Err(StreamError::WrongBranch);
        }
        let mut candidate = self.clone();
        candidate.stream.accept(update.actor, update.cursor)?;
        match update.body {
            UpdateBody::Observation { state, event } => {
                if state.observation.actor != update.actor
                    || state.observation.tick != update.cursor.tick
                    || state.revision <= candidate.snapshot.state.revision
                {
                    return Err(StreamError::InconsistentState);
                }
                if let Some(entry) = event {
                    if !matches!(entry.content, HistoryContent::Action { .. }) {
                        return Err(StreamError::InconsistentState);
                    }
                    candidate.remember(*entry, update.cursor.tick)?;
                }
                candidate.snapshot.state = *state;
            }
            UpdateBody::Annotation { entry } => {
                if update.cursor.tick != candidate.snapshot.state.observation.tick
                    || !matches!(entry.content, HistoryContent::Annotation { .. })
                {
                    return Err(StreamError::InconsistentState);
                }
                candidate.remember(*entry, update.cursor.tick)?;
            }
            UpdateBody::Control { has_control } => {
                if update.cursor.tick != candidate.snapshot.state.observation.tick {
                    return Err(StreamError::InconsistentState);
                }
                candidate.snapshot.has_control = has_control;
            }
        }
        candidate.snapshot.cursor = update.cursor;
        *self = candidate;
        Ok(())
    }

    fn remember(&mut self, entry: HistoryEntry, tick: u64) -> Result<(), StreamError> {
        validate_entry(&entry, self.snapshot.actor, &self.snapshot.branch, tick)?;
        if let Some(previous) = self
            .snapshot
            .history
            .entries
            .iter()
            .find(|previous| previous.id == entry.id)
        {
            return if previous == &entry {
                Ok(())
            } else {
                Err(StreamError::InconsistentState)
            };
        }
        self.snapshot.history.entries.push(entry);
        if self.snapshot.history.entries.len() > MAX_HISTORY_PAGE {
            self.snapshot.history.entries.remove(0);
            self.snapshot.history.older_before = self
                .snapshot
                .history
                .entries
                .first()
                .map(|entry| entry.id.clone());
        }
        Ok(())
    }
}

fn validate_entry(
    entry: &HistoryEntry,
    actor: ActorId,
    branch: &BranchId,
    tick: u64,
) -> Result<(), StreamError> {
    if entry.actor != actor || &entry.branch != branch || entry.tick > tick {
        return Err(StreamError::InconsistentState);
    }
    Ok(())
}
