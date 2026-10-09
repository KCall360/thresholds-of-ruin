use crate::{ObservationStream, StreamError};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use tor_protocol::*;

/// Last disclosed contents of one cell, not current world truth. Only a fresh
/// observation of this exact cell can replace its remembered contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RememberedCell {
    pub door: Option<DoorView>,
    pub material: String,
    pub key: String,
    /// Relative offset at the last sighting, not a current map location.
    pub position: Position,
    pub wall: bool,
    pub place_hint: bool,
    pub last_seen_tick: u64,
    pub last_seen_revision: u64,
    pub ground_items: Vec<GroundItemView>,
    pub visible_actors: Vec<ActorView>,
    pub stairs_up: bool,
    pub stairs_down: bool,
}

impl RememberedCell {
    pub fn cell_view(&self) -> CellView {
        CellView {
            key: self.key.clone(),
            position: self.position,
            wall: self.wall,
            stairs_up: self.stairs_up,
            stairs_down: self.stairs_down,
            place_hint: self.place_hint,
            door: self.door.clone(),
            material: self.material.clone(),
            asset: None,
        }
    }
}

/// Shared presentation state for all frontends. Older history pages can be
/// requested separately; this model retains at most the latest 100 entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientState {
    snapshot: Snapshot,
    stream: ObservationStream,
    observation_base: ObservationBase,
    memory: BTreeMap<String, RememberedCell>,
    map_memory: crate::map_memory::MapMemory,
    narration: Vec<String>,
}

impl ClientState {
    pub fn from_snapshot(snapshot: Snapshot) -> Result<Self, StreamError> {
        if !snapshot.context.is_valid() {
            return Err(StreamError::WrongStreamContext);
        }
        validate_intentions(&snapshot.intentions, snapshot.actor, &snapshot.branch)?;
        snapshot
            .state
            .validate()
            .map_err(|_| StreamError::InconsistentState)?;
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
        validate_readiness(&snapshot.readiness, &snapshot)?;
        let mut client = Self {
            stream: ObservationStream::from_snapshot(snapshot.actor, snapshot.cursor),
            observation_base: ObservationBase {
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            },
            snapshot,
            memory: BTreeMap::new(),
            map_memory: Default::default(),
            narration: Vec::new(),
        };
        client.remember_view();
        Ok(client)
    }

    /// Local observations only; place names and history never manufacture views.
    /// Memory lasts for this connection and is cleared on a branch change.
    pub fn memory(&self) -> impl Iterator<Item = &RememberedCell> {
        self.memory.values()
    }

    /// Disclosed sightings aligned to the current view, without remembered actors.
    /// Ambiguous/disconnected views start a new chart; at most 4096 cells are kept.
    pub fn map_memory(&self) -> impl Iterator<Item = &RememberedCell> {
        self.map_memory.cells.values()
    }

    /// Current disclosure wins over aligned stale map memory. This never
    /// manufactures knowledge from an unaligned historical sighting.
    pub fn map_cell(&self, position: Position) -> Option<CellView> {
        self.state()
            .observation
            .visible_cells
            .iter()
            .find(|c| c.position == position)
            .cloned()
            .or_else(|| {
                self.map_memory
                    .cells
                    .get(&(position.x, position.y, position.z))
                    .map(RememberedCell::cell_view)
            })
    }

    /// A validated snapshot establishes a new stream boundary atomically.
    pub fn replace_snapshot(&mut self, snapshot: Snapshot) -> Result<(), StreamError> {
        if snapshot.actor != self.snapshot.actor {
            return Err(StreamError::WrongActor);
        }
        if snapshot.context.stream != self.snapshot.context.stream
            || snapshot.context.epoch <= self.snapshot.context.epoch
        {
            return Err(StreamError::WrongStreamContext);
        }
        let mut candidate = Self::from_snapshot(snapshot)?;
        if candidate.branch() == self.branch() {
            candidate.memory = std::mem::take(&mut self.memory);
            candidate.map_memory = std::mem::take(&mut self.map_memory);
            candidate.remember_view();
        }
        *self = candidate;
        Ok(())
    }

    fn remember_view(&mut self) {
        let observation = &self.snapshot.state.observation;
        for cell in &observation.visible_cells {
            self.memory.insert(
                cell.key.clone(),
                RememberedCell {
                    door: cell.door.clone(),
                    material: cell.material.clone(),
                    key: cell.key.clone(),
                    position: cell.position,
                    wall: cell.wall,
                    place_hint: cell.place_hint,
                    last_seen_tick: observation.tick,
                    last_seen_revision: self.snapshot.state.revision,
                    ground_items: observation
                        .ground_items
                        .iter()
                        .filter(|item| item.position == cell.position)
                        .cloned()
                        .collect(),
                    visible_actors: observation
                        .visible_actors
                        .iter()
                        .filter(|actor| actor.position == cell.position)
                        .cloned()
                        .collect(),
                    stairs_up: cell.stairs_up,
                    stairs_down: cell.stairs_down,
                },
            );
        }
        self.map_memory.observe(observation, &self.memory);
    }

    /// Current disclosed stream boundary, excluding connection-local memory.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.clone()
    }

    pub fn travel(&self) -> Option<&TravelStatus> {
        self.snapshot.travel.as_ref()
    }

    pub fn intentions(&self) -> &[IntentionStatus] {
        &self.snapshot.intentions
    }

    pub fn has_pending_intention(&self) -> bool {
        self.intentions()
            .iter()
            .any(|status| status.phase.pending())
    }

    /// Published permissions can briefly lag their causing lifecycle/control
    /// updates. Block input during that boundary instead of inferring availability.
    pub fn can_admit_intention(&self) -> bool {
        self.readiness().admission && validate_readiness(self.readiness(), &self.snapshot).is_ok()
    }

    /// Select queued work before independent preparation, regardless of delivery order.
    /// Input builders and presentation share this selection even without control.
    pub fn intention_for_input(&self) -> Option<&IntentionStatus> {
        self.intentions()
            .iter()
            .find(|status| status.phase.pending())
            .or_else(|| {
                self.intentions()
                    .iter()
                    .find(|status| status.phase.active())
            })
    }

    /// Explicit fresh input references the currently disclosed queued work.
    pub fn resume_intention_request(&self) -> Option<Request> {
        self.intention_request(true)
    }

    pub fn cancel_intention_request(&self) -> Option<Request> {
        self.intention_request(false)
    }

    pub fn can_resume_intention(&self) -> bool {
        self.permitted_intention(true).is_some()
    }

    pub fn can_cancel_intention(&self) -> bool {
        self.permitted_intention(false).is_some()
    }

    fn permitted_intention(&self, resume: bool) -> Option<&IntentionStatus> {
        if validate_readiness(self.readiness(), &self.snapshot).is_err() {
            return None;
        }
        let selected = self.intention_for_input()?;
        let permitted = if resume {
            &self.readiness().resume
        } else {
            &self.readiness().cancel
        };
        if !permitted.contains(&selected.intention) {
            return None;
        }
        Some(selected)
    }

    fn intention_request(&self, resume: bool) -> Option<Request> {
        let intention = self.permitted_intention(resume)?.intention.clone();
        let expected_revision = self.state().revision;
        let command = if resume {
            Command::ResumeIntention {
                expected_revision,
                intention,
            }
        } else {
            Command::CancelIntention {
                expected_revision,
                intention,
            }
        };
        Some(self.command_request(command))
    }

    pub fn state(&self) -> &StateView {
        &self.snapshot.state
    }
    /// Latest observation's prose, derived only from disclosed facts. Not history.
    pub fn narration(&self) -> &[String] {
        &self.narration
    }
    pub fn history(&self) -> &[HistoryEntry] {
        &self.snapshot.history.entries
    }
    pub fn older_before(&self) -> Option<&EntryId> {
        self.snapshot.history.older_before.as_ref()
    }
    /// Capture at input construction, never restamp queued or retried commands.
    pub fn input_context(&self) -> InputContext {
        InputContext {
            stream: self.context().clone(),
            readiness_revision: self.readiness().revision,
        }
    }

    /// Capture the disclosed state at a reply boundary without modifying it.
    /// A receipt's original branch or actor is a separate operation identity.
    pub fn reply_context(&self) -> ReplyContext {
        self.snapshot.reply_context()
    }

    /// Replies name the complete currently disclosed boundary. Validation never
    /// installs permissions or observations from a reply in place of stream updates.
    pub fn validate_reply_context(&self, context: &ReplyContext) -> Result<(), StreamError> {
        if &context.input.stream != self.context() {
            return Err(StreamError::WrongStreamContext);
        }
        if context.actor != self.snapshot.actor {
            return Err(StreamError::WrongActor);
        }
        if &context.branch != self.branch() {
            return Err(StreamError::WrongBranch);
        }
        if context.cursor != self.snapshot.cursor {
            return Err(StreamError::SequenceMismatch);
        }
        if context.revision != self.state().revision
            || context.input.readiness_revision != self.readiness().revision
        {
            return Err(StreamError::InconsistentState);
        }
        Ok(())
    }

    pub fn command_request(&self, command: Command) -> Request {
        Request::Command {
            context: self.input_context(),
            branch: self.branch().clone(),
            command,
        }
    }

    pub fn readiness(&self) -> &Readiness {
        &self.snapshot.readiness
    }

    pub fn has_control(&self) -> bool {
        self.snapshot.has_control
    }
    pub fn context(&self) -> &StreamContext {
        &self.snapshot.context
    }

    pub fn branch(&self) -> &BranchId {
        &self.snapshot.branch
    }
    pub fn observation_base(&self) -> ObservationBase {
        self.observation_base
    }

    pub fn cursor(&self) -> StreamCursor {
        self.stream.cursor()
    }

    pub fn apply(&mut self, update: StreamUpdate) -> Result<(), StreamError> {
        if update.context != self.snapshot.context {
            return Err(StreamError::WrongStreamContext);
        }
        if update.branch != self.snapshot.branch {
            return Err(StreamError::WrongBranch);
        }
        // Validate ordering separately. Each payload arm completes all fallible
        // checks (including history validation) before publishing any changes.
        let mut stream = self.stream.clone();
        stream.accept(update.actor, update.cursor)?;
        let body = match update.body {
            UpdateBody::ObservationDelta { base, state, event } => {
                if base != self.observation_base {
                    return Err(StreamError::WrongObservationBase);
                }
                UpdateBody::Observation {
                    state: Arc::new(
                        state
                            .apply(&self.snapshot.state)
                            .map_err(|_| StreamError::InconsistentState)?,
                    ),
                    event,
                }
            }
            body => body,
        };
        match body {
            UpdateBody::Readiness { readiness } => {
                if update.cursor.tick != self.snapshot.state.observation.tick
                    || self.snapshot.readiness.revision.checked_add(1) != Some(readiness.revision)
                {
                    return Err(StreamError::InconsistentState);
                }
                validate_readiness(&readiness, &self.snapshot)?;
                self.snapshot.readiness = readiness;
            }
            UpdateBody::Intention { status } => {
                if update.cursor.tick != self.snapshot.state.observation.tick
                    || !status.valid_context(update.actor, &update.branch)
                {
                    return Err(StreamError::InconsistentState);
                }
                let previous = self
                    .snapshot
                    .intentions
                    .iter()
                    .position(|old| old.intention == status.intention);
                if let Some(index) = previous {
                    let old = &self.snapshot.intentions[index];
                    if old.entry_id != status.entry_id || !status.phase.can_follow(old.phase) {
                        return Err(StreamError::InconsistentState);
                    }
                } else if status.phase != IntentionPhase::Queued {
                    return Err(StreamError::InconsistentState);
                }
                let mut intentions = self.snapshot.intentions.clone();
                if let Some(index) = previous {
                    if status.phase.active() {
                        intentions[index] = status;
                    } else {
                        intentions.remove(index);
                    }
                } else {
                    intentions.push(status);
                }
                validate_intentions(&intentions, update.actor, &update.branch)?;
                self.snapshot.intentions = intentions;
            }
            UpdateBody::Travel { status, entry } => {
                if update.cursor.tick != self.snapshot.state.observation.tick {
                    return Err(StreamError::InconsistentState);
                }
                if let Some(previous) = self
                    .snapshot
                    .travel
                    .as_ref()
                    .filter(|old| old.id == status.id)
                {
                    if previous.destination != status.destination
                        || status.completed_steps < previous.completed_steps
                        || (previous.phase != TravelPhase::Active
                            && status.phase == TravelPhase::Active)
                    {
                        return Err(StreamError::InconsistentState);
                    }
                } else if entry.is_none() {
                    return Err(StreamError::InconsistentState);
                }
                if let Some(entry) = entry {
                    if !matches!(&entry.content, HistoryContent::Travel { destination } if destination == &status.destination)
                        || entry.id != status.id
                    {
                        return Err(StreamError::InconsistentState);
                    }
                    self.remember(*entry, update.cursor.tick)?;
                }
                self.snapshot.travel = Some(status);
            }
            UpdateBody::Observation { state, event } => {
                state
                    .validate()
                    .map_err(|_| StreamError::InconsistentState)?;
                if state.observation.actor != update.actor
                    || state.observation.tick != update.cursor.tick
                    || state.revision <= self.snapshot.state.revision
                {
                    return Err(StreamError::InconsistentState);
                }
                let own_action = event.as_ref().and_then(|entry| match &entry.content {
                    HistoryContent::Action { event, .. } => Some(event.clone()),
                    _ => None,
                });
                if let Some(entry) = event {
                    if !matches!(
                        entry.content,
                        HistoryContent::Action { .. } | HistoryContent::PlaceRenamed { .. }
                    ) {
                        return Err(StreamError::InconsistentState);
                    }
                    self.remember(*entry, update.cursor.tick)?;
                }
                self.narration = crate::narration::observation(
                    &self.snapshot.state.observation,
                    &state.observation,
                    own_action.as_ref(),
                );
                self.observation_base = ObservationBase {
                    cursor: update.cursor,
                    revision: state.revision,
                };
                self.snapshot.state = state;
                self.remember_view();
            }
            UpdateBody::ObservationDelta { .. } => unreachable!("expanded above"),
            UpdateBody::Annotation { entry } => {
                if update.cursor.tick != self.snapshot.state.observation.tick
                    || !matches!(entry.content, HistoryContent::Annotation { .. })
                {
                    return Err(StreamError::InconsistentState);
                }
                self.remember(*entry, update.cursor.tick)?;
            }
            UpdateBody::Control { has_control } => {
                if update.cursor.tick != self.snapshot.state.observation.tick {
                    return Err(StreamError::InconsistentState);
                }
                self.snapshot.has_control = has_control;
            }
        }
        self.snapshot.cursor = update.cursor;
        self.stream = stream;
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

fn validate_intentions(
    intentions: &[IntentionStatus],
    actor: ActorId,
    branch: &BranchId,
) -> Result<(), StreamError> {
    if intentions.len() > 2
        || intentions
            .iter()
            .any(|status| !status.phase.active() || !status.valid_context(actor, branch))
        || intentions
            .iter()
            .filter(|status| status.phase.pending())
            .count()
            > 1
        || intentions
            .iter()
            .filter(|status| {
                matches!(
                    status.phase,
                    IntentionPhase::Started | IntentionPhase::Paused
                )
            })
            .count()
            > 1
        || intentions.len() == 2 && intentions[0].intention == intentions[1].intention
    {
        return Err(StreamError::InconsistentState);
    }
    Ok(())
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

fn validate_readiness(readiness: &Readiness, snapshot: &Snapshot) -> Result<(), StreamError> {
    if (!snapshot.has_control
        && (readiness.admission || !readiness.resume.is_empty() || !readiness.cancel.is_empty()))
        || readiness.resume.len() > 2
        || readiness.cancel.len() > 2
        || (readiness.admission
            && (snapshot
                .intentions
                .iter()
                .any(|status| status.phase.pending())
                || snapshot
                    .travel
                    .as_ref()
                    .is_some_and(|travel| travel.phase == TravelPhase::Active)
                || snapshot
                    .state
                    .observation
                    .combat
                    .as_ref()
                    .is_some_and(|combat| combat.dead || combat.terminal)))
    {
        return Err(StreamError::InconsistentState);
    }
    for (ids, resume) in [(&readiness.resume, true), (&readiness.cancel, false)] {
        for (index, id) in ids.iter().enumerate() {
            if ids[..index].contains(id)
                || !snapshot.intentions.iter().any(|status| {
                    &status.intention == id
                        && if resume {
                            matches!(
                                status.phase,
                                IntentionPhase::Suspended | IntentionPhase::Paused
                            )
                        } else {
                            status.phase.active()
                        }
                })
            {
                return Err(StreamError::InconsistentState);
            }
        }
    }
    Ok(())
}
