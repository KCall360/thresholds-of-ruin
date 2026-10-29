//! Candidate-local change facts for demand-driven observer publication.
//!
//! This is a conservative dependency proof, not another observation or a saved
//! cache. Borrowing both immutable boundaries prevents stale authority reuse.
use crate::{ActorId, Game, GameError};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationChange {
    Changed,
    Unchanged,
    NeedsProjection,
}

pub struct BoundaryEffects<'a> {
    before: &'a Game,
    after: &'a Game,
    changed_actors: BTreeSet<ActorId>,
    shared_observation_inputs_equal: bool,
}

impl Game {
    pub fn boundary_effects_since<'a>(&'a self, before: &'a Game) -> BoundaryEffects<'a> {
        let changed_actors = self
            .actors
            .keys()
            .chain(before.actors.keys())
            .copied()
            .filter(|id| !self.actors.shares_actor_payload(&before.actors, *id))
            .collect();
        BoundaryEffects {
            before,
            after: self,
            changed_actors,
            shared_observation_inputs_equal: std::sync::Arc::ptr_eq(&self.content, &before.content)
                && self.seed == before.seed
                && self.world.shares_storage(&before.world)
                && self.items.shares_storage(&before.items)
                && self
                    .spatial_registry
                    .shares_storage(&before.spatial_registry)
                && self.chart_history.shares_storage(&before.chart_history)
                && self.emitter_signature() == before.emitter_signature()
                && self.combat.run == before.combat.run
                && self.combat.selected == before.combat.selected
                && self.combat.characters == before.combat.characters
                && self.combat.hostility == before.combat.hostility
                && self.unique_retrieval == before.unique_retrieval,
        }
    }
}

impl BoundaryEffects<'_> {
    /// Payload detachment is only a conservative indication, never a visible
    /// change. A write that restores the same value may therefore fall back.
    pub fn changed_actors(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.changed_actors.iter().copied()
    }

    /// Compares the simulation's rich observation, ordinary scene, and charts.
    /// Readiness is owned by the server and must be compared separately.
    /// Bind this boundary's event receipts before constructing these facts.
    pub fn observation_change(&self, observer: ActorId) -> Result<ObservationChange, GameError> {
        let before = self.before;
        let after = self.after;
        let old = before
            .actors
            .get(&observer)
            .ok_or(GameError::UnknownActor)?;
        let current = after.actors.get(&observer).ok_or(GameError::UnknownActor)?;
        // Live charts can change outside ordinary sight and their preparation
        // can fail. Retain the canonical full path until explicitly classified.
        if before.monster_detection_active(observer) || after.monster_detection_active(observer) {
            return Ok(ObservationChange::NeedsProjection);
        }
        let result = if before.tick != after.tick || old.location != current.location {
            ObservationChange::Changed
        } else if !self.shared_observation_inputs_equal
            || self.changed_actors.contains(&observer)
            || before.actor_clock(observer) != after.actor_clock(observer)
            || !match (
                before.explored_maps.get(&observer),
                after.explored_maps.get(&observer),
            ) {
                (Some(old), Some(current)) => old.shares_storage(current),
                (None, None) => true,
                _ => false,
            }
            || before.physics.displaced.contains(&observer)
                != after.physics.displaced.contains(&observer)
            || before
                .physics
                .impacts
                .iter()
                .any(|e| e.entity == crate::PhysicsEntity::Actor(observer))
                != after
                    .physics
                    .impacts
                    .iter()
                    .any(|e| e.entity == crate::PhysicsEntity::Actor(observer))
            || before.observation_depends_on_any(observer, &self.changed_actors)?
            || after.observation_depends_on_any(observer, &self.changed_actors)?
            || before.disclosed_event_history(observer) != after.disclosed_event_history(observer)
            || item_events(before, observer) != item_events(after, observer)
        {
            ObservationChange::NeedsProjection
        } else {
            ObservationChange::Unchanged
        };
        if result != ObservationChange::NeedsProjection {
            // These are the fallible DTO-only checks omitted by light event
            // facts. Preserve rejection of malformed observer/item definitions
            // and hallucination arithmetic, including before-side failures.
            before.validate_observation_projection_inputs(observer)?;
            after.validate_observation_projection_inputs(observer)?;
        }
        Ok(result)
    }
}

#[derive(PartialEq, Eq)]
enum ItemEvent<'a> {
    Completed(&'a crate::Work),
    Refused(&'a crate::Work, crate::ItemRefusal),
    CurseBlocked(crate::ItemId),
}
fn item_events(game: &Game, observer: ActorId) -> Vec<ItemEvent<'_>> {
    use crate::combat::CombatEvent;
    game.event_log
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            if !game.owns_event_receipt(observer, index) {
                return None;
            }
            match event {
                CombatEvent::ItemCompleted { actor, work, .. } if *actor == observer => {
                    Some(ItemEvent::Completed(work))
                }
                CombatEvent::ItemRefused {
                    actor,
                    work,
                    reason,
                    ..
                } if *actor == observer => Some(ItemEvent::Refused(work, *reason)),
                CombatEvent::ItemCurseBlocked { actor, item, .. } if *actor == observer => {
                    Some(ItemEvent::CurseBlocked(*item))
                }
                _ => None,
            }
        })
        .collect()
}
