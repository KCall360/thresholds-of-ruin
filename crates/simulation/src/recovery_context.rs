//! Perception-conditioned recovery uses the existing actor-active Health clock.
//! The watched-owner index and scenes are derived; only the current clock context
//! is recorded, so its denominator remains meaningful while an actor is frozen.
use crate::health_recovery::RecoveryContext;
use crate::{ActorId, Game, GameError};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct RecoveryWitness {
    actors: u64,
    world: tor_world::PerceptionSnapshot,
    hostility: BTreeMap<String, BTreeSet<String>>,
}
impl RecoveryWitness {
    fn matches(&self, game: &Game) -> bool {
        self.actors == game.actors.perception_revision()
            && self.world == game.world.perception_snapshot()
            && self.hostility == game.combat.hostility
    }
}

/// Derived, branch-local proof that the current world has been reconciled.
/// Its absence only adds work. Saved clock context never gets authority from it.
#[derive(Clone, Debug, Default)]
pub(crate) struct RecoveryContexts(tor_world::Shared<Option<RecoveryWitness>>);
impl PartialEq for RecoveryContexts {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for RecoveryContexts {}

impl Game {
    pub(crate) fn senses_hostile(&self, actor: ActorId) -> Result<bool, GameError> {
        let scene = self.streaming_scene(actor)?;
        Ok(!self.hostiles_in_scene(actor, &scene).is_empty())
    }

    pub(crate) fn senses_faction_hostile(&self, actor: ActorId) -> Result<bool, GameError> {
        // Safety knows personally provoked contacts, but never their hidden affiliation.
        self.senses_hostile(actor)
    }

    /// A frozen owner has captured clock context, not permission to inspect the
    /// current world. An existing ordinary actor cannot acquire that history.
    pub(crate) fn recovery_source_context(
        &self,
        actor: ActorId,
    ) -> Result<RecoveryContext, GameError> {
        if !self.actor_frozen(actor) {
            return self
                .senses_hostile(actor)
                .map(|sensed_hostile| RecoveryContext::Known { sensed_hostile });
        }
        let creature = self
            .actors
            .get(&actor)
            .and_then(|actor| actor.combat.as_ref())
            .ok_or(GameError::UnknownActor)?
            .creature();
        if !creature.derived().contextual_health_recovery() {
            return Err(GameError::ActorBusy);
        }
        Ok(creature.recovery_context())
    }

    pub(crate) fn recovery_context_at_placement(
        &self,
        actor: ActorId,
        placement: crate::abilities::Placement,
    ) -> Result<RecoveryContext, GameError> {
        if self.actor_frozen(actor) {
            return self.recovery_source_context(actor);
        }
        let (scene, sight) = self.scene_snapshot_at_placement(actor, placement)?;
        Ok(RecoveryContext::Known {
            sensed_hostile: !self.hostiles_in_projection(actor, &scene, &sight, Some(placement)).is_empty(),
        })
    }

    /// Only the final authoritative actor fold can grant the first contextual
    /// source through growth. The callback stays lazy for ordinary growth and
    /// retained contexts; a frozen owner must never perceive a live scene here.
    pub(crate) fn retry_growth_with_recovery_context(
        &self,
        id: ActorId,
        placement: crate::abilities::Placement,
        after: &mut crate::actor_creatures::CombatState,
        before: &crate::actor_creatures::CombatState,
    ) -> Result<crate::creatures::RebuildOutcome, GameError> {
        let mut context_error = None;
        after
            .retry_growth_after_fold(before, |base_period| {
                if self.actor_frozen(id) {
                    Ok(RecoveryContext::Pending { base_period })
                } else {
                    self.recovery_context_at_placement(id, placement)
                        .map_err(|error| {
                            context_error = Some(error);
                            crate::creatures::CreatureStateError::Inconsistent
                        })
                }
            })
            .map_err(|error| context_error.unwrap_or(GameError::Growth(error)))
    }

    /// Geometry is already admitted, but the freeze stamp and suspended timer
    /// index must still be intact. This explicit boundary bypasses the derived
    /// witness: a frozen owner can need resolution without any world mutation.
    pub(crate) fn resolve_frozen_recovery_context(&mut self, id: ActorId) -> Result<(), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let Some(combat) = actor.combat.as_ref() else {
            return Ok(());
        };
        if !self.actor_frozen(id)
            || !actor.alive()
            || !combat.creature().derived().contextual_health_recovery()
        {
            return Ok(());
        }
        let sensed = self.senses_hostile(id)?;
        if combat.creature().recovery_context()
            != (RecoveryContext::Known {
                sensed_hostile: sensed,
            })
        {
            self.actors
                .get_mut(&id)
                .expect("validated thaw owner")
                .combat
                .as_mut()
                .expect("validated thaw creature")
                .set_recovery_context(sensed);
        }
        Ok(())
    }

    /// Pure source-policy query: discovering whether a condition needs context
    /// must not perceive a placement that the effect is about to leave.
    pub(crate) fn condition_uses_recovery_context(
        &self,
        condition: &crate::content_ids::ConditionId,
    ) -> Result<bool, GameError> {
        let definition = self
            .content
            .rules
            .condition(condition)
            .ok_or(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ))?;
        Ok(self
            .content
            .rules
            .modifiers_use_recovery_context(&definition.modifiers))
    }

    pub(crate) fn condition_recovery_context(
        &self,
        recipient: ActorId,
        condition: &crate::content_ids::ConditionId,
    ) -> Result<Option<RecoveryContext>, GameError> {
        if self.condition_uses_recovery_context(condition)? {
            self.recovery_source_context(recipient).map(Some)
        } else {
            Ok(None)
        }
    }
    pub(crate) fn equipment_recovery_context(
        &self,
        recipient: ActorId,
        equipment: &[crate::creatures::EquipmentSource],
    ) -> Result<Option<RecoveryContext>, GameError> {
        if equipment.iter().any(|entry| {
            self.content
                .rules
                .modifiers_use_recovery_context(&entry.modifiers)
        }) {
            self.recovery_source_context(recipient).map(Some)
        } else {
            Ok(None)
        }
    }
    /// Public setup operations keep their original cheap path when no contextual
    /// owner can be affected and the mutation needs no independent rollback.
    /// Otherwise the existing candidate transaction owns world mutation, all
    /// settlements, and any defensive reconciliation error.
    pub(crate) fn with_recovery_transition<R>(
        &mut self,
        requires_candidate: bool,
        change: impl FnOnce(&mut Self) -> Result<R, GameError>,
    ) -> Result<R, GameError> {
        if !requires_candidate && self.actors.contextual_recovery_owners().next().is_none() {
            let result = change(self)?;
            self.retire_settled_falls();
            return Ok(result);
        }
        let mut candidate = self.clone();
        let result = change(&mut candidate)?;
        candidate.retire_settled_falls();
        candidate.reconcile_recovery_contexts()?;
        *self = candidate;
        Ok(result)
    }

    /// Called only at authoritative mutation/time boundaries. Unchanged policy
    /// never settles lazy time, and unrelated actors never request an observation.
    pub(crate) fn reconcile_recovery_contexts(&mut self) -> Result<(), GameError> {
        if self.actors.contextual_recovery_owners().next().is_none()
            || self
                .lifecycle
                .recovery_contexts
                .0
                .as_ref()
                .is_some_and(|witness| witness.matches(self))
        {
            return Ok(());
        }
        // Presence indexing includes cancelling contributions. Settlement can
        // expire/remove them, never introduce a previously unindexed owner.
        // Only the first settlement of each owner's pre-boundary interval can
        // change life/perception; a period-only rephase cannot create new work.
        let maximum_changes = self.actors.contextual_recovery_owners().count();
        for _ in 0..=maximum_changes {
            let owners: Vec<_> = self.actors.contextual_recovery_owners().collect();
            let mut changed = false;
            for id in owners {
                if self.actor_frozen(id) || !self.alive(id) {
                    continue;
                }
                let sensed = self.senses_hostile(id)?;
                let Some(combat) = self.actors[&id].combat.as_ref() else {
                    continue;
                };
                if combat.creature().recovery_context()
                    == (RecoveryContext::Known {
                        sensed_hostile: sensed,
                    })
                {
                    continue;
                }
                // Preserve all earned time under the old rate, then rephase once.
                let mut outcome = self.settled_actor_outcome(id)?;
                outcome
                    .state
                    .combat
                    .as_mut()
                    .expect("watched creature")
                    .set_recovery_context(sensed);
                let prepared = self.prepare_actor_outcome(outcome)?;
                self.publish_actor_outcome(prepared)?;
                changed = true;
            }
            // Settling an expiring condition can change life/perception. One
            // more pass sees that boundary; unchanged owners earn no extra time.
            if !changed {
                self.remember_recovery_contexts();
                return Ok(());
            }
        }
        Err(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))
    }

    /// Only after reconciliation or independent restore validation succeeds.
    /// Capture after settlement, because expiry may have changed life or owners.
    pub(crate) fn remember_recovery_contexts(&mut self) {
        self.lifecycle.recovery_contexts.0 = tor_world::Shared::new(Some(RecoveryWitness {
            actors: self.actors.perception_revision(),
            world: self.world.perception_snapshot(),
            hostility: self.combat.hostility.clone(),
        }));
    }

    /// An active recorded context is a consistency witness, not trusted knowledge.
    /// Frozen contexts instead remain captured until the legitimate thaw boundary.
    pub(crate) fn recovery_contexts_valid(&self) -> bool {
        self.actors.iter().all(|(id, actor)| {
            actor.combat.as_ref().is_none_or(|combat| {
                let creature = combat.creature();
                creature.recovery_context_valid_at(
                    self.actor_frozen(*id).then(|| self.actor_clock(*id)),
                    actor.active_since,
                ) && (self.actor_frozen(*id)
                    || !actor.alive()
                    || !creature.derived().contextual_health_recovery()
                    || self.senses_hostile(*id).is_ok_and(|sensed_hostile| {
                        creature.recovery_context() == RecoveryContext::Known { sensed_hostile }
                    }))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        attributes::{ManaBinding, Skill},
        content_ids::{ConditionFamilyId, ConditionId},
        creatures::{Content, CreatureBuild, ScenarioCatalog},
        progression::{HdLedger, HdSource},
        rules::{
            ConditionDef, ConditionLifecycle, Eviction, Modifier, ModifierOp, Ruleset, Scope,
            Stacking, Stat,
        },
        CreatureIdentity,
    };
    use std::{
        collections::{BTreeMap, BTreeSet},
        num::NonZeroU64,
    };
    use tor_world::{Extent, Location, Position, Region, RegionId, World};

    fn expiry_boundary_game() -> Game {
        let mut document = crate::test_creatures::ruleset().document().clone();
        let condition = ConditionId::new("borrowed_vitality").unwrap();
        document.conditions.push(ConditionDef {
            id: condition.clone(),
            family: ConditionFamilyId::new("borrowed_vitality").unwrap(),
            lifecycle: ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Borrowed vitality".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: 100,
                },
            }],
            stacking: Stacking::Longest,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
        let mut species = crate::test_creatures::species();
        species.modifiers.push(Modifier {
            scope: Scope::WhileNoSensedHostile,
            op: ModifierOp::Add {
                stat: Stat::HealthRecoveryPeriodPercent,
                value: -25,
            },
        });
        let species_id = species.id.clone();
        let content = Content::new(
            Ruleset::validate(document).unwrap(),
            ScenarioCatalog {
                species: BTreeMap::from([(species_id.clone(), species)]),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content.clone(),
            species_id,
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        let world = World::new(
            vec![Region {
                id: RegionId(1),
                name: "room".into(),
                bounds: Extent::new(12, 3, 1).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let mut game = Game::new(world, 42, content);
        for (x, faction) in [(1, "hero"), (9, "foe")] {
            let actor = game
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(20).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                CreatureIdentity {
                    name: faction.into(),
                    faction: faction.into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::from([
                ("hero".into(), BTreeSet::from(["foe".into()])),
                ("foe".into(), BTreeSet::from(["hero".into()])),
            ]),
        )
        .unwrap();
        let base = game.creature(ActorId(2)).unwrap().health().maximum();
        game.apply_condition(ActorId(2), &condition, ActorId(2), 10)
            .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(2), u32::try_from(base + 1).unwrap())
            .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(1), 3).unwrap();
        // Exercise the mutation helper at an unsettled active boundary. The
        // first owner can settle; the second expires and cannot allocate a corpse.
        game.tick = 10;
        game
    }

    #[test]
    fn contextual_recovery_late_settlement_failure_keeps_world_and_all_clocks_atomic() {
        let mut game = expiry_boundary_game();
        game.next_item_id = u64::MAX;
        let before = game.clone();
        assert_eq!(
            game.set_region_light(RegionId(1), false),
            Err(GameError::IdentityExhausted)
        );
        assert!(
            game == before,
            "a late reconciliation failure cannot retain a changed world or an earlier owner's rephase"
        );
    }

    #[test]
    fn contextual_recovery_nutrition_settlement_reconciles_expiry_death_atomically() {
        use crate::health_recovery::RecoveryNutrition;
        let mut rejected = expiry_boundary_game();
        rejected.next_item_id = u64::MAX;
        let before = rejected.clone();
        assert_eq!(
            rejected.set_recovery_nutrition(ActorId(2), RecoveryNutrition::Normal),
            Err(GameError::IdentityExhausted)
        );
        assert!(
            rejected == before,
            "failed corpse allocation must remain atomic"
        );

        let mut game = expiry_boundary_game();
        game.set_recovery_nutrition(ActorId(2), RecoveryNutrition::Normal)
            .unwrap();
        assert!(
            !game.alive(ActorId(2)),
            "the existing settlement path must discover expiry death"
        );
        let survivor = game.creature(ActorId(1)).unwrap();
        assert_eq!(
            (
                survivor.health().injury(),
                survivor.health_recovery().elapsed(),
                survivor.health_recovery().period()
            ),
            (3, 7, 900),
            "all affected clocks must reconcile before the public setter publishes"
        );
    }

    #[test]
    fn contextual_recovery_followup_stationary_owners_above_scene_capacity_do_not_rescan_on_wait() {
        // One tiny isolated region per owner keeps the counter regression bounded.
        // Build the private fixture in one pass instead of reconciling 513 partial
        // public configurations. Execution below uses the real action boundary.
        const OWNERS: u64 = 513;
        let mut species = crate::test_creatures::species();
        species.modifiers.push(Modifier {
            scope: Scope::WhileNoSensedHostile,
            op: ModifierOp::Add {
                stat: Stat::HealthRecoveryPeriodPercent,
                value: -25,
            },
        });
        let species_id = species.id.clone();
        let content = Content::new(
            crate::test_creatures::ruleset(),
            ScenarioCatalog {
                species: BTreeMap::from([(species_id.clone(), species)]),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content.clone(),
            species_id,
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        let world = World::new(
            (1..=OWNERS)
                .map(|id| Region {
                    id: RegionId(id),
                    name: format!("room {id}"),
                    bounds: Extent::new(1, 1, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let mut game = Game::new(world, 42, content);
        let prototype = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 0, y: 0, z: 0 },
                },
                NonZeroU64::new(1).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            prototype,
            CreatureIdentity {
                name: "owner".into(),
                faction: "owners".into(),
            },
            build,
        )
        .unwrap();
        let prototype = game.actors[&prototype].clone();
        game.actors = crate::actor_store::ActorStore::from_entries(
            (1..=OWNERS)
                .map(|id| {
                    let mut actor = prototype.clone();
                    actor.location.region = RegionId(id);
                    actor.visited = BTreeSet::from([RegionId(id)]);
                    (ActorId(id), actor)
                })
                .collect(),
        );
        game.next_actor_id = OWNERS + 1;
        assert_eq!(
            game.actors.contextual_recovery_owners().count(),
            OWNERS as usize
        );
        game.reconcile_recovery_contexts().unwrap();
        let before = crate::diagnostics::work_counts();
        for _ in 0..3 {
            game.act(game.next_actor().unwrap(), crate::Action::Wait)
                .unwrap();
        }
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.observations - before.observations, 0);
        assert_eq!(
            after.ordinary_scenes - before.ordinary_scenes,
            0,
            "unchanged recovery contexts must not churn the bounded scene cache"
        );
        assert_eq!(
            after.scene_cells_copied - before.scene_cells_copied,
            0,
            "cached policy must avoid even copying all unchanged owner scenes"
        );
        assert_eq!(
            after.actor_candidates - before.actor_candidates,
            0,
            "no unchanged owner should repeat a perceived-actor query"
        );
    }
}
