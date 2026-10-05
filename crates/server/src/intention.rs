//! Durable execution of work selected by the simulation-owned scheduler.
use super::*;
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
        | JournalContent::IntentionFailed { intention, .. } => {
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
            if after.preparation(actor).is_some_and(|p| p.intention == Some(intention))
                || (!loaded.contains(&actor) && after.known_actor_region(actor).is_some()) {
                return None;
            }
            let resolved = after.combat_events().iter().any(|event| matches!(event,
                tor_simulation::combat::CombatEvent::Resolved { actor: owner, intention: Some(id), .. }
                    if *owner == actor && *id == intention));
            let immediate = current == Some(intention) && matches!(&entry.content,
                JournalContent::IntentionStarted { action, .. } if !matches!(action, Action::Attack { .. }));
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

impl Engine {
    pub(crate) fn queued_human_actors(&self) -> Vec<ActorId> {
        self.game
            .queued_intentions()
            .filter(|queued| queued.origin == tor_simulation::IntentionOrigin::Human)
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
        let mut candidate = self.capture_command_candidate(None);
        let content = self.prepare_intention_change(
            &mut candidate,
            actor,
            &admission,
            IntentionChange::Suspended,
        )?;
        let entry = JournalEntry {
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
            .map(Some)
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
        if admitted.actor != actor
            || candidate
                .game
                .pending_intention(SimActor(actor.0))
                .is_none_or(|queued| {
                    queued.id != *intention
                        || queued.origin != tor_simulation::IntentionOrigin::Human
                        || queued.work
                            != tor_simulation::IntentionWork::Action(adapt::action(action))
                })
        {
            return Err(unavailable());
        }
        apply_intention_change(&mut candidate.game, actor, *intention, change)?;
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
        Some(IntentionStatus {
            actor: entry.actor,
            branch: branch.clone(),
            intention: IntentionId(entry.id.0.clone()),
            entry_id: entry.id.clone(),
            phase,
        })
    }

    pub(crate) fn pending_intentions(&self, actor: ActorId) -> Vec<IntentionStatus> {
        let mut statuses = Vec::new();
        if let Some(pending) = self.game.pending_intention(SimActor(actor.0)) {
            if let Some(status) = self.status_for_intention(
                pending.id,
                self.branch(),
                if pending.state == tor_simulation::IntentionState::Suspended {
                    IntentionPhase::Suspended
                } else {
                    IntentionPhase::Queued
                },
            ) {
                statuses.push(status);
            }
        }
        if let Some(id) = self
            .game
            .preparation(SimActor(actor.0))
            .and_then(|p| p.intention)
        {
            if let Some(status) =
                self.status_for_intention(id, self.branch(), IntentionPhase::Started)
            {
                statuses.push(status);
            }
        }
        statuses
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
        match &result.entry.content {
            JournalContent::IntentionChanged {
                intention, change, ..
            } => self.status_for_intention(
                *intention,
                &result.entry.branch,
                match change {
                    IntentionChange::Suspended => IntentionPhase::Suspended,
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
                } else if matches!(event, crate::journal::Event::AttackStarted { .. }) {
                    IntentionPhase::Started
                } else {
                    IntentionPhase::Resolved
                },
            }),
            JournalContent::IntentionFailed { admission, .. } => Some(IntentionStatus {
                actor: result.entry.actor,
                branch: result.entry.branch.clone(),
                intention: IntentionId(admission.0.clone()),
                entry_id: admission.clone(),
                phase: IntentionPhase::Failed,
            }),
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
        } else {
            self.history_index
                .intention_resolution(&result.entry.branch, intention)
                .map(|index| match self.archive.records[index].entry.content {
                    JournalContent::IntentionStarted {
                        event: crate::journal::Event::AttackStarted { .. },
                        ..
                    } => IntentionPhase::Started,
                    JournalContent::IntentionStarted { .. } => IntentionPhase::Resolved,
                    JournalContent::IntentionFailed { .. } => IntentionPhase::Failed,
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
        let mut candidate = self.capture_command_candidate(None);
        let Some(execution) = candidate.game.execute_next_intention() else {
            return Ok(None);
        };
        let actor = ActorId(execution.intention.actor.0);
        let intention = execution.intention.id;
        let source = self
            .history_index
            .intention_admission(intention)
            .ok_or_else(invalid_archive)?;
        let admitted = &self.archive.records[source].entry;
        let JournalContent::IntentionAdmitted {
            intention: admitted_id,
            action: admitted_action,
        } = &admitted.content
        else {
            return Err(invalid_archive());
        };
        if admitted.actor != actor
            || *admitted_id != intention
            || execution.intention.origin != tor_simulation::IntentionOrigin::Human
            || execution.intention.work
                != tor_simulation::IntentionWork::Action(adapt::action(admitted_action))
        {
            return Err(invalid_archive());
        }
        let admission = admitted.id.clone();
        let content = match execution.outcome {
            Ok(outcome) => {
                let action = execution
                    .action
                    .and_then(adapt::disclosed_action)
                    .ok_or_else(invalid_archive)?;
                let (outcome, _) = self.resolve_action_transition(
                    &mut candidate,
                    actor,
                    &action,
                    Some(outcome),
                    None,
                )?;
                candidate.transition(self.regions.as_mut(), None)?;
                JournalContent::IntentionStarted {
                    admission,
                    intention,
                    action,
                    event: adapt::event(outcome.kind),
                }
            }
            Err(_) => JournalContent::IntentionFailed {
                admission,
                intention,
            },
        };
        let entry = JournalEntry {
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
            .map(Some)
    }
}
