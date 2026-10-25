//! First-body delivery borrows the existing ray plan ledger and publication path.
use super::{
    contact_policy::RayResolution,
    contact_trace::{ContactEnd, MAX_CONTACT_STATES},
    *,
};
use crate::{Game, GameError};
use tor_world::Direction;

impl Game {
    pub(super) fn resolve_contact_release(
        &mut self,
        ray: &RayPlan,
        intention: Option<crate::IntentionId>,
        preparation: Option<&crate::combat::Preparation>,
        disclosure: crate::event_history::SourceAbilityDisclosure,
    ) -> Result<(), GameError> {
        let invalid = || GameError::Rule(RuleError::InvalidPlan);
        let trace = self
            .trace_first_body(
                ray.actor,
                ray.origin.location,
                Direction::from(ray.direction).rotated(ray.origin.orientation),
                &ray.launch_footprint,
                MAX_CONTACT_STATES,
            )
            .map_err(|_| invalid())?;
        let target = match trace.end {
            ContactEnd::Body(target) => target,
            ContactEnd::NeedsRegions(_) => return Err(invalid()), // residency precedes payment
            ContactEnd::Obstruction | ContactEnd::Frontier | ContactEnd::Cycle => return Ok(()),
        };
        // The body intercepted regardless of whether it owns combat state.
        // Never search behind it or invent a defense/Health sheet.
        if self.creature(target).is_none() {
            return Ok(());
        }
        self.settle_actor_time(target)?; // frozen actor clocks remain frozen
        let diagnostic = preparation
            .and_then(|preparation| self.begin_combat_diagnostic(ray.actor, target, preparation));
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        match &ray.resolution {
            RayResolution::Checkless => {
                // A closed validated cancellation source is not an ordinary
                // descriptor-resisted damage payload. No check/token/RNG seam.
                if ray.effects != [PlannedEffect::CancelSpecials] {
                    return Err(invalid());
                }
                let outcome = Outcome {
                    actor: ray.actor,
                    target,
                    ability: Some(ray.ability.clone()),
                    intention,
                    result: OutcomeStatus::Applied,
                    check: None,
                    resistance: None,
                    participation: AttemptedParticipation::target(),
                    target_rider_participated: false,
                    effects: vec![ResolvedEffect::CancelSpecials { changed: false }],
                };
                // Acquire disclosure BEFORE removing guise/invisibility.
                let origin = self
                    .capture_event_origin(ray.actor, target)
                    .with_ability_disclosure(disclosure);
                let receipt = self.apply_resolved_with_disclosure(
                    outcome,
                    crate::lifecycle::ResolutionPublication::Primary { strike: 0 },
                    &mut activity,
                    origin,
                )?;
                let outcome = self.published_resolution(receipt)?.clone();
                self.finish_combat_diagnostic(diagnostic, outcome);
            }
            RayResolution::Opposed { .. } => {
                let mut rng = self.rules_rng;
                let threshold = if let Some(observer) = self
                    .combat_diagnostics
                    .as_mut()
                    .and_then(|diagnostics| diagnostics.active.as_mut())
                {
                    super::resolve::resolve_ray_launch(ray, &mut rng, observer)
                } else {
                    super::resolve::resolve_ray_launch(ray, &mut rng, &mut ())
                };
                self.rules_rng = rng;
                let mut budget = super::resolve::StrikeBudget::ray(ray);
                let outcome = self.resolve_ray_encounter(
                    ray,
                    target,
                    0,
                    threshold,
                    true,
                    intention,
                    disclosure,
                    &mut budget,
                    &mut activity,
                )?;
                self.finish_combat_diagnostic(diagnostic, outcome);
            }
        }
        Ok(())
    }
}
