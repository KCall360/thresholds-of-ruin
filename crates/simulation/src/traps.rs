//! Hidden floor-contact hazards. Contact identity is actor-owned and persisted;
//! observation and restoration never synthesize a new physical entry.
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tor_world::{ConcealedFeatureId, Direction, Shared, TrapKind};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct TrapContacts(Vec<u64>);
impl TrapContacts {
    pub(crate) fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.0.iter().copied()
    }
}
pub(crate) fn read_contacts<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<TrapContacts>, D::Error> {
    let values = crate::bounded::Bounded::<u64, 64>::deserialize(d)?.0;
    if values.iter().any(|id| *id == 0 || *id == u64::MAX)
        || !values.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(serde::de::Error::custom("invalid trap contacts"));
    }
    Ok(Shared::new(TrapContacts(values)))
}
/// Catalog validation shared by authored compilation and current-save restoration.
/// The payload selects a shared effect; no content identity is special-cased.
pub fn trap_kind_valid(rules: &crate::rules::Ruleset, kind: &TrapKind) -> bool {
    use crate::content_ids::{ConditionId, EquipmentSlot};
    use crate::rules::{
        ConditionLifecycle, ConditionPreparation, ConditionRestriction, ConditionTiming,
    };
    kind.valid()
        && match kind {
            TrapKind::Rust { slot, .. } => EquipmentSlot::new(slot)
                .ok()
                .is_some_and(|id| rules.equipment_slot(&id).is_some()),
            TrapKind::SleepingGas { condition, .. } => ConditionId::new(condition)
                .ok()
                .and_then(|id| rules.condition(&id))
                .is_some_and(|definition| {
                    definition.lifecycle == ConditionLifecycle::Timed
                        && matches!(definition.timing, ConditionTiming::ActorActive { .. })
                        && definition.preparation == ConditionPreparation::Pause
                        && definition
                            .restrictions
                            .contains(&ConditionRestriction::VoluntaryActions)
                }),
        }
}

impl Game {
    pub(crate) fn loaded_traps_valid(&self) -> bool {
        self.world.loaded_regions().all(|region| {
            self.world
                .traps_in_region(region)
                .all(|(_, trap)| trap_kind_valid(self.content.rules(), &trap.kind))
        })
    }
    /// `imposed_normal` is a real collision normal in the anchor region. Flight
    /// cancels gravity, not a physical collision caused by imposed momentum.
    pub(crate) fn reconcile_trap_contacts(
        &mut self,
        id: ActorId,
        imposed_normal: Option<[i64; 3]>,
    ) -> Result<(), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if !actor.alive() || self.actor_frozen(id) {
            return Ok(());
        }
        if !self.world.has_concealed_features() && actor.trap_contacts.ids().next().is_none() {
            return Ok(());
        }
        let flying = actor.locomotion().flying;
        let body_normal = imposed_normal.map(|normal| {
            tor_world::rotate_vector(tor_world::inverse_rotation(actor.orientation), normal)
        });
        let cells = self
            .body_cells(actor.location, actor.orientation, &actor.body)
            .ok_or(GameError::InvalidLocation)?;
        let mut contacts = BTreeSet::new();
        for (at, frame) in cells {
            let supported = self
                .world
                .physics_neighbor(at, Direction::Down)
                .is_some_and(|(below, _)| self.world.is_wall(below));
            let imposed = body_normal
                .is_some_and(|normal| tor_world::rotate_vector(frame, normal) == [0, 0, 1]);
            let velocity = tor_world::rotate_vector(frame, actor.motion.velocity);
            let displacement = tor_world::rotate_vector(frame, actor.motion.displacement);
            if supported && (imposed || (!flying && velocity[2] == 0 && displacement[2] == 0)) {
                if let Some(trap) = self.world.trap(at) {
                    contacts.insert(trap.id);
                }
            }
        }
        let previous: BTreeSet<_> = actor.trap_contacts.ids().collect();
        let fresh: Vec<_> = contacts.difference(&previous).copied().collect();
        self.actors
            .get_mut(&id)
            .expect("validated actor")
            .trap_contacts = Shared::new(TrapContacts(contacts.into_iter().collect()));
        for trap in fresh {
            let location = self
                .world
                .trap_location(trap)
                .ok_or(GameError::InvalidLocation)?;
            let trap = self
                .world
                .trap(location)
                .ok_or(GameError::InvalidLocation)?
                .clone();
            // Capture perceptibility before gas can deny the victim's senses.
            let mut observers = Vec::new();
            for (&observer, owner) in self.actors.iter() {
                if !owner.alive()
                    || self.actor_frozen(observer)
                    || !self.condition_allows_voluntary_input(observer)
                {
                    continue;
                }
                if observer == id
                    || (owner.perception().sight
                        && self
                            .scene_snapshot(observer)?
                            .1
                            .iter()
                            .any(|cell| cell.physical && cell.location == location))
                {
                    observers.push(observer);
                }
            }
            let effect = match trap.kind {
                TrapKind::Rust { slot, amount } => crate::rules::Effect::CorrodeArmor {
                    slot: crate::EquipmentSlot::new(slot)
                        .map_err(|_| GameError::InvalidLocation)?,
                    amount,
                },
                TrapKind::SleepingGas {
                    condition,
                    duration,
                } => crate::rules::Effect::ApplyCondition {
                    condition: crate::content_ids::ConditionId::new(condition)
                        .map_err(|_| GameError::InvalidLocation)?,
                    duration,
                },
            };
            self.apply_effects_in_transaction(id, &[effect])?;
            for observer in observers {
                self.reveal_feature_family(observer, ConcealedFeatureId::Trap(trap.id))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "traps_validation_tests.rs"]
mod validation_tests;
