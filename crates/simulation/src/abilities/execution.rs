use super::*;
use crate::{Game, GameError};
impl Game {
    pub(crate) fn plan_actor_available(&self, plan: &Plan) -> bool {
        self.actors
            .get(&plan.actor())
            .is_some_and(|actor| actor.alive())
            && self.creature(plan.actor()).map_or_else(
                || {
                    matches!(plan, Plan::Equip { .. } | Plan::Unequip { .. })
                        || matches!(plan, Plan::Drink { effects, .. } if effects.is_empty())
                },
                |creature| {
                    !creature.health().dead()
                        && plan
                            .as_ability()
                            .is_none_or(|plan| creature.derived().abilities.contains(&plan.ability))
                },
            )
    }
    fn plan_available(&self, plan: &Plan, sight: Option<&[tor_world::SightCell]>) -> bool {
        self.plan_actor_available(plan)
            && match plan {
                Plan::Ability(ability) => self.target_in_reach_snapshot(
                    ability.actor,
                    ability.target,
                    &ability.reach,
                    sight,
                ),
                Plan::Equip { .. } | Plan::Unequip { .. } | Plan::Drink { .. } => {
                    self.item_work_available(plan.actor(), &plan.work())
                        && (plan.on_success().is_empty()
                            || self.target_in_reach_snapshot(
                                plan.actor(),
                                plan.actor(),
                                &crate::rules::Reach::SelfOnly,
                                sight,
                            ))
                }
            }
    }
    /// A paid prefix owns its frozen tail even if its old target has left reach.
    /// Funding is progress-aware: the first strike settles the only cost hold.
    pub(crate) fn preparation_still_available(
        &self,
        preparation: &crate::combat::Preparation,
        sight: Option<&[tor_world::SightCell]>,
    ) -> bool {
        preparation.progress_valid()
            && self.preparation_items_valid(preparation, false)
            && (preparation.has_prefix()
                || preparation.plan.cost().is_none_or(|cost| {
                    preparation.hold.is_some_and(|owner| {
                        self.creature(preparation.plan.actor())
                            .is_some_and(|creature| creature.validate_cost(owner, cost).is_ok())
                    })
                }))
            && if preparation.has_prefix() {
                self.plan_actor_available(&preparation.plan)
            } else {
                self.plan_available(&preparation.plan, sight)
            }
    }
    /// Resolve one due strike and its ordered riders with one residual damage budget.
    /// The caller has already advanced progress, so self-effects see the new cursor.
    pub(crate) fn resolve_strike(
        &mut self,
        ability: &AbilityPlan,
        index: usize,
        intention: Option<crate::IntentionId>,
    ) -> Result<Outcome, GameError> {
        let strike = ability
            .strikes
            .get(index)
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        crate::diagnostics::strike_resolution();
        if !self.target_in_reach_snapshot(ability.actor, ability.target, &ability.reach, None) {
            let outcome = Outcome {
                actor: ability.actor,
                target: ability.target,
                ability: Some(ability.ability.clone()),
                intention,
                result: OutcomeStatus::Miss,
                check: None,
                effects: vec![],
            };
            // An unavailable/dead/detached target has no mutation or RNG draw.
            self.event_log.push(crate::combat::CombatEvent::Resolved {
                outcome: outcome.clone(),
                strike: Some(index as u16),
            });
            return Ok(outcome);
        }
        let mut budget = resolve::StrikeBudget::new(strike);
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        let primary = self.resolve_batch_in_transaction(
            ResolutionPlan::Strike { ability, strike },
            &mut budget,
            &mut activity,
            intention,
            Some(index as u16),
        )?;
        if let Some(branch) = primary.selected_branch {
            for (ordinal, rider) in strike.riders.iter().enumerate() {
                if rider.trigger != branch {
                    continue;
                }
                if !self.creature(ability.actor).is_some_and(|actor| {
                    !actor.health().dead() && actor.derived().abilities.contains(&ability.ability)
                }) || !self
                    .creature(ability.target)
                    .is_some_and(|target| !target.health().dead())
                {
                    break;
                }
                crate::diagnostics::rider_resolution();
                self.resolve_batch_in_transaction(
                    ResolutionPlan::Rider {
                        ability,
                        rider,
                        ordinal: ordinal as u16,
                    },
                    &mut budget,
                    &mut activity,
                    intention,
                    None,
                )?;
            }
        }
        Ok(primary.outcome)
    }
    fn resolve_batch_in_transaction(
        &mut self,
        plan: ResolutionPlan<'_>,
        budget: &mut resolve::StrikeBudget,
        activity: &mut crate::lifecycle::DamageActivityGuard,
        intention: Option<crate::IntentionId>,
        strike: Option<u16>,
    ) -> Result<resolve::BatchResolution, GameError> {
        let target = match plan {
            ResolutionPlan::Ability(ability)
            | ResolutionPlan::Strike { ability, .. }
            | ResolutionPlan::Rider { ability, .. } => ability.target,
            ResolutionPlan::Effects { target, .. } => target,
        };
        let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
        let snapshot = Snapshot::capture(&creature);
        let mut rng = self.rules_rng;
        let mut result = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
        {
            resolve::resolve_batch(plan, &snapshot, &mut rng, observer, budget)
        } else {
            resolve::resolve_batch(plan, &snapshot, &mut rng, &mut (), budget)
        };
        result.outcome.intention = intention;
        self.apply_resolved_outcome(result.outcome.clone(), strike, activity)?;
        self.rules_rng = rng;
        Ok(result)
    }
    /// The caller has validated and spent the exact finite source in this same
    /// transaction. The frozen effects do not depend on that deleted item.
    pub(crate) fn resolve_consumed_item(
        &mut self,
        preparation: &crate::combat::Preparation,
    ) -> Result<Outcome, GameError> {
        let plan = &preparation.plan;
        let intention = preparation.intention;
        if !matches!(plan, Plan::Drink { .. })
            || !matches!(
                preparation.item_payment,
                crate::supplies::ItemPayment::Settled
            )
            || !self.plan_actor_available(plan)
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let target = self
            .creature(plan.target())
            .ok_or(GameError::UnknownActor)?;
        let snapshot = Snapshot::capture(&target);
        let resolution = plan
            .resolution()
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        let mut rng = self.rules_rng;
        let mut outcome = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
        {
            resolve(resolution, &snapshot, &mut rng, observer)
        } else {
            resolve(resolution, &snapshot, &mut rng, &mut ())
        };
        outcome.intention = intention;
        self.apply_outcome(outcome.clone())?;
        self.rules_rng = rng;
        Ok(outcome)
    }
}
