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
    event: usize,
    role: PublishedRole,
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
    pub(crate) spent_items: Option<crate::supplies::PreparationLineage>,
    events: Vec<CombatEvent>,
    input: InputBoundary,
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
                .as_ability()
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
    Equipment(ActorOutcome),
    Consumed { actor: ActorOutcome },
}

/// Valid only inside the caller's uninterrupted transaction or discarded preview.
/// Fallible actor/death preparation is finished; publication never repeats it.
/// This is not Clone/Serialize and cannot become persisted or client authority.
pub(crate) struct PreparedActorOutcome {
    outcome: ActorOutcome,
    death: Option<DeathOutcome>,
}
impl PreparedActorOutcome {
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
}

impl Game {
    pub(crate) fn actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let state = self.actors.get(&id).ok_or(GameError::UnknownActor)?.clone();
        Ok(ActorOutcome {
            id,
            state,
            spent_items: None,
            events: vec![],
            input: InputBoundary::Preserve,
        })
    }

    /// Read a settled actor snapshot without committing timers on a later error.
    pub(crate) fn settled_actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let mut outcome = self.actor_outcome(id)?;
        let state = &mut outcome.state;
        let clock = self.actor_clock(id);
        let elapsed = clock.saturating_sub(state.active_since);
        if elapsed > 0 {
            if let Some(combat) = state.combat.as_mut() {
                combat.advance_active(elapsed);
            }
            state.active_since = clock;
            #[cfg(test)]
            crate::creature_time::count_settlement();
        }
        Ok(outcome)
    }

    /// External effect application owns one transaction; internal lifecycle callers
    /// use the already existing action/physics transaction and actor-local snapshots.
    pub fn apply(&mut self, outcome: Outcome) -> Result<(), GameError> {
        let mut candidate = self.clone();
        candidate.apply_outcome(outcome)?;
        *self = candidate;
        Ok(())
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
                self.event_log.push(CombatEvent::ItemCompleted {
                    actor: outcome.actor,
                    work: outcome.work,
                    intention: outcome.intention,
                });
                match outcome.change {
                    ItemChange::Equipment(actor) => self.commit_actor_outcome(actor)?,
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
        mut outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
    ) -> Result<PublishedResolution, GameError> {
        if outcome.result != crate::abilities::OutcomeStatus::Miss {
            outcome.result = outcome.canonical_result();
        }
        if outcome.target_rider_participated || !outcome.valid() {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let role = self.publication_role(&outcome, site)?;
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
        let mut transition = self.settled_actor_outcome(recipient_id)?;
        let mut damaged = false;
        let mut applied_count = 0;
        for index in 0..outcome.effects.len() {
            let effect = &outcome.effects[index];
            if matches!(effect, ResolvedEffect::ApplyCondition { .. })
                && !self.actors.contains_key(&outcome.actor)
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
            let Some(applied) = transition
                .state
                .combat
                .as_mut()
                .ok_or(GameError::Rule(RuleError::InvalidDefinition))?
                .apply_effect(outcome.actor, effect, &mut transition.state.public_plan)?
            else {
                continue;
            };
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
        // A resolution event precedes its interruption/death consequences.
        transition
            .events
            .insert(0, CombatEvent::Resolved { outcome, strike });
        let receipt = PublishedResolution {
            event: self.event_log.len(),
            role,
        };
        self.commit_actor_outcome(transition)?;
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

    fn published_resolution(&self, receipt: PublishedResolution) -> Result<&Outcome, GameError> {
        let Some(CombatEvent::Resolved { outcome, strike }) = self.event_log.get(receipt.event)
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
        let died = before.alive() && !outcome.state.alive();
        let invalid_preparation = outcome.state.pending.as_ref().is_some_and(|pending| {
            outcome.state.combat.as_ref().is_some_and(|combat| {
                pending
                    .hold
                    .is_some_and(|owner| combat.creature().costs().reservation(owner).is_none())
                    || matches!(&pending.plan,crate::abilities::Plan::Respec(plan) if plan.basis.build_epoch!=combat.creature().build_epoch())
                    || pending.plan.as_ability().is_some_and(|plan| {
                        !combat
                            .creature()
                            .derived()
                            .abilities
                            .contains(&plan.ability)
                    })
            })
        });
        if invalid_preparation {
            outcome.interrupt(Interruption::Discard, self.actor_clock(outcome.id))?;
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
        Ok(PreparedActorOutcome { outcome, death })
    }

    /// Publish exactly one already finalized actor under the existing transaction.
    pub(crate) fn publish_actor_outcome(
        &mut self,
        prepared: PreparedActorOutcome,
    ) -> Result<(), GameError> {
        let PreparedActorOutcome { outcome, death } = prepared;
        let id = outcome.id;
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
        self.actors.insert(id, outcome.state);
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
                    self.combat.input_boundaries.insert(id);
                }
            }
            InputBoundary::Require => {
                self.combat.input_boundaries.insert(id);
            }
        }
        self.event_log.extend(outcome.events);
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
        }
        Ok(())
    }

    fn death_outcome(&self, outcome: &mut ActorOutcome) -> Result<DeathOutcome, GameError> {
        let actor = &mut outcome.state;
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
            if preparation.plan.as_ability().is_some() {
                outcome.events.push(CombatEvent::SequenceEnded {
                    actor: outcome.id,
                    intention: preparation.intention,
                    reason: crate::combat::SequenceEnd::Abandoned,
                });
            }
        }
        actor.equipment.clear();
        let body = Item {
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
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::CombatEvent;
    use crate::content_ids::{AbilityId, TemplateId};
    use crate::rules::{Modifier, ModifierOp, Scope};
    use crate::{Action, ActorId, CreatureIdentity, Game};

    fn game() -> Game {
        let mut document = crate::test_support::test_ruleset().document().clone();
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
                ability: AbilityId::new("power_strike").unwrap(),
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
                target: ActorId(2),
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
                target: ActorId(2),
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
                    id: AbilityId::new("power_strike").unwrap(),
                    target: ActorId(2),
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
