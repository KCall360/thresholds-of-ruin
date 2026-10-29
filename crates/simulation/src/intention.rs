//! Simulation-owned admitted work. Admission is distinct from execution, and
//! execution rebuilds transient preparation against the current authoritative state.
use crate::{Action, ActionOutcome, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_QUEUED_INTENTIONS: usize = 4096;

pub(crate) mod checkpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionOrigin {
    Human,
    Autonomous,
    Travel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionState {
    Queued,
    Suspended,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "action",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum IntentionWork {
    Action(Action),
    AiDecision,
    ResumePreparation { work: crate::Work },
}

/// Region-local movement meaning at admission, including portal frame changes.
/// This is a guard, not a prepared action or cached permission to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MovementContext {
    from: tor_world::Location,
    orientation: u8,
    to: tor_world::Location,
    next_orientation: u8,
}
impl<'de> Deserialize<'de> for MovementContext {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::location")]
            from: tor_world::Location,
            orientation: u8,
            #[serde(deserialize_with = "crate::bounded::location")]
            to: tor_world::Location,
            next_orientation: u8,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let value = Self {
            from: fields.from,
            orientation: fields.orientation,
            to: fields.to,
            next_orientation: fields.next_orientation,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid movement context"));
        }
        Ok(value)
    }
}

impl MovementContext {
    pub(crate) fn valid(&self) -> bool {
        self.from.region.0 != 0
            && self.to.region.0 != 0
            && self.orientation < 24
            && self.next_orientation < 24
    }
    /// Backend audit checks compare the saved region-local destination.
    pub fn destination(&self) -> tor_world::Location {
        self.to
    }
    pub(crate) fn destination_orientation(&self) -> u8 {
        self.next_orientation
    }

    pub(crate) fn matches(
        &self,
        from: tor_world::Location,
        orientation: u8,
        to: tor_world::Location,
        next_orientation: u8,
    ) -> bool {
        (self.from, self.orientation, self.to, self.next_orientation)
            == (from, orientation, to, next_orientation)
    }
}

/// An intentional terminal action has an endpoint and exact bound item,
/// never a fabricated movement destination. Ownership is rechecked at execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalExitContext {
    at: tor_world::Location,
    direction: tor_world::Direction,
    item: crate::ItemId,
}
impl<'de> Deserialize<'de> for TerminalExitContext {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::location")]
            at: tor_world::Location,
            direction: tor_world::Direction,
            item: crate::ItemId,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.at.region.0 == 0
            || fields.direction != tor_world::Direction::Up
            || fields.item.0 == 0
            || fields.item.0 == u64::MAX
        {
            return Err(serde::de::Error::custom("invalid terminal exit context"));
        }
        Ok(Self {
            at: fields.at,
            direction: fields.direction,
            item: fields.item,
        })
    }
}
impl TerminalExitContext {
    fn policy_matches(self, game: &Game, actor: ActorId, action: &Action) -> bool {
        matches!(action, Action::Move(direction) if *direction == self.direction)
            && game.is_terminal_exit(actor, self.at, self.direction)
            && game
                .unique_retrieval_state()
                .is_some_and(|state| state.item == Some(self.item))
    }
    fn live_matches(self, game: &Game, actor: ActorId, action: &Action) -> bool {
        self.policy_matches(game, actor, action)
            && game
                .actors
                .get(&actor)
                .is_some_and(|state| state.location == self.at)
    }
}

fn deserialize_movement_context<'de, D>(
    deserializer: D,
) -> Result<Option<MovementContext>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<MovementContext>::deserialize(deserializer)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedIntention {
    pub id: IntentionId,
    pub actor: ActorId,
    pub work: IntentionWork,
    pub origin: IntentionOrigin,
    pub state: IntentionState,
    #[serde(deserialize_with = "deserialize_movement_context")]
    pub movement_context: Option<MovementContext>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub terminal_exit_context: Option<TerminalExitContext>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub human_operands: Option<crate::HumanOperandBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentionExecution {
    pub intention: QueuedIntention,
    /// A concrete action is selected only at execution for deferred AI work.
    pub action: Option<Action>,
    pub outcome: Result<ActionOutcome, GameError>,
}

/// Derived queue capacity and existing-work controls. Backend-only identities
/// and availability are never authority to act: attachment, actor validity,
/// targets and timing must still be checked at the appropriate boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentionInput {
    pub slot_available: bool,
    controls: [Option<IntentionControl>; 2],
}

impl IntentionInput {
    /// Queue first, then independent preparation. No intermediate list allocates.
    pub fn controls(&self) -> impl Iterator<Item = IntentionControl> + '_ {
        self.controls.iter().flatten().copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentionControlState {
    Queued,
    Suspended,
    Started,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentionControl {
    pub intention: IntentionId,
    pub state: IntentionControlState,
    pub can_resume: bool,
    pub can_cancel: bool,
}

/// The validated resume operation determines its mutation without rechecking
/// a second policy. Existing queue work never allocates another identity.
enum ResumeIntention {
    Queue,
    Preparation { work: crate::Work },
}

enum CancelIntention {
    Queue,
    Preparation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntentionQueue {
    next_id: u64,
    entries: BTreeMap<ActorId, QueuedIntention>,
}

impl Default for IntentionQueue {
    fn default() -> Self {
        Self {
            next_id: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl Game {
    /// Validate currently available work without moving time or applying effects.
    /// A later execution can still fail if the actor, target or topology changes.
    pub fn admit_intention(
        &mut self,
        actor: ActorId,
        action: Action,
        origin: IntentionOrigin,
    ) -> Result<IntentionId, GameError> {
        self.admit_action_intention(actor, action, origin, None)
    }

    /// Admit one planned region-local step without moving time or applying effects.
    /// The ordinary movement context also protects its portal frame at execution.
    pub fn admit_travel_intention(
        &mut self,
        actor: ActorId,
        step: crate::TravelStep,
    ) -> Result<IntentionId, GameError> {
        self.admit_action_intention(
            actor,
            Action::Move(step.direction),
            IntentionOrigin::Travel,
            Some(step.destination),
        )
    }

    fn admit_action_intention(
        &mut self,
        actor: ActorId,
        action: Action,
        origin: IntentionOrigin,
        destination: Option<tor_world::Location>,
    ) -> Result<IntentionId, GameError> {
        let next = self.check_intention_slot(actor)?;
        if origin == IntentionOrigin::Travel && !matches!(action, Action::Move(_)) {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        let human_operands = if origin == IntentionOrigin::Human {
            Some(self.prepare_human_intention(actor, action.clone())?)
        } else {
            self.validate_intention_action(actor, action.clone())?;
            None
        };
        let movement_context = if self.terminal_exit_context(actor, &action).is_some() {
            None
        } else {
            action
                .selected_step()
                .and_then(|direction| self.movement_context(actor, direction))
        };
        if destination.is_some_and(|destination| {
            movement_context.is_none_or(|context| context.to != destination)
        }) {
            return Err(GameError::InvalidLocation);
        }
        Ok(self.insert_intention(
            actor,
            IntentionWork::Action(action),
            origin,
            next,
            movement_context,
            human_operands,
        ))
    }

    /// Queue the decision itself; no observation, search or RNG work runs here.
    pub fn admit_ai_intention(&mut self, actor: ActorId) -> Result<IntentionId, GameError> {
        let next = self.check_intention_slot(actor)?;
        if self.run_outcome().terminal
            || !self.is_ai(actor)
            || self.health(actor).is_none_or(|(hp, _)| hp == 0)
        {
            return Err(GameError::UnknownActor);
        }
        Ok(self.insert_intention(
            actor,
            IntentionWork::AiDecision,
            IntentionOrigin::Autonomous,
            next,
            None,
            None,
        ))
    }

    /// Observe input-related queue state without preparing an action, observing
    /// the world, allocating an identity, changing time or consuming randomness.
    /// Slot availability is the shared capacity/identity check, not a claim that
    /// any particular actor, action or target is valid.
    pub fn intention_input(&self, actor: ActorId) -> IntentionInput {
        let queued = self.pending_intention(actor);
        let queue_control = queued.map(|queued| {
            self.intention_control(
                actor,
                queued.id,
                match queued.state {
                    IntentionState::Queued => IntentionControlState::Queued,
                    IntentionState::Suspended => IntentionControlState::Suspended,
                },
            )
        });
        let preparation_control = self.preparation(actor).and_then(|preparation| {
            let id = preparation
                .intention
                .filter(|id| queued.is_none_or(|queued| queued.id != *id))?;
            Some(self.intention_control(
                actor,
                id,
                if preparation.active {
                    IntentionControlState::Started
                } else {
                    IntentionControlState::Paused
                },
            ))
        });
        IntentionInput {
            slot_available: self.check_intention_slot(actor).is_ok(),
            controls: [queue_control, preparation_control],
        }
    }

    fn intention_control(
        &self,
        actor: ActorId,
        intention: IntentionId,
        state: IntentionControlState,
    ) -> IntentionControl {
        IntentionControl {
            intention,
            state,
            can_resume: self.validate_resume_intention(actor, intention).is_ok(),
            can_cancel: self.validate_cancel_intention(actor, intention).is_ok(),
        }
    }

    fn check_intention_slot(&self, actor: ActorId) -> Result<u64, GameError> {
        if self.intentions.entries.contains_key(&actor) {
            return Err(GameError::ActorBusy);
        }
        if self.intentions.entries.len() >= MAX_QUEUED_INTENTIONS {
            return Err(GameError::Schedule(crate::ScheduleError::QueueFull));
        }
        self.intentions
            .next_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)
    }

    fn insert_intention(
        &mut self,
        actor: ActorId,
        work: IntentionWork,
        origin: IntentionOrigin,
        next: u64,
        movement_context: Option<MovementContext>,
        human_operands: Option<crate::HumanOperandBinding>,
    ) -> IntentionId {
        let id = IntentionId(self.intentions.next_id);
        let terminal_exit_context = match &work {
            IntentionWork::Action(action) => self.terminal_exit_context(actor, action),
            _ => None,
        };
        self.intentions.entries.insert(
            actor,
            QueuedIntention {
                id,
                actor,
                work,
                origin,
                state: IntentionState::Queued,
                movement_context,
                terminal_exit_context,
                human_operands,
            },
        );
        self.intentions.next_id = next;
        id
    }

    pub fn pending_intention(&self, actor: ActorId) -> Option<&QueuedIntention> {
        self.intentions.entries.get(&actor)
    }

    fn terminal_exit_context(
        &self,
        actor: ActorId,
        action: &Action,
    ) -> Option<TerminalExitContext> {
        let Action::Move(direction) = action else {
            return None;
        };
        let item = self.prepare_terminal_escape(actor, *direction).ok()?;
        Some(TerminalExitContext {
            at: self.actors.get(&actor)?.location,
            direction: *direction,
            item,
        })
    }

    pub(crate) fn movement_context(
        &self,
        actor: ActorId,
        direction: tor_world::Direction,
    ) -> Option<MovementContext> {
        let state = self.actors.get(&actor)?;
        let (to, next_orientation) = self.actor_translation(actor, direction).or_else(|| {
            self.movement_guise_contacts(actor, direction)
                .map(|(goal, _)| goal)
        })?;
        Some(MovementContext {
            from: state.location,
            orientation: state.orientation,
            to,
            next_orientation,
        })
    }

    /// Backend recovery inspects only the bounded admitted work, including
    /// unavailable actors whose work still requires a terminal scheduler result.
    pub fn queued_intentions(&self) -> impl Iterator<Item = &QueuedIntention> {
        self.intentions.entries.values()
    }

    fn intention_actor_unavailable(&self, actor: ActorId) -> bool {
        self.known_actor_region(actor).is_none()
            || self.actors.get(&actor).is_some_and(|actor| !actor.alive())
    }

    fn selected_intention(&self) -> Option<&QueuedIntention> {
        if !self.arena_execution_enabled() {
            return None;
        }
        // Unavailable actors cannot become due. Resolve their work in stable
        // admission order, while preserving unloaded actors in the identity directory.
        let unavailable = self
            .intentions
            .entries
            .values()
            .filter(|entry| self.intention_actor_unavailable(entry.actor))
            .min_by_key(|entry| entry.id);
        unavailable.or_else(|| {
            self.intentions
                .entries
                .get(&self.next_actor()?)
                .filter(|entry| entry.state == IntentionState::Queued)
        })
    }

    /// Host scheduling uses the same selection as execution, including terminal
    /// work whose actor can never become due. Selection has no simulation effects.
    pub fn next_intention_actor(&self) -> Option<ActorId> {
        self.selected_intention().map(|entry| entry.actor)
    }

    /// Preserve the actor scheduler's ordering, including waiting human boundaries.
    /// A failed execution consumes the intention but never substitutes another target.
    pub fn execute_next_intention(&mut self) -> Option<IntentionExecution> {
        let intention = self.selected_intention()?.clone();
        let actor = intention.actor;
        if self.intention_actor_unavailable(actor) {
            self.intentions.entries.remove(&intention.actor);
            let action = match intention.work.clone() {
                IntentionWork::Action(action) => Some(action),
                IntentionWork::AiDecision => None,
                IntentionWork::ResumePreparation { work } => Some(work.action()),
            };
            return Some(IntentionExecution {
                intention,
                action,
                outcome: Err(GameError::UnknownActor),
            });
        }
        let (action, outcome) = match intention.work.clone() {
            IntentionWork::Action(action) => (
                Some(action.clone()),
                if intention.terminal_exit_context.is_some_and(|context| {
                    intention.movement_context.is_some()
                        || !context.live_matches(self, actor, &action)
                }) || (action.selected_step().is_some()
                    && intention.movement_context.is_none()
                    && intention.terminal_exit_context.is_none())
                {
                    Err(GameError::InvalidLocation)
                } else {
                    self.act_with_context(
                        actor,
                        action,
                        Some(intention.id),
                        intention.movement_context,
                    )
                },
            ),
            IntentionWork::AiDecision => match self.act_ai_with_intention(actor, intention.id) {
                Ok((action, outcome)) => (Some(action), Ok(outcome)),
                Err(error) => (None, Err(error)),
            },
            IntentionWork::ResumePreparation { work } => {
                let action = work.action();
                let mut outcome = self.resume_preparation_with_intention(actor, work, intention.id);
                if outcome.is_err() {
                    if let Err(error) = self.discard_intention_preparation(actor, intention.id) {
                        outcome = Err(error);
                    }
                }
                (Some(action), outcome)
            }
        };
        self.intentions.entries.remove(&actor);
        Some(IntentionExecution {
            intention,
            action,
            outcome,
        })
    }

    fn validate_cancel_intention(
        &self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<CancelIntention, GameError> {
        if self
            .preparation(actor)
            .is_some_and(|p| p.intention == Some(id) && p.plan.work().item().is_some())
        {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        if self
            .pending_intention(actor)
            .is_some_and(|queued| queued.id == id)
        {
            Ok(CancelIntention::Queue)
        } else if self
            .preparation(actor)
            .is_some_and(|preparation| preparation.intention == Some(id))
        {
            Ok(CancelIntention::Preparation)
        } else {
            Err(GameError::Schedule(crate::ScheduleError::InvalidIntention))
        }
    }

    pub fn cancel_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        self.cancel_intention_with_step(actor, id, None)
    }

    /// Cancel the exact admitted work and optionally make one ordinary step.
    /// Everything, including queue retirement, token consumption and RNG, commits
    /// together only after movement and schedule admission succeed.
    pub fn cancel_intention_with_step(
        &mut self,
        actor: ActorId,
        id: IntentionId,
        step: Option<tor_world::Direction>,
    ) -> Result<(), GameError> {
        let kind = self.validate_cancel_intention(actor, id)?;
        if let Some(direction) = step {
            return self
                .cancel_preparation_with_step(actor, Some(id), direction, None, None)
                .map(|_| ());
        }
        let mut candidate = self.clone();
        if matches!(kind, CancelIntention::Queue) {
            candidate.intentions.entries.remove(&actor);
        }
        candidate.discard_intention_preparation(actor, id)?;
        candidate.seal_participant_events()?;
        *self = candidate;
        Ok(())
    }

    /// One atomic cancellation-and-movement boundary for human and AI actors.
    /// `expected` names the old preparation; a new AI decision keeps its own queue.
    pub(crate) fn cancel_preparation_with_step(
        &mut self,
        actor: ActorId,
        expected: Option<IntentionId>,
        direction: tor_world::Direction,
        movement_context: Option<MovementContext>,
        ai: Option<(crate::ai::Ai, crate::behavior::Behavior)>,
    ) -> Result<crate::ActionOutcome, GameError> {
        if self.actor_frozen(actor)
            || !self.preparation(actor).is_some_and(|pending| {
                pending.intention == expected && pending.plan.ability_id().is_some()
            })
        {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        let mut candidate = self.clone();
        candidate.begin_action_boundary(actor)?;
        if candidate
            .pending_intention(actor)
            .is_some_and(|queued| Some(queued.id) == expected)
        {
            candidate.intentions.entries.remove(&actor);
        }
        let mut discarded = candidate.actor_outcome(actor)?;
        discarded.discard(candidate.actor_clock(actor))?;
        candidate.apply_outcome(discarded)?;
        if let Some((ai, behavior)) = ai {
            candidate.combat.ai.insert(actor, ai);
            candidate.install_behavior(actor, behavior)?;
            candidate.reconcile_recovery_contexts()?;
        }
        let outcome = candidate.escape_step_in_transaction(actor, direction, movement_context)?;
        candidate.seal_participant_events()?;
        *self = candidate;
        Ok(outcome)
    }

    fn discard_intention_preparation(
        &mut self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<bool, GameError> {
        // Queued work can belong to a detached actor without any preparation.
        // Retire that queue without loading or deriving an actor outcome. Escape
        // steps use their separate ordinary loaded-movement admission above.
        if !self
            .preparation(actor)
            .is_some_and(|pending| pending.intention == Some(id))
        {
            return Ok(false);
        }
        let mut outcome = self.actor_outcome(actor)?;
        outcome.discard(self.actor_clock(actor))?;
        self.apply_outcome(outcome)?;
        Ok(true)
    }

    pub fn suspend_human_intentions(&mut self) {
        for intention in self.intentions.entries.values_mut() {
            if intention.origin == IntentionOrigin::Human {
                intention.state = IntentionState::Suspended;
            }
        }
    }

    pub fn suspend_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        if !self.is_ai(actor)
            && self
                .preparation(actor)
                .is_some_and(|p| p.intention == Some(id) && p.active)
        {
            self.pause_preparation(actor)
                .ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
            return Ok(());
        }
        if self.pending_intention(actor).is_none_or(|entry| {
            entry.id != id
                || entry.origin != IntentionOrigin::Human
                || entry.state != IntentionState::Queued
        }) {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        self.intentions
            .entries
            .get_mut(&actor)
            .expect("checked intention")
            .state = IntentionState::Suspended;
        Ok(())
    }

    fn validate_resume_intention(
        &self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<ResumeIntention, GameError> {
        if !self.condition_allows_voluntary_input(actor) {
            return Err(GameError::ActorBusy);
        }
        if let Some(queued) = self.pending_intention(actor) {
            if queued.id == id
                && queued.origin == IntentionOrigin::Human
                && queued.state == IntentionState::Suspended
            {
                let action = match &queued.work {
                    IntentionWork::Action(action) => Some(action.clone()),
                    IntentionWork::ResumePreparation { work } => Some(work.action()),
                    IntentionWork::AiDecision => None,
                };
                if action.is_some_and(|action| self.condition_blocks_action(actor, &action)) {
                    return Err(GameError::ActorBusy);
                }
                return Ok(ResumeIntention::Queue);
            }
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        let preparation = self
            .preparation(actor)
            .ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
        if preparation.intention != Some(id) || preparation.active || self.is_ai(actor) {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        if self.condition_blocks_action(actor, &preparation.plan.work().action()) {
            return Err(GameError::ActorBusy);
        }
        if self.intentions.entries.len() >= MAX_QUEUED_INTENTIONS {
            return Err(GameError::Schedule(crate::ScheduleError::QueueFull));
        }
        Ok(ResumeIntention::Preparation {
            work: preparation.plan.work(),
        })
    }

    pub fn resume_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        match self.validate_resume_intention(actor, id)? {
            ResumeIntention::Queue => {
                self.intentions
                    .entries
                    .get_mut(&actor)
                    .expect("checked intention")
                    .state = IntentionState::Queued;
            }
            ResumeIntention::Preparation { work } => {
                self.intentions.entries.insert(
                    actor,
                    QueuedIntention {
                        id,
                        actor,
                        work: IntentionWork::ResumePreparation { work },
                        origin: IntentionOrigin::Human,
                        state: IntentionState::Queued,
                        movement_context: None,
                        terminal_exit_context: None,
                        human_operands: None,
                    },
                );
            }
        }
        Ok(())
    }

    /// An ordinary matching Action also resumes the captured plan, potentially
    /// under a newer queue identity. Deferred AI decisions and genuinely fresh
    /// actions have not selected that existing work and remain ordinary queued work.
    fn condition_pauses_queued_continuation(&self, queued: &QueuedIntention) -> bool {
        self.actors.get(&queued.actor).is_some_and(|state| {
            let Some(pending) = &state.pending else {
                return false;
            };
            let work = pending.plan.work();
            let continues = match &queued.work {
                IntentionWork::ResumePreparation { work: selected } => {
                    pending.intention == Some(queued.id) && work == *selected
                }
                IntentionWork::Action(action) => work.action() == *action,
                IntentionWork::AiDecision => false,
            };
            continues
                && state.combat.as_ref().is_some_and(|combat| {
                    combat
                        .creature()
                        .condition_pauses_preparation(&work.action())
                })
        })
    }

    /// Previously queued continuations cannot silently thaw work. Preserve both
    /// queue and paid-plan identities until a fresh ordinary Resume control.
    pub(crate) fn suspend_condition_denied_continuation(&mut self, actor: ActorId) {
        let suspend = self.pending_intention(actor).is_some_and(|queued| {
            queued.state == IntentionState::Queued
                && self.condition_pauses_queued_continuation(queued)
        });
        if suspend {
            self.intentions
                .entries
                .get_mut(&actor)
                .expect("matched queued resume")
                .state = IntentionState::Suspended;
        }
    }

    pub fn continue_intention_ids(&mut self, later: &Game) {
        self.intentions.next_id = self.intentions.next_id.max(later.intentions.next_id);
    }

    pub(crate) fn intentions_valid(&self) -> bool {
        let mut ids = BTreeSet::new();
        let owners: BTreeMap<_, _> =
            self.intentions
                .entries
                .values()
                .map(|entry| (entry.id, entry.actor))
                .chain(self.actors.iter().filter_map(|(&actor, state)| {
                    Some((state.pending.as_ref()?.intention?, actor))
                }))
                .collect();
        let mut origins = BTreeSet::new();
        self.intentions.next_id != 0
            && self.intentions.entries.len() <= MAX_QUEUED_INTENTIONS
            && self.intentions.entries.iter().all(|(actor, entry)| {
                actor.0 != 0
                    && *actor == entry.actor
                    && entry.id.0 != 0
                    && entry.id.0 < self.intentions.next_id
                    && ids.insert(entry.id)
                    && match (&entry.work, entry.origin, &entry.human_operands) {
                        (IntentionWork::Action(action), IntentionOrigin::Human, Some(binding)) =>
                            self.human_operands_checkpoint_valid(binding, action),
                        (IntentionWork::Action(_), IntentionOrigin::Human, None) => false,
                        (_, _, None) => true,
                        _ => false,
                    }
                    && (entry.state != IntentionState::Queued
                        || !self.condition_pauses_queued_continuation(entry))
                    && (entry.origin != IntentionOrigin::Travel
                        || (entry.state == IntentionState::Queued
                            && matches!(entry.work, IntentionWork::Action(Action::Move(_)))))
                    && match (&entry.work, entry.movement_context, entry.terminal_exit_context) {
                        (IntentionWork::Action(action), None, Some(context)) => {
                            entry.origin != IntentionOrigin::Travel && context.policy_matches(self, *actor, action)
                        }
                        (IntentionWork::Action(action), Some(context), None) if action.selected_step().is_some() => context.valid(),
                        (work, None, None) => !matches!(work, IntentionWork::Action(action) if action.selected_step().is_some()),
                        _ => false,
                    }
                    && match entry.work.clone() {
                        IntentionWork::Action(Action::ChooseGrowthSource { basis, source }) => {
                            basis.valid()
                                && basis.actor == *actor
                                && source.valid(&self.content.rules)
                        }
                        IntentionWork::Action(Action::Respec {
                            basis,
                            replacement,
                            supplies,
                        }) => {
                            basis.valid()
                                && basis.actor == *actor
                                && replacement.structural_valid(&self.content.rules)
                                && supplies.valid(1)
                        }
                        IntentionWork::Action(Action::ChooseIntellectExpertise { basis, choice }) => {
                            basis.valid() && basis.actor == *actor && choice.structural_valid(&self.content.rules)
                        }
                        IntentionWork::Action(Action::Purchase { basis, purchase }) => {
                            // Stale permanent builds remain queued until execution
                            // reports their ordinary terminal failure. Restore checks
                            // shape, catalog references and owner binding only.
                            basis.valid()
                                && basis.actor == *actor
                                && purchase.structural_valid(&self.content.rules)
                        }
                        IntentionWork::Action(Action::UseAbility {
                            id,
                            invocation,
                            supplies,
                        }) => {
                            self.content.rules.ability(&id).is_some_and(|definition| {
                                    if !definition.invocation.accepts(invocation.kind())
                                        || !supplies.valid(definition.supply_costs.len())
                                    {
                                        return false;
                                    }
                                    if matches!(invocation.aim, crate::abilities::AbilityAim::DirectionalRay { .. }) {
                                        return matches!(definition.reach, crate::rules::Reach::Sight { .. } | crate::rules::Reach::UntilObstruction);
                                    }
                                    if matches!(invocation.aim, crate::abilities::AbilityAim::DirectionalProjectile { .. }) {
                                        return invocation.valid() && matches!(definition.reach, crate::rules::Reach::Projectile);
                                    }
                                    let target = invocation.target(*actor);
                                    if target.0 == 0 || target.0 >= self.next_actor_id {
                                        return false;
                                    }
                                    match definition.reach {
                                        crate::rules::Reach::Projectile | crate::rules::Reach::UntilObstruction => false,
                                        crate::rules::Reach::SelfOnly => target == *actor,
                                        crate::rules::Reach::SelfOrAlly { .. } => true,
                                        crate::rules::Reach::Melee
                                        | crate::rules::Reach::Sight { .. }
                                        | crate::rules::Reach::MutualSight { .. } => target != *actor,
                                    }
                                })
                        }
                        IntentionWork::Action(Action::TouchApparentObject { subject, quantity }) => subject.0 > 0 && subject.0 < self.next_actor_id && quantity.is_none_or(|n| n == 1),
                        IntentionWork::Action(Action::Escape { hold }) => hold.0 > 0 && hold.0 < self.next_hold_id,
                        IntentionWork::Action(Action::ExamineTrap { trap }) => trap != 0,
                        IntentionWork::Action(Action::SetDoor { door, .. }) => door != 0,
                        IntentionWork::Action(Action::Identify {choice,item})=>self.intention_issued(choice)&&item.is_none_or(|id|id.0>0&&id.0<self.next_item_id),
                        IntentionWork::Action(Action::Equip { item, slot }) => {
                            item.0 > 0 && slot.0 < 64
                        }
                        IntentionWork::Action(
                            Action::Unequip { item } | Action::Ignite { item } | Action::Extinguish { item }
                            | Action::Read {item}
                            | Action::Zap {item,..}
                            | Action::Drink { item }
                            | Action::Eat { item },
                        ) => item.0 > 0,
                        IntentionWork::Action(
                            Action::Take { item, quantity } | Action::Drop { item, quantity },
                        ) => item.0 != 0 && quantity.is_none_or(|quantity| quantity != 0),
                        IntentionWork::Action(Action::Move(_) | Action::Wait) => true,
                        IntentionWork::AiDecision => entry.origin == IntentionOrigin::Autonomous,
                        IntentionWork::ResumePreparation { work } => {
                            work.structural_valid(*actor)
                                && entry.origin == IntentionOrigin::Human
                                && if self.actors.contains_key(actor) {
                                    self.preparation(*actor).is_some_and(|preparation| {
                                        preparation.intention == Some(entry.id)
                                            && preparation.plan.work() == work
                                            && !preparation.active
                                    })
                                } else {
                                    self.known_actor_region(*actor).is_some()
                                }
                        }
                    }
            })
            && self
                .actors
                .iter()
                .filter_map(|(actor, state)| Some((*actor, state.pending.as_ref()?)))
                .all(|(actor, preparation)| {
                    (!preparation.plan.needs_item_payment()
                        || preparation.origin_intention().is_some())
                        && preparation.origin_intention().is_none_or(|origin| {
                            origin.0 != 0
                                && origin.0 < self.intentions.next_id
                                && preparation.intention.is_none_or(|latest| origin <= latest)
                                && origins.insert(origin)
                                && owners.get(&origin).is_none_or(|owner| *owner == actor)
                        })
                        && preparation.intention.is_none_or(|id| {
                            id.0 != 0
                                && id.0 < self.intentions.next_id
                                && (ids.insert(id)
                                    || self.pending_intention(actor).is_some_and(|queued| {
                                        queued.id == id
                                            && queued.work
                                                == IntentionWork::ResumePreparation {
                                                    work: preparation.plan.work(),
                                                }
                                    }))
                        })
                })
    }

    /// A deterministic reaction receives provenance without creating a second
    /// user/AI queue entry or replacing the toucher's current intention.
    pub(crate) fn issue_reaction_intention(&mut self) -> Result<IntentionId, GameError> {
        let id = IntentionId(self.intentions.next_id);
        self.intentions.next_id = self
            .intentions
            .next_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        Ok(id)
    }

    pub(crate) fn intention_issued(&self, id: IntentionId) -> bool {
        id.0 != 0 && id.0 < self.intentions.next_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ActorId, Game, GameError};
    use std::num::NonZeroU64;
    use tor_world::{Direction, Location, Position, RegionId};

    fn fixture() -> (Game, ActorId) {
        let mut game = Game::new(
            (*Game::two_room(42, crate::test_creatures::content()).world).clone(),
            42,
            crate::test_creatures::content(),
        );
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        (game, actor)
    }

    #[test]
    fn input_controls_match_queue_and_preparation_operations_without_mutating_state() {
        let (mut game, actor, _, original) = paused_attack_fixture();
        let paused = game.clone();
        let input = game.intention_input(actor);
        let controls: Vec<_> = input.controls().collect();
        assert!(input.slot_available);
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].intention, original);
        assert_eq!(controls[0].state, IntentionControlState::Paused);
        assert!(controls[0].can_resume);
        assert!(controls[0].can_cancel);
        assert_eq!(
            game, paused,
            "query preserves time, RNG, preparation and identities"
        );

        let independent = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let before = game.clone();
        let input = game.intention_input(actor);
        let controls: Vec<_> = input.controls().collect();
        assert!(!input.slot_available);
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0].intention, independent);
        assert_eq!(controls[0].state, IntentionControlState::Queued);
        assert!(!controls[0].can_resume);
        assert!(controls[0].can_cancel);
        assert_eq!(controls[1].intention, original);
        assert!(!controls[1].can_resume);
        assert!(controls[1].can_cancel);
        assert_eq!(game, before);
        for control in controls {
            let mut resumed = game.clone();
            assert_eq!(
                resumed.resume_intention(actor, control.intention).is_ok(),
                control.can_resume
            );
            if !control.can_resume {
                assert_eq!(resumed, before);
            }
            let mut cancelled = game.clone();
            assert_eq!(
                cancelled.cancel_intention(actor, control.intention).is_ok(),
                control.can_cancel
            );
        }
        game.suspend_intention(actor, independent).unwrap();
        let input = game.intention_input(actor);
        let controls: Vec<_> = input.controls().collect();
        assert_eq!(controls[0].state, IntentionControlState::Suspended);
        assert!(controls[0].can_resume);
        game.resume_intention(actor, independent).unwrap();
        game.cancel_intention(actor, independent).unwrap();
        assert_eq!(game.intention_input(actor).controls().count(), 1);
        game.resume_intention(actor, original).unwrap();
        let input = game.intention_input(actor);
        let controls: Vec<_> = input.controls().collect();
        assert_eq!(
            controls.len(),
            1,
            "queued continuation and preparation share one identity"
        );
        assert_eq!(controls[0].state, IntentionControlState::Queued);
        assert!(!controls[0].can_resume);
        assert!(controls[0].can_cancel);
    }

    #[test]
    fn travel_origin_queues_only_movement_and_preserves_its_checkpoint_identity() {
        let origin: IntentionOrigin = serde_json::from_str("\"travel\"").unwrap();
        let (mut game, actor) = fixture();
        let before = game.clone();
        assert_eq!(
            game.admit_intention(actor, Action::Wait, origin),
            Err(GameError::Schedule(crate::ScheduleError::InvalidIntention))
        );
        assert_eq!(game, before);
        let id = game
            .admit_intention(actor, Action::Move(Direction::East), origin)
            .unwrap();
        assert_eq!(game.tick(), before.tick());
        assert_eq!(game.actors[&actor].location, before.actors[&actor].location);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored.pending_intention(actor).unwrap().id, id);
        assert_eq!(restored.pending_intention(actor).unwrap().origin, origin);
        assert_eq!(
            restored.execute_next_intention(),
            game.execute_next_intention()
        );
        assert_eq!(restored, game);
        assert_eq!(game.tick(), 100);
    }

    #[test]
    fn planned_travel_destination_is_checked_without_consuming_queue_identity() {
        let (mut game, actor) = fixture();
        let before = game.clone();
        let wrong = crate::TravelStep {
            direction: Direction::East,
            destination: Location {
                region: RegionId(2),
                position: Position { x: 2, y: 1, z: 0 },
            },
        };
        assert_eq!(
            game.admit_travel_intention(actor, wrong),
            Err(GameError::InvalidLocation)
        );
        assert_eq!(game, before);
        let step = crate::TravelStep {
            destination: Location {
                region: RegionId(1),
                ..wrong.destination
            },
            ..wrong
        };
        let id = game.admit_travel_intention(actor, step).unwrap();
        assert_eq!(id, IntentionId(1));
        assert_eq!(
            game.pending_intention(actor).unwrap().origin,
            IntentionOrigin::Travel
        );
        let mut cancelled = game.clone();
        cancelled.cancel_intention(actor, id).unwrap();
        assert_eq!(cancelled.tick(), before.tick());
        assert!(cancelled.pending_intention(actor).is_none());
        assert!(game.execute_next_intention().unwrap().outcome.is_ok());
        assert_eq!(game.tick(), 100);
        assert_eq!(game.actors[&actor].location, step.destination);
    }

    #[test]
    fn queued_move_does_not_reinterpret_direction_after_teleport_or_frame_change() {
        for teleport in [true, false] {
            let (mut game, actor) = fixture();
            let id = game
                .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
                .unwrap();
            if teleport {
                game.teleport(
                    actor,
                    Location {
                        region: RegionId(1),
                        position: Position { x: 3, y: 1, z: 0 },
                    },
                )
                .unwrap();
            } else {
                game.actors.get_mut(&actor).unwrap().orientation = 1;
            }
            let mut expected = game.clone();
            expected.cancel_intention(actor, id).unwrap();
            let execution = game.execute_next_intention().unwrap();
            assert_eq!(execution.intention.id, id);
            assert_eq!(execution.outcome, Err(GameError::InvalidLocation));
            assert_eq!(
                game, expected,
                "failed guard must not move time, state or RNG"
            );
        }
    }

    #[test]
    fn saved_movement_context_is_required_and_preserves_execution() {
        let (mut game, actor) = fixture();
        game.admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
            .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let original = serde_json::to_value((snapshot, shared)).unwrap();
        let (snapshot, shared) = serde_json::from_value(original.clone()).unwrap();
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(
            restored.execute_next_intention(),
            game.execute_next_intention()
        );
        assert_eq!(restored, game);
        // An absent context is representable for non-movement intentions, but
        // cannot restore this queued move. Malformed present contexts fail at
        // the current strict decoder before checkpoint admission is reached.
        let mut absent = original.clone();
        absent[1]["intentions"]["entries"][0]["movement_context"] = serde_json::Value::Null;
        let (snapshot, shared) = serde_json::from_value(absent).unwrap();
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
        let mut malformed = original.clone();
        malformed[1]["intentions"]["entries"][0]["movement_context"] = serde_json::json!({
            "from": {"region": 0, "position": {"x": 1, "y": 1, "z": 0}},
            "orientation": 0, "to": {"region": 1, "position": {"x": 2, "y": 1, "z": 0}},
            "next_orientation": 24
        });
        assert!(serde_json::from_value::<(
            crate::checkpoint::Snapshot,
            crate::checkpoint::SharedState
        )>(malformed)
        .is_err());
        let mut missing = original;
        missing[1]["intentions"]["entries"][0]
            .as_object_mut()
            .unwrap()
            .remove("movement_context");
        assert!(serde_json::from_value::<(
            crate::checkpoint::Snapshot,
            crate::checkpoint::SharedState
        )>(missing)
        .is_err());
    }

    #[test]
    fn queued_move_preserves_rotated_portal_destination_and_frame() {
        use tor_world::{Extent, Passage, Region, World};
        for origin in [IntentionOrigin::Human, IntentionOrigin::Travel] {
            let mut world = World::new(
                (1..=2)
                    .map(|id| Region {
                        id: RegionId(id),
                        name: id.to_string(),
                        bounds: Extent::new(3, 3, 1).unwrap(),
                    })
                    .collect(),
                vec![],
            )
            .unwrap();
            let from = Location {
                region: RegionId(1),
                position: Position { x: 2, y: 1, z: 0 },
            };
            let to = Location {
                region: RegionId(2),
                position: Position { x: 1, y: 0, z: 0 },
            };
            world
                .connect(
                    Passage {
                        from,
                        direction: Direction::East,
                        to,
                    },
                    1,
                )
                .unwrap();
            let mut game = Game::new(world, 42, crate::test_creatures::content());
            let actor = game
                .spawn_actor(from, NonZeroU64::new(100).unwrap())
                .unwrap();
            let id = if origin == IntentionOrigin::Travel {
                game.admit_travel_intention(
                    actor,
                    crate::TravelStep {
                        direction: Direction::East,
                        destination: to,
                    },
                )
            } else {
                game.admit_intention(actor, Action::Move(Direction::East), origin)
            }
            .unwrap();
            let context = game
                .pending_intention(actor)
                .unwrap()
                .movement_context
                .unwrap();
            assert_eq!((context.to, context.next_orientation), (to, 1));
            let mut remapped = game.clone();
            let mut changed_world = World::new(
                (1..=2)
                    .map(|id| Region {
                        id: RegionId(id),
                        name: id.to_string(),
                        bounds: Extent::new(3, 3, 1).unwrap(),
                    })
                    .collect(),
                vec![],
            )
            .unwrap();
            changed_world
                .connect(
                    Passage {
                        from,
                        direction: Direction::East,
                        to,
                    },
                    3,
                )
                .unwrap();
            remapped.world = tor_world::Shared::new(changed_world);
            assert!(
                remapped.actor_translation(actor, Direction::East).is_some(),
                "the changed portal still permits movement, but has a different meaning"
            );
            let mut rejected = remapped.clone();
            rejected.cancel_intention(actor, id).unwrap();
            assert_eq!(
                remapped.execute_next_intention().unwrap().outcome,
                Err(GameError::InvalidLocation)
            );
            assert_eq!(remapped, rejected);
            let mut expected = game.clone();
            expected.cancel_intention(actor, id).unwrap();
            expected.act(actor, Action::Move(Direction::East)).unwrap();
            assert!(game.execute_next_intention().unwrap().outcome.is_ok());
            assert_eq!(game.actors[&actor].location, to);
            assert_eq!(game.actors[&actor].orientation, 1);
            assert_eq!(game, expected);
        }
    }

    fn paused_attack_fixture() -> (Game, ActorId, ActorId, IntentionId) {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            crate::test_creatures::configure(
                &mut game,
                id,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        assert!(
            game.creature(actor).is_some(),
            "queued subjects own their builds"
        );
        game.actors.get_mut(&target).unwrap().ready_at = 50;
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let full_wind_up = game.preparation(actor).unwrap().remaining;
        game.pause_preparation(actor).unwrap();
        let preparation = game.preparation(actor).unwrap().clone();
        assert!(preparation.remaining > 0 && preparation.remaining < full_wind_up);
        (game, actor, target, intention)
    }

    #[test]
    fn paused_attack_resume_queues_original_identity_without_restarting_progress() {
        let (mut game, actor, target, intention) = paused_attack_fixture();
        let preparation = game.preparation(actor).unwrap().clone();
        let tick = game.tick();
        let combat = game.combat.clone();
        game.resume_intention(actor, intention).unwrap();
        assert_eq!(
            game.tick(),
            tick,
            "resume admits work without advancing time"
        );
        assert_eq!(
            game.combat, combat,
            "admission preserves combat and RNG state"
        );
        assert_eq!(game.preparation(actor), Some(&preparation));
        assert_eq!(game.pending_intention(actor).unwrap().id, intention);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        let execution = restored.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, intention);
        execution.outcome.unwrap();
        if let Some(progress) = restored.preparation(actor) {
            assert_eq!(progress.intention, Some(intention));
            assert_eq!(progress.plan.work().target(), Some(target));
            assert!(progress.remaining <= preparation.remaining);
            assert!(progress.active);
        } else {
            assert!(restored.combat_events().iter().any(|event| matches!(event,
                crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(intention))));
        }
    }

    #[test]
    fn cancelling_original_progress_preserves_independent_queued_work() {
        let (mut game, actor, _, original) = paused_attack_fixture();
        let later = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let queued = game.pending_intention(actor).unwrap().clone();
        let next_id = game.intentions.next_id;
        game.cancel_intention(actor, original).unwrap();
        assert!(game.preparation(actor).is_none());
        assert_eq!(game.pending_intention(actor), Some(&queued));
        assert_eq!(game.intentions.next_id, next_id);
        let executed = game.execute_next_intention().unwrap();
        assert_eq!(executed.intention.id, later);
        executed.outcome.unwrap();
    }

    #[test]
    fn cancelling_paused_or_queued_continuation_removes_original_progress_without_time() {
        for queued in [false, true] {
            let (mut game, actor, _, intention) = paused_attack_fixture();
            if queued {
                game.resume_intention(actor, intention).unwrap();
            }
            let before = game.clone();
            assert!(game
                .cancel_intention(actor, IntentionId(intention.0 + 1))
                .is_err());
            assert_eq!(game, before, "wrong identity must preserve progress");
            game.cancel_intention(actor, intention).unwrap();
            assert_eq!(game.tick(), before.tick());
            assert_eq!(
                game.combat, before.combat,
                "cancellation preserves RNG and combat events"
            );
            assert_eq!(game.intentions.next_id, before.intentions.next_id);
            assert!(game.preparation(actor).is_none());
            assert!(game.pending_intention(actor).is_none());
            assert!(game.resume_intention(actor, intention).is_err());
            assert!(game.cancel_intention(actor, intention).is_err());
            let mut shared = crate::checkpoint::SharedState::default();
            let snapshot = game.checkpoint(&mut shared);
            assert_eq!(
                Game::restore_checkpoint(snapshot, &shared, game.content().clone()).ok(),
                Some(game)
            );
        }
    }

    #[test]
    fn failed_continuation_concludes_original_progress_without_retargeting() {
        let (mut game, actor, target, intention) = paused_attack_fixture();
        game.resume_intention(actor, intention).unwrap();
        game.teleport(
            target,
            Location {
                region: RegionId(1),
                position: Position { x: 3, y: 1, z: 0 },
            },
        )
        .unwrap();
        let alternate = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            alternate,
            "neutral",
            crate::test_creatures::species(),
        );
        let tick = game.tick();
        let health = game.health(alternate);
        let rng = game.rules_rng;
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, intention);
        assert_eq!(
            execution.outcome,
            Err(GameError::Rule(crate::abilities::RuleError::OutOfReach))
        );
        assert_eq!(game.tick(), tick);
        assert_eq!(game.health(alternate), health);
        assert_eq!(game.rules_rng, rng);
        assert!(
            game.preparation(actor).is_none(),
            "failed continuation is terminal"
        );
        assert!(game.pending_intention(actor).is_none());
    }

    #[test]
    fn admitted_attack_identity_survives_wind_up_restore_and_impact() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            crate::test_creatures::configure(
                &mut game,
                id,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let preparation = game
            .preparation(actor)
            .expect("target's turn holds wind-up open");
        assert_eq!(
            serde_json::to_value(preparation).unwrap()["intention"],
            serde_json::json!(intention)
        );
        let mut missing = serde_json::to_value(preparation).unwrap();
        missing.as_object_mut().unwrap().remove("intention");
        assert!(
            serde_json::from_value::<crate::combat::Preparation>(missing).is_err(),
            "saved progress requires an explicit admission context, including null for direct calls"
        );
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        for corrupt in [0, intention.0 + 1] {
            let value = serde_json::to_value(&snapshot).unwrap();
            let mut pool = serde_json::to_value(&shared).unwrap();
            let actors = value["actors"].as_u64().unwrap() as usize;
            pool["actors"][actors]["1"]["pending"]["intention"] = serde_json::json!(corrupt);
            let invalid_shared = serde_json::from_value(pool).unwrap();
            assert!(Game::restore_checkpoint(
                serde_json::from_value(value).unwrap(),
                &invalid_shared,
                game.content().clone()
            )
            .is_err());
        }
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        restored.act(target, Action::Wait).unwrap();
        assert!(restored.preparation(actor).is_none());
        let resolved = restored
            .combat_events()
            .iter()
            .find_map(|event| {
                let value = serde_json::to_value(event).unwrap();
                value.get("Resolved").cloned()
            })
            .expect("attack impact");
        assert_eq!(
            resolved["outcome"]["intention"],
            serde_json::json!(intention)
        );
    }

    #[test]
    fn admitted_attack_identity_is_assigned_before_same_execution_impact() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut species = crate::test_creatures::species();
        species.melee = crate::attacks::NaturalAttack::from_melee(crate::test_creatures::melee(
            2,
            1,
            crate::combat::DamageType::Impact,
            4,
        ))
        .unwrap();
        game.content = crate::test_creatures::content_with(species.clone(), vec![]);
        for id in [actor, target] {
            crate::test_creatures::configure(&mut game, id, "neutral", species.clone());
        }
        game.actors.get_mut(&target).unwrap().ready_at = 100;
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert!(game.preparation(actor).is_none());
        assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(intention))));
    }

    #[test]
    fn interrupted_wind_up_retains_its_admission_identity() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            crate::test_creatures::configure(
                &mut game,
                id,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Vital,
                    None,
                    1,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        // Complete the outer publication omitted by the private damage helper.
        game.seal_event_observations().unwrap();
        assert!(game.event_receipts_valid());
        let preparation = game.preparation(actor).unwrap();
        assert_eq!(preparation.intention, Some(intention));
        assert!(!preparation.active);
        assert!(game.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Interrupted { intention: Some(id), .. } if *id == intention)));
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
            game
        );
    }

    #[test]
    fn preparation_checkpoint_requires_bounded_original_timing() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            crate::test_creatures::configure(
                &mut game,
                id,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        game.act(
            actor,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
        )
        .unwrap();
        let original = game.preparation(actor).unwrap();
        assert_eq!(
            (original.plan.preparation(), original.plan.recovery()),
            (60, 40)
        );
        for field in ["preparation", "recovery"] {
            let mut missing = serde_json::to_value(original).unwrap();
            missing["plan"].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<crate::combat::Preparation>(missing).is_err());
        }
        for (duration, recovery) in [(0, 40), (1_000_001, 40), (60, 1_000_001), (60, u64::MAX)] {
            let mut forged = game.clone();
            let mut state = forged.actors.get_mut(&actor).unwrap();
            let pending = state.pending.as_mut().unwrap();
            let crate::abilities::Plan::Ability(plan) = &mut pending.plan else {
                panic!("ability plan")
            };
            plan.preparation = duration;
            plan.recovery = recovery;
            drop(state);
            let mut shared = crate::checkpoint::SharedState::default();
            let snapshot = forged.checkpoint(&mut shared);
            assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
        }
    }

    #[test]
    fn new_resume_admission_preserves_the_original_preparation_identity() {
        let (mut game, actor) = fixture();
        let target = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for id in [actor, target] {
            crate::test_creatures::configure(
                &mut game,
                id,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        let original = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Vital,
                    None,
                    1,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let remaining = game.preparation(actor).unwrap().remaining;
        let resumed = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        assert_ne!(original, resumed);
        game.execute_next_intention().unwrap().outcome.unwrap();
        let preparation = game.preparation(actor).unwrap();
        assert_eq!(preparation.intention, Some(resumed));
        assert_eq!(preparation.remaining, remaining);
        assert_eq!(preparation.origin_intention(), Some(original));
        assert_eq!(preparation.hold, None);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(
            restored.preparation(actor).unwrap().origin_intention(),
            Some(original)
        );
        assert_eq!(restored, game);
        let mut missing = serde_json::to_value(game.preparation(actor).unwrap()).unwrap();
        missing.as_object_mut().unwrap().remove("origin");
        assert!(serde_json::from_value::<crate::combat::Preparation>(missing).is_err());
        for invalid in [IntentionId(0), IntentionId(resumed.0 + 1)] {
            let mut forged = game.clone();
            forged
                .actors
                .get_mut(&actor)
                .unwrap()
                .pending
                .as_mut()
                .unwrap()
                .origin = Some(invalid);
            let mut shared = crate::checkpoint::SharedState::default();
            let snapshot = forged.checkpoint(&mut shared);
            assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
        }
        let foreign = game
            .admit_intention(target, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let mut forged = game.clone();
        let mut actor_state = forged.actors.get_mut(&actor).unwrap();
        let preparation = actor_state.pending.as_mut().unwrap();
        preparation.intention = None;
        preparation.origin = Some(foreign);
        drop(actor_state);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = forged.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }

    #[test]
    fn admission_has_no_gameplay_effect_until_simulation_executes() {
        let (mut game, actor) = fixture();
        let before = game.clone();
        let id = game
            .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
            .unwrap();
        assert_eq!(game.tick(), before.tick());
        assert_eq!(game.actors[&actor].location, before.actors[&actor].location);
        assert_eq!(game.actors[&actor].ready_at, before.actors[&actor].ready_at);
        assert_eq!(game.pending_intention(actor).unwrap().id, id);
        let executed = game.execute_next_intention().unwrap();
        assert_eq!(executed.intention.id, id);
        assert!(executed.outcome.is_ok());
        assert_eq!(
            game.actors[&actor].location.position,
            Position { x: 2, y: 1, z: 0 }
        );
        assert!(game.pending_intention(actor).is_none());
    }

    #[test]
    fn queued_actions_follow_actor_scheduler_order_not_submission_order() {
        let (mut game, first) = fixture();
        let second = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 3, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let second_id = game
            .admit_intention(second, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let first_id = game
            .admit_intention(first, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        assert_eq!(
            game.execute_next_intention().unwrap().intention.id,
            first_id
        );
        assert_eq!(
            game.execute_next_intention().unwrap().intention.id,
            second_id
        );
        assert!(game.execute_next_intention().is_none());
    }

    #[test]
    fn deferred_ai_selects_once_at_execution_and_matches_direct_simulation() {
        let (mut game, actor) = fixture();
        crate::test_creatures::configure(&mut game, actor, "foe", crate::test_creatures::species());
        let hero = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(&mut game, hero, "hero", crate::test_creatures::species());
        game.configure_run(
            hero,
            BTreeSet::from([hero]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(actor, crate::ai::AiProfile::general())
            .unwrap();
        game.act(hero, Action::Wait).unwrap();
        game.refresh_navigation();
        let before = crate::diagnostics::work_counts().route_searches;
        let id = game.admit_ai_intention(actor).unwrap();
        assert_eq!(crate::diagnostics::work_counts().route_searches, before);
        let mut expected = game.clone();
        expected.cancel_intention(actor, id).unwrap();
        // Supply the same provenance so equality checks rules, RNG and AI state
        // as well as the newly retained admission identity.
        let (action, outcome) = expected.act_ai_with_intention(actor, id).unwrap();
        let before = crate::diagnostics::work_counts().route_searches;
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
        assert_eq!(execution.action, Some(action));
        assert_eq!(execution.outcome, Ok(outcome));
        assert_eq!(game, expected);
    }

    #[test]
    fn execution_revalidates_the_original_target_and_records_failure_without_effect() {
        let (mut game, actor) = fixture();
        let at = game.actors[&actor].location;
        let item = game.place_item(at, "token".into()).unwrap();
        let id = game
            .admit_intention(
                actor,
                Action::Take {
                    item,
                    quantity: None,
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.items.remove(&item);
        let tick = game.tick();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, id);
        assert_eq!(execution.outcome, Err(GameError::ItemUnavailable));
        assert_eq!(game.tick(), tick);
        assert!(game.pending_intention(actor).is_none());
        assert!(game.observe(actor).unwrap().inventory.is_empty());
    }

    #[test]
    fn saved_queue_preserves_identity_and_requires_explicit_resume_when_suspended() {
        let (mut game, actor) = fixture();
        let id = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let before = game.clone();
        assert!(
            game.resume_intention(actor, id).is_err(),
            "queued work is not suspended work"
        );
        assert!(game
            .suspend_intention(actor, IntentionId(id.0 + 1))
            .is_err());
        assert_eq!(game, before);
        game.suspend_intention(actor, id).unwrap();
        let suspended = game.clone();
        assert!(game.suspend_intention(actor, id).is_err());
        assert_eq!(game, suspended);
        game.suspend_human_intentions();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
        let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        assert_eq!(restored.pending_intention(actor).unwrap().id, id);
        assert_eq!(restored.next_intention_actor(), None);
        assert!(restored.execute_next_intention().is_none());
        restored.resume_intention(actor, id).unwrap();
        assert_eq!(restored.next_intention_actor(), Some(actor));
        assert_eq!(restored.execute_next_intention().unwrap().intention.id, id);
    }

    #[test]
    fn malformed_saved_action_structure_is_rejected_without_rechecking_availability() {
        let (mut game, actor) = fixture();
        let item = game
            .place_item(game.actors[&actor].location, "token".into())
            .unwrap();
        game.admit_intention(
            actor,
            Action::Take {
                item,
                quantity: Some(1),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let original = serde_json::to_value((snapshot, shared)).unwrap();
        for field in ["quantity", "item"] {
            let mut invalid = original.clone();
            invalid[1]["intentions"]["entries"][0]["work"]["action"]["value"][field] =
                serde_json::json!(0);
            let (snapshot, shared) = serde_json::from_value(invalid).unwrap();
            assert!(
                Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
                "invalid {field}"
            );
        }
    }

    #[test]
    fn rewind_restores_pending_work_without_reusing_abandoned_future_identities() {
        let (mut game, actor) = fixture();
        let first = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        let mut boundary = game.clone();
        game.execute_next_intention().unwrap();
        let abandoned = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        boundary.continue_intention_ids(&game);
        assert_eq!(boundary.pending_intention(actor).unwrap().id, first);
        boundary.cancel_intention(actor, first).unwrap();
        let next = boundary
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        assert!(next > abandoned);
        assert!(boundary.cancel_intention(actor, first).is_err());
        assert_eq!(boundary.pending_intention(actor).unwrap().id, next);
    }

    fn capacity_fixture() -> (Game, Vec<ActorId>) {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "queue".into(),
                bounds: tor_world::Extent::new(MAX_QUEUED_INTENTIONS as i32 + 2, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        let mut actors = Vec::new();
        for x in 0..=MAX_QUEUED_INTENTIONS {
            actors.push(
                game.spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position {
                            x: x as i32,
                            y: 1,
                            z: 0,
                        },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap(),
            );
        }
        (game, actors)
    }

    #[test]
    fn capacity_and_busy_rejections_preserve_state_and_identity_allocation() {
        let (mut game, actors) = capacity_fixture();
        for &actor in &actors[..MAX_QUEUED_INTENTIONS] {
            game.admit_intention(actor, Action::Wait, IntentionOrigin::Human)
                .unwrap();
        }
        let full = game.clone();
        assert!(!game.intention_input(actors[0]).slot_available);
        assert!(
            !game
                .intention_input(actors[MAX_QUEUED_INTENTIONS])
                .slot_available
        );
        assert_eq!(game, full);
        assert_eq!(
            game.admit_intention(actors[0], Action::Wait, IntentionOrigin::Human),
            Err(GameError::ActorBusy)
        );
        assert_eq!(
            game.admit_intention(
                actors[MAX_QUEUED_INTENTIONS],
                Action::Wait,
                IntentionOrigin::Human
            ),
            Err(GameError::Schedule(crate::ScheduleError::QueueFull))
        );
        assert_eq!(game, full);
        game.cancel_intention(actors[0], game.pending_intention(actors[0]).unwrap().id)
            .unwrap();
        assert!(
            game.intention_input(actors[MAX_QUEUED_INTENTIONS])
                .slot_available
        );
        let id = game
            .admit_intention(
                actors[MAX_QUEUED_INTENTIONS],
                Action::Wait,
                IntentionOrigin::Human,
            )
            .unwrap();
        assert_eq!(id.0, MAX_QUEUED_INTENTIONS as u64 + 1);
    }

    #[test]
    fn recovery_availability_distinguishes_capacity_from_new_identity_exhaustion() {
        let (mut game, actors) = capacity_fixture();
        let actor = actors[0];
        let target = actors[1];
        for id in [actor, target] {
            crate::test_creatures::configure(
                &mut game,
                id,
                "neutral",
                crate::test_creatures::species(),
            );
        }
        game.actors.get_mut(&target).unwrap().ready_at = 50;
        let original = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.pause_preparation(actor).unwrap();
        for &other in &actors[1..] {
            game.admit_intention(other, Action::Wait, IntentionOrigin::Human)
                .unwrap();
        }
        // Exhausted fresh identities do not prohibit reusing existing identities.
        game.intentions.next_id = u64::MAX;
        let before = game.clone();
        let input = game.intention_input(actor);
        assert!(!input.slot_available);
        let control = input.controls().next().unwrap();
        assert_eq!(control.intention, original);
        assert!(
            !control.can_resume,
            "paused preparation needs one free queue slot"
        );
        assert!(control.can_cancel);
        assert_eq!(game, before);
        assert_eq!(
            game.resume_intention(actor, original),
            Err(GameError::Schedule(crate::ScheduleError::QueueFull))
        );
        assert_eq!(game, before);

        // Existing suspended work occupies a slot already, so it can resume
        // even when both queue capacity and fresh identity allocation are full.
        let existing = game.pending_intention(target).unwrap().id;
        game.suspend_intention(target, existing).unwrap();
        let full = game.clone();
        let control = game.intention_input(target).controls().next().unwrap();
        assert_eq!(control.state, IntentionControlState::Suspended);
        assert!(control.can_resume);
        assert_eq!(game, full);
        game.resume_intention(target, existing).unwrap();
        assert_eq!(game.intentions.next_id, u64::MAX);
        game.cancel_intention(target, existing).unwrap();

        let input = game.intention_input(actor);
        assert!(
            !input.slot_available,
            "fresh admission still needs a new identity"
        );
        assert!(input.controls().next().unwrap().can_resume);
        let mut shared = crate::checkpoint::SharedState::default();
        let checkpoint = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
        assert_eq!(restored.intention_input(actor), input);
        restored.resume_intention(actor, original).unwrap();
        assert_eq!(restored.pending_intention(actor).unwrap().id, original);
        assert_eq!(restored.intentions.next_id, u64::MAX);
    }

    #[test]
    fn a_dead_queued_actor_yields_a_failure_instead_of_stuck_work() {
        let (mut game, actor) = fixture();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        let id = game
            .admit_intention(actor, Action::Wait, IntentionOrigin::Human)
            .unwrap();
        game.apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Vital,
                    None,
                    100,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let before = game.tick();
        assert_eq!(game.next_intention_actor(), Some(actor));
        assert_eq!(game.tick(), before);
        let failed = game
            .execute_next_intention()
            .expect("dead queued actor must resolve");
        assert_eq!(failed.intention.id, id);
        assert_eq!(failed.outcome, Err(GameError::UnknownActor));
        assert_eq!(game.tick(), before);
        assert!(game.pending_intention(actor).is_none());
    }
    #[test]
    fn queued_data_only_self_heal_round_trips_and_executes_identically() {
        use crate::rules::{AbilityDef, AiHints, AmountExpr, Effect, Reach, Timing, TimingSource};
        let (mut game, actor) = fixture();
        let ability = crate::content_ids::AbilityId::new("self_mend").unwrap();
        let mut document = game.content.rules.document().clone();
        document.abilities.push(AbilityDef {
            cancellable: false,
            interaction: crate::rules::Interaction::NonHostile,
            invocation: Default::default(),
            id: ability.clone(),
            tags: vec![],
            name: "Self Mend".into(),
            basic_attack: false,
            outgoing_edge_policy: Default::default(),
            cost: None,
            cooldown: None,
            supply_costs: vec![],
            timing: Timing {
                source: TimingSource::Fixed {
                    prepare: 1,
                    recover: 1,
                },
                scaled_by: None,
            },
            reach: Reach::SelfOnly,
            strikes: crate::rules::StrikeSource::Authored {
                strikes: vec![crate::rules::StrikeDef {
                    cancellable: false,
                    offset: 0,
                    unlock: None,
                    check: None,
                    on_success: vec![Effect::Heal {
                        amount: AmountExpr::Fixed { amount: 3 },
                    }],
                    on_failure: vec![],
                    descriptors: vec![],
                    riders: vec![],
                }],
            },
            ai: AiHints {
                priority: 0,
                hostile: false,
                minimum_resource_percent: 0,
                range_preference: crate::rules::RangePreference::Any,
            },
        });
        document.constants.default_abilities.push(ability.clone());
        game.content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            (*game.content.catalog).clone(),
        )
        .unwrap();
        let build = crate::creatures::CreatureBuild::new(
            game.content.clone(),
            crate::test_creatures::species().id,
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "healer".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        game.apply_damage(
            actor,
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Vital,
                    None,
                    5,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        // Complete the outer publication omitted by the private damage helper.
        game.seal_event_observations().unwrap();
        assert!(game.event_receipts_valid());
        let before = game.health(actor).unwrap().0;
        let receipt = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: ability,
                    invocation: crate::abilities::AbilityInvocation::actor_target(actor),
                    supplies: Default::default(),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let saved = game.checkpoint(&mut shared);
        let mut restored = Game::restore_checkpoint(saved, &shared, game.content.clone()).unwrap();
        assert_eq!(restored, game);
        for candidate in [&mut game, &mut restored] {
            let executed = candidate.execute_next_intention().unwrap();
            assert_eq!(executed.intention.id, receipt);
            executed.outcome.unwrap();
            assert_eq!(candidate.health(actor).unwrap().0, before + 3);
        }
        assert_eq!(restored, game);
    }
}
