//! Journal-derived work phases, including bounded rewind lineage.
//! The simulation remains authoritative for physical action effects.
use super::*;
use crate::journal::{IntentionChange, IntentionEndKind, WizardResult};

type Id = tor_simulation::IntentionId;
// Each actor owns one queue and one preparation. A continuation uses the same
// identity in both; an independent queued action may coexist with preparation.
type Phases = BTreeMap<ActorId, ActorPhases>;

#[derive(Clone, Copy)]
struct WorkPhase {
    intention: Id,
    phase: Phase,
    /// Original execution target, derived from the typed start record.
    target: Option<ActorId>,
}

#[derive(Clone, Copy, Default)]
struct ActorPhases {
    work: [Option<WorkPhase>; 2],
}

impl ActorPhases {
    fn phase(&self, id: Id) -> Option<Phase> {
        self.work
            .iter()
            .flatten()
            .find(|work| work.intention == id)
            .map(|work| work.phase)
    }

    fn set(&mut self, id: Id, phase: Phase) -> Result<(), Failure> {
        if let Some(work) = self
            .work
            .iter_mut()
            .flatten()
            .find(|work| work.intention == id)
        {
            work.phase = phase;
        } else {
            let slot = self
                .work
                .iter_mut()
                .find(|slot| slot.is_none())
                .ok_or_else(invalid_archive)?;
            *slot = Some(WorkPhase {
                intention: id,
                phase,
                target: None,
            });
        }
        Ok(())
    }

    fn remove(&mut self, id: Id) {
        for slot in &mut self.work {
            if slot.is_some_and(|work| work.intention == id) {
                *slot = None;
            }
        }
    }

    fn valid(&self) -> bool {
        let queued = self
            .work
            .iter()
            .flatten()
            .filter(|work| work.phase.queued())
            .count();
        let preparing = self
            .work
            .iter()
            .flatten()
            .filter(|work| work.phase.preparing())
            .count();
        queued <= 1 && preparing <= 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    InitialQueued,
    InitialSuspended,
    Running,
    ProgressPaused,
    ProgressQueued,
    ProgressQueueSuspended,
}

impl Phase {
    fn queued(self) -> bool {
        matches!(
            self,
            Self::InitialQueued
                | Self::InitialSuspended
                | Self::ProgressQueued
                | Self::ProgressQueueSuspended
        )
    }

    fn preparing(self) -> bool {
        matches!(
            self,
            Self::Running
                | Self::ProgressPaused
                | Self::ProgressQueued
                | Self::ProgressQueueSuspended
        )
    }
}

/// The operation determines which work facts it may carry. Metadata cannot
/// borrow simulation effects to manufacture a different authoritative operation.
enum EffectScope {
    None,
    Terminal {
        intention: Id,
        kind: IntentionEndKind,
    },
    Simulation,
}

impl EffectScope {
    fn for_content(content: &JournalContent) -> Self {
        match content {
            JournalContent::IntentionChanged {
                intention,
                change: IntentionChange::Cancelled,
                ..
            } => Self::Terminal {
                intention: *intention,
                kind: IntentionEndKind::Cancelled,
            },
            JournalContent::IntentionFailed { intention, .. }
            | JournalContent::IntentionContinuationFailed { intention, .. } => Self::Terminal {
                intention: *intention,
                kind: IntentionEndKind::Failed,
            },
            JournalContent::IntentionChanged {
                change: IntentionChange::Suspended | IntentionChange::Resumed,
                ..
            }
            | JournalContent::IntentionAdmitted { .. }
            | JournalContent::AutonomousIntentionAdmitted { .. }
            | JournalContent::TravelIntentionAdmitted { .. }
            | JournalContent::PlaceRenamed { .. }
            | JournalContent::Travel { .. }
            | JournalContent::Annotation { .. }
            | JournalContent::Wizard {
                operation: WizardOperation::Rewind { .. },
                ..
            } => Self::None,
            JournalContent::IntentionStarted { .. }
            | JournalContent::IntentionContinued { .. }
            | JournalContent::Action { .. }
            | JournalContent::Wizard { .. } => Self::Simulation,
        }
    }

    fn valid_for(self, entry: &JournalEntry) -> bool {
        // Each work identity has at most one derived phase at a boundary.
        // Validate ordering before using binary search to prove disjointness.
        if entry
            .intention_suspensions
            .windows(2)
            .any(|pair| pair[0].intention >= pair[1].intention)
            || entry
                .intention_ends
                .windows(2)
                .any(|pair| pair[0].intention >= pair[1].intention)
            || entry.intention_suspensions.iter().any(|suspension| {
                entry
                    .intention_ends
                    .binary_search_by_key(&suspension.intention, |end| end.intention)
                    .is_ok()
            })
        {
            return false;
        }
        match self {
            Self::None => entry.intention_ends.is_empty() && entry.intention_suspensions.is_empty(),
            Self::Terminal { intention, kind } => {
                entry.intention_suspensions.is_empty()
                    && matches!(entry.intention_ends.as_slice(), [end]
                    if end.intention == intention && end.actor == entry.actor && end.kind == kind)
            }
            Self::Simulation => true,
        }
    }
}

struct PhaseBoundary {
    id: Option<EntryId>,
    selectable: bool,
    phases: Arc<Phases>,
}

pub(super) struct JournalLifecycle<'a> {
    roots: BTreeMap<Id, &'a Record>,
    journeys: BTreeMap<EntryId, &'a JournalEntry>,
    travel_steps: BTreeMap<EntryId, &'a JournalEntry>,
    travel_resolutions: BTreeMap<Id, &'a JournalEntry>,
    branch: BranchId,
    phases: Arc<Phases>,
    boundaries: VecDeque<PhaseBoundary>,
}

impl<'a> JournalLifecycle<'a> {
    pub(super) fn new(branch: BranchId) -> Self {
        let phases = Arc::new(BTreeMap::new());
        let boundaries = VecDeque::from([PhaseBoundary {
            id: None,
            selectable: true,
            phases: phases.clone(),
        }]);
        Self {
            roots: BTreeMap::new(),
            journeys: BTreeMap::new(),
            travel_steps: BTreeMap::new(),
            travel_resolutions: BTreeMap::new(),
            branch,
            phases,
            boundaries,
        }
    }

    fn phase_at(&self, phases: &Phases, id: Id) -> Option<Phase> {
        let actor = self.root(id)?.entry.actor;
        phases.get(&actor)?.phase(id)
    }

    fn target_at(&self, phases: &Phases, id: Id) -> Option<ActorId> {
        let actor = self.root(id)?.entry.actor;
        phases
            .get(&actor)?
            .work
            .iter()
            .flatten()
            .find(|work| work.intention == id)?
            .target
    }

    fn phase(&self, id: Id) -> Result<Phase, Failure> {
        self.phase_at(&self.phases, id).ok_or_else(invalid_archive)
    }

    fn transition(&mut self, id: Id, phase: Phase) -> Result<(), Failure> {
        let actor = self.root(id).ok_or_else(invalid_archive)?.entry.actor;
        Arc::make_mut(&mut self.phases)
            .entry(actor)
            .or_default()
            .set(id, phase)
    }

    fn remove(&mut self, id: Id) -> Result<(), Failure> {
        let actor = self.root(id).ok_or_else(invalid_archive)?.entry.actor;
        let phases = Arc::make_mut(&mut self.phases);
        let owned = phases.get_mut(&actor).ok_or_else(invalid_archive)?;
        owned.remove(id);
        if owned.work.iter().all(Option::is_none) {
            phases.remove(&actor);
        }
        Ok(())
    }

    pub(super) fn root(&self, id: Id) -> Option<&'a Record> {
        self.roots.get(&id).copied()
    }

    pub(super) fn observe(&mut self, record: &'a Record) -> Result<(), Failure> {
        let entry = &record.entry;
        if !EffectScope::for_content(&entry.content).valid_for(entry) {
            return Err(invalid_archive());
        }

        if let JournalContent::Wizard {
            operation: WizardOperation::Rewind { target },
            result:
                WizardResult::Rewound {
                    from_branch,
                    branch,
                    ..
                },
            ..
        } = &entry.content
        {
            if from_branch != &self.branch
                || branch != &entry.branch
                || branch.0 != entry.id.0
                || branch == &self.branch
            {
                return Err(invalid_archive());
            }
            self.phases = self
                .boundaries
                .iter()
                .find(|boundary| boundary.selectable && &boundary.id == target)
                .map(|boundary| boundary.phases.clone())
                .ok_or_else(invalid_archive)?;
            self.branch = branch.clone();
        } else if entry.branch != self.branch {
            return Err(invalid_archive());
        }
        if matches!(entry.content, JournalContent::Travel { .. }) {
            self.journeys.insert(entry.id.clone(), entry);
        }
        if let JournalContent::TravelIntentionAdmitted { journey, step, .. } = &entry.content {
            let root = self.journeys.get(journey).ok_or_else(invalid_archive)?;
            let previous = match self.travel_steps.get(journey) {
                None => None,
                Some(admitted) => {
                    let (id, _) = admitted.content.admission().ok_or_else(invalid_archive)?;
                    Some((
                        *admitted,
                        *self
                            .travel_resolutions
                            .get(&id)
                            .ok_or_else(invalid_archive)?,
                    ))
                }
            };
            if !super::intention::valid_travel_link(
                root,
                previous,
                entry.actor,
                &entry.branch,
                *step,
            ) {
                return Err(invalid_archive());
            }
            self.travel_steps.insert(journey.clone(), entry);
        }
        match &entry.content {
            JournalContent::IntentionAdmitted { intention, .. }
            | JournalContent::AutonomousIntentionAdmitted { intention }
            | JournalContent::TravelIntentionAdmitted { intention, .. } => {
                if self.roots.insert(*intention, record).is_some() {
                    return Err(invalid_archive());
                }
                self.transition(*intention, Phase::InitialQueued)?;
            }
            JournalContent::IntentionChanged {
                admission,
                intention,
                change,
            } => {
                self.validate_source(record, admission, *intention, false)?;
                let phase = self.phase(*intention)?;
                match (change, phase) {
                    (IntentionChange::Suspended, Phase::InitialQueued) => {
                        self.transition(*intention, Phase::InitialSuspended)?
                    }
                    (IntentionChange::Suspended, Phase::Running) => {
                        self.transition(*intention, Phase::ProgressPaused)?
                    }
                    (IntentionChange::Suspended, Phase::ProgressQueued) => {
                        self.transition(*intention, Phase::ProgressQueueSuspended)?
                    }
                    (IntentionChange::Resumed, Phase::InitialSuspended) => {
                        self.transition(*intention, Phase::InitialQueued)?
                    }
                    (
                        IntentionChange::Resumed,
                        Phase::ProgressPaused | Phase::ProgressQueueSuspended,
                    ) => self.transition(*intention, Phase::ProgressQueued)?,
                    (IntentionChange::Cancelled, _) => {}
                    _ => return Err(invalid_archive()),
                }
            }
            JournalContent::IntentionStarted {
                admission,
                intention,
                action,
                ..
            } => {
                self.validate_source(record, admission, *intention, true)?;
                if self.phase(*intention)? != Phase::InitialQueued {
                    return Err(invalid_archive());
                }
                self.transition(*intention, Phase::Running)?;
                if let Action::Attack { target } = action {
                    let actor = self
                        .root(*intention)
                        .ok_or_else(invalid_archive)?
                        .entry
                        .actor;
                    let owned = Arc::make_mut(&mut self.phases)
                        .get_mut(&actor)
                        .ok_or_else(invalid_archive)?;
                    owned
                        .work
                        .iter_mut()
                        .flatten()
                        .find(|work| work.intention == *intention)
                        .ok_or_else(invalid_archive)?
                        .target = Some(ActorId(target.0));
                }
                if !matches!(action, Action::Attack { .. }) {
                    self.require_end(record, *intention, IntentionEndKind::Resolved)?;
                }
            }
            JournalContent::IntentionFailed {
                admission,
                intention,
            } => {
                self.validate_source(record, admission, *intention, true)?;
                if self.phase(*intention)? != Phase::InitialQueued {
                    return Err(invalid_archive());
                }
            }
            JournalContent::IntentionContinued {
                admission,
                intention,
                ..
            }
            | JournalContent::IntentionContinuationFailed {
                admission,
                intention,
            } => {
                self.validate_source(record, admission, *intention, true)?;
                if self.phase(*intention)? != Phase::ProgressQueued {
                    return Err(invalid_archive());
                }
                if matches!(entry.content, JournalContent::IntentionContinued { .. }) {
                    self.transition(*intention, Phase::Running)?;
                }
            }
            _ => {}
        }
        for suspension in &entry.intention_suspensions {
            let root = self
                .root(suspension.intention)
                .ok_or_else(invalid_archive)?;
            if root.entry.actor != suspension.actor
                || self.phase(suspension.intention)? != Phase::Running
            {
                return Err(invalid_archive());
            }
            self.transition(suspension.intention, Phase::ProgressPaused)?;
        }
        for end in &entry.intention_ends {
            let root = self.root(end.intention).ok_or_else(invalid_archive)?;
            let phase = self.phase(end.intention)?;
            let direct = record
                .resolution()
                .is_some_and(|(_, id)| id == end.intention)
                || matches!(entry.content, JournalContent::IntentionChanged {
                    intention, change: IntentionChange::Cancelled, ..
                } if intention == end.intention);
            if root.entry.actor != end.actor
                || (!direct && matches!(phase, Phase::InitialQueued | Phase::InitialSuspended))
            {
                return Err(invalid_archive());
            }
            self.remove(end.intention)?;
        }
        if let Some((_, id)) = record.resolution() {
            if self.root(id).is_some_and(|root| {
                matches!(
                    root.entry.content,
                    JournalContent::TravelIntentionAdmitted { .. }
                )
            }) {
                self.travel_resolutions.insert(id, entry);
            }
        }
        // Effects may replace preparation within this atomic record. Check the
        // final owned slots after terminal facts, not during intermediate phases.
        let mut affected = std::iter::once(entry.actor)
            .chain(entry.intention_suspensions.iter().map(|fact| fact.actor))
            .chain(entry.intention_ends.iter().map(|fact| fact.actor));
        if !affected.all(|actor| self.phases.get(&actor).is_none_or(ActorPhases::valid)) {
            return Err(invalid_archive());
        }
        if !matches!(entry.content, JournalContent::Annotation { .. }) {
            self.boundaries.push_back(PhaseBoundary {
                id: Some(entry.id.clone()),
                selectable: entry.content.rewindable(),
                phases: self.phases.clone(),
            });
            retain_boundaries(&mut self.boundaries, |boundary| boundary.selectable);
        }
        Ok(())
    }

    fn validate_source(
        &self,
        record: &Record,
        source: &EntryId,
        id: Id,
        execution: bool,
    ) -> Result<(), Failure> {
        let root = self.root(id).ok_or_else(invalid_archive)?;
        if &root.entry.id != source
            || if execution {
                !record.valid_resolution(root)
            } else {
                !record.valid_change(root)
            }
        {
            return Err(invalid_archive());
        }
        Ok(())
    }

    fn require_end(&self, record: &Record, id: Id, kind: IntentionEndKind) -> Result<(), Failure> {
        if !record
            .entry
            .intention_ends
            .iter()
            .any(|end| end.intention == id && end.kind == kind)
        {
            return Err(invalid_archive());
        }
        Ok(())
    }

    pub(super) fn valid_game(&self, game: &Game, boundary: &Option<EntryId>) -> bool {
        let Some(phases) = self
            .boundaries
            .iter()
            .find(|saved| &saved.id == boundary)
            .map(|saved| saved.phases.as_ref())
        else {
            return false;
        };
        if !game.queued_intentions().all(|queued| {
            let Some(root) = self.root(queued.id) else {
                return false;
            };
            let phase = self.phase_at(phases, queued.id);
            let queued_phase = match phase {
                Some(Phase::InitialQueued | Phase::ProgressQueued) => {
                    tor_simulation::IntentionState::Queued
                }
                Some(Phase::InitialSuspended | Phase::ProgressQueueSuspended) => {
                    tor_simulation::IntentionState::Suspended
                }
                _ => return false,
            };
            let work_matches_phase = matches!(
                (phase, queued.work),
                (
                    Some(Phase::InitialQueued | Phase::InitialSuspended),
                    tor_simulation::IntentionWork::Action(_)
                        | tor_simulation::IntentionWork::AiDecision
                ) | (
                    Some(Phase::ProgressQueued | Phase::ProgressQueueSuspended),
                    tor_simulation::IntentionWork::ResumeAttack { .. }
                )
            );
            work_matches_phase
                && root.entry.actor.0 == queued.actor.0
                && queued.state == queued_phase
                && root
                    .entry
                    .content
                    .admission()
                    .is_some_and(|(_, work)| work.matches_intention(queued))
        }) {
            return false;
        }
        if !game.loaded_actor_ids().all(|actor| {
            game.preparation(actor).is_none_or(|progress| {
                progress.intention.is_none_or(|id| {
                    self.root(id).is_some_and(|root| {
                        root.entry.actor.0 == actor.0
                            && self.target_at(phases, id) == Some(ActorId(progress.target.0))
                    }) && match self.phase_at(phases, id) {
                        Some(Phase::Running) => progress.active,
                        Some(
                            Phase::ProgressPaused
                            | Phase::ProgressQueued
                            | Phase::ProgressQueueSuspended,
                        ) => !progress.active,
                        _ => false,
                    }
                })
            })
        }) {
            return false;
        }
        phases
            .values()
            .flat_map(|owned| owned.work.iter().flatten())
            .all(|work| {
                let id = work.intention;
                let phase = work.phase;
                let Some(root) = self.root(id) else {
                    return false;
                };
                let actor = SimActor(root.entry.actor.0);
                match phase {
                    Phase::InitialQueued | Phase::InitialSuspended => game
                        .pending_intention(actor)
                        .is_some_and(|queued| queued.id == id),
                    Phase::Running
                    | Phase::ProgressPaused
                    | Phase::ProgressQueued
                    | Phase::ProgressQueueSuspended => {
                        let preparation = game
                            .preparation(actor)
                            .is_some_and(|progress| progress.intention == Some(id))
                            || (!game.loaded_actor_ids().any(|loaded| loaded == actor)
                                && game.known_actor_region(actor).is_some());
                        preparation
                            && (!phase.queued()
                                || game
                                    .pending_intention(actor)
                                    .is_some_and(|queued| queued.id == id))
                    }
                }
            })
    }
}
