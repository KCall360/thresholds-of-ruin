//! Exact scope-local Drain membership, independent of sight and room lighting.
use crate::{Game, GameError};
use std::collections::BTreeSet;
use tor_world::{Location, RegionId};

impl Game {
    pub(crate) fn drain_life_regions(&self, at: Location) -> Result<BTreeSet<RegionId>, GameError> {
        let scope = self
            .spatial_registry
            .scopes()
            .find(|scope| scope.members.binary_search(&at.region).is_ok())
            .ok_or(GameError::InvalidLocation)?;
        self.spatial_registry
            .lookup(&scope.id, at)
            .ok_or(GameError::InvalidLocation)?;
        Ok(scope.members.iter().copied().collect())
    }
    pub(crate) fn apply_drain_life(
        &mut self,
        outcome: &crate::abilities::Outcome,
        activity: &mut crate::lifecycle::DamageActivityGuard,
    ) -> Result<bool, GameError> {
        let source = outcome.actor;
        let at = self
            .actors
            .get(&source)
            .ok_or(GameError::UnknownActor)?
            .location;
        let health = self.health(source).ok_or(GameError::UnknownActor)?.0;
        if health < 2 {
            return Ok(false);
        }
        let scope = self
            .spatial_registry
            .scopes()
            .find(|scope| scope.members.binary_search(&at.region).is_ok())
            .ok_or(GameError::InvalidLocation)?;
        let membership = self
            .spatial_registry
            .lookup(&scope.id, at)
            .ok_or(GameError::InvalidLocation)?;
        // Required-region preparation must have loaded every structural member;
        // a partial census is never an empty/less-populated successful effect.
        if scope
            .members
            .iter()
            .any(|region| !self.loaded_regions().any(|loaded| loaded == *region))
        {
            return Err(GameError::InvalidLocation);
        }
        let targets: Vec<_> = self
            .actors
            .iter()
            .filter_map(|(id, actor)| {
                (*id != source
                    && actor.alive()
                    && actor.combat.is_some()
                    && self
                        .spatial_registry
                        .lookup(&scope.id, actor.location)
                        .is_some_and(|target| membership.includes_drain_target(target)))
                .then_some(*id)
            })
            .collect();
        if targets.is_empty() {
            return Ok(false);
        }
        let remaining = health / 2;
        let count = u64::try_from(targets.len()).map_err(|_| GameError::IdentityExhausted)?;
        let share = remaining / count;
        // Immutable census and arithmetic precede all death/growth side effects.
        for (target, loss) in std::iter::once((source, health - remaining))
            .chain(targets.into_iter().map(|target| (target, share)))
        {
            let damage = crate::abilities::Outcome {
                actor: source,
                target,
                ability: outcome.ability.clone(),
                intention: outcome.intention,
                result: crate::abilities::OutcomeStatus::Applied,
                check: None,
                resistance: None,
                participation: crate::abilities::AttemptedParticipation::target(),
                target_rider_participated: false,
                effects: vec![crate::abilities::ResolvedEffect::DirectHealthLoss { amount: loss }],
            };
            // Publication captures the original disclosure ceiling before contact
            // breaks a guise. No new visual identity is manufactured by the drain.
            if loss > 0 {
                self.apply_resolved_outcome(
                    damage,
                    crate::lifecycle::ResolutionPublication::Standalone,
                    activity,
                )?;
            }
            if target != source && self.alive(target) {
                self.release_control_hold(target)?;
                self.reveal_guise(target)?;
                if self.is_ai(target) {
                    let memory = self.actors[&target]
                        .behavior_policy()
                        .ok_or(GameError::UnknownActor)?
                        .memory_ticks;
                    if memory > 0 {
                        let expires = self
                            .actor_clock(target)
                            .checked_add(memory)
                            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
                        self.actors
                            .get_mut(&target)
                            .ok_or(GameError::UnknownActor)?
                            .awareness
                            .alert = Some(crate::awareness::LocationAlert {
                            location: at,
                            expires,
                        });
                    }
                }
            }
        }
        Ok(true)
    }
}
