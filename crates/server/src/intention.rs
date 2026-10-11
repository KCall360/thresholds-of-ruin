//! Durable execution of work selected by the simulation-owned scheduler.
use super::*;

pub(super) fn admitted_work_matches(work: tor_simulation::IntentionWork, action: &Action) -> bool {
    crate::journal::AdmittedWork::Human(action).matches(work)
}

/// Private lifecycle work can change disclosed preparation/readiness without a turn.
/// Use the same comparison during execution and recovery, retaining the new views.
pub(super) fn preparation_revision_updates(
    before: &Game,
    after: &Game,
    actor: ActorId,
    revisions: &mut Revisions,
) -> Result<BTreeMap<ActorId, Arc<RevisionView>>, Failure> {
    if before.preparation(SimActor(actor.0)) == after.preparation(SimActor(actor.0)) {
        return Ok(BTreeMap::new());
    }
    let mut observations = BTreeMap::new();
    for (&observer, revision) in revisions.iter_mut() {
        let old = revision_view(before, observer)?;
        let new = revision_view(after, observer)?;
        if new != old {
            *revision = revision
                .checked_add(1)
                .ok_or_else(|| Failure::new(ErrorCode::InvalidAction, "Revision exhausted"))?;
        }
        observations.insert(observer, Arc::new(new));
    }
    Ok(observations)
}
use crate::journal::{IntentionChange, IntentionEnd, IntentionEndKind};

pub(super) fn apply_intention_change(
    game: &mut Game,
    actor: ActorId,
    intention: tor_simulation::IntentionId,
    change: IntentionChange,
) -> Result<(), Failure> {
    match change {
        IntentionChange::Suspended => game.suspend_intention(SimActor(actor.0), intention),
        IntentionChange::Resumed => game.resume_intention(SimActor(actor.0), intention),
        IntentionChange::Cancelled => game
            .cancel_intention(SimActor(actor.0), intention)
            .map(|_| ()),
    }
    .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Intention is unavailable"))
}

pub(super) fn derive_intention_suspensions(
    before: &Game,
    after: &Game,
    entry: &JournalEntry,
    same_branch: bool,
) -> Vec<crate::journal::IntentionSuspension> {
    if !same_branch {
        return Vec::new();
    }
    let mut active: BTreeMap<_, _> = before
        .loaded_actor_ids()
        .filter_map(|actor| {
            let preparation = before.preparation(actor)?;
            preparation
                .active
                .then_some((preparation.intention?, actor))
        })
        .collect();
    // A successful start or continuation becomes active within this boundary.
    // Another scheduled effect can interrupt it before the boundary publishes.
    if let JournalContent::IntentionStarted { intention, .. }
    | JournalContent::IntentionContinued { intention, .. } = entry.content
    {
        active.insert(intention, SimActor(entry.actor.0));
    }
    active
        .into_iter()
        .filter_map(|(intention, actor)| {
            let current = after.preparation(actor)?;
            if current.active
                || current.intention != Some(intention)
                || matches!(entry.content, JournalContent::IntentionChanged {
                    intention: changed, change: IntentionChange::Suspended, ..
                } if changed == intention)
            {
                return None;
            }
            Some(crate::journal::IntentionSuspension {
                actor: ActorId(actor.0),
                intention,
            })
        })
        .collect()
}

pub(super) fn derive_intention_ends(
    before: &Game,
    after: &Game,
    entry: &JournalEntry,
    same_branch: bool,
) -> Vec<IntentionEnd> {
    // Rewind restores another branch's progress; it does not conclude that
    // branch's unfinished work in the restored timeline.
    if !same_branch {
        return Vec::new();
    }
    let mut active: BTreeMap<_, _> = before
        .loaded_actor_ids()
        .filter_map(|actor| Some((before.preparation(actor)?.intention?, actor)))
        .collect();
    let current = match &entry.content {
        JournalContent::IntentionStarted { intention, .. }
        | JournalContent::IntentionFailed { intention, .. }
        | JournalContent::IntentionContinued { intention, .. }
        | JournalContent::IntentionContinuationFailed { intention, .. } => {
            active.insert(*intention, SimActor(entry.actor.0));
            Some(*intention)
        }
        JournalContent::IntentionChanged {
            intention,
            change: IntentionChange::Cancelled,
            ..
        } => {
            active.insert(*intention, SimActor(entry.actor.0));
            Some(*intention)
        }
        _ => None,
    };
    let loaded: std::collections::BTreeSet<_> = after.loaded_actor_ids().collect();
    active.into_iter().filter_map(|(intention, actor)| {
            // Ordinary queued work has no same-identity preparation in a detached
            // record. Its explicit cancellation is terminal even while unloaded.
            let cancelled_queue = matches!(&entry.content, JournalContent::IntentionChanged {
                intention: id, change: IntentionChange::Cancelled, .. } if *id == intention)
                && before.pending_intention(actor).is_some_and(|queued| {
                    queued.id == intention && !matches!(queued.work, tor_simulation::IntentionWork::ResumePreparation { .. })
                }) && after.pending_intention(actor).is_none();
            if after.preparation(actor).is_some_and(|p| p.intention == Some(intention))
                || (!cancelled_queue && !loaded.contains(&actor) && after.known_actor_region(actor).is_some()) {
                return None;
            }
            let resolved = after.combat_events().iter().any(|event| matches!(event,
                tor_simulation::combat::CombatEvent::Resolved { actor: owner, intention: Some(id), .. }
                | tor_simulation::combat::CombatEvent::AbilityResolved { actor: owner, intention: Some(id), .. }
                | tor_simulation::combat::CombatEvent::ItemCompleted { actor: owner, intention: Some(id), .. }
                    if *owner == actor && *id == intention));
            let immediate = current == Some(intention) && matches!(&entry.content,
                JournalContent::IntentionStarted { action, .. } if !action.is_prepared());
            let replaced = current != Some(intention) && entry.actor.0 == actor.0
                && matches!(&entry.content, JournalContent::Action { action, .. }
                    | JournalContent::IntentionStarted { action, .. } if !matches!(action, Action::Wait));
            Some(IntentionEnd { actor: ActorId(actor.0), intention, kind: if resolved || immediate {
                IntentionEndKind::Resolved
            } else if replaced || matches!(&entry.content, JournalContent::IntentionChanged {
                intention: id, change: IntentionChange::Cancelled, .. } if *id == intention) {
                IntentionEndKind::Cancelled
            } else { IntentionEndKind::Failed } })
        }).collect()
}

/// One linkage rule serves live indexed admission and chronological recovery.
pub(super) fn valid_travel_link(
    root: &JournalEntry,
    previous: Option<(&JournalEntry, &JournalEntry)>,
    actor: ActorId,
    branch: &BranchId,
    ordinal: u64,
) -> bool {
    if root.actor != actor
        || &root.branch != branch
        || !matches!(root.content, JournalContent::Travel { .. })
    {
        return false;
    }
    let Some((admitted, resolved)) = previous else {
        return ordinal == 1;
    };
    let JournalContent::TravelIntentionAdmitted {
        intention,
        journey,
        step,
        ..
    } = &admitted.content
    else {
        return false;
    };
    journey == &root.id
        && admitted.actor == actor
        && &admitted.branch == branch
        && step.checked_add(1) == Some(ordinal)
        && resolved.actor == actor
        && &resolved.branch == branch
        && matches!(&resolved.content, JournalContent::IntentionStarted { admission, intention: id, .. }
            if admission == &admitted.id && id == intention)
        && resolved.intention_ends.iter().any(|end| {
            end.actor == actor
                && end.intention == *intention
                && end.kind == IntentionEndKind::Resolved
        })
}

impl Engine {
    /// A convenience-driver retry resumes the admitted decision, without a new ID.
    pub(super) fn ensure_ai_admission(
        &mut self,
        actor: ActorId,
        profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        if !self.is_ai(actor)
            || self.next_actor() != Some(actor)
            || self
                .next_intention_actor()
                .is_some_and(|selected| selected != actor)
        {
            return Err(Failure::new(
                ErrorCode::InvalidAction,
                "Scenario AI is not ready",
            ));
        }
        match self.game.pending_intention(SimActor(actor.0)) {
            Some(queued)
                if queued.origin == tor_simulation::IntentionOrigin::Autonomous
                    && queued.work == tor_simulation::IntentionWork::AiDecision
                    && queued.state == tor_simulation::IntentionState::Queued =>
            {
                Ok(())
            }
            Some(_) => Err(Failure::new(
                ErrorCode::InvalidAction,
                "Scenario AI queue is unavailable",
            )),
            None => self.admit_ai_inner(actor, None, profile).map(|_| ()),
        }
    }

    /// Admission queues a decision without observation, route search or RNG work.
    pub(crate) fn admit_ai(&mut self, actor: ActorId) -> Result<CommandResult, Failure> {
        self.admit_ai_inner(actor, None, None)
    }

    pub(super) fn admit_ai_inner(
        &mut self,
        actor: ActorId,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        if !self.is_ai(actor) || self.next_actor() != Some(actor) {
            return Err(Failure::new(
                ErrorCode::InvalidAction,
                "Scenario AI is not ready",
            ));
        }
        let mut candidate = self.capture_command_candidate(profile.as_deref_mut());
        let intention = candidate
            .game
            .admit_ai_intention(SimActor(actor.0))
            .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Scenario AI has no queue slot"))?;
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: candidate.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: "scheduler".into(),
            },
            audience: Audience::Private,
            content: JournalContent::AutonomousIntentionAdmitted { intention },
        };
        self.commit_candidate(candidate, entry, None, profile)
    }

    pub(crate) fn admit_travel(
        &mut self,
        actor: ActorId,
        journey: &EntryId,
        ordinal: u64,
        step: tor_simulation::TravelStep,
    ) -> Result<CommandResult, Failure> {
        self.admit_travel_inner(actor, journey, ordinal, step, None)
    }

    pub(super) fn admit_travel_inner(
        &mut self,
        actor: ActorId,
        journey: &EntryId,
        ordinal: u64,
        step: tor_simulation::TravelStep,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
        let unavailable = || Failure::new(ErrorCode::InvalidAction, "Journey step is unavailable");
        let source = self.history_index.find(journey).ok_or_else(unavailable)?;
        let root = &self.archive.records[source].entry;
        let previous = match self.history_index.latest_travel_step(journey) {
            None => None,
            Some(position) => {
                let admitted = &self.archive.records[position].entry;
                let (id, _) = admitted.content.admission().ok_or_else(unavailable)?;
                let resolved = self
                    .history_index
                    .intention_resolution(self.branch(), id)
                    .ok_or_else(unavailable)?;
                Some((admitted, &self.archive.records[resolved].entry))
            }
        };
        if !valid_travel_link(root, previous, actor, self.branch(), ordinal)
            || self.next_actor() != Some(actor)
        {
            return Err(unavailable());
        }
        let mut candidate = self.capture_command_candidate(None);
        let intention = candidate
            .game
            .admit_travel_intention(SimActor(actor.0), step)
            .map_err(|_| unavailable())?;
        let action = adapt::recorded_action(tor_simulation::Action::Move(step.direction))
            .ok_or_else(unavailable)?;
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: candidate.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: "scheduler".into(),
            },
            audience: Audience::Private,
            content: JournalContent::TravelIntentionAdmitted {
                intention,
                journey: journey.clone(),
                step: ordinal,
                action,
                destination: step.destination,
            },
        };
        self.commit_candidate(candidate, entry, None, None)
    }

    /// Settle a pending journey step without permitting manual human resumption.
    pub(crate) fn cancel_travel(
        &mut self,
        actor: ActorId,
    ) -> Result<Option<CommandResult>, Failure> {
        self.cancel_travel_inner(actor, None)
    }

    pub(super) fn cancel_travel_inner(
        &mut self,
        actor: ActorId,
        recorded_id: Option<EntryId>,
    ) -> Result<Option<CommandResult>, Failure> {
        let Some(queued) = self.game.pending_intention(SimActor(actor.0)) else {
            return Ok(None);
        };
        if queued.origin != tor_simulation::IntentionOrigin::Travel {
            return Ok(None);
        }
        let intention = queued.id;
        let source = self
            .history_index
            .intention_admission(intention)
            .ok_or_else(invalid_archive)?;
        let root = &self.archive.records[source].entry;
        if root.actor != actor
            || !root
                .content
                .admission()
                .is_some_and(|(_, work)| work.matches_intention(queued))
        {
            return Err(invalid_archive());
        }
        let admission = root.id.clone();
        let mut candidate = self.capture_command_candidate(None);
        apply_intention_change(
            &mut candidate.game,
            actor,
            intention,
            IntentionChange::Cancelled,
        )?;
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: candidate.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: "scheduler".into(),
            },
            audience: Audience::Actor,
            content: JournalContent::IntentionChanged {
                admission,
                intention,
                change: IntentionChange::Cancelled,
            },
        };
        self.commit_candidate(candidate, entry, None, None)
            .map(Some)
    }

    pub(crate) fn queued_travel_admission(&self, actor: ActorId) -> Option<&EntryId> {
        let queued = self.game.pending_intention(SimActor(actor.0))?;
        if queued.origin != tor_simulation::IntentionOrigin::Travel {
            return None;
        }
        let source = self.history_index.intention_admission(queued.id)?;
        Some(&self.archive.records[source].entry.id)
    }

    pub(crate) fn next_intention_origin(&self) -> Option<tor_simulation::IntentionOrigin> {
        let actor = self.game.next_intention_actor()?;
        Some(self.game.pending_intention(actor)?.origin)
    }
    pub(crate) fn queued_human_actors(&self) -> Vec<ActorId> {
        self.queued_actors(tor_simulation::IntentionOrigin::Human)
    }
    pub(crate) fn queued_travel_actors(&self) -> Vec<ActorId> {
        self.queued_actors(tor_simulation::IntentionOrigin::Travel)
    }
    fn queued_actors(&self, origin: tor_simulation::IntentionOrigin) -> Vec<ActorId> {
        self.game
            .queued_intentions()
            .filter(|queued| queued.origin == origin)
            .map(|queued| ActorId(queued.actor.0))
            .collect()
    }

    pub(crate) fn suspend_queued_intention(
        &mut self,
        actor: ActorId,
    ) -> Result<Option<CommandResult>, Failure> {
        self.suspend_queued_intention_inner(actor, None)
    }

    pub(super) fn suspend_queued_intention_inner(
        &mut self,
        actor: ActorId,
        recorded_id: Option<EntryId>,
    ) -> Result<Option<CommandResult>, Failure> {
        let Some(queued) = self.game.pending_intention(SimActor(actor.0)) else {
            return Ok(None);
        };
        if queued.origin != tor_simulation::IntentionOrigin::Human
            || queued.state == tor_simulation::IntentionState::Suspended
        {
            return Ok(None);
        }
        let source = self
            .history_index
            .intention_admission(queued.id)
            .ok_or_else(invalid_archive)?;
        let admission = self.archive.records[source].entry.id.clone();
        self.suspend_intention_admission(actor, &admission, recorded_id)
            .map(Some)
    }

    pub(super) fn suspend_intention_admission(
        &mut self,
        actor: ActorId,
        admission: &EntryId,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
        let mut candidate = self.capture_command_candidate(None);
        let content = self.prepare_intention_change(
            &mut candidate,
            actor,
            admission,
            IntentionChange::Suspended,
        )?;
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: candidate.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: "scheduler".into(),
            },
            audience: Audience::Actor,
            content,
        };
        self.commit_candidate(candidate, entry, None, None)
    }

    pub(super) fn prepare_intention_change(
        &self,
        candidate: &mut Candidate,
        actor: ActorId,
        admission: &EntryId,
        change: IntentionChange,
    ) -> Result<JournalContent, Failure> {
        let unavailable = || Failure::new(ErrorCode::InvalidAction, "Intention is unavailable");
        let source = self.history_index.find(admission).ok_or_else(unavailable)?;
        let admitted = &self.archive.records[source].entry;
        let JournalContent::IntentionAdmitted { intention, action } = &admitted.content else {
            return Err(unavailable());
        };
        let original = adapt::action(action);
        let queued_matches = candidate
            .game
            .pending_intention(SimActor(actor.0))
            .is_some_and(|queued| {
                queued.id == *intention
                    && queued.origin == tor_simulation::IntentionOrigin::Human
                    && admitted_work_matches(queued.work, action)
            });
        let preparation_matches = candidate
            .game
            .preparation(SimActor(actor.0))
            .is_some_and(|p| {
                p.intention == Some(*intention)
                    && p.work.action() == original
                    && !candidate.game.is_ai(SimActor(actor.0))
            });
        if admitted.actor != actor || !(queued_matches || preparation_matches) {
            return Err(unavailable());
        }
        apply_intention_change(&mut candidate.game, actor, *intention, change)?;
        let observations = preparation_revision_updates(
            &self.game,
            &candidate.game,
            actor,
            &mut candidate.revisions,
        )?;
        candidate.observations.extend(observations);
        Ok(JournalContent::IntentionChanged {
            admission: admission.clone(),
            intention: *intention,
            change,
        })
    }
    fn status_for_intention(
        &self,
        id: tor_simulation::IntentionId,
        branch: &BranchId,
        phase: IntentionPhase,
    ) -> Option<IntentionStatus> {
        let entry = &self.archive.records[self.history_index.intention_admission(id)?].entry;
        if !matches!(entry.content, JournalContent::IntentionAdmitted { .. }) {
            return None;
        }
        Some(IntentionStatus {
            actor: entry.actor,
            branch: branch.clone(),
            intention: IntentionId(entry.id.0.clone()),
            entry_id: entry.id.clone(),
            phase,
        })
    }

    /// Derived actor permissions contain only journal-filtered opaque identities;
    /// Session applies controller/role policy and assigns the transport revision.
    pub(crate) fn input_readiness(&self, actor: ActorId) -> tor_protocol::Readiness {
        let input = self.game.intention_input(SimActor(actor.0));
        let terminal = self.game.run_outcome().terminal;
        let mut readiness = tor_protocol::Readiness {
            revision: 0,
            admission: input.slot_available && self.alive(actor) && !self.is_ai(actor) && !terminal,
            resume: Vec::new(),
            cancel: Vec::new(),
        };
        for control in input.controls() {
            let Some(status) =
                self.status_for_intention(control.intention, self.branch(), IntentionPhase::Queued)
            else {
                continue;
            };
            if control.can_resume && !terminal {
                readiness.resume.push(status.intention.clone());
            }
            if control.can_cancel {
                readiness.cancel.push(status.intention);
            }
        }
        readiness
    }

    pub(crate) fn pending_intentions(&self, actor: ActorId) -> Vec<IntentionStatus> {
        self.game
            .intention_input(SimActor(actor.0))
            .controls()
            .filter_map(|control| {
                let phase = match control.state {
                    tor_simulation::IntentionControlState::Queued => IntentionPhase::Queued,
                    tor_simulation::IntentionControlState::Suspended => IntentionPhase::Suspended,
                    tor_simulation::IntentionControlState::Started => IntentionPhase::Started,
                    tor_simulation::IntentionControlState::Paused => IntentionPhase::Paused,
                };
                self.status_for_intention(control.intention, self.branch(), phase)
            })
            .collect()
    }

    fn end_phase(kind: IntentionEndKind) -> IntentionPhase {
        match kind {
            IntentionEndKind::Resolved => IntentionPhase::Resolved,
            IntentionEndKind::Failed => IntentionPhase::Failed,
            IntentionEndKind::Cancelled => IntentionPhase::Cancelled,
        }
    }

    pub(crate) fn intention_updates(&self, result: &CommandResult) -> Vec<IntentionStatus> {
        let mut updates: Vec<_> = result
            .entry
            .intention_ends
            .iter()
            .filter_map(|end| {
                self.status_for_intention(
                    end.intention,
                    &result.entry.branch,
                    Self::end_phase(end.kind),
                )
            })
            .collect();
        updates.extend(
            result
                .entry
                .intention_suspensions
                .iter()
                .filter_map(|suspension| {
                    self.status_for_intention(
                        suspension.intention,
                        &result.entry.branch,
                        IntentionPhase::Paused,
                    )
                }),
        );
        if let Some(status) = self.intention_status(result) {
            if !updates
                .iter()
                .any(|update| update.intention == status.intention)
            {
                updates.push(status);
            }
        }
        updates
    }

    pub(crate) fn intention_status(&self, result: &CommandResult) -> Option<IntentionStatus> {
        if let JournalContent::IntentionStarted { admission, .. }
        | JournalContent::IntentionFailed { admission, .. }
        | JournalContent::IntentionContinued { admission, .. }
        | JournalContent::IntentionContinuationFailed { admission, .. } = &result.entry.content
        {
            let root = &self.archive.records[self.history_index.find(admission)?].entry;
            if !matches!(root.content, JournalContent::IntentionAdmitted { .. }) {
                return None;
            }
        }
        match &result.entry.content {
            JournalContent::IntentionChanged {
                intention, change, ..
            } => self.status_for_intention(
                *intention,
                &result.entry.branch,
                match change {
                    IntentionChange::Suspended => {
                        if self
                            .game
                            .pending_intention(SimActor(result.entry.actor.0))
                            .is_some_and(|queued| queued.id == *intention)
                        {
                            IntentionPhase::Suspended
                        } else {
                            IntentionPhase::Paused
                        }
                    }
                    IntentionChange::Resumed => IntentionPhase::Queued,
                    IntentionChange::Cancelled => IntentionPhase::Cancelled,
                },
            ),
            JournalContent::IntentionAdmitted { .. } => {
                let RequestReceipt::Admitted {
                    actor,
                    branch,
                    intention,
                    entry_id,
                    phase,
                } = self.request_receipt(result)
                else {
                    unreachable!("admission receipt");
                };
                Some(IntentionStatus {
                    actor,
                    branch,
                    intention,
                    entry_id,
                    phase,
                })
            }
            JournalContent::IntentionStarted {
                admission,
                intention,
                event,
                ..
            }
            | JournalContent::IntentionContinued {
                admission,
                intention,
                event,
                ..
            } => Some(IntentionStatus {
                actor: result.entry.actor,
                branch: result.entry.branch.clone(),
                intention: IntentionId(admission.0.clone()),
                entry_id: admission.clone(),
                phase: if let Some(end) = result
                    .entry
                    .intention_ends
                    .iter()
                    .find(|end| end.intention == *intention)
                {
                    Self::end_phase(end.kind)
                } else if matches!(
                    event,
                    crate::journal::Event::AttackStarted { .. }
                        | crate::journal::Event::AbilityStarted { .. }
                        | crate::journal::Event::ItemStarted { .. }
                ) {
                    IntentionPhase::Started
                } else {
                    IntentionPhase::Resolved
                },
            }),
            JournalContent::IntentionFailed { admission, .. }
            | JournalContent::IntentionContinuationFailed { admission, .. } => {
                Some(IntentionStatus {
                    actor: result.entry.actor,
                    branch: result.entry.branch.clone(),
                    intention: IntentionId(admission.0.clone()),
                    entry_id: admission.clone(),
                    phase: IntentionPhase::Failed,
                })
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn has_pending_intention(&self, actor: ActorId) -> bool {
        self.game.pending_intention(SimActor(actor.0)).is_some()
    }

    pub(crate) fn next_intention_actor(&self) -> Option<ActorId> {
        self.game
            .next_intention_actor()
            .map(|actor| ActorId(actor.0))
    }

    pub(crate) fn request_receipt(&self, result: &CommandResult) -> RequestReceipt {
        let JournalContent::IntentionAdmitted { intention, .. } = result.entry.content else {
            return RequestReceipt::Immediate {
                actor: result.entry.actor,
                branch: result.entry.branch.clone(),
                entry_id: Some(result.entry.id.clone()),
            };
        };
        let phase = if let Some(index) = self
            .history_index
            .intention_end(&result.entry.branch, intention)
        {
            let end = self.archive.records[index]
                .entry
                .intention_ends
                .iter()
                .find(|end| end.intention == intention)
                .expect("derived conclusion index");
            Self::end_phase(end.kind)
        } else if let Some(phase) = (self.branch() == &result.entry.branch)
            .then(|| {
                self.pending_intentions(result.entry.actor)
                    .into_iter()
                    .find(|status| status.entry_id == result.entry.id)
                    .map(|status| status.phase)
            })
            .flatten()
        {
            phase
        } else {
            self.history_index
                .intention_resolution(&result.entry.branch, intention)
                .map(|index| match self.archive.records[index].entry.content {
                    JournalContent::IntentionStarted {
                        event:
                            crate::journal::Event::AttackStarted { .. }
                            | crate::journal::Event::AbilityStarted { .. }
                            | crate::journal::Event::ItemStarted { .. },
                        ..
                    }
                    | JournalContent::IntentionContinued {
                        event:
                            crate::journal::Event::AttackStarted { .. }
                            | crate::journal::Event::AbilityStarted { .. }
                            | crate::journal::Event::ItemStarted { .. },
                        ..
                    } => IntentionPhase::Started,
                    JournalContent::IntentionStarted { .. } => IntentionPhase::Resolved,
                    JournalContent::IntentionContinued { .. } => IntentionPhase::Resolved,
                    JournalContent::IntentionFailed { .. }
                    | JournalContent::IntentionContinuationFailed { .. } => IntentionPhase::Failed,
                    _ => unreachable!("resolution index contains lifecycle records"),
                })
                .unwrap_or_else(|| {
                    if self.branch() != &result.entry.branch {
                        IntentionPhase::Cancelled
                    } else if self
                        .game
                        .pending_intention(SimActor(result.entry.actor.0))
                        .is_some_and(|pending| {
                            pending.id == intention
                                && pending.state == tor_simulation::IntentionState::Suspended
                        })
                    {
                        IntentionPhase::Suspended
                    } else {
                        IntentionPhase::Queued
                    }
                })
        };
        RequestReceipt::Admitted {
            actor: result.entry.actor,
            branch: result.entry.branch.clone(),
            intention: IntentionId(result.entry.id.0.clone()),
            entry_id: result.entry.id.clone(),
            phase,
        }
    }

    pub fn execute_next_intention(&mut self) -> Result<Option<CommandResult>, Failure> {
        self.execute_intention(None)
    }

    pub(super) fn execute_intention(
        &mut self,
        recorded_id: Option<EntryId>,
    ) -> Result<Option<CommandResult>, Failure> {
        self.execute_intention_profiled(recorded_id, None)
    }

    pub(super) fn execute_intention_profiled(
        &mut self,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<Option<CommandResult>, Failure> {
        let mut candidate = self.capture_command_candidate(profile.as_deref_mut());
        let started = Instant::now();
        let Some(execution) = candidate.game.execute_next_intention() else {
            return Ok(None);
        };
        if let Some(profile) = profile.as_deref_mut() {
            profile.simulation_transition += started.elapsed();
            profile.simulation_transitions += 1;
        }
        let actor = ActorId(execution.intention.actor.0);
        let intention = execution.intention.id;
        let source = self
            .history_index
            .intention_admission(intention)
            .ok_or_else(invalid_archive)?;
        let admitted = &self.archive.records[source].entry;
        let Some((admitted_id, admitted_work)) = admitted.content.admission() else {
            return Err(invalid_archive());
        };
        if admitted.actor != actor
            || admitted_id != intention
            || !admitted_work.matches_intention(&execution.intention)
        {
            return Err(invalid_archive());
        }
        let admission = admitted.id.clone();
        let continuation = matches!(
            execution.intention.work,
            tor_simulation::IntentionWork::ResumePreparation { .. }
        );
        let content = match execution.outcome {
            Ok(outcome) => {
                let action = execution
                    .action
                    .and_then(adapt::recorded_action)
                    .ok_or_else(invalid_archive)?;
                let outcome = self.resolve_action_transition(
                    &mut candidate,
                    actor,
                    &action,
                    Some(outcome),
                    profile.as_deref_mut(),
                )?;
                candidate.transition(self.regions.as_mut(), profile.as_deref_mut())?;
                if continuation {
                    JournalContent::IntentionContinued {
                        admission,
                        intention,
                        action,
                        event: adapt::event(outcome.kind),
                    }
                } else {
                    JournalContent::IntentionStarted {
                        admission,
                        intention,
                        action,
                        event: adapt::event(outcome.kind),
                    }
                }
            }
            Err(_) => {
                let observations = preparation_revision_updates(
                    &self.game,
                    &candidate.game,
                    actor,
                    &mut candidate.revisions,
                )?;
                candidate.observations.extend(observations);
                if continuation {
                    JournalContent::IntentionContinuationFailed {
                        admission,
                        intention,
                    }
                } else {
                    JournalContent::IntentionFailed {
                        admission,
                        intention,
                    }
                }
            }
        };
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: candidate.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: "scheduler".into(),
            },
            audience: Audience::Actor,
            content,
        };
        self.commit_candidate(candidate, entry, None, profile)
            .map(Some)
    }
}
