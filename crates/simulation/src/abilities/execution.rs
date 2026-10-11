use super::*;
use crate::{Game, GameError};
impl Game {
    fn plan_actor_available(&self, plan: &Plan) -> bool {
        self.actors
            .get(&plan.actor())
            .is_some_and(|actor| actor.alive())
            && self.creature(plan.actor()).map_or_else(
                || {
                    matches!(plan, Plan::Equip { .. } | Plan::Unequip { .. })
                        || matches!(plan, Plan::Drink { effects, .. } if effects.is_empty())
                },
                |creature| !creature.health().dead(),
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
    pub(crate) fn plan_still_available_in_scene(
        &self,
        plan: &Plan,
        sight: &[tor_world::SightCell],
    ) -> bool {
        self.plan_available(plan, Some(sight))
    }
    pub(crate) fn plan_still_available(&self, plan: &Plan) -> bool {
        self.plan_available(plan, None)
    }
    pub(crate) fn resolve_plan(
        &mut self,
        plan: &Plan,
        intention: Option<crate::IntentionId>,
    ) -> Result<Outcome, GameError> {
        if !self.plan_still_available(plan) {
            return Err(GameError::Rule(RuleError::OutOfReach));
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
