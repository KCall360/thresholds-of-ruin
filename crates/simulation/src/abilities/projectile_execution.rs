//! A collided physical attack shares the ordinary effect/check kernel. Source
//! rows are frozen; defenses and incoming modifiers belong to the live target.
use super::*;
use crate::conditions::TokenReservation;
use crate::creatures::CheckContext;
use crate::rules::{CheckActor, DefenseKind};
use crate::weapon_profiles::RangeClassification;
use crate::{ActorId, Game, GameError};
use std::collections::BTreeMap;

impl Game {
    pub(super) fn projectile_collision_candidates(
        &self,
        projectile: &ProjectilePlan,
        target: ActorId,
    ) -> Result<Vec<TokenReservation>, GameError> {
        let source = self
            .creature(projectile.actor)
            .ok_or(GameError::UnknownActor)?;
        Ok(source
            .conditions()
            .qualifying_tokens_for(
                &self.content.rules,
                projectile.actor,
                false,
                Some((target, &projectile.ability, CheckActor::User)),
            )
            .into_iter()
            .filter(|live| {
                projectile.conditional_tokens.iter().any(|captured| {
                    // Age order may compact after unrelated conditions expire. The
                    // persisted generation, key and qualification identify this
                    // exact instance; a reapplication can never join the capture.
                    live.holder == captured.holder
                        && live.condition == captured.condition
                        && live.source == captured.source
                        && live.generation == captured.generation
                        && live.qualification == captured.qualification
                })
            })
            .collect())
    }

    pub(super) fn resolve_projectile_encounter(
        &mut self,
        projectile: &ProjectilePlan,
        target: ActorId,
        distance_feet: u32,
        intention: Option<crate::IntentionId>,
    ) -> Result<Outcome, GameError> {
        let invalid = || GameError::Rule(RuleError::InvalidPlan);
        let range_penalty = match projectile.range.classify(distance_feet) {
            RangeClassification::Normal => 0,
            RangeClassification::Disadvantage => 1,
            RangeClassification::Unavailable => return Err(invalid()),
        };
        crate::diagnostics::strike_resolution();
        self.release_control_hold(target)?;
        let awareness = self.capture_attack_awareness(projectile.actor, target)?;
        let conditional = self.projectile_collision_candidates(projectile, target)?;
        let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
        let rules = &self.content.rules;
        let incoming = creature.conditions().qualifying_tokens(rules, target, true);
        let selected: Vec<_> = projectile
            .tokens
            .iter()
            .map(|claim| claim.reservation.clone())
            .chain(conditional.iter().cloned())
            .chain(incoming.iter().cloned())
            .collect();
        let definition = rules.ability(&projectile.ability).ok_or_else(invalid)?;
        let (edge, effect_edge) = creature
            .derived()
            .projectile_edges(
                &projectile.outgoing,
                rules,
                CheckContext {
                    ability: Some(&projectile.ability),
                    roller_role: CheckActor::User,
                    roller: projectile.actor,
                    opponent: target,
                    defense: Some(DefenseKind::Physical),
                },
                definition.outgoing_edge_policy,
                &selected,
            )
            .map_err(|_| invalid())?;
        let check = PlannedCheck {
            actor: CheckActor::User,
            target_participates: true,
            bonus: projectile.bonus,
            threshold: CheckThreshold::Fixed {
                value: i64::from(creature.derived().defenses.physical),
            },
            edge: edge
                .checked_sub(range_penalty + i64::from(projectile.threatened))
                .ok_or_else(invalid)?,
        };
        let plan = ResolutionPlan::ProjectileEncounter {
            projectile,
            target,
            check: &check,
            effect_edge,
        };
        let snapshot = Snapshot::capture(&creature).with_context(self.resolution_context(
            plan,
            projectile.actor,
            target,
        )?);
        let mut rng = self.rules_rng;
        let mut budget = resolve::StrikeBudget::projectile(effect_edge);
        let mut result = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|value| value.active.as_mut())
        {
            resolve::resolve_batch(plan, &snapshot, &mut rng, observer, &mut budget)
        } else {
            resolve::resolve_batch(plan, &snapshot, &mut rng, &mut (), &mut budget)
        }?;
        result.outcome.intention = intention;
        self.rules_rng = rng;
        if result.outcome.check.is_some() {
            let mut holders: BTreeMap<ActorId, Vec<TokenReservation>> = BTreeMap::new();
            for token in conditional.into_iter().chain(incoming) {
                holders.entry(token.holder).or_default().push(token);
            }
            for (holder, tokens) in holders {
                let mut update = self.actor_outcome(holder)?;
                let combat = update
                    .state
                    .combat
                    .as_mut()
                    .ok_or(GameError::UnknownActor)?;
                combat.reserve_tokens(&tokens, projectile.actor)?;
                combat.consume_tokens(&tokens, projectile.actor);
                self.apply_outcome(update)?;
            }
        }
        self.consume_resistance_tokens(&result.resistance_tokens)?;
        let origin = self.capture_event_origin(projectile.actor, target);
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        let receipt = self.apply_resolved_with_disclosure(
            result.outcome.clone(),
            crate::lifecycle::ResolutionPublication::Primary { strike: 0 },
            &mut activity,
            origin,
        )?;
        self.seal_participant_events()?;
        self.provoke_from_strike(&result.outcome, receipt, awareness)?;
        Ok(result.outcome)
    }
}
