//! Actor-local lifecycle outcomes and authoritative state application.
//!
//! Planners edit owned actor snapshots. This boundary alone publishes their
//! creature state, reconciles preparations, performs death cleanup, and emits
//! combat events. It runs inside the caller's existing Game transaction.
use crate::abilities::{Outcome, ResolvedEffect, RuleError};
use crate::combat::CombatEvent;
use crate::{Actor, ActorId, Game, GameError, Item, ItemId, ItemLocation, ItemSpec, Work};

#[cfg(test)]
thread_local! {
    static TOKEN_PLAN_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One strike has at most a source and a target; riders never retarget.
/// This is transient publication state, never part of a frozen or saved plan.
#[derive(Default)]
pub(crate) struct DamageActivityGuard {
    actors: [Option<ActorId>; 2],
}
impl DamageActivityGuard {
    fn first(&mut self, actor: ActorId) -> bool {
        if self.actors.contains(&Some(actor)) {
            return false;
        }
        let Some(slot) = self.actors.iter_mut().find(|slot| slot.is_none()) else {
            // A bounded resolution unit never introduces a third participant.
            return false;
        };
        *slot = Some(actor);
        true
    }
}

/// A receipt is valid only within the caller's existing strike transaction. Its
/// fields are private so only successful lifecycle publication can create it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublishedResolution {
    created_hold: Option<crate::linked_hold::HoldId>,
    event: usize,
    role: PublishedRole,
}
impl PublishedResolution {
    pub(crate) fn created_hold(self) -> Option<crate::linked_hold::HoldId> {
        self.created_hold
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublishedRole {
    Standalone,
    Primary {
        strike: u16,
    },
    Rider {
        primary: usize,
        strike: u16,
        ordinal: u16,
    },
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum ResolutionPublication {
    Standalone,
    Primary {
        strike: u16,
    },
    Rider {
        primary: PublishedResolution,
        strike: u16,
        ordinal: u16,
    },
}

pub(crate) enum ApplicationOutcome {
    Resolved(Outcome, Option<u16>),
    Actor(ActorOutcome),
    Item(ItemOutcome),
}
impl From<Outcome> for ApplicationOutcome {
    fn from(value: Outcome) -> Self {
        Self::Resolved(value, None)
    }
}
impl From<ActorOutcome> for ApplicationOutcome {
    fn from(value: ActorOutcome) -> Self {
        Self::Actor(value)
    }
}
impl From<ItemOutcome> for ApplicationOutcome {
    fn from(value: ItemOutcome) -> Self {
        Self::Item(value)
    }
}

pub(crate) struct ActorOutcome {
    pub id: ActorId,
    pub state: Actor,
    pub(crate) entry_cause: crate::floor_activation::EntryCause,
    pub(crate) spent_items: Option<crate::supplies::PreparationLineage>,
    pub(crate) nutrition_empty_depletion: bool,
    pub(crate) survival: crate::survival_diagnostics::SurvivalFlows,
    events: Vec<CombatEvent>,
    disclosure_origin: Option<crate::event_history::EventOrigin>,
    input: InputBoundary,
    fatal_cause: Option<crate::earned_growth::FatalCause>,
}
#[derive(Default)]
enum InputBoundary {
    #[default]
    Preserve,
    Human,
    Require,
}
pub(crate) enum Interruption {
    Suspend,
    Discard,
}
impl ActorOutcome {
    pub(crate) fn interrupt(&mut self, mode: Interruption, clock: u64) -> Result<(), GameError> {
        let Some(preparation) = self.state.pending.as_ref() else {
            return Ok(());
        };
        let mode = if matches!(preparation.plan, crate::abilities::Plan::Respec(_)) {
            Interruption::Discard
        } else {
            mode
        };
        if matches!(mode, Interruption::Suspend) && !preparation.active {
            return Ok(());
        }
        let intention = preparation.intention;
        match mode {
            Interruption::Suspend => {
                self.pause(clock);
            }
            Interruption::Discard => {
                self.discard(clock)?;
            }
        }
        if !self.events.iter().any(|event| matches!(event,
            CombatEvent::Interrupted { actor, intention: prior } if *actor == self.id && *prior == intention)) {
            self.events.push(CombatEvent::Interrupted { actor: self.id, intention });
        }
        self.input = InputBoundary::Human;
        Ok(())
    }
    /// Abandon only unresolved work. A resolved prefix has an irrevocable tail.
    pub(crate) fn discard(&mut self, clock: u64) -> Result<bool, GameError> {
        let Some(preparation) = self.state.pending.as_ref() else {
            return Ok(false);
        };
        let debt = preparation.abandon_delay(clock)?;
        let ready_at = if preparation.has_prefix() {
            clock
                .checked_add(debt)
                .and_then(|ready| {
                    // Ordinary waiting while suspended must not erase either debt.
                    ready.checked_add(if preparation.active {
                        0
                    } else {
                        self.state.ready_at.saturating_sub(clock)
                    })
                })
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?
        } else if preparation.active {
            clock
        } else {
            self.state.ready_at
        };
        let ended =
            preparation
                .plan
                .ability_id()
                .map(|_| crate::combat::CombatEvent::SequenceEnded {
                    actor: self.id,
                    intention: preparation.intention,
                    reason: crate::combat::SequenceEnd::Abandoned,
                });
        self.state.cancel_preparation();
        self.state.ready_at = ready_at;
        if let Some(event) = ended {
            self.events.push(event);
        }
        self.input = InputBoundary::Human;
        Ok(true)
    }
    pub(crate) fn pause(&mut self, clock: u64) -> Option<Work> {
        let preparation = self.state.pending.as_mut().filter(|p| p.active)?;
        preparation.remaining = preparation
            .remaining
            .saturating_sub(clock.saturating_sub(preparation.started));
        preparation.active = false;
        self.state.ready_at = clock;
        self.input = InputBoundary::Require;
        Some(preparation.plan.work())
    }
}

pub(crate) struct ItemOutcome {
    pub actor: ActorId,
    pub work: Work,
    pub intention: Option<crate::IntentionId>,
    pub change: ItemChange,
}
pub(crate) enum ItemChange {
    CurseBlocked(ActorOutcome),
    Equipment(ActorOutcome),
    Consumed { actor: ActorOutcome },
}

/// Valid only inside the caller's uninterrupted transaction or discarded preview.
/// Fallible actor/death preparation is finished; publication never repeats it.
/// This is not Clone/Serialize and cannot become persisted or client authority.
pub(crate) struct PreparedActorOutcome {
    outcome: ActorOutcome,
    death: Option<DeathOutcome>,
    rebuild: crate::creatures::RebuildOutcome,
}
impl PreparedActorOutcome {
    /// Preparation can add interruptions or death cleanup after a caller has
    /// finished changing its owned actor. Only this finalized value can prove
    /// that publication will not append lifecycle events.
    pub(crate) fn emits_events(&self) -> bool {
        !self.outcome.events.is_empty() || self.death.is_some()
    }

    pub(crate) fn rebuild_outcome(&self) -> &crate::creatures::RebuildOutcome {
        &self.rebuild
    }
    pub(crate) fn creature(&self) -> Option<&crate::creatures::CreatureState> {
        self.outcome
            .state
            .combat
            .as_ref()
            .map(|combat| combat.creature())
    }
}

struct DeathOutcome {
    corpse: ItemId,
    next_item_id: u64,
    body: Item,
    inventory: Vec<ItemId>,
    award: Option<DeathAward>,
}
struct DeathAward {
    owner: ActorId,
    amount: u32,
}

impl Game {
    pub(crate) fn actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let state = self.actors.get(&id).ok_or(GameError::UnknownActor)?.clone();
        Ok(ActorOutcome {
            id,
            state,
            spent_items: None,
            nutrition_empty_depletion: false,
            survival: Default::default(),
            events: vec![],
            disclosure_origin: None,
            input: InputBoundary::Preserve,
            fatal_cause: None,
            entry_cause: crate::floor_activation::EntryCause::Movement,
        })
    }

    /// Materialize an actor-local candidate without committing on a later error.
    pub(crate) fn settled_actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let outcome = self.actor_outcome_at_clock(id, self.survival_diagnostics.is_some())?;
        #[cfg(test)]
        if outcome.state.active_since != self.actors[&id].active_since {
            crate::creature_time::count_settlement();
        }
        Ok(outcome)
    }

    /// Inspection shares exact boundary projection without materializing an
    /// authoritative candidate or affecting timer-settlement diagnostics.
    pub(crate) fn projected_actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        self.actor_outcome_at_clock(id, false)
    }

    fn actor_outcome_at_clock(
        &self,
        id: ActorId,
        collect: bool,
    ) -> Result<ActorOutcome, GameError> {
        let mut outcome = self.actor_outcome(id)?;
        let state = &mut outcome.state;
        let clock = self.actor_clock(id);
        let elapsed = clock.saturating_sub(state.active_since);
        if elapsed > 0 {
            if state.alive() {
                state.periodic_relocations.advance(elapsed)?;
            }
            if state.search_clock.next_in(state).is_some() {
                let creature = state
                    .combat
                    .as_ref()
                    .expect("search source creature")
                    .creature();
                let check = crate::discovery::FrozenDiscoveryCheck::capture_for(creature, id)?;
                state.search_clock.advance(elapsed, check)?;
            }
            if let Some(combat) = state.combat.as_mut() {
                let denied = combat
                    .creature()
                    .condition_restricts(crate::rules::ConditionRestriction::VoluntaryActions);
                let expires = state
                    .behavior
                    .engagement
                    .and_then(|entry| {
                        entry
                            .last_seen
                            .checked_add(combat.creature().derived().behavior.memory_ticks)
                    })
                    .filter(|deadline| *deadline <= clock);
                let prefix = expires.map_or(elapsed, |deadline| {
                    deadline.saturating_sub(state.active_since).min(elapsed)
                });
                let endpoint_context = if expires.is_some()
                    && combat.creature().derived().contextual_health_recovery()
                {
                    Some(self.senses_faction_hostile(id)?)
                } else {
                    None
                };
                let settled = combat.advance_active_with_endpoint_context(prefix, endpoint_context);
                outcome.nutrition_empty_depletion = settled.empty_time_depletion;
                if collect {
                    outcome.survival.active(&settled);
                }
                if expires.is_some() {
                    state.behavior.engagement = None;
                    // Only the final positive interval licenses a depletion
                    // trigger. A zero-length suffix preserves the prefix event.
                    if elapsed > prefix {
                        let settled = combat.advance_active(elapsed - prefix);
                        outcome.nutrition_empty_depletion = settled.empty_time_depletion;
                        if collect {
                            outcome.survival.active(&settled);
                        }
                    }
                }
                if denied && state.pending.as_ref().is_none_or(|pending| !pending.active) {
                    // Input-denied actors can outlive their ordinary readiness.
                    // Normalize only elapsed readiness, never future recovery or
                    // the paused work's frozen remaining commitment.
                    state.ready_at = state.ready_at.max(clock);
                }
            }
            state.holds.advance(elapsed);
            state.active_since = clock;
        }
        Ok(outcome)
    }

    /// External effect application owns one transaction; internal lifecycle callers
    /// use the already existing action/physics transaction and actor-local snapshots.
    pub fn apply(&mut self, outcome: Outcome) -> Result<(), GameError> {
        self.with_effect_publication(|candidate| {
            // Standalone resolved effects can arrive between timer sweeps. Earned
            // automatic work belongs to the old policy, before this effect can
            // change senses, placement or eligibility.
            let mut owners = [outcome.actor, outcome.target];
            owners.sort();
            candidate.settle_actor_time(owners[0])?;
            if owners[1] != owners[0] {
                candidate.settle_actor_time(owners[1])?;
            }
            candidate.apply_outcome(outcome)
        })
    }

    /// Publish one complete public effect operation. Internal effect/rider work
    /// stays on apply_outcome until its owning transaction finishes participation.
    /// A host candidate retains its observer extension for the outer publication.
    pub(crate) fn with_effect_publication<R>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<R, GameError>,
    ) -> Result<R, GameError> {
        let mut candidate = self.clone();
        candidate.begin_effect_publication()?;
        let result = change(&mut candidate)?;
        candidate.finish_effect_publication()?;
        *self = candidate;
        Ok(result)
    }

    /// Existing private candidates share the same publication boundary without
    /// nesting another whole-Game transaction.
    pub(crate) fn begin_effect_publication(&mut self) -> Result<(), GameError> {
        if !self.host_region_continuation {
            self.seal_event_observations()?;
        }
        Ok(())
    }

    pub(crate) fn finish_effect_publication(&mut self) -> Result<(), GameError> {
        self.reconcile_recovery_contexts()?;
        self.reconcile_passive_discovery()?;
        self.settle_periodic_searches()?;
        self.settle_periodic_relocations()?;
        self.seal_participant_events()
    }

    /// Public actor mutations keep the cheap eventless setup path. Any finalized
    /// lifecycle event, new contextual owner or caller-owned rollback need uses
    /// the same atomic publication as a complete resolved effect operation.
    pub(crate) fn publish_actor_effect(
        &mut self,
        prepared: PreparedActorOutcome,
        requires_candidate: bool,
    ) -> Result<(), GameError> {
        let requires_candidate = requires_candidate
            || prepared.outcome.state.search_clock.pending()
            || prepared.outcome.state.periodic_relocations.pending()
            || prepared.emits_events()
            || prepared
                .creature()
                .is_some_and(|creature| creature.derived().contextual_health_recovery());
        self.with_recovery_transition(requires_candidate, |candidate| {
            candidate.publish_actor_outcome_after_search(prepared)
        })
    }

    /// Trusted source/placement changes settle an earned scan at the old place,
    /// before installing the new policy. Pure previews never call this boundary.
    pub(crate) fn publish_actor_outcome_after_search(
        &mut self,
        mut prepared: PreparedActorOutcome,
    ) -> Result<(), GameError> {
        if let Some(check) = prepared.outcome.state.search_clock.take_pending() {
            let id = prepared.outcome.id;
            self.run_active_discovery(id, &check)?;
            prepared.outcome.state.discovery = self.actors[&id].discovery.clone();
        }
        self.publish_actor_outcome(prepared)
    }

    pub(crate) fn apply_outcome(
        &mut self,
        outcome: impl Into<ApplicationOutcome>,
    ) -> Result<(), GameError> {
        match outcome.into() {
            ApplicationOutcome::Resolved(outcome, strike) => {
                let site = strike.map_or(ResolutionPublication::Standalone, |strike| {
                    ResolutionPublication::Primary { strike }
                });
                self.apply_resolved_outcome(outcome, site, &mut DamageActivityGuard::default())?;
            }
            ApplicationOutcome::Actor(outcome) => self.commit_actor_outcome(outcome)?,
            ApplicationOutcome::Item(outcome) => {
                self.push_event(
                    if matches!(&outcome.change, ItemChange::CurseBlocked(_)) {
                        CombatEvent::ItemCurseBlocked {
                            actor: outcome.actor,
                            item: outcome.work.item().ok_or(GameError::ItemUnavailable)?,
                            intention: outcome.intention,
                        }
                    } else {
                        CombatEvent::ItemCompleted {
                            actor: outcome.actor,
                            work: outcome.work,
                            intention: outcome.intention,
                        }
                    },
                    None,
                )?;
                match outcome.change {
                    ItemChange::CurseBlocked(actor) | ItemChange::Equipment(actor) => {
                        self.commit_actor_outcome(actor)?
                    }
                    ItemChange::Consumed { actor } => self.commit_actor_outcome(actor)?,
                }
            }
        }
        Ok(())
    }

    /// Shared authoritative effect publication for a standalone outcome or a
    /// sequence of primary/rider batches sharing one first-damage policy guard.
    pub(crate) fn apply_resolved_outcome(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_origin(outcome, site, activity, None)
    }

    /// Future damaging-condition/displacement adapters use this same boundary.
    /// They must materialize the causal owner without activating its floor before
    /// execution; an unloaded owner fails atomically instead of losing XP.
    pub(crate) fn apply_resolved_outcome_with_origin(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        origin: Option<crate::earned_growth::FatalOrigin>,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_transfer(outcome, site, activity, origin, None, None)
    }

    pub(crate) fn apply_resolved_with_disclosure(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        disclosure_origin: crate::event_history::EventOrigin,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_transfer(
            outcome,
            site,
            activity,
            None,
            None,
            Some(disclosure_origin),
        )
    }

    pub(crate) fn apply_resolved_theft(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        receipt: crate::theft::TransferReceipt,
        disclosure_origin: crate::event_history::EventOrigin,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_transfer(
            outcome,
            site,
            activity,
            None,
            Some(receipt),
            Some(disclosure_origin),
        )
    }

    fn apply_resolved_outcome_with_transfer(
        &mut self,
        mut outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        origin: Option<crate::earned_growth::FatalOrigin>,
        transfer: Option<crate::theft::TransferReceipt>,
        disclosure_origin: Option<crate::event_history::EventOrigin>,
    ) -> Result<PublishedResolution, GameError> {
        let disclosure_origin = disclosure_origin
            .unwrap_or_else(|| self.capture_event_origin(outcome.actor, outcome.target));
        let recorded_transfer = outcome.effects.iter().find_map(|effect| match effect {
            ResolvedEffect::Theft { quantity, .. } => Some(*quantity),
            _ => None,
        });
        if transfer
            .as_ref()
            .is_some_and(|receipt| !receipt.matches(outcome.actor, outcome.target))
            || recorded_transfer
                != transfer
                    .as_ref()
                    .map(crate::theft::TransferReceipt::quantity)
            || (recorded_transfer.is_some()
                && (outcome.effects.len() != 1
                    || outcome.actor == outcome.target
                    || !matches!(site, ResolutionPublication::Rider { .. })))
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        if outcome.result != crate::abilities::OutcomeStatus::Miss {
            outcome.result = outcome.canonical_result();
        }
        if outcome.target_rider_participated || !outcome.valid() {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let role = self.publication_role(&outcome, site)?;
        if outcome.actor != outcome.target
            && outcome.target_participated()
            && outcome
                .ability
                .as_ref()
                .and_then(|ability| self.content.rules.ability(ability))
                .is_some_and(|ability| {
                    ability.interaction == crate::rules::Interaction::HostileToTarget
                })
            && self
                .actors
                .get(&outcome.target)
                .is_some_and(|actor| actor.guise.appearance().is_some())
        {
            // The attack breaks a newly acquired guise, but its historical
            // reference ceiling was captured before this reveal.
            self.reveal_guise(outcome.target)?;
        }

        let strike = match role {
            PublishedRole::Primary { strike } => Some(strike),
            PublishedRole::Standalone | PublishedRole::Rider { .. } => None,
        };
        let recipient = outcome.effects.first().map_or(
            crate::abilities::EffectRecipient::Target,
            ResolvedEffect::recipient,
        );
        let recipient_id = match recipient {
            crate::abilities::EffectRecipient::Target => outcome.target,
            crate::abilities::EffectRecipient::Source => outcome.actor,
        };
        self.validate_equipment_alterations(recipient_id, &outcome.effects)?;
        let mut transition = self.settled_actor_outcome(recipient_id)?;
        transition.disclosure_origin = Some(disclosure_origin.clone());
        let mut placement = crate::abilities::Placement {
            location: transition.state.location,
            orientation: transition.state.orientation,
        };
        let original_placement = placement;
        // Relocation witnesses are live execution facts, not preparation-time
        // destinations. Recheck immediately before publication; a blocked exact
        // endpoint remains a paid failed effect and cannot fall back elsewhere.
        for effect in &mut outcome.effects {
            if let ResolvedEffect::Relocate { result, .. } = effect {
                if let crate::abilities::MovementResult::Moved { from, to } = *result {
                    if from != placement {
                        *result = crate::abilities::MovementResult::Stale;
                    } else if !self.relocation_destination_valid(recipient_id, to) {
                        *result = crate::abilities::MovementResult::Blocked;
                    }
                }
            }
        }
        let mut forced = false;
        let mut damaged = false;
        let mut applied_count = 0;
        let mut created_hold = None;
        for index in 0..outcome.effects.len() {
            let effect = &outcome.effects[index];
            if let ResolvedEffect::SearchAdjacentGuises { roll } = effect {
                if recipient_id != outcome.actor || outcome.actor != outcome.target {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
                self.apply_discovery_roll(outcome.actor, *roll)?;
                transition.state.discovery = self.actors[&outcome.actor].discovery.clone();
                outcome.effects[applied_count] = effect.clone();
                applied_count += 1;
                continue;
            }
            if matches!(effect, ResolvedEffect::Theft { .. }) {
                outcome.effects[applied_count] = effect.clone();
                applied_count += 1;
                continue;
            }
            if let ResolvedEffect::AlterEquipment { alteration } = effect {
                if self.alter_equipment_in_outcome(&mut transition, alteration)? {
                    outcome.effects[applied_count] = effect.clone();
                    applied_count += 1;
                }
                continue;
            }
            if let ResolvedEffect::CorrodeArmor { slot, amount } = effect {
                let applied = self.corrode_armor_in_outcome(&mut transition, slot, *amount)?;
                if applied != 0 {
                    outcome.effects[applied_count] = ResolvedEffect::CorrodeArmor {
                        slot: slot.clone(),
                        amount: applied,
                    };
                    applied_count += 1;
                }
                continue;
            }
            if let ResolvedEffect::ApplyLinkedHold { policy } = effect {
                // Earlier effects become authoritative before testing contact/capability.
                transition.state.location = placement.location;
                transition.state.orientation = placement.orientation;
                let mut events = std::mem::take(&mut transition.events);
                let event_start = self.event_log.len();
                self.commit_actor_outcome(transition)?;
                events.extend(self.take_event_suffix(event_start)?.0);
                let highwater = self.next_hold_id;
                let applied = self.apply_linked_hold_in_transaction(
                    outcome.actor,
                    recipient_id,
                    policy.clone(),
                )?;
                if self.next_hold_id != highwater {
                    created_hold = created_hold.or(applied);
                }
                transition = self.actor_outcome(recipient_id)?;
                transition.disclosure_origin = Some(disclosure_origin.clone());
                transition.events.extend(events);
                if applied.is_some() {
                    outcome.effects[applied_count] = effect.clone();
                    applied_count += 1;
                }
                continue;
            }
            let alive_before = transition.state.alive();
            if matches!(
                effect,
                ResolvedEffect::ApplyCondition { .. }
                    | ResolvedEffect::ApplyPersistentCondition { .. }
            ) && !self.actors.contains_key(&outcome.actor)
                && !self.detached_actor(outcome.actor)
            {
                return Err(GameError::UnknownActor);
            }
            if let ResolvedEffect::ApplyQualifiedConditionToSource { qualification, .. } = effect {
                if qualification.opponent != outcome.target
                    || (!self.actors.contains_key(&qualification.opponent)
                        && !self.detached_actor(qualification.opponent))
                {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
            }
            if let ResolvedEffect::ObservePublicPlan { observation } = effect {
                if !observation.valid(&self.content.rules, &self.world)
                    || observation.observed_at != self.tick
                {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
            }
            let context = match effect {
                ResolvedEffect::ApplyCondition { id, .. }
                | ResolvedEffect::ApplyPersistentCondition { id }
                | ResolvedEffect::RemoveCondition { id }
                | ResolvedEffect::ApplyQualifiedConditionToSource { id, .. } => {
                    if self.condition_uses_recovery_context(id)? {
                        Some(self.recovery_context_at_placement(recipient_id, placement)?)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let nutrition_before = transition
                .state
                .combat
                .as_ref()
                .map(|combat| combat.creature().nutrition());
            let Some(applied) = transition
                .state
                .combat
                .as_mut()
                .ok_or(GameError::Rule(RuleError::InvalidDefinition))?
                .apply_effect(
                    outcome.actor,
                    effect,
                    &mut transition.state.public_plan,
                    &mut placement,
                    context,
                )?
            else {
                continue;
            };
            if self.survival_diagnostics.is_some() {
                transition.survival.effect(&applied, nutrition_before);
            }
            if matches!(applied, ResolvedEffect::ArrestDescent) {
                self.arrest_descent_in_outcome(&mut transition)?;
            }
            if let ResolvedEffect::Relocate { mode, result } = &applied {
                if result.changed() {
                    transition.entry_cause = crate::floor_activation::EntryCause::Teleport;
                    transition.state.motion = crate::MotionState::default();
                    if *mode == crate::abilities::MovementMode::Forced {
                        transition
                            .interrupt(Interruption::Discard, self.actor_clock(recipient_id))?;
                    }
                }
            }
            if matches!(applied, ResolvedEffect::Translate { mode: crate::abilities::MovementMode::Forced, result } if result.changed())
            {
                forced = true;
                transition.entry_cause = crate::floor_activation::EntryCause::Physics;
                let episode = self.physics.next_fall_episode;
                self.physics.next_fall_episode =
                    episode.checked_add(1).ok_or(GameError::IdentityExhausted)?;
                transition.state.motion.active_fall = Some(crate::physics::ActiveFall {
                    owner: outcome.actor,
                    episode,
                    started_at: self.tick,
                });
                // Retire pending work now, including actors already scanned by
                // this preparation scheduler boundary. Do not depend on ID order.
                transition.interrupt(Interruption::Discard, self.actor_clock(recipient_id))?;
            }
            if matches!(applied, ResolvedEffect::Translate {
                mode: crate::abilities::MovementMode::Voluntary, result,
            } if result.changed())
            {
                transition.state.motion.active_fall = None;
            }
            if alive_before && !transition.state.alive() {
                let effect = u16::try_from(index)
                    .map_err(|_| GameError::Rule(RuleError::InvalidDefinition))?;
                transition.fatal_cause = Some(crate::earned_growth::FatalCause {
                    owner: outcome.actor,
                    origin: origin.clone().unwrap_or_else(|| {
                        crate::earned_growth::FatalOrigin::Direct {
                            ability: outcome.ability.clone(),
                            intention: outcome.intention,
                            strike,
                            effect,
                        }
                    }),
                });
            }
            if matches!(applied, ResolvedEffect::Damage { amount } if amount > 0) {
                // The policy at the first actual damage wins; a later
                // effect cannot retroactively preserve earlier progress.
                if !damaged && activity.first(recipient_id) {
                    transition
                        .state
                        .combat
                        .as_mut()
                        .expect("resolved creature")
                        .damage_activity();
                }
                damaged = true;
                transition.interrupt(Interruption::Suspend, self.actor_clock(recipient_id))?;
            }
            if matches!(&applied, ResolvedEffect::Translate { result, .. } | ResolvedEffect::Relocate { result, .. } if result.changed())
                || self.hold_capability_lost(recipient_id, &transition.state)
            {
                // The accepted spatial prefix is authoritative before later
                // effects, including a fatal effect that allocates a corpse.
                // Use ordinary lifecycle publication so claims/clocks retire
                // once, then continue from that candidate's current actor.
                transition.state.location = placement.location;
                transition.state.orientation = placement.orientation;
                transition.state.visited.insert(placement.location.region);
                if transition.state.motion.active_fall.is_some()
                    && !self.fall_continues(recipient_id, &transition.state)
                {
                    transition.state.motion.active_fall = None;
                }
                let entry_cause = transition.entry_cause;
                let event_start = self.event_log.len();
                self.commit_actor_outcome(transition)?;
                let events = self.take_event_suffix(event_start)?.0;
                transition = self.actor_outcome(recipient_id)?;
                transition.disclosure_origin = Some(disclosure_origin.clone());
                transition.entry_cause = entry_cause;
                // Publication can emit interruption/sequence consequences.
                // Keep them behind this batch's one original Resolved event,
                // without applying any resolved effect a second time.
                transition.events = events;
            }
            // Compact in place so event accounting contains only actual
            // applied amounts, without a second resolution or allocation.
            outcome.effects[applied_count] = applied;
            applied_count += 1;
        }
        outcome.effects.truncate(applied_count);
        outcome.result = outcome.canonical_result();
        // Damage components, self-damage and source/target overlap must not
        // apply percentage quiet resets more than once per strike.
        let source_id = outcome.actor;
        let target_id = recipient_id;
        transition.state.location = placement.location;
        transition.state.orientation = placement.orientation;
        if placement != original_placement {
            transition.state.visited.insert(placement.location.region);
            if transition.state.motion.active_fall.is_some()
                && !self.fall_continues(recipient_id, &transition.state)
            {
                transition.state.motion.active_fall = None;
            }
        }
        // A resolution event precedes its interruption/death consequences.
        transition.events.insert(
            0,
            CombatEvent::Resolved {
                outcome,
                strike,
                source_disclosure: crate::combat::SourceAbilityDisclosure::Public,
            },
        );
        let receipt = PublishedResolution {
            created_hold,
            event: self.event_log.len(),
            role,
        };
        self.commit_actor_outcome(transition)?;
        if forced {
            self.physics.displaced.insert(target_id);
        }
        if placement != original_placement {
            self.sync_actor_lifecycle(target_id)?;
        }
        // Target death/cancellation can remove source-held token claims.
        // Capture only after that publication, never overwrite the source
        // with an older snapshot. Full/dead Health has no banked work.
        let source_needs_reset = damaged
            && source_id != target_id
            && activity.first(source_id)
            && self
                .actors
                .get(&source_id)
                .and_then(|actor| actor.combat.as_ref())
                .is_some_and(|combat| {
                    let creature = combat.creature();
                    !creature.health().dead()
                        && creature.health().injury() > 0
                        && creature.derived().health_recovery_reset_percent > 0
                });
        if source_needs_reset {
            let mut source = self.settled_actor_outcome(source_id)?;
            if let Some(combat) = source.state.combat.as_mut() {
                combat.damage_activity();
            }
            self.commit_actor_outcome(source)?;
        }
        self.published_resolution(receipt)?;
        Ok(receipt)
    }

    /// The private publication receipt is the causal selector. Validate the
    /// exact final primary outcome, including rider participation, before any
    /// consumer uses its event index; ordinals alone are not event identities.
    pub(crate) fn published_primary_event(
        &self,
        receipt: PublishedResolution,
        outcome: &Outcome,
    ) -> Result<usize, GameError> {
        if !matches!(receipt.role, PublishedRole::Primary { .. })
            || self.published_resolution(receipt)? != outcome
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        Ok(receipt.event)
    }

    fn published_resolution(&self, receipt: PublishedResolution) -> Result<&Outcome, GameError> {
        let Some(CombatEvent::Resolved {
            outcome, strike, ..
        }) = self.event_log.get(receipt.event)
        else {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        };
        let valid_role = match receipt.role {
            PublishedRole::Standalone => strike.is_none() && !outcome.target_rider_participated,
            PublishedRole::Primary { strike: ordinal } => {
                *strike == Some(ordinal) && outcome.ability.is_some()
            }
            PublishedRole::Rider { .. } => {
                strike.is_none() && outcome.ability.is_none() && !outcome.target_rider_participated
            }
        };
        if valid_role && outcome.valid() {
            Ok(outcome)
        } else {
            Err(GameError::Rule(RuleError::InvalidPlan))
        }
    }

    fn publication_role(
        &self,
        outcome: &Outcome,
        site: ResolutionPublication,
    ) -> Result<PublishedRole, GameError> {
        let invalid = || GameError::Rule(RuleError::InvalidPlan);
        match site {
            ResolutionPublication::Standalone => Ok(PublishedRole::Standalone),
            ResolutionPublication::Primary { strike }
                if usize::from(strike) < crate::limits::STRIKES && outcome.ability.is_some() =>
            {
                Ok(PublishedRole::Primary { strike })
            }
            ResolutionPublication::Rider {
                primary,
                strike,
                ordinal,
            } => {
                if primary.role != (PublishedRole::Primary { strike })
                    || usize::from(ordinal) >= crate::limits::RIDERS_PER_STRIKE
                    || outcome.ability.is_some()
                {
                    return Err(invalid());
                }
                let parent = self.published_resolution(primary)?;
                if (parent.actor, parent.target, parent.intention)
                    != (outcome.actor, outcome.target, outcome.intention)
                    || matches!(
                        parent.participation.payload,
                        crate::abilities::PayloadParticipation::Unselected { .. }
                    )
                {
                    return Err(invalid());
                }
                Ok(PublishedRole::Rider {
                    primary: primary.event,
                    strike,
                    ordinal,
                })
            }
            ResolutionPublication::Primary { .. } => Err(invalid()),
        }
    }

    /// Only a committed rider can license its own strike's existing public event.
    /// This never changes the primary check, result, effects, or event ordering.
    pub(crate) fn merge_committed_rider_participation(
        &mut self,
        primary: PublishedResolution,
        rider: PublishedResolution,
    ) -> Result<bool, GameError> {
        if self.event_is_sealed(primary.event) {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let PublishedRole::Primary { strike } = primary.role else {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        };
        if !matches!(rider.role, PublishedRole::Rider { primary: parent, strike: rider_strike, .. } if parent == primary.event && rider_strike == strike)
            || rider.event <= primary.event
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let parent = self.published_resolution(primary)?;
        let child = self.published_resolution(rider)?;
        if (parent.actor, parent.target, parent.intention)
            != (child.actor, child.target, child.intention)
            || matches!(
                parent.participation.payload,
                crate::abilities::PayloadParticipation::Unselected { .. }
            )
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let participated =
            parent.target_rider_participated || child.participation.target_participated();
        let Some(CombatEvent::Resolved { outcome, .. }) = self.event_log.get_mut(primary.event)
        else {
            unreachable!("validated primary receipt")
        };
        outcome.target_rider_participated = participated;
        self.bind_event_participation(primary.event)?;
        Ok(participated)
    }

    fn commit_actor_outcome(&mut self, outcome: ActorOutcome) -> Result<(), GameError> {
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_outcome(prepared)
    }

    /// Finalize the owned actor and validate death allocation without mutation.
    /// Preview and execution see the same post-equipment-removal creature state.
    pub(crate) fn prepare_actor_outcome(
        &self,
        mut outcome: ActorOutcome,
    ) -> Result<PreparedActorOutcome, GameError> {
        let before = self
            .actors
            .get(&outcome.id)
            .ok_or(GameError::UnknownActor)?;
        let mut rebuild = crate::creatures::RebuildOutcome::default();
        if let (Some(after), Some(prior)) = (outcome.state.combat.as_mut(), before.combat.as_ref())
        {
            rebuild = self.retry_growth_with_recovery_context(
                outcome.id,
                crate::abilities::Placement {
                    location: outcome.state.location,
                    orientation: outcome.state.orientation,
                },
                after,
                prior,
            )?;
        }
        // A supported species rebuild settles old active time first. Its new
        // explicit memory policy can shorten an engagement, never resurrect one
        // already removed by the old policy or reset the actor's random stream.
        outcome
            .state
            .behavior
            .validate_deadline(outcome.state.behavior_policy())?;
        if before.behavior_policy() != outcome.state.behavior_policy() {
            outcome.state.behavior.engagement =
                outcome.state.engagement_at(self.actor_clock(outcome.id));
        }
        let died = before.alive() && !outcome.state.alive();
        if let Some(horizon) = outcome
            .state
            .combat
            .as_ref()
            .and_then(|combat| combat.creature().condition_horizon())
        {
            outcome
                .state
                .active_since
                .checked_add(horizon)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        rebuild.died |= died;
        let invalid_preparation = outcome.state.pending.as_ref().is_some_and(|pending| {
            outcome.state.combat.as_ref().is_some_and(|combat| {
                pending
                    .hold
                    .is_some_and(|owner| combat.creature().costs().reservation(owner).is_none())
                    || matches!(&pending.plan,crate::abilities::Plan::Respec(plan) if plan.basis.build_epoch!=combat.creature().build_epoch())
                    || pending.plan.ability_id().is_some_and(|ability| {
                        !combat
                            .creature()
                            .derived()
                            .abilities
                            .contains(ability)
                    })
            })
        });
        if invalid_preparation {
            if let Some(owner) = outcome
                .state
                .pending
                .as_ref()
                .and_then(|pending| pending.hold)
            {
                if !rebuild.canceled.contains(&owner) {
                    rebuild.canceled.push(owner);
                }
            }
            outcome.interrupt(Interruption::Discard, self.actor_clock(outcome.id))?;
        }
        // An authored status pauses whatever valid work still exists. Earlier
        // damage, grant loss or quiet-gate invalidation can already have discarded
        // it; this never recreates work or adopts that cause's discard policy.
        if outcome.state.pending.as_ref().is_some_and(|pending| {
            outcome.state.combat.as_ref().is_some_and(|combat| {
                combat
                    .creature()
                    .condition_pauses_preparation(&pending.plan.work().action())
            })
        }) {
            let intention = outcome
                .state
                .pending
                .as_ref()
                .and_then(|pending| pending.intention);
            if outcome.pause(self.actor_clock(outcome.id)).is_some()
                && !outcome.events.iter().any(|event| {
                    matches!(event,
                    CombatEvent::Interrupted { actor, intention: prior }
                    if *actor == outcome.id && *prior == intention)
                })
            {
                outcome.events.push(CombatEvent::Interrupted {
                    actor: outcome.id,
                    intention,
                });
            }
        }
        // Validate every fallible death operation before publishing the actor.
        let death = if died {
            Some(self.death_outcome(&mut outcome)?)
        } else {
            None
        };
        if let Some(combat) = outcome.state.combat.as_ref() {
            let learned: Vec<_> = combat
                .creature()
                .derived()
                .abilities
                .iter()
                .filter(|ability| !outcome.state.recognized_abilities.contains(*ability))
                .cloned()
                .collect();
            if !learned.is_empty() {
                outcome.state.recognized_abilities.extend(learned);
            }
        }
        outcome
            .state
            .search_clock
            .rephase(outcome.state.perception().search_interval);
        self.reconcile_relocation_sources(&mut outcome)?;
        Ok(PreparedActorOutcome {
            outcome,
            death,
            rebuild,
        })
    }

    /// Publish exactly one already finalized actor under the existing transaction.
    pub(crate) fn publish_actor_outcome(
        &mut self,
        prepared: PreparedActorOutcome,
    ) -> Result<(), GameError> {
        let PreparedActorOutcome {
            mut outcome, death, ..
        } = prepared;
        let id = outcome.id;
        let from = self.actors[&id].location;
        let to = outcome.state.location;
        let ground_contact_changed = from != to
            || self.actors[&id].orientation != outcome.state.orientation
            || self.actors[&id].locomotion().hovers() != outcome.state.locomotion().hovers();
        let entry_cause = outcome.entry_cause;
        // Capture private prior knowledge before publishing the destination:
        // discovery caused by this entry cannot retroactively avoid its trap.
        let avoidable_traps = if entry_cause == crate::floor_activation::EntryCause::Movement
            && (from != to || self.actors[&id].orientation != outcome.state.orientation)
        {
            self.known_traps_at_placement(id, to, outcome.state.orientation)?
        } else {
            std::collections::BTreeSet::new()
        };
        let disclosure_origin = outcome
            .disclosure_origin
            .take()
            .or_else(|| (!outcome.events.is_empty()).then(|| self.capture_event_origin(id, id)));
        let perception = outcome.state.perception();
        outcome
            .state
            .guise
            .reconcile(&perception, from != to || !outcome.state.alive());
        if outcome.state.guise.needs_concealment_roll() {
            let source = perception
                .object_guise_check
                .ok_or(GameError::Rule(RuleError::InvalidDefinition))?;
            let creature = outcome
                .state
                .combat
                .as_ref()
                .ok_or(GameError::UnknownActor)?
                .creature();
            let check = crate::discovery::FrozenDiscoveryCheck::capture_skill_for(
                &creature,
                id,
                source.skill,
                source.edge,
            )?;
            let total = check.roll(&mut self.rules_rng, false);
            outcome.state.guise.capture_concealment(total);
        }
        outcome
            .state
            .guise
            .retire_claim(outcome.state.pending.as_ref());
        self.reconcile_readied_ambush(&mut outcome)?;
        let previous_items = self.actors[&id]
            .pending
            .as_ref()
            .filter(|previous| !previous.item_payment.refs().is_empty())
            .filter(|previous| {
                outcome.state.pending.as_ref().is_none_or(|next| {
                    previous.item_lineage() != next.item_lineage()
                        || previous.item_payment != next.item_payment
                })
            })
            .cloned();
        if let Some(previous) = previous_items {
            if outcome.spent_items == previous.item_lineage() {
                if outcome.state.pending.as_ref().is_some_and(|next| {
                    next.item_lineage() != previous.item_lineage()
                        || !matches!(next.item_payment, crate::supplies::ItemPayment::Settled)
                }) {
                    return Err(GameError::ItemUnavailable);
                }
            } else {
                self.release_item_payment(&previous)?;
            }
        } else if outcome.spent_items.is_some() {
            return Err(GameError::ItemUnavailable);
        }
        let consumed = self.actors[&id]
            .pending
            .as_ref()
            // No claims means no retirement work, regardless of plan changes.
            .filter(|previous| previous.plan.tokens().len() > 0)
            .filter(|previous| {
                outcome.state.pending.as_ref().is_none_or(|next| {
                    #[cfg(test)]
                    TOKEN_PLAN_COMPARISONS.with(|count| count.set(count.get() + 1));
                    previous.plan != next.plan
                })
            })
            .map(|previous| previous.plan.tokens().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if self.survival_diagnostics.is_some() {
            outcome.survival.boundary(&self.actors[&id], &outcome.state);
            self.record_survival(id, &outcome.survival);
        }
        if self.actors[&id].locomotion().hovers() != outcome.state.locomotion().hovers() {
            outcome.state.motion.acceleration_remainder = [0; 3];
        }
        if outcome.state.motion.active_fall.is_some() && !self.fall_continues(id, &outcome.state) {
            outcome.state.motion.active_fall = None;
        }
        self.publish_relocation_sources(&outcome)?;
        self.actors.insert(id, outcome.state);
        self.reconcile_holds_for_actor(id)?;
        self.reconcile_spatial_condition_anchors(id)?;
        self.suspend_condition_denied_continuation(id);
        // Completion, replacement, discard and death all retire the original
        // reservation. Suspended or resumed plans keep it. Tokens are already
        // spent, so this never refunds an edge or recreates a condition.
        for holder_id in consumed
            .iter()
            .map(|token| token.holder)
            .collect::<std::collections::BTreeSet<_>>()
        {
            let tokens: Vec<_> = consumed
                .iter()
                .filter(|token| token.holder == holder_id)
                .cloned()
                .collect();
            if let Some(mut holder) = self.actors.get_mut(&holder_id) {
                if let Some(combat) = holder.combat.as_mut() {
                    combat.consume_tokens(&tokens, id);
                }
            }
        }
        match outcome.input {
            InputBoundary::Preserve => {}
            InputBoundary::Human => {
                if !self.is_ai(id) {
                    self.combat
                        .input_boundaries
                        .insert(id, self.actor_input_eligibility(id));
                }
            }
            InputBoundary::Require => {
                self.combat
                    .input_boundaries
                    .insert(id, self.actor_input_eligibility(id));
            }
        }
        // Preserve membership and its historical cause even during denial.
        // Detached actors retain this bounded value until record admission can
        // validate the live condition book again.
        if self.combat.input_boundaries.contains_key(&id) {
            self.combat
                .input_boundaries
                .insert(id, self.actor_input_eligibility(id));
        }
        for event in outcome.events {
            self.push_event(event, disclosure_origin.clone())?;
        }
        if let Some(death) = death {
            self.combat.input_boundaries.remove(&id);
            for item in death.inventory {
                self.items
                    .edit(item, |item| {
                        item.location = death.body.location;
                        item.motion = death.body.motion.clone();
                        item.orientation = death.body.orientation;
                    })
                    .expect("carried item from current inventory index");
            }
            self.next_item_id = death.next_item_id;
            self.items.insert(death.corpse, death.body);
            self.on_actor_death(id)?;
            if let Some(award) = death.award {
                // Capture after death/claim retirement: never overwrite the
                // owner's current conditions or its paid frozen strike prefix.
                let mut owner = self.settled_actor_outcome(award.owner)?;
                owner
                    .state
                    .combat
                    .as_mut()
                    .ok_or(GameError::UnknownActor)?
                    .stage_xp_award(award.amount)
                    .map_err(GameError::Creature)?;
                self.commit_actor_outcome(owner)?;
            }
        }
        // Publish the entry that actually happened before a destination trap
        // can relocate the actor again. Nested entries own their own cause and
        // must not be followed by a stale outer population/retrieval witness.
        if self.floor_activation.is_some() && from.region != to.region {
            self.sync_actor_lifecycle(id)?;
            self.accepted_region_entry(id, from, to, entry_cause)?;
        }
        if ground_contact_changed {
            if self.actor_frozen(id) {
                // Trusted relocation can move a frozen actor. Old contact
                // identity must not follow it, and an active destination must
                // thaw before its new contact is evaluated.
                self.actors
                    .get_mut(&id)
                    .expect("published actor")
                    .trap_contacts = tor_world::Shared::new(crate::traps::TrapContacts::default());
                self.sync_actor_lifecycle(id)?;
            }
            self.reconcile_trap_contacts_after_entry(id, None, avoidable_traps)?;
        }
        Ok(())
    }

    fn death_outcome(&self, outcome: &mut ActorOutcome) -> Result<DeathOutcome, GameError> {
        let award = self.prepare_death_award(outcome)?;
        let actor = &mut outcome.state;
        actor.identification = None;
        let combat = actor.combat.as_mut().ok_or(GameError::UnknownActor)?;
        combat
            .set_equipment(Vec::new())
            .map_err(GameError::Creature)?;
        let next_item_id = self
            .next_item_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let mut spec = ItemSpec::ordinary(format!("{} corpse", combat.identity.name));
        spec.class = crate::ItemClass::Corpse;
        spec.properties
            .insert("actor".into(), outcome.id.0.to_string());
        spec.properties
            .insert("death_tick".into(), self.tick.to_string());
        if let Some(preparation) = actor.cancel_preparation() {
            if preparation.plan.ability_id().is_some() {
                outcome.events.push(CombatEvent::SequenceEnded {
                    actor: outcome.id,
                    intention: preparation.intention,
                    reason: crate::combat::SequenceEnd::Abandoned,
                });
            }
        }
        actor.equipment.clear();
        actor.motion.active_fall = None;
        let body = Item {
            relocation_elapsed: None,
            condition: crate::armor::ItemCondition::default(),
            charges: None,
            corpse: Some(crate::CorpseAppearance {
                actor: outcome.id,
                asset: actor.asset.clone(),
            }),
            holds: vec![],
            spec: tor_world::Shared::new(spec),
            quantity: 1,
            location: ItemLocation::Ground(actor.location),
            motion: std::mem::take(&mut actor.motion),
            orientation: actor.orientation,
        };
        outcome.events.push(CombatEvent::Died { actor: outcome.id });
        Ok(DeathOutcome {
            corpse: ItemId(self.next_item_id),
            next_item_id,
            body,
            inventory: self.items.at(ItemLocation::Carried(outcome.id)).collect(),
            award,
        })
    }

    fn prepare_death_award(&self, outcome: &ActorOutcome) -> Result<Option<DeathAward>, GameError> {
        let Some(cause) = &outcome.fatal_cause else {
            return Ok(None);
        };
        if cause.owner == outcome.id {
            return Ok(None);
        }
        let owner = self
            .actors
            .get(&cause.owner)
            .and_then(|actor| actor.combat.as_ref())
            .ok_or(GameError::UnknownActor)?
            .creature();
        let victim = self
            .actors
            .get(&outcome.id)
            .and_then(|actor| actor.combat.as_ref())
            .ok_or(GameError::UnknownActor)?
            .creature();
        let policy = &owner.build().ruleset().document().growth;
        let amount = policy.reward(
            victim.earned_progress().peak_threat(),
            owner.earned_progress().highest_earned_tier(),
        );
        let mut checked = owner.earned_progress().clone();
        checked
            .award(amount, owner.build().ledger().total_hd(), policy)
            .map_err(|error| {
                GameError::Creature(crate::creatures::CreatureStateError::Growth(error))
            })?;
        Ok(Some(DeathAward {
            owner: cause.owner,
            amount,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::CombatEvent;
    use crate::content_ids::{AbilityId, TemplateId};
    use crate::rules::{Modifier, ModifierOp, Scope};
    use crate::{Action, ActorId, CreatureIdentity, Game};

    fn game() -> Game {
        game_with_configuration(|_| {})
    }
    pub(super) fn game_with_configuration(
        configure: impl FnOnce(&mut crate::rules::RulesetDocument),
    ) -> Game {
        let mut document = crate::test_support::test_ruleset().document().clone();
        document.abilities.push(crate::test_creatures::paid_melee());
        configure(&mut document);
        let mut ability = document
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .clone();
        ability.id = AbilityId::new("borrowed_strike").unwrap();
        ability.name = "Borrowed strike".into();
        ability.basic_attack = false;
        ability.cost = None;
        document.abilities.push(ability.clone());
        document.conditions.push(crate::rules::ConditionDef {
            id: crate::content_ids::ConditionId::new("borrowed_grant").unwrap(),
            family: crate::content_ids::ConditionFamilyId::new("borrowed_grant").unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Borrowed grant".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: ability.id.clone(),
                },
            }],
            stacking: crate::rules::Stacking::Longest,
            source_scoped: true,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Oldest,
        });
        let mut template =
            crate::creatures::Template::new(TemplateId::new("borrowed_grant").unwrap(), 0);
        template.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ability.id,
            },
        });
        template.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("fixture_paid_melee").unwrap(),
            },
        });
        let species = crate::test_creatures::species();
        let species_id = species.id.clone();
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            crate::creatures::ScenarioCatalog {
                species: [(species_id.clone(), species)].into(),
                templates: [(template.id.clone(), template)].into(),
            },
        )
        .unwrap();
        let mut build = crate::creatures::CreatureBuild::new(
            content.clone(),
            species_id,
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 7)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        build
            .set_templates(vec![TemplateId::new("borrowed_grant").unwrap()])
            .unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        for x in [1, 2] {
            let id = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    std::num::NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                id,
                CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game
    }

    fn prepared_game_with_token(token: bool) -> Game {
        let mut game = game();
        if token {
            game.apply_condition(
                ActorId(1),
                &crate::content_ids::ConditionId::new("readied").unwrap(),
                ActorId(2),
                1000,
            )
            .unwrap();
        }
        let plan = game
            .ability_plan(
                ActorId(1),
                &AbilityId::new("borrowed_strike").unwrap(),
                ActorId(2),
            )
            .unwrap();
        assert_eq!(plan.tokens().len(), usize::from(token));
        game.start_work(plan, None).unwrap();
        game
    }

    #[test]
    fn token_free_outcomes_skip_retirement_plan_comparisons() {
        let prepared = prepared_game_with_token(false);
        let actor = ActorId(1);
        let mut comparisons = Vec::new();
        for change in ["unchanged", "suspended", "replaced", "completed"] {
            let mut game = prepared.clone();
            let mut outcome = game.actor_outcome(actor).unwrap();
            match change {
                "suspended" => {
                    outcome
                        .interrupt(super::Interruption::Suspend, game.actor_clock(actor))
                        .unwrap();
                }
                "replaced" => {
                    let crate::abilities::Plan::Ability(plan) =
                        &mut outcome.state.pending.as_mut().unwrap().plan
                    else {
                        panic!("expected an ability plan");
                    };
                    // A different frozen operand, with the same kind of work.
                    plan.strikes[0].check.as_mut().unwrap().edge += 1;
                    assert!(plan.valid(&game.content.rules));
                }
                "completed" => outcome.state.pending = None,
                _ => {}
            }
            let before = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get);
            game.apply_outcome(outcome).unwrap();
            let count = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get) - before;
            comparisons.push((change, count));
            assert!(game.token_reservations_valid(actor, false));
        }
        assert_eq!(
            comparisons,
            [
                ("unchanged", 0),
                ("suspended", 0),
                ("replaced", 0),
                ("completed", 0)
            ]
        );
    }

    #[test]
    fn reserved_token_outcomes_compare_plans_and_retire_only_replacements() {
        let mut game = prepared_game_with_token(true);
        let actor = ActorId(1);
        let original = game.preparation(actor).unwrap().plan.clone();
        let mut suspended = game.actor_outcome(actor).unwrap();
        suspended
            .interrupt(super::Interruption::Suspend, game.actor_clock(actor))
            .unwrap();
        let before = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get);
        game.apply_outcome(suspended).unwrap();
        assert_eq!(
            super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get) - before,
            1
        );
        assert_eq!(game.preparation(actor).unwrap().plan, original);
        assert!(!game.preparation(actor).unwrap().active);
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .conditions()
                .instances()
                .next()
                .unwrap()
                .reserved_by,
            Some(actor)
        );
        assert!(game.token_reservations_valid(actor, false));

        let replacement = game
            .ability_plan(
                actor,
                &AbilityId::new("borrowed_strike").unwrap(),
                ActorId(2),
            )
            .unwrap();
        assert_eq!(replacement.work(), original.work());
        assert_ne!(replacement, original);
        assert!(replacement.tokens().len() == 0);
        let mut replaced = game.actor_outcome(actor).unwrap();
        replaced.state.pending.as_mut().unwrap().plan = replacement.clone();
        let before = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get);
        game.apply_outcome(replaced).unwrap();
        assert_eq!(
            super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get) - before,
            1
        );
        assert_eq!(game.preparation(actor).unwrap().plan, replacement);
        assert!(game.creature(actor).unwrap().conditions().is_empty());
        assert!(game.token_reservations_valid(actor, false));
    }

    #[test]
    fn rebuild_losing_a_free_ability_cancels_its_captured_preparation() {
        let mut game = game();
        let actor = ActorId(1);
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("borrowed_strike").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert!(game.preparation(actor).unwrap().hold.is_none());
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        assert!(game.preparation(actor).is_none());
        assert!(matches!(
            game.combat_events().last(),
            Some(crate::combat::CombatEvent::Interrupted {
                actor: ActorId(1),
                ..
            })
        ));
    }

    #[test]
    fn expired_free_grant_interrupts_before_preparation_completion() {
        let mut game = game();
        let actor = ActorId(1);
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        game.apply_condition(
            actor,
            &crate::content_ids::ConditionId::new("borrowed_grant").unwrap(),
            ActorId(2),
            10,
        )
        .unwrap();
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("borrowed_strike").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert!(game.preparation(actor).is_some());
        game.tick = 10;
        game.settle_creature_time().unwrap();
        assert!(game.preparation(actor).is_none());
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Interrupted {
                        actor: ActorId(1),
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(!game
            .combat_events()
            .iter()
            .any(|event| matches!(event, CombatEvent::Resolved { .. })));
    }

    #[test]
    fn lethal_paid_work_emits_one_interruption_and_one_death() {
        let mut game = game();
        let actor = ActorId(1);
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: AbilityId::new("fixture_paid_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(2)),
                    supplies: Default::default(),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.preparation(actor).unwrap().hold, Some(intention));
        crate::test_creatures::injure(&mut game, actor, 100).unwrap();
        assert!(!game.alive(actor));
        assert!(game.preparation(actor).is_none());
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Interrupted {
                        actor: ActorId(1),
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(event, CombatEvent::Died { actor: ActorId(1) }))
                .count(),
            1
        );
    }

    #[test]
    fn rejected_condition_does_not_commit_elapsed_timers_or_events() {
        let mut game = game();
        let actor = ActorId(1);
        let fear = crate::content_ids::ConditionId::new("fear").unwrap();
        game.apply_condition(actor, &fear, ActorId(2), 100).unwrap();
        game.tick = 10;
        let before = game.clone();
        assert_eq!(
            game.apply_condition(actor, &fear, ActorId(999), 20),
            Err(crate::GameError::UnknownActor)
        );
        assert!(
            game == before,
            "rejected edit must preserve the complete game"
        );
    }

    fn participation_outcome(ability: bool) -> crate::abilities::Outcome {
        crate::abilities::Outcome {
            resistance: None,
            actor: ActorId(1),
            target: ActorId(2),
            ability: ability.then(|| AbilityId::new("basic_melee").unwrap()),
            intention: None,
            result: crate::abilities::OutcomeStatus::Unaffected,
            check: None,
            participation: crate::abilities::AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![crate::abilities::ResolvedEffect::Heal { amount: 0 }],
        }
    }
    #[test]
    fn public_plan_rider_review_receipts_bind_primary_strike_and_participants() {
        use super::{DamageActivityGuard, ResolutionPublication};
        let mut game = game();
        let mut activity = DamageActivityGuard::default();
        let mut source = participation_outcome(true);
        source.participation = crate::abilities::AttemptedParticipation::source();
        source.effects.clear();
        let first = game
            .apply_resolved_outcome(
                source.clone(),
                ResolutionPublication::Primary { strike: 0 },
                &mut activity,
            )
            .unwrap();
        let second = game
            .apply_resolved_outcome(
                source.clone(),
                ResolutionPublication::Primary { strike: 1 },
                &mut activity,
            )
            .unwrap();
        let repeated_ordinal = game
            .apply_resolved_outcome(
                source,
                ResolutionPublication::Primary { strike: 0 },
                &mut activity,
            )
            .unwrap();
        let rider = game
            .apply_resolved_outcome(
                participation_outcome(false),
                ResolutionPublication::Rider {
                    primary: first,
                    strike: 0,
                    ordinal: 0,
                },
                &mut activity,
            )
            .unwrap();
        for parent in [second, repeated_ordinal, rider] {
            let before = game.clone();
            assert!(game
                .merge_committed_rider_participation(parent, rider)
                .is_err());
            assert_eq!(game, before, "a crossed receipt cannot change any event");
        }
        for variant in 0..4 {
            let mut input = participation_outcome(false);
            let strike = if variant == 0 { 1 } else { 0 };
            match variant {
                1 => input.actor = ActorId(2),
                2 => input.target = ActorId(1),
                3 => input.intention = Some(crate::IntentionId(999)),
                _ => {}
            }
            let before = game.clone();
            assert!(game
                .apply_resolved_outcome(
                    input,
                    ResolutionPublication::Rider {
                        primary: first,
                        strike,
                        ordinal: 0
                    },
                    &mut activity
                )
                .is_err());
            assert_eq!(
                game, before,
                "invalid rider context rejects before publication"
            );
        }
        let before = game.published_resolution(first).unwrap().clone();
        assert!(game
            .merge_committed_rider_participation(first, rider)
            .unwrap());
        let mut expected = before;
        expected.target_rider_participated = true;
        assert_eq!(
            game.published_resolution(first).unwrap(),
            &expected,
            "only the summary scalar changes"
        );
        assert!(
            !game
                .published_resolution(second)
                .unwrap()
                .target_rider_participated
        );
        assert!(
            !game
                .published_resolution(repeated_ordinal)
                .unwrap()
                .target_rider_participated
        );
        assert!(game.published_resolution(rider).unwrap().ability.is_none());
    }
    #[test]
    fn public_plan_rider_review_blocked_primary_and_prefilled_summary_reject() {
        use super::{DamageActivityGuard, ResolutionPublication};
        use crate::abilities::{
            AttemptedParticipation, CheckParticipation, OutcomeStatus, PayloadParticipation,
            UnselectedReason,
        };
        let mut game = game();
        let mut activity = DamageActivityGuard::default();
        let mut prefilled = participation_outcome(true);
        prefilled.target_rider_participated = true;
        let before = game.clone();
        assert!(game.apply(prefilled).is_err());
        assert_eq!(game, before);
        for reason in [
            UnselectedReason::Dead,
            UnselectedReason::UnavailableTarget,
            UnselectedReason::Immune,
        ] {
            let mut blocked = participation_outcome(true);
            blocked.effects.clear();
            blocked.result = if reason == UnselectedReason::UnavailableTarget {
                OutcomeStatus::Miss
            } else {
                OutcomeStatus::Unaffected
            };
            blocked.participation = AttemptedParticipation {
                check: if reason == UnselectedReason::Immune {
                    CheckParticipation::TargetInvolved
                } else {
                    CheckParticipation::None
                },
                payload: PayloadParticipation::Unselected { reason },
            };
            let primary = game
                .apply_resolved_outcome(
                    blocked,
                    ResolutionPublication::Primary { strike: 0 },
                    &mut activity,
                )
                .unwrap();
            let before = game.clone();
            assert!(game
                .apply_resolved_outcome(
                    participation_outcome(false),
                    ResolutionPublication::Rider {
                        primary,
                        strike: 0,
                        ordinal: 0
                    },
                    &mut activity
                )
                .is_err());
            assert_eq!(
                game, before,
                "an early-blocked primary cannot publish a rider"
            );
        }
    }
}

#[cfg(test)]
mod growth_tests {
    use super::*;
    use crate::abilities::{OutcomeStatus, ResolvedEffect};
    use crate::earned_growth::{FatalOrigin, FutureGrowth, GrowthSource};
    fn game() -> Game {
        let mut game = super::tests::game_with_configuration(|rules| {
            rules.growth.reward_base = 100;
            rules.growth.reward_per_relative_tier = 0;
            rules.growth.minimum_reward = 100;
            rules.growth.maximum_reward = 100;
        });
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        game
    }
    fn fatal(owner: ActorId, target: ActorId) -> Outcome {
        Outcome {
            resistance: None,
            actor: owner,
            target,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            participation: crate::abilities::AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![ResolvedEffect::Damage { amount: 1_000_000 }],
        }
    }
    #[test]
    fn corpse_captures_appearance_before_the_actor_can_change() {
        let mut game = game();
        game.actors.get_mut(&ActorId(2)).unwrap().asset =
            Some("creature.humanoid.sentinel.red".into());
        game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
        game.actors.get_mut(&ActorId(2)).unwrap().asset =
            Some("creature.humanoid.sentinel.blue".into());
        let (corpse_id, corpse) = game
            .items
            .iter()
            .find(|(_, item)| item.spec.class == crate::ItemClass::Corpse)
            .map(|(id, item)| (*id, item))
            .unwrap();
        let value = serde_json::to_value(corpse).unwrap();
        assert_eq!(value["corpse"], serde_json::json!({
            "actor": 2, "asset": "creature.humanoid.sentinel.red"
        }), "a corpse needs its captured source identity and appearance, independent of its name or the current actor record");
        game.apply_transfer(corpse_id, corpse_id, 1, ItemLocation::Carried(ActorId(1)))
            .unwrap();
        let inventory = game.observe(ActorId(1)).unwrap().inventory;
        let carried = inventory.iter().find(|item| item.id == corpse_id).unwrap();
        assert_eq!(carried.corpse.as_ref().unwrap().actor, ActorId(2));
        assert_eq!(
            carried.corpse.as_ref().unwrap().asset.as_deref(),
            Some("creature.humanoid.sentinel.red")
        );
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(
            restored, game,
            "captured appearance survives current checkpoint encoding and restoration"
        );
    }
    fn growth_obstruction(build: &crate::creatures::CreatureBuild) -> Vec<crate::rules::Modifier> {
        let recovery = &build.ruleset().document().health_recovery;
        let current = build.ledger().total_hd();
        let base_period = recovery.period(current);
        let next = crate::HdCount(current.get().checked_add(1).unwrap());
        assert!(recovery.period(next) < base_period);
        // The current raw fold is -NUMERIC, then the recovery rule floors it to 1.
        // The next HD lowers the base and crosses the retained bounded-stat limit.
        vec![
            crate::test_creatures::add(
                crate::rules::Stat::HealthRecoveryPeriod,
                -crate::limits::NUMERIC,
            ),
            crate::test_creatures::add(
                crate::rules::Stat::HealthRecoveryPeriod,
                -i64::try_from(base_period).unwrap(),
            ),
        ]
    }
    fn equip_growth_obstruction(game: &mut Game, modifiers: Vec<crate::rules::Modifier>) {
        let mut owner = game.actors.get_mut(&ActorId(1)).unwrap();
        owner
            .combat
            .as_mut()
            .unwrap()
            .set_equipment(vec![crate::creatures::EquipmentSource {
                weapon_profile: None,
                item: crate::ItemId(1),
                socket: crate::EquipmentSlotId(0),
                attack: None,
                modifiers,
            }])
            .unwrap();
    }
    fn assert_fatal_award_retained_without_growth(game: &mut Game) {
        let owner = game.creature(ActorId(1)).unwrap();
        let build = owner.build().clone();
        let epoch = owner.build_epoch();
        let health = owner.health();
        game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
        assert!(!game.alive(ActorId(2)));
        let owner = game.creature(ActorId(1)).unwrap();
        assert_eq!(owner.build(), &build);
        assert_eq!(owner.build_epoch(), epoch);
        assert_eq!(owner.health(), health);
        assert_eq!(owner.earned_progress().earned_xp(), 100);
        assert_eq!(owner.earned_progress().held_xp(), 100);
        assert_eq!(owner.earned_progress().spent_xp(), 0);
        assert_eq!(owner.earned_progress().highest_earned_tier().get(), 2);
    }
    #[test]
    fn temporary_derived_limit_retains_fatal_xp_without_vetoing_death() {
        let mut game = game();
        let modifiers = growth_obstruction(game.creature(ActorId(1)).unwrap().build());
        equip_growth_obstruction(&mut game, modifiers);
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .derived()
                .health_recovery_period,
            1
        );
        assert_fatal_award_retained_without_growth(&mut game);
    }
    #[test]
    fn removing_a_temporary_growth_obstruction_applies_retained_xp_once() {
        let mut game = game();
        let original = game.creature(ActorId(1)).unwrap().build().clone();
        let modifiers = growth_obstruction(game.creature(ActorId(1)).unwrap().build());
        equip_growth_obstruction(&mut game, modifiers);
        assert_fatal_award_retained_without_growth(&mut game);
        let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
        outcome
            .state
            .combat
            .as_mut()
            .unwrap()
            .set_equipment(vec![])
            .unwrap();
        game.apply_outcome(outcome).unwrap();
        let mut expected = original;
        expected
            .add_hit_die(crate::progression::HdSource::Racial, 7)
            .unwrap();
        let owner = game.creature(ActorId(1)).unwrap();
        assert_eq!(owner.build(), &expected);
        assert_eq!(owner.earned_progress().held_xp(), 0);
        assert_eq!(owner.earned_progress().spent_xp(), 100);
        let epoch = owner.build_epoch();
        let outcome = game.actor_outcome(ActorId(1)).unwrap();
        game.apply_outcome(outcome).unwrap();
        assert_eq!(game.creature(ActorId(1)).unwrap().build_epoch(), epoch);
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .earned_progress()
                .spent_xp(),
            100
        );
    }
    #[test]
    fn condition_expiry_retries_eligible_retained_growth_only_at_authoritative_publication() {
        let mut game = game();
        let condition = crate::content_ids::ConditionId::new("readied").unwrap();
        game.apply_condition(ActorId(1), &condition, ActorId(1), 10)
            .unwrap();
        // A strictly restorable retained balance is deliberately pending: restore
        // does not silently spend it or invent a separate catch-up timer.
        let mut owner = game.actors.get_mut(&ActorId(1)).unwrap();
        let creature = owner.combat.as_ref().unwrap().creature().clone();
        let mut value =
            serde_json::to_value(crate::creatures::record::CreatureRecord::capture(&creature))
                .unwrap();
        value["earned_progress"]["earned_xp"] = serde_json::json!(100);
        value["earned_progress"]["held_xp"] = serde_json::json!(100);
        value["earned_progress"]["highest_earned_tier"] = serde_json::json!(2);
        let record: crate::creatures::record::CreatureRecord =
            serde_json::from_value(value).unwrap();
        let restored = record
            .restore(creature.build().content().clone(), vec![])
            .unwrap();
        let identity = owner.combat.as_ref().unwrap().identity();
        owner.combat = Some(crate::combat::CombatState::from_creature(
            identity, restored,
        ));
        drop(owner);
        game.tick = 10;
        let before_read = game.clone();
        let view = game.creature(ActorId(1)).unwrap();
        assert!(view.conditions().is_empty());
        assert_eq!(view.build().ledger().total_hd().get(), 1);
        assert_eq!(view.earned_progress().held_xp(), 100);
        assert_eq!(
            game, before_read,
            "settled presentation cannot publish an HD or spend XP"
        );
        game.settle_actor_time(ActorId(1)).unwrap();
        let owner = game.creature(ActorId(1)).unwrap();
        assert!(owner.conditions().is_empty());
        assert_eq!(owner.build().ledger().total_hd().get(), 2);
        assert_eq!(owner.earned_progress().held_xp(), 0);
        assert_eq!(owner.earned_progress().spent_xp(), 100);
        assert_eq!(owner.derived(), &owner.build().derive().unwrap());
        let settled = game.clone();
        game.settle_actor_time(ActorId(1)).unwrap();
        assert_eq!(
            game, settled,
            "the same expired condition cannot spend the threshold again"
        );
    }
    #[test]
    fn unchanged_damage_timer_and_read_paths_do_not_retry_growth_or_derive() {
        for pending in [false, true] {
            let mut game = game();
            game.apply_condition(
                ActorId(1),
                &crate::content_ids::ConditionId::new("readied").unwrap(),
                ActorId(1),
                1000,
            )
            .unwrap();
            if pending {
                let modifiers = growth_obstruction(game.creature(ActorId(1)).unwrap().build());
                equip_growth_obstruction(&mut game, modifiers);
                assert_fatal_award_retained_without_growth(&mut game);
            }
            let before = game.clone();
            let folds = crate::diagnostics::work_counts().health_derivations;
            for _ in 0..3 {
                let _view = game.inspect_creature(ActorId(1)).unwrap();
            }
            assert_eq!(game, before);
            game.apply(Outcome {
                resistance: None,
                actor: ActorId(1),
                target: ActorId(1),
                ability: None,
                intention: None,
                result: OutcomeStatus::Applied,
                check: None,
                participation: crate::abilities::AttemptedParticipation::target(),
                target_rider_participated: false,
                effects: vec![ResolvedEffect::Damage { amount: 1 }],
            })
            .unwrap();
            game.tick = 1;
            game.settle_actor_time(ActorId(1)).unwrap();
            let _view = game.inspect_creature(ActorId(1)).unwrap();
            assert_eq!(
                crate::diagnostics::work_counts().health_derivations,
                folds,
                "pending={pending}"
            );
            assert_eq!(
                game.creature(ActorId(1))
                    .unwrap()
                    .build()
                    .ledger()
                    .total_hd()
                    .get(),
                1
            );
            assert_eq!(
                game.creature(ActorId(1))
                    .unwrap()
                    .earned_progress()
                    .held_xp(),
                if pending { 100 } else { 0 }
            );
        }
    }
    #[test]
    fn temporary_override_conflict_retains_fatal_xp_without_vetoing_death() {
        let mut game = super::tests::game_with_configuration(|rules| {
            rules.growth.reward_base = 100;
            rules.growth.reward_per_relative_tier = 0;
            rules.growth.minimum_reward = 100;
            rules.growth.maximum_reward = 100;
            rules
                .classes
                .iter_mut()
                .find(|class| class.id.as_str() == "mage")
                .unwrap()
                .modifiers
                .push(crate::rules::Modifier {
                    scope: crate::rules::Scope::Always,
                    op: crate::rules::ModifierOp::Set {
                        stat: crate::rules::Stat::Health,
                        value: 20,
                    },
                });
        });
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        let class = crate::content_ids::ClassId::new("mage").unwrap();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Class {
                    class: class.clone(),
                },
            })
            .unwrap();
        game.rebuild_creature(ActorId(1), build.clone()).unwrap();
        build
            .add_hit_die(crate::progression::HdSource::Class(class), 7)
            .unwrap();
        assert_eq!(
            build.derive().unwrap().maximum_health,
            20,
            "the permanent next build is independently valid"
        );
        equip_growth_obstruction(
            &mut game,
            vec![crate::rules::Modifier {
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Set {
                    stat: crate::rules::Stat::Health,
                    value: 30,
                },
            }],
        );
        assert_eq!(
            game.creature(ActorId(1)).unwrap().derived().maximum_health,
            30
        );
        assert_fatal_award_retained_without_growth(&mut game);
    }
    #[test]
    fn recorded_only_owner_retains_fatal_xp_without_inventing_a_health_root() {
        let mut game = game();
        let original = game.creature(ActorId(1)).unwrap().build().clone();
        let mut build = crate::creatures::CreatureBuild::from_recorded(
            original.content().clone(),
            original.species_id().clone(),
            original.initial_attributes(),
            crate::progression::HdLedger::new(original.ledger().entries().to_vec()).unwrap(),
            original.binding(),
            original.advancement().clone(),
            original.template_ids().to_vec(),
        )
        .unwrap();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .build()
                .ledger()
                .health_root_seed(),
            None
        );
        assert_fatal_award_retained_without_growth(&mut game);
    }
    #[test]
    fn late_corpse_allocation_failure_rolls_back_fatal_damage_and_xp() {
        let mut game = game();
        game.next_item_id = u64::MAX;
        let before = game.clone();
        assert_eq!(
            game.apply(fatal(ActorId(1), ActorId(2))),
            Err(GameError::IdentityExhausted)
        );
        assert_eq!(game, before);
    }
    #[test]
    fn typed_posthumous_cause_retains_xp_without_resurrecting_its_dead_owner() {
        let mut game = game();
        game.apply(fatal(ActorId(1), ActorId(1))).unwrap();
        let dead = game.creature(ActorId(1)).unwrap().health();
        let hd = game
            .creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd();
        game.apply_resolved_outcome_with_origin(
            fatal(ActorId(1), ActorId(2)),
            ResolutionPublication::Standalone,
            &mut DamageActivityGuard::default(),
            Some(FatalOrigin::Condition {
                condition: crate::content_ids::ConditionId::new("readied").unwrap(),
                application: 1,
            }),
        )
        .unwrap();
        let owner = game.creature(ActorId(1)).unwrap();
        assert_eq!(owner.health(), dead);
        assert!(owner.health().dead());
        assert_eq!(owner.build().ledger().total_hd(), hd);
        assert_eq!(owner.earned_progress().held_xp(), 100);
        assert_eq!(owner.earned_progress().spent_xp(), 0);
    }
    #[test]
    fn xp_overflow_preflight_rolls_back_the_whole_death_transition() {
        for kind in [
            "selected",
            "disabled",
            "dead",
            "maximum",
            "exhausted_authored",
        ] {
            let mut game = game();
            let mut build = game.creature(ActorId(1)).unwrap().build().clone();
            match kind {
                "disabled" => build.set_future_growth(FutureGrowth::Disabled {}).unwrap(),
                "maximum" => {
                    for _ in 1..crate::limits::HIT_DICE {
                        build
                            .add_hit_die(crate::progression::HdSource::Racial, 7)
                            .unwrap();
                    }
                }
                "exhausted_authored" => {
                    let mut advancement = build.advancement().clone();
                    advancement.restoration = crate::creatures::RestorationPolicy::Authored(vec![
                        crate::creatures::PlannedAdvancement {
                            source: crate::progression::HdSource::Racial,
                            choices: build.choices()[0].clone(),
                        },
                    ]);
                    build = crate::creatures::CreatureBuild::from_recorded(
                        build.content().clone(),
                        build.species_id().clone(),
                        build.initial_attributes(),
                        build.ledger().clone(),
                        build.binding(),
                        advancement,
                        build.template_ids().to_vec(),
                    )
                    .unwrap();
                    build.set_future_growth(FutureGrowth::Authored {}).unwrap();
                }
                _ => {}
            }
            game.rebuild_creature(ActorId(1), build).unwrap();
            if kind == "dead" {
                game.apply(fatal(ActorId(1), ActorId(1))).unwrap();
            }
            let mut state = game.actors.get_mut(&ActorId(1)).unwrap();
            let creature = state.combat.as_ref().unwrap().creature().clone();
            let mut record =
                serde_json::to_value(crate::creatures::record::CreatureRecord::capture(&creature))
                    .unwrap();
            record["earned_progress"]["earned_xp"] = serde_json::json!(u64::MAX);
            record["earned_progress"]["held_xp"] = serde_json::json!(u64::MAX);
            let policy = &creature.build().ruleset().document().growth;
            assert_eq!(
                (policy.threshold_base, policy.threshold_per_tier),
                (100, 100)
            );
            let applied = creature.earned_progress().highest_applied_tier();
            assert_eq!(creature.build().ledger().total_hd(), applied);
            // Independently invert this fixture's 100*tier triangular prices.
            // MAX XP funds the largest end with end*(end-1) no greater than
            // applied*(applied-1) + 2*floor(MAX/100).
            let applied = u128::from(applied.get());
            let triangle = applied * (applied - 1) + 2 * u128::from(u64::MAX / 100);
            let funded = u64::try_from((1 + 4 * triangle).isqrt().div_ceil(2)).unwrap();
            record["earned_progress"]["highest_earned_tier"] = serde_json::json!(funded);
            let record: crate::creatures::record::CreatureRecord =
                serde_json::from_value(record).unwrap();
            let restored = record
                .restore(creature.build().content().clone(), vec![])
                .unwrap();
            let identity = state.combat.as_ref().unwrap().identity();
            state.combat = Some(crate::combat::CombatState::from_creature(
                identity, restored,
            ));
            drop(state);
            let before = game.clone();
            assert_eq!(
                game.apply(fatal(ActorId(1), ActorId(2))),
                Err(GameError::Creature(
                    crate::creatures::CreatureStateError::Growth(
                        crate::earned_growth::GrowthError::Arithmetic
                    )
                ))
            );
            assert_eq!(game, before, "{kind}");
        }
    }
    #[test]
    fn a_dead_target_cannot_reassign_credit_to_a_later_effect_owner() {
        let mut game = game();
        game.apply(fatal(ActorId(1), ActorId(2))).unwrap();
        let before = game.creature(ActorId(1)).unwrap().earned_progress().clone();
        game.apply(fatal(ActorId(2), ActorId(2))).unwrap();
        assert_eq!(
            game.creature(ActorId(1)).unwrap().earned_progress(),
            &before
        );
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .earned_progress()
                .earned_xp(),
            0
        );
    }

    fn obstructed_growth_selection_work(game: &Game) -> usize {
        let owner = ActorId(1);
        let basis = crate::player_advancement::AdvancementBasis {
            actor: owner,
            branch: "main".into(),
            stream: "owner".into(),
            reset: 0,
            build_epoch: game.creature(owner).unwrap().build_epoch(),
        };
        let before = game.clone();
        let folds = crate::diagnostics::work_counts().health_derivations;
        let preview = game
            .preview_player_growth_source(owner, &basis, &GrowthSource::Racial {})
            .unwrap();
        let work = crate::diagnostics::work_counts().health_derivations - folds;
        assert_eq!(game, &before, "preview must remain pure");
        assert_eq!(preview.build().ledger().total_hd().get(), 1);
        assert_eq!(preview.runtime().health_recovery.period(), 1);
        assert_eq!(
            preview.build().future_growth(),
            &FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            }
        );
        work
    }

    #[test]
    fn choosing_future_source_attempts_obstructed_growth_only_once_per_fold() {
        let mut pending = game();
        let mut build = pending.creature(ActorId(1)).unwrap().build().clone();
        build.set_future_growth(FutureGrowth::Disabled {}).unwrap();
        pending.rebuild_creature(ActorId(1), build).unwrap();
        let modifiers = growth_obstruction(pending.creature(ActorId(1)).unwrap().build());
        equip_growth_obstruction(&mut pending, modifiers);
        let no_xp = pending.clone();
        pending.apply(fatal(ActorId(1), ActorId(2))).unwrap();
        assert!(!pending.alive(ActorId(2)));
        assert_eq!(
            pending
                .creature(ActorId(1))
                .unwrap()
                .earned_progress()
                .held_xp(),
            100
        );
        assert_eq!(
            pending
                .creature(ActorId(1))
                .unwrap()
                .earned_progress()
                .spent_xp(),
            0
        );
        let baseline = obstructed_growth_selection_work(&no_xp);
        let actual = obstructed_growth_selection_work(&pending);
        assert_eq!(
            actual,
            baseline + 4,
            "one permanent HD2 fold and one rejected runtime HD2 fold, with no second attempt"
        );
    }

    fn obstructed_equipment_reconciliation_work(
        mut game: Game,
        modifiers: Vec<crate::rules::Modifier>,
    ) -> usize {
        let owner = ActorId(1);
        let before = game.creature(owner).unwrap().clone().into_owned();
        let mut outcome = game.actor_outcome(owner).unwrap();
        let folds = crate::diagnostics::work_counts().health_derivations;
        outcome
            .state
            .combat
            .as_mut()
            .unwrap()
            .set_equipment(vec![crate::creatures::EquipmentSource {
                weapon_profile: None,
                item: crate::ItemId(2),
                socket: crate::EquipmentSlotId(0),
                attack: None,
                modifiers,
            }])
            .unwrap();
        game.apply_outcome(outcome).unwrap();
        let work = crate::diagnostics::work_counts().health_derivations - folds;
        let after = game.creature(owner).unwrap();
        assert_eq!(after.build(), before.build());
        assert_eq!(after.build_epoch(), before.build_epoch());
        assert_eq!(after.health(), before.health());
        assert_eq!(after.earned_progress(), before.earned_progress());
        assert_eq!(after.build().ledger().total_hd().get(), 1);
        work
    }

    #[test]
    fn changed_equipment_attempts_obstructed_growth_only_once_per_fold() {
        let mut pending = game();
        let modifiers = growth_obstruction(pending.creature(ActorId(1)).unwrap().build());
        equip_growth_obstruction(&mut pending, modifiers.clone());
        let no_xp = pending.clone();
        assert_fatal_award_retained_without_growth(&mut pending);
        let baseline = obstructed_equipment_reconciliation_work(no_xp, modifiers.clone());
        let actual = obstructed_equipment_reconciliation_work(pending, modifiers);
        assert_eq!(
            actual,
            baseline + 4,
            "one permanent HD2 fold and one rejected runtime HD2 fold, with no second attempt"
        );
    }
    fn timed_growth_obstruction_game(obstruct: bool) -> (Game, crate::content_ids::ConditionId) {
        let modifiers = if obstruct {
            growth_obstruction(game().creature(ActorId(1)).unwrap().build())
        } else {
            vec![crate::test_creatures::add(
                crate::rules::Stat::HealthRecoveryPeriod,
                1,
            )]
        };
        let condition = crate::content_ids::ConditionId::new("growth_health_buffer").unwrap();
        let mut game = super::tests::game_with_configuration(|rules| {
            rules.growth.reward_base = 100;
            rules.growth.reward_per_relative_tier = 0;
            rules.growth.minimum_reward = 100;
            rules.growth.maximum_reward = 100;
            rules.conditions.push(crate::rules::ConditionDef {
                id: condition.clone(),
                family: crate::content_ids::ConditionFamilyId::new("growth_health_buffer").unwrap(),
                lifecycle: crate::rules::ConditionLifecycle::Timed,
                timing: tor_simulation::rules::ConditionTiming::expiring(),
                restrictions: vec![],
                preparation: tor_simulation::rules::ConditionPreparation::Preserve,
                name: "Temporary growth health buffer".into(),
                modifiers,
                stacking: crate::rules::Stacking::Longest,
                source_scoped: false,
                descriptors: vec![],
                capacity: crate::rules::Eviction::Oldest,
            });
        });
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        game.apply_condition(ActorId(1), &condition, ActorId(1), 10)
            .unwrap();
        (game, condition)
    }

    #[test]
    fn automatic_growth_retains_valid_timed_condition_then_retries_once_at_expiry() {
        let (mut game, condition) = timed_growth_obstruction_game(true);
        let owner = ActorId(1);
        let before = game.creature(owner).unwrap().into_owned();
        assert_eq!(before.derived().health_recovery_period, 1);
        assert_eq!(before.conditions().remaining(&condition, owner), Some(10));
        let mut expected = before.build().clone();
        expected
            .add_hit_die(
                crate::progression::HdSource::Racial,
                expected.ledger().health_root_seed().unwrap(),
            )
            .unwrap();
        assert!(
            expected.derive().unwrap().maximum_health
                > before.build().derive().unwrap().maximum_health,
            "the permanent next HD is independently valid and increases Health"
        );

        game.apply(fatal(owner, ActorId(2))).unwrap();
        assert!(
            !game.alive(ActorId(2)),
            "temporary obstruction cannot veto death"
        );
        let retained = game.creature(owner).unwrap();
        assert_eq!(
            retained.conditions(),
            before.conditions(),
            "automatic growth must not evict a valid timed condition to fit its next HD"
        );
        assert_eq!(retained.build(), before.build());
        assert_eq!(retained.build_epoch(), before.build_epoch());
        assert_eq!(retained.health(), before.health());
        assert_eq!(retained.costs(), before.costs());
        assert_eq!(retained.health_recovery(), before.health_recovery());
        assert_eq!(retained.earned_progress().earned_xp(), 100);
        assert_eq!(retained.earned_progress().held_xp(), 100);
        assert_eq!(retained.earned_progress().spent_xp(), 0);
        assert_eq!(retained.earned_progress().highest_earned_tier().get(), 2);
        drop(retained);

        game.tick = 9;
        let folds = crate::diagnostics::work_counts().health_derivations;
        game.settle_actor_time(owner).unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().health_derivations,
            folds,
            "unchanged timed source cannot retry the same growth obstruction"
        );
        assert_eq!(
            game.creature(owner)
                .unwrap()
                .conditions()
                .remaining(&condition, owner),
            Some(1)
        );
        game.tick = 10;
        let before_read = game.clone();
        let projected = game.creature(owner).unwrap();
        assert!(projected.conditions().is_empty());
        assert_eq!(projected.build().ledger().total_hd().get(), 1);
        assert_eq!(projected.earned_progress().held_xp(), 100);
        drop(projected);
        assert_eq!(
            game, before_read,
            "read-only expiry projection cannot publish growth"
        );
        game.settle_actor_time(owner).unwrap();
        let grown = game.creature(owner).unwrap();
        assert!(grown.conditions().is_empty());
        assert_eq!(grown.build(), &expected);
        assert_eq!(grown.earned_progress().earned_xp(), 100);
        assert_eq!(grown.earned_progress().spent_xp(), 100);
        assert_eq!(grown.earned_progress().held_xp(), 0);
        drop(grown);
        let once = game.clone();
        game.settle_actor_time(owner).unwrap();
        assert_eq!(game, once, "same expiry must not publish or spend twice");
    }

    #[test]
    fn compatible_timed_condition_does_not_defer_automatic_growth() {
        let (mut game, condition) = timed_growth_obstruction_game(false);
        let owner = ActorId(1);
        let conditions = game.creature(owner).unwrap().conditions().clone();
        game.apply(fatal(owner, ActorId(2))).unwrap();
        assert!(!game.alive(ActorId(2)));
        let grown = game.creature(owner).unwrap();
        assert_eq!(grown.conditions(), &conditions);
        assert_eq!(grown.conditions().remaining(&condition, owner), Some(10));
        assert_eq!(grown.build().ledger().total_hd().get(), 2);
        assert_eq!(grown.earned_progress().earned_xp(), 100);
        assert_eq!(grown.earned_progress().spent_xp(), 100);
        assert_eq!(grown.earned_progress().held_xp(), 0);
    }
    #[test]
    fn explicit_rebuild_keeps_its_existing_timed_condition_eviction_semantics() {
        let (mut game, condition) = timed_growth_obstruction_game(true);
        let owner = ActorId(1);
        let mut next = game.creature(owner).unwrap().build().clone();
        next.add_hit_die(
            crate::progression::HdSource::Racial,
            next.ledger().health_root_seed().unwrap(),
        )
        .unwrap();
        let outcome = game.rebuild_creature(owner, next.clone()).unwrap();
        let creature = game.creature(owner).unwrap();
        assert_eq!(creature.build(), &next);
        assert_eq!(creature.conditions().remaining(&condition, owner), None);
        assert!(
            outcome.conditions_cleared,
            "Game returns final explicit rebuild consequences"
        );
        assert!(!outcome.died);
        assert_eq!(creature.earned_progress().earned_xp(), 0);
    }

    #[test]
    fn standalone_domain_equipment_and_source_setters_still_apply_held_growth_immediately() {
        let mut pending = game();
        let modifiers = growth_obstruction(pending.creature(ActorId(1)).unwrap().build());
        equip_growth_obstruction(&mut pending, modifiers);
        assert_fatal_award_retained_without_growth(&mut pending);
        let mut creature = pending.creature(ActorId(1)).unwrap().into_owned();
        creature.set_equipment(vec![]).unwrap();
        assert_eq!(creature.build().ledger().total_hd().get(), 2);
        assert_eq!(creature.earned_progress().spent_xp(), 100);

        let mut creature = game().creature(ActorId(1)).unwrap().into_owned();
        let mut disabled = creature.build().clone();
        disabled
            .set_future_growth(FutureGrowth::Disabled {})
            .unwrap();
        creature.rebuild(disabled).unwrap();
        creature.award_xp(100).unwrap();
        assert_eq!(creature.build().ledger().total_hd().get(), 1);
        let mut selected = creature.build().clone();
        selected
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        creature.rebuild(selected).unwrap();
        assert_eq!(creature.build().ledger().total_hd().get(), 2);
        assert_eq!(creature.earned_progress().spent_xp(), 100);
    }

    #[test]
    fn automatic_growth_still_removes_a_genuinely_newly_immune_timed_condition() {
        let obstruction = growth_obstruction(game().creature(ActorId(1)).unwrap().build());
        let condition = crate::content_ids::ConditionId::new("growth_immune_buffer").unwrap();
        let class = crate::content_ids::ClassId::new("mage").unwrap();
        let mut game = super::tests::game_with_configuration(|rules| {
            rules.growth.reward_base = 100;
            rules.growth.reward_per_relative_tier = 0;
            rules.growth.minimum_reward = 100;
            rules.growth.maximum_reward = 100;
            let mut buffer = rules
                .conditions
                .iter()
                .find(|def| def.id.as_str() == "fear")
                .unwrap()
                .clone();
            assert!(!buffer.descriptors.is_empty());
            let modifiers: Vec<_> = buffer
                .descriptors
                .iter()
                .cloned()
                .map(|descriptor| crate::rules::Modifier {
                    scope: crate::rules::Scope::Always,
                    op: crate::rules::ModifierOp::Immunity {
                        selector: crate::rules::Selector::Descriptor { descriptor },
                    },
                })
                .collect();
            rules
                .classes
                .iter_mut()
                .find(|def| def.id == class)
                .unwrap()
                .modifiers
                .extend(modifiers);
            buffer.id = condition.clone();
            buffer.family =
                crate::content_ids::ConditionFamilyId::new("growth_immune_buffer").unwrap();
            buffer.lifecycle = crate::rules::ConditionLifecycle::Timed;
            buffer.modifiers = obstruction;
            rules.conditions.push(buffer);
        });
        let owner = ActorId(1);
        let mut build = game.creature(owner).unwrap().build().clone();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Class { class },
            })
            .unwrap();
        game.rebuild_creature(owner, build).unwrap();
        game.apply_condition(owner, &condition, owner, 10).unwrap();
        assert_eq!(
            game.creature(owner)
                .unwrap()
                .derived()
                .health_recovery_period,
            1
        );
        game.apply(fatal(owner, ActorId(2))).unwrap();
        let creature = game.creature(owner).unwrap();
        assert!(!game.alive(ActorId(2)));
        assert_eq!(creature.build().ledger().total_hd().get(), 2);
        assert_eq!(creature.earned_progress().spent_xp(), 100);
        assert_eq!(creature.earned_progress().held_xp(), 0);
        assert_eq!(creature.conditions().remaining(&condition, owner), None);
    }
}
