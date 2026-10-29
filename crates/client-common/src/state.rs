use crate::{ObservationStream, StreamError};
use serde::Serialize;
use std::sync::Arc;
use tor_protocol::*;

/// Last disclosed appearance. Hydrated records have no invented sighting time,
/// entity handles, occupants, reachability or supply affordances.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RememberedCell {
    #[serde(flatten)]
    pub appearance: RememberedMapCell,
    pub last_seen_tick: Option<u64>,
    pub last_seen_revision: Option<u64>,
}
impl std::ops::Deref for RememberedCell {
    type Target = RememberedMapCell;
    fn deref(&self) -> &Self::Target {
        &self.appearance
    }
}

/// Shared presentation state for all frontends. Older history pages can be
/// requested separately; this model retains at most the latest 100 entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientState {
    session: ClientSessionState,
    observation_base: ObservationBase,
    last_visual_change: u64,
    map_memory: crate::map_memory::MapMemory,
    map_hydration: crate::map_hydration::MapHydration,
    narration: Vec<String>,
    narration_lines: Vec<crate::narration::Line>,
}

/// Live client data is deliberately not a wire `Snapshot`: ordered authority
/// can advance while its immutable picture remains at an earlier boundary.
/// Clone this value for comparisons; only `ClientState::reset_snapshot` may
/// construct a reset envelope, after checking its synchronous invariants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientSessionState {
    metadata: SessionMetadata,
    stream: ObservationStream,
    authority: SessionAuthority,
    picture: Arc<StateView>,
    history: HistoryPage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionMetadata {
    context: StreamContext,
    actor: ActorId,
    branch: BranchId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionAuthority {
    readiness: Readiness,
    intentions: Vec<IntentionStatus>,
    travel: Option<TravelStatus>,
    has_control: bool,
    outcome_status: Option<OutcomeStatusFacts>,
    character_ready: bool,
}

impl ClientState {
    pub fn from_snapshot(snapshot: Snapshot) -> Result<Self, StreamError> {
        validate_reset_snapshot(&snapshot)?;
        let mut client = Self {
            last_visual_change: snapshot.cursor.sequence,
            observation_base: ObservationBase {
                snapshot: snapshot.state.snapshot.clone(),
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            },
            session: ClientSessionState {
                stream: ObservationStream::from_snapshot(snapshot.actor, snapshot.cursor),
                metadata: SessionMetadata {
                    context: snapshot.context,
                    actor: snapshot.actor,
                    branch: snapshot.branch,
                },
                authority: SessionAuthority {
                    character_ready: snapshot.state.observation.ready,
                    outcome_status: snapshot
                        .state
                        .observation
                        .combat
                        .as_ref()
                        .map(OutcomeStatusFacts::from),
                    readiness: snapshot.readiness,
                    intentions: snapshot.intentions,
                    travel: snapshot.travel,
                    has_control: snapshot.has_control,
                },
                picture: snapshot.state,
                history: snapshot.history,
            },
            map_memory: Default::default(),
            map_hydration: Default::default(),
            narration: Vec::new(),
            narration_lines: Vec::new(),
        };
        client.remember_view();
        Ok(client)
    }

    /// Bounded appearance records from received sightings and actor-owned hydration.
    /// Place names never manufacture views; branch changes clear this cache.
    pub fn memory(&self) -> impl Iterator<Item = &RememberedCell> {
        self.map_memory.records.values()
    }

    /// Disclosed sightings aligned to the current view, without remembered actors.
    /// Ambiguous/disconnected views start a new chart. All six viewport planes fit.
    pub fn map_memory(&self) -> impl Iterator<Item = crate::map_memory::MapCell<'_>> {
        self.map_memory.positioned()
    }

    /// Current disclosure wins over aligned stale map memory. This never
    /// manufactures knowledge from an unaligned historical sighting.
    pub fn map_cell(&self, position: Position) -> Option<RememberedMapCell> {
        let view = &self.state().observation;
        view.visible_cells
            .iter()
            .find(|c| c.position == position)
            .map(|cell| {
                let items: Vec<_> = view
                    .ground_items
                    .iter()
                    .filter(|i| i.position == position)
                    .collect();
                crate::map_memory::appearance(cell, &items)
            })
            .or_else(|| self.map_memory.at(position))
    }

    /// A validated snapshot establishes a new stream boundary atomically.
    pub fn replace_snapshot(&mut self, snapshot: Snapshot) -> Result<(), StreamError> {
        if snapshot.actor != self.session.metadata.actor {
            return Err(StreamError::WrongActor);
        }
        if snapshot.context.stream != self.session.metadata.context.stream
            || snapshot.context.epoch <= self.session.metadata.context.epoch
        {
            return Err(StreamError::WrongStreamContext);
        }
        let mut candidate = Self::from_snapshot(snapshot)?;
        if candidate.branch() == self.branch() {
            candidate.map_memory = std::mem::take(&mut self.map_memory);
            candidate.remember_view();
            candidate.map_memory.coverage = None;
        }
        *self = candidate;
        Ok(())
    }

    fn remember_view(&mut self) {
        let moved = self.map_memory.observe(
            &self.session.picture.observation,
            self.session.picture.revision,
        );
        let context = self.context().clone();
        let branch = self.branch().clone();
        self.map_hydration.observe(
            context,
            self.session.metadata.actor,
            branch,
            self.observation_base.clone(),
            moved,
        );
        if self.map_memory.limited {
            self.map_hydration.status.limited = true;
            self.map_hydration.status.complete = false;
        }
    }

    /// Hydration never changes ordinary live readiness or input authority.
    pub fn map_hydration(&self) -> &crate::MapHydrationStatus {
        &self.map_hydration.status
    }

    pub fn next_map_query(&mut self) -> Option<MapQuery> {
        let Some(window) = MapWindow::normal().checked_translate(self.state().observation.position)
        else {
            self.map_hydration.status.failed = true;
            self.map_hydration.status.complete = false;
            return None;
        };
        let missing = self.map_memory.next_uncovered(window);
        self.map_hydration
            .query(missing.unwrap_or(window), missing.is_some())
    }
    pub fn bind_map_query(
        &mut self,
        query: &MapQuery,
        request_id: &str,
    ) -> Result<(), &'static str> {
        self.map_hydration.bind(query, request_id)
    }
    pub fn map_query_pending(&self, request_id: &str, query: &MapQuery) -> bool {
        self.map_hydration.pending(request_id, query)
    }
    pub fn reject_map_query(&mut self, request_id: &str) -> bool {
        self.map_hydration.reject(request_id)
    }
    pub fn cancel_map_query(&mut self) {
        self.map_hydration.cancel();
    }
    pub fn accept_map_page(
        &mut self,
        context: &ReplyContext,
        request_id: &str,
        page: &MapPage,
        encoded_response_bytes: usize,
    ) -> Result<(), String> {
        self.validate_reply_context(context)
            .map_err(|e| format!("Invalid map reply context: {e:?}"))?;
        if let Some(update) =
            self.map_hydration
                .accept(context, request_id, page, encoded_response_bytes)?
        {
            let viewport = MapWindow::normal()
                .checked_translate(self.state().observation.position)
                .ok_or("Invalid map viewport")?;
            self.map_memory
                .install_region(viewport, update.window, update.cells)?;
            self.map_hydration.status.complete = !self.map_memory.needs_window(viewport);
            // Exact current disclosure overlays stale memory, including seen emptiness.
            self.map_memory.overlay(
                &self.session.picture.observation,
                self.session.picture.revision,
            );
            if self.map_memory.limited {
                self.map_hydration.status.limited = true;
                self.map_hydration.status.complete = false;
            }
        }
        Ok(())
    }
    /// For the native presentation thread, after transport has validated a complete transfer.
    pub fn map_memory_update(&self) -> Option<crate::MapMemoryUpdate> {
        MapWindow::normal().checked_translate(self.state().observation.position)?;
        Some(crate::MapMemoryUpdate {
            context: self.context().clone(),
            actor: self.session.metadata.actor,
            branch: self.branch().clone(),
            base: self.observation_base.clone(),
            coverage: self.map_memory.coverage,
            cells: self
                .map_memory()
                .map(|c| (c.position, c.appearance()))
                .collect(),
            status: self.map_hydration.status.clone(),
        })
    }
    pub fn apply_map_memory_update(
        &mut self,
        update: crate::MapMemoryUpdate,
    ) -> Result<(), StreamError> {
        if update.context != *self.context()
            || update.actor != self.session.metadata.actor
            || update.branch != *self.branch()
            || update.base != self.observation_base
        {
            return Err(StreamError::WrongObservationBase);
        }
        {
            self.map_memory
                .install_snapshot(update.coverage, update.cells);
            self.map_memory.overlay(
                &self.session.picture.observation,
                self.session.picture.revision,
            );
        }
        self.map_hydration.status = update.status;
        if self.map_memory.limited {
            self.map_hydration.status.limited = true;
            self.map_hydration.status.complete = false;
        }
        Ok(())
    }

    /// Live semantic state and its independently aged picture, never a reset.
    pub fn session(&self) -> &ClientSessionState {
        &self.session
    }

    pub fn actor(&self) -> ActorId {
        self.session.metadata.actor
    }

    /// Export a reset only when the picture and authority really share a
    /// synchronous boundary. Callers must request a fresh server snapshot when
    /// this fails, not stamp the current cursor onto an older picture.
    pub fn reset_snapshot(&self) -> Result<Snapshot, StreamError> {
        let picture_status = self
            .state()
            .observation
            .combat
            .as_ref()
            .map(OutcomeStatusFacts::from);
        if !self.observation_current()
            || self.outcome_status() != picture_status.as_ref()
            || self.character_ready() != self.state().observation.ready
        {
            return Err(StreamError::InconsistentState);
        }
        let snapshot = Snapshot {
            context: self.context().clone(),
            actor: self.actor(),
            branch: self.branch().clone(),
            cursor: self.cursor(),
            readiness: self.readiness().clone(),
            intentions: self.intentions().to_vec(),
            travel: self.travel().cloned(),
            has_control: self.has_control(),
            state: Arc::clone(&self.session.picture),
            history: self.session.history.clone(),
        };
        validate_reset_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub fn travel(&self) -> Option<&TravelStatus> {
        self.session.authority.travel.as_ref()
    }

    pub fn intentions(&self) -> &[IntentionStatus] {
        &self.session.authority.intentions
    }

    pub fn has_pending_intention(&self) -> bool {
        self.intentions()
            .iter()
            .any(|status| status.phase.pending())
    }

    /// Published permissions can briefly lag their causing lifecycle/control
    /// updates. Block input during that boundary instead of inferring availability.
    /// Whether the character can act now, independently of permission to
    /// enqueue future work. Ordered pacing facts advance this even while the
    /// displayed picture is stale; later pictures never rewind causal state.
    pub fn character_ready(&self) -> bool {
        self.session.authority.character_ready
    }

    pub fn can_admit_intention(&self) -> bool {
        self.readiness().admission
            && validate_readiness(
                self.readiness(),
                &self.session.authority.intentions,
                self.travel(),
                self.has_control(),
            )
            .is_ok()
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

    /// Atomic cancellation plus one ordinary step, only after a disclosed prefix.
    pub fn cancel_intention_with_step_request(&self, step: Direction) -> Option<Request> {
        if !self.can_cancel_sequence() {
            return None;
        }
        let intention = self.permitted_sequence_intention()?.intention.clone();
        Some(self.command_request(Command::CancelIntention {
            intention,
            step: Some(step),
        }))
    }

    pub fn can_cancel_sequence(&self) -> bool {
        self.permitted_sequence_intention().is_some()
    }

    fn permitted_sequence_intention(&self) -> Option<&IntentionStatus> {
        if validate_readiness(
            self.readiness(),
            &self.session.authority.intentions,
            self.travel(),
            self.has_control(),
        )
        .is_err()
            || !self
                .outcome_status()
                .and_then(|status| status.sequence.as_ref())
                .is_some_and(|sequence| sequence.completed > 0)
        {
            return None;
        }
        let preparation = self.intentions().iter().find(|status| {
            matches!(
                status.phase,
                IntentionPhase::Started | IntentionPhase::Paused
            )
        });
        // A queued resume replaces the phase on the same identity. Only an
        // unambiguous sole active identity may stand in for the paused phase.
        let selected = preparation.or_else(|| {
            let mut active = self
                .intentions()
                .iter()
                .filter(|status| status.phase.active());
            let selected = active.next()?;
            active.next().is_none().then_some(selected)
        })?;
        self.readiness()
            .cancel
            .contains(&selected.intention)
            .then_some(selected)
    }

    pub fn can_resume_intention(&self) -> bool {
        self.permitted_intention(true).is_some()
    }

    pub fn can_cancel_intention(&self) -> bool {
        self.permitted_intention(false).is_some()
    }

    fn permitted_intention(&self, resume: bool) -> Option<&IntentionStatus> {
        if validate_readiness(
            self.readiness(),
            &self.session.authority.intentions,
            self.travel(),
            self.has_control(),
        )
        .is_err()
        {
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
        let command = if resume {
            Command::ResumeIntention { intention }
        } else {
            Command::CancelIntention {
                intention,
                step: None,
            }
        };
        Some(self.command_request(command))
    }

    pub fn state(&self) -> &StateView {
        &self.session.picture
    }
    /// Latest ordered status, independent of the age of the displayed picture.
    pub fn outcome_status(&self) -> Option<&OutcomeStatusFacts> {
        self.session.authority.outcome_status.as_ref()
    }

    /// Latest outcome prose; visual publication clears it without replaying it.
    pub fn narration(&self) -> &[String] {
        &self.narration
    }
    /// [`Self::narration`], with what each line reports.
    pub fn narration_lines(&self) -> &[crate::narration::Line] {
        &self.narration_lines
    }
    pub fn history(&self) -> &[HistoryEntry] {
        &self.session.history.entries
    }
    pub fn older_before(&self) -> Option<&EntryId> {
        self.session.history.older_before.as_ref()
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
        ReplyContext {
            input: self.input_context(),
            actor: self.actor(),
            branch: self.branch().clone(),
            cursor: self.cursor(),
            snapshot: self.state().snapshot.clone(),
            revision: self.state().revision,
        }
    }

    /// Replies name the complete currently disclosed boundary. Validation never
    /// installs permissions or observations from a reply in place of stream updates.
    pub fn validate_reply_context(&self, context: &ReplyContext) -> Result<(), StreamError> {
        if &context.input.stream != self.context() {
            return Err(StreamError::WrongStreamContext);
        }
        if context.actor != self.session.metadata.actor {
            return Err(StreamError::WrongActor);
        }
        if &context.branch != self.branch() {
            return Err(StreamError::WrongBranch);
        }
        if context.cursor != self.cursor() {
            return Err(StreamError::SequenceMismatch);
        }
        if context.snapshot != self.state().snapshot
            || context.revision != self.state().revision
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
        &self.session.authority.readiness
    }

    pub fn has_control(&self) -> bool {
        self.session.authority.has_control
    }
    pub fn context(&self) -> &StreamContext {
        &self.session.metadata.context
    }

    pub fn branch(&self) -> &BranchId {
        &self.session.metadata.branch
    }
    /// Whether all disclosed picture changes have been materialized. Uses
    /// stream ordering, not ticks: multiple semantic actions may share a tick.
    pub fn observation_current(&self) -> bool {
        self.observation_base.cursor.sequence >= self.last_visual_change
    }

    pub fn observation_base(&self) -> ObservationBase {
        self.observation_base.clone()
    }

    pub fn cursor(&self) -> StreamCursor {
        self.session.stream.cursor()
    }

    pub fn apply(&mut self, update: StreamUpdate) -> Result<(), StreamError> {
        if update.context != self.session.metadata.context {
            return Err(StreamError::WrongStreamContext);
        }
        if update.branch != self.session.metadata.branch {
            return Err(StreamError::WrongBranch);
        }
        // Validate ordering separately. Each payload arm completes all fallible
        // checks (including history validation) before publishing any changes.
        let mut stream = self.session.stream.clone();
        stream.accept(update.actor, update.cursor)?;
        let body = match update.body {
            UpdateBody::ObservationDelta { base, state, event } => {
                if base != self.observation_base {
                    return Err(StreamError::WrongObservationBase);
                }
                UpdateBody::Observation {
                    state: Arc::new(
                        state
                            .apply(&self.session.picture)
                            .map_err(|_| StreamError::InconsistentState)?,
                    ),
                    event,
                }
            }
            body => body,
        };
        match body {
            UpdateBody::Outcome {
                facts,
                event,
                visual_changed,
            } => {
                // Ordered outcomes advance semantic time independently of the
                // last requested picture. Historical handles are not live input.
                if facts.self_target != self.state().observation.self_target {
                    return Err(StreamError::InconsistentState);
                }
                facts
                    .validate_at(update.cursor.tick)
                    .map_err(|_| StreamError::InconsistentState)?;
                if let Some(entry) = event {
                    if !matches!(
                        entry.content,
                        HistoryContent::Action { .. } | HistoryContent::PlaceRenamed { .. }
                    ) {
                        return Err(StreamError::InconsistentState);
                    }
                    self.remember(*entry, update.cursor.tick)?;
                }
                for fact in &facts.facts {
                    if let tor_protocol::OutcomeFact::Pacing { ready } = fact {
                        self.session.authority.character_ready = *ready;
                    }
                }
                // Terminal outcomes intentionally omit ordinary pacing prose.
                // Their semantic status still ends character readiness, without
                // inventing the separately ordered input-permission update.
                if facts
                    .status
                    .as_ref()
                    .is_some_and(|status| status.dead || status.terminal)
                {
                    self.session.authority.character_ready = false;
                }
                self.session.authority.outcome_status = facts.status.clone();
                if visual_changed {
                    self.last_visual_change = update.cursor.sequence;
                }
                self.narration_lines = crate::narration::outcome_lines(&facts);
                self.narration = self
                    .narration_lines
                    .iter()
                    .map(|line| line.text.clone())
                    .collect();
            }
            UpdateBody::Readiness { readiness } => {
                if update.cursor.tick != self.session.stream.cursor().tick
                    || self.session.authority.readiness.revision.checked_add(1)
                        != Some(readiness.revision)
                {
                    return Err(StreamError::InconsistentState);
                }
                validate_readiness(
                    &readiness,
                    &self.session.authority.intentions,
                    self.travel(),
                    self.has_control(),
                )?;
                if readiness.admission
                    && self
                        .outcome_status()
                        .is_some_and(|status| status.dead || status.terminal)
                {
                    return Err(StreamError::InconsistentState);
                }
                self.session.authority.readiness = readiness;
            }
            UpdateBody::Intention { status } => {
                if update.cursor.tick != self.session.stream.cursor().tick
                    || !status.valid_context(update.actor, &update.branch)
                {
                    return Err(StreamError::InconsistentState);
                }
                let previous = self
                    .session
                    .authority
                    .intentions
                    .iter()
                    .position(|old| old.intention == status.intention);
                if let Some(index) = previous {
                    let old = &self.session.authority.intentions[index];
                    if old.entry_id != status.entry_id || !status.phase.can_follow(old.phase) {
                        return Err(StreamError::InconsistentState);
                    }
                } else if status.phase != IntentionPhase::Queued {
                    return Err(StreamError::InconsistentState);
                }
                let mut intentions = self.session.authority.intentions.clone();
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
                self.session.authority.intentions = intentions;
            }
            UpdateBody::Travel { status, entry } => {
                if update.cursor.tick != self.session.stream.cursor().tick {
                    return Err(StreamError::InconsistentState);
                }
                if let Some(previous) = self
                    .session
                    .authority
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
                self.session.authority.travel = Some(status);
            }
            UpdateBody::Observation { state, event } => {
                state
                    .validate()
                    .map_err(|_| StreamError::InconsistentState)?;
                if state.observation.actor != update.actor
                    || state.observation.tick != update.cursor.tick
                    || state.revision <= self.session.picture.revision
                    || state.snapshot == self.session.picture.snapshot
                {
                    return Err(StreamError::InconsistentState);
                }
                if event.is_some() {
                    // Narration and history belong exclusively to ordered outcomes.
                    return Err(StreamError::InconsistentState);
                }
                self.narration_lines.clear();
                self.narration.clear();
                self.observation_base = ObservationBase {
                    snapshot: state.snapshot.clone(),
                    cursor: update.cursor,
                    revision: state.revision,
                };
                self.session.picture = state;
                self.remember_view();
            }
            UpdateBody::ObservationDelta { .. } => unreachable!("expanded above"),
            UpdateBody::Annotation { entry } => {
                if update.cursor.tick != self.session.stream.cursor().tick
                    || !matches!(entry.content, HistoryContent::Annotation { .. })
                {
                    return Err(StreamError::InconsistentState);
                }
                self.remember(*entry, update.cursor.tick)?;
            }
            UpdateBody::Control { has_control } => {
                if update.cursor.tick != self.session.stream.cursor().tick {
                    return Err(StreamError::InconsistentState);
                }
                self.session.authority.has_control = has_control;
            }
        }
        self.session.stream = stream;
        Ok(())
    }

    fn remember(&mut self, entry: HistoryEntry, tick: u64) -> Result<(), StreamError> {
        validate_entry(
            &entry,
            self.session.metadata.actor,
            &self.session.metadata.branch,
            tick,
        )?;
        if let Some(previous) = self
            .session
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
        self.session.history.entries.push(entry);
        if self.session.history.entries.len() > MAX_HISTORY_PAGE {
            self.session.history.entries.remove(0);
            self.session.history.older_before = self
                .session
                .history
                .entries
                .first()
                .map(|entry| entry.id.clone());
        }
        Ok(())
    }
}

fn validate_reset_snapshot(snapshot: &Snapshot) -> Result<(), StreamError> {
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
    validate_readiness(
        &snapshot.readiness,
        &snapshot.intentions,
        snapshot.travel.as_ref(),
        snapshot.has_control,
    )?;
    // A full reset's picture and readiness share one authoritative boundary.
    if snapshot.readiness.admission
        && snapshot
            .state
            .observation
            .combat
            .as_ref()
            .is_some_and(|combat| combat.dead || combat.terminal)
    {
        return Err(StreamError::InconsistentState);
    }
    Ok(())
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

fn validate_readiness(
    readiness: &Readiness,
    intentions: &[IntentionStatus],
    travel: Option<&TravelStatus>,
    has_control: bool,
) -> Result<(), StreamError> {
    // Readiness is input authority. A visual combat record may predate even a
    // same-tick semantic action, so it cannot veto current admission.
    if (!has_control
        && (readiness.admission || !readiness.resume.is_empty() || !readiness.cancel.is_empty()))
        || readiness.resume.len() > 2
        || readiness.cancel.len() > 2
        || (readiness.admission
            && (intentions.iter().any(|status| status.phase.pending())
                || travel.is_some_and(|travel| travel.phase == TravelPhase::Active)))
    {
        return Err(StreamError::InconsistentState);
    }
    for (ids, resume) in [(&readiness.resume, true), (&readiness.cancel, false)] {
        for (index, id) in ids.iter().enumerate() {
            if ids[..index].contains(id)
                || !intentions.iter().any(|status| {
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

#[cfg(test)]
mod outcome_stream_tests {
    use super::*;

    fn client() -> ClientState {
        // Independent positive fixture: stream contracts must be testable before
        // a newly versioned live recording is generated.
        let actor = ActorId(1);
        let observation = Observation {
            charts: vec![],
            self_appearance: None,
            interactions: None,
            combat: Some(CombatView {
                own_stats: None,
                hp: 10,
                max_hp: 10,
                preparation_remaining: None,
                preparation_active: false,
                sequence: None,
                public_plan: None,
                recovery_remaining: 0,
                actors: vec![],
                events: vec![],
                objective: None,
                exit: None,
                victory: false,
                dead: false,
                terminal: false,
            }),
            motion: None,
            places: vec![],
            actor,
            self_target: ActorTarget::from_digest([1; 32]),
            linked_holds: vec![],
            tick: 0,
            position: Position { x: 0, y: 0, z: 0 },
            visible_cells: vec![],
            ground_items: vec![],
            inventory: vec![],
            visible_actors: vec![],
            ready: true,
        };
        ClientState::from_snapshot(Snapshot {
            context: StreamContext {
                stream: StreamId("outcome-test-stream".into()),
                epoch: 0,
            },
            readiness: Readiness {
                revision: 0,
                admission: true,
                resume: vec![],
                cancel: vec![],
            },
            intentions: vec![],
            travel: None,
            actor,
            branch: BranchId("outcome-test-branch".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            state: Arc::new(StateView {
                snapshot: SnapshotId("outcome-test-initial".into()),
                wizard_game: false,
                revision: 1,
                observation,
            }),
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        })
        .unwrap()
    }

    fn update(client: &ClientState, tick: u64, body: UpdateBody) -> StreamUpdate {
        StreamUpdate {
            context: client.context().clone(),
            actor: client.actor(),
            branch: client.branch().clone(),
            cursor: StreamCursor {
                sequence: client.cursor().sequence + 1,
                tick,
            },
            body,
        }
    }

    fn outcome(client: &ClientState) -> UpdateBody {
        UpdateBody::Outcome {
            facts: OutcomeFacts {
                self_target: client.state().observation.self_target,
                actor_names: vec![],
                status: client.outcome_status().cloned(),
                facts: vec![OutcomeFact::OwnAction {
                    event: Event::Waited,
                    name: None,
                    underfoot: None,
                    basic_attack: false,
                }],
            },
            event: None,
            visual_changed: true,
        }
    }

    #[test]
    fn causal_character_readiness_is_not_queue_admission_or_an_older_picture() {
        let mut client = client();
        assert!(client.character_ready());
        assert!(client.can_admit_intention());
        let mut body = outcome(&client);
        let UpdateBody::Outcome { facts, .. } = &mut body else {
            unreachable!()
        };
        facts.facts.push(OutcomeFact::Pacing { ready: false });
        client.apply(update(&client, 1, body)).unwrap();
        assert!(!client.character_ready());
        assert!(
            client.can_admit_intention(),
            "explicit queued input remains permitted"
        );
        assert!(client.state().observation.ready, "picture can still be old");
        let mut picture = client.state().clone();
        picture.snapshot = SnapshotId("delayed-picture".into());
        picture.revision += 1;
        picture.observation.tick = 1;
        client
            .apply(update(
                &client,
                1,
                UpdateBody::Observation {
                    state: Arc::new(picture),
                    event: None,
                },
            ))
            .unwrap();
        assert!(
            !client.character_ready(),
            "picture cannot overwrite causal pacing"
        );
        let mut body = outcome(&client);
        let UpdateBody::Outcome { facts, .. } = &mut body else {
            unreachable!()
        };
        facts.facts.push(OutcomeFact::Pacing { ready: true });
        client.apply(update(&client, 2, body)).unwrap();
        assert!(client.character_ready());
    }

    #[test]
    fn terminal_status_without_pacing_ends_character_readiness_and_allows_current_reset() {
        for dead in [false, true] {
            let mut client = client();
            let mut picture = client.state().clone();
            picture.snapshot = SnapshotId("terminal-picture".into());
            picture.revision += 1;
            picture.observation.tick = 1;
            picture.observation.ready = false;
            let combat = picture.observation.combat.as_mut().unwrap();
            combat.terminal = true;
            combat.dead = dead;
            combat.victory = !dead;
            if dead {
                combat.hp = 0;
            }
            let mut body = outcome(&client);
            let UpdateBody::Outcome { facts, .. } = &mut body else {
                unreachable!()
            };
            facts.status = Some(OutcomeStatusFacts::from(&*combat));
            assert!(!facts
                .facts
                .iter()
                .any(|fact| matches!(fact, OutcomeFact::Pacing { .. })));
            client.apply(update(&client, 1, body)).unwrap();
            assert!(!client.character_ready());
            assert!(
                client.readiness().admission,
                "permissions retain their separate boundary"
            );
            client
                .apply(update(
                    &client,
                    1,
                    UpdateBody::Readiness {
                        readiness: Readiness {
                            revision: 1,
                            admission: false,
                            resume: vec![],
                            cancel: vec![],
                        },
                    },
                ))
                .unwrap();
            client
                .apply(update(
                    &client,
                    1,
                    UpdateBody::Observation {
                        state: Arc::new(picture),
                        event: None,
                    },
                ))
                .unwrap();
            assert!(client.reset_snapshot().is_ok());
        }
    }

    #[test]
    fn outcomes_advance_authority_without_a_picture_and_pictures_do_not_narrate() {
        let mut client = client();
        let visual = client.state().clone();
        let base = client.observation_base();
        let tick = client.cursor().tick + 1;
        let mut body = outcome(&client);
        let UpdateBody::Outcome { facts, .. } = &mut body else {
            unreachable!()
        };
        let status = facts.status.as_mut().expect("combat fixture");
        status.max_hp += 1;
        let expected_status = facts.status.clone();
        let first = update(&client, tick, body);
        client.apply(first.clone()).unwrap();
        assert_eq!(client.outcome_status(), expected_status.as_ref());
        assert_eq!(client.state(), &visual);
        assert_eq!(client.observation_base(), base);
        assert!(!client.observation_current());
        assert_eq!(client.cursor().tick, tick);
        assert!(!client.narration_lines().is_empty());
        let stable = client.clone();
        assert_eq!(client.apply(first), Err(StreamError::SequenceMismatch));
        assert_eq!(client, stable);
        // Control/readiness belong to semantic time, not the old picture's tick.
        client
            .apply(update(
                &client,
                tick,
                UpdateBody::Control {
                    has_control: client.has_control(),
                },
            ))
            .unwrap();
        let mut readiness = client.readiness().clone();
        readiness.revision += 1;
        client
            .apply(update(&client, tick, UpdateBody::Readiness { readiness }))
            .unwrap();
        let mut state = client.state().clone();
        state.snapshot = SnapshotId("outcome-test-next".into());
        state.revision += 1;
        state.observation.tick = tick;
        client
            .apply(update(
                &client,
                tick,
                UpdateBody::Observation {
                    state: Arc::new(state),
                    event: None,
                },
            ))
            .unwrap();
        assert!(client.narration_lines().is_empty());
        assert!(client.narration().is_empty());
        assert_eq!(client.state().observation.tick, tick);
        assert_ne!(client.observation_base(), base);
        assert!(client.observation_current());
        assert_eq!(
            client.outcome_status(),
            expected_status.as_ref(),
            "a visual publication cannot roll back ordered status"
        );
    }

    #[test]
    fn live_session_cannot_be_exported_as_a_stale_reset() {
        let mut client = client();
        let reset = client.reset_snapshot().unwrap();
        assert_eq!(
            ClientState::from_snapshot(reset).unwrap().session(),
            client.session()
        );
        let picture = client.state().clone();
        client
            .apply(update(&client, client.cursor().tick + 1, outcome(&client)))
            .unwrap();
        assert_eq!(client.state(), &picture);
        assert_eq!(client.reset_snapshot(), Err(StreamError::InconsistentState));
        assert_eq!(client.reply_context().cursor, client.cursor());
        assert_eq!(client.reply_context().snapshot, picture.snapshot);
        let mut current_picture = picture;
        current_picture.snapshot = SnapshotId("reset-export-current".into());
        current_picture.revision += 1;
        current_picture.observation.tick = client.cursor().tick;
        client
            .apply(update(
                &client,
                client.cursor().tick,
                UpdateBody::Observation {
                    state: Arc::new(current_picture),
                    event: None,
                },
            ))
            .unwrap();
        assert_eq!(
            ClientState::from_snapshot(client.reset_snapshot().unwrap())
                .unwrap()
                .session(),
            client.session()
        );
    }

    #[test]
    fn sequence_cancellation_uses_current_outcome_not_stale_picture() {
        let mut reset = client().reset_snapshot().unwrap();
        let intention = IntentionId("sequence-owner".into());
        reset.intentions.push(IntentionStatus {
            actor: reset.actor,
            branch: reset.branch.clone(),
            intention: intention.clone(),
            entry_id: EntryId("sequence-entry".into()),
            phase: IntentionPhase::Started,
        });
        reset.readiness.admission = false;
        reset.readiness.cancel.push(intention);
        let mut client = ClientState::from_snapshot(reset).unwrap();
        assert!(!client.can_cancel_sequence());
        let mut body = outcome(&client);
        let UpdateBody::Outcome { facts, .. } = &mut body else {
            unreachable!()
        };
        let status = facts.status.as_mut().unwrap();
        status.preparation_active = true;
        status.preparation_remaining = Some(5);
        status.sequence = Some(SequenceView {
            completed: 1,
            total: 3,
            next_strike_remaining: 5,
            cancel_recovery_remaining: 35,
        });
        facts.validate_at(client.cursor().tick + 1).unwrap();
        client
            .apply(update(&client, client.cursor().tick + 1, body))
            .unwrap();
        assert!(client
            .state()
            .observation
            .combat
            .as_ref()
            .unwrap()
            .sequence
            .is_none());
        assert!(client.can_cancel_sequence());
        assert!(client
            .cancel_intention_with_step_request(Direction::East)
            .is_some());
    }

    #[test]
    fn foreign_outcome_self_target_is_rejected_without_partial_publication() {
        let mut client = client();
        let before = client.clone();
        let mut body = outcome(&client);
        let UpdateBody::Outcome { facts, event, .. } = &mut body else {
            unreachable!()
        };
        facts.self_target = ActorTarget::from_digest([2; 32]);
        facts.validate_at(client.cursor().tick).unwrap();
        *event = Some(Box::new(HistoryEntry {
            id: EntryId("wrong-self-event".into()),
            branch: client.branch().clone(),
            actor: client.actor(),
            tick: client.cursor().tick,
            author: Author::Backend {
                component: "test".into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::Action {
                action: Action::Wait,
                event: Box::new(Event::Waited),
            },
        }));
        assert_eq!(
            client.apply(update(&client, client.cursor().tick, body)),
            Err(StreamError::InconsistentState)
        );
        assert_eq!(client, before);
    }

    #[test]
    fn future_control_and_invalid_outcomes_do_not_advance_the_stream() {
        let mut client = client();
        let before = client.clone();
        let tick = client.cursor().tick + 1;
        assert_eq!(
            client.apply(update(
                &client,
                tick,
                UpdateBody::Control {
                    has_control: client.has_control()
                }
            )),
            Err(StreamError::InconsistentState)
        );
        assert_eq!(client, before);
        let mut body = outcome(&client);
        let UpdateBody::Outcome { facts, .. } = &mut body else {
            unreachable!()
        };
        facts.actor_names.push(DisclosedActorName {
            actor: facts.self_target,
            name: "invalid self alias".into(),
            asset: None,
            current_disclosed_appearance: None,
        });
        assert_eq!(
            client.apply(update(&client, tick, body)),
            Err(StreamError::InconsistentState)
        );
        assert_eq!(client, before);
    }

    #[test]
    fn visual_history_is_rejected_without_partial_publication() {
        let mut client = client();
        let before = client.clone();
        let tick = client.cursor().tick;
        let entry = HistoryEntry {
            id: EntryId("outcome-only".into()),
            branch: client.branch().clone(),
            actor: client.actor(),
            tick,
            author: Author::Backend {
                component: "test".into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::Action {
                action: Action::Wait,
                event: Box::new(Event::Waited),
            },
        };
        let mut state = client.state().clone();
        state.snapshot = SnapshotId("forbidden-visual-event".into());
        state.revision += 1;
        assert_eq!(
            client.apply(update(
                &client,
                tick,
                UpdateBody::Observation {
                    state: Arc::new(state),
                    event: Some(Box::new(entry.clone()))
                }
            )),
            Err(StreamError::InconsistentState)
        );
        assert_eq!(client, before);
        let mut body = outcome(&client);
        let UpdateBody::Outcome { event, .. } = &mut body else {
            unreachable!()
        };
        *event = Some(Box::new(entry));
        client.apply(update(&client, tick, body)).unwrap();
        assert_eq!(client.history().len(), 1);
        assert!(!client.narration_lines().is_empty());
        assert_eq!(client.state(), before.state());
    }
}
