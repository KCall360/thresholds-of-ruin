//! Catalog-independent actor DTOs. Decoding and restoring are separate boundaries.
use crate::actor_creatures::{CombatState, CreatureIdentity};
use crate::creatures::{record::CreatureRecord, Content};
use crate::{Actor, AnatomySpec, BodySpec, EquipmentSlotId, ItemId, MotionState};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
    sync::Arc,
};
use tor_world::{Location, RegionId, Shared};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActorRecord {
    active_since: u64,
    anatomy: Shared<AnatomySpec>,
    equipment: BTreeMap<EquipmentSlotId, ItemId>,
    pending: Option<crate::combat::Preparation>,
    combat: Option<CombatantRecord>,
    pub(crate) body: Shared<BodySpec>,
    motion: MotionState,
    location: Location,
    orientation: u8,
    turn_ticks: NonZeroU64,
    ready_at: u64,
    visited: BTreeSet<RegionId>,
    knowledge: Shared<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CombatantRecord {
    identity: CreatureIdentity,
    creature: CreatureRecord,
}
impl ActorRecord {
    pub(crate) fn capture(actor: &Actor) -> Self {
        Self {
            active_since: actor.active_since,
            anatomy: actor.anatomy.clone(),
            equipment: actor.equipment.clone(),
            pending: actor.pending.clone(),
            combat: actor.combat.as_ref().map(|combat| CombatantRecord {
                identity: combat.identity(),
                creature: CreatureRecord::capture(combat.creature()),
            }),
            body: actor.body.clone(),
            motion: actor.motion.clone(),
            location: actor.location,
            orientation: actor.orientation,
            turn_ticks: actor.turn_ticks,
            ready_at: actor.ready_at,
            visited: actor.visited.clone(),
            knowledge: actor.knowledge.clone(),
            asset: actor.asset.clone(),
        }
    }
    pub(crate) fn restore(
        &self,
        actor: crate::ActorId,
        content: Arc<Content>,
        items: &BTreeMap<ItemId, crate::Item>,
    ) -> Result<Actor, crate::checkpoint::PersistError> {
        let sources = self
            .equipment
            .iter()
            .map(|(socket, id)| {
                let equipment = items
                    .get(id)
                    .and_then(|item| item.spec.equipment.as_ref())
                    .ok_or(crate::checkpoint::PersistError::Equipment { actor, item: *id })?;
                Ok(crate::creatures::EquipmentSource {
                    item: *id,
                    socket: *socket,
                    modifiers: equipment.modifiers.clone(),
                    attack: equipment.attack.clone(),
                })
            })
            .collect::<Result<Vec<_>, crate::checkpoint::PersistError>>()?;
        Ok(Actor {
            active_since: self.active_since,
            anatomy: self.anatomy.clone(),
            equipment: self.equipment.clone(),
            pending: self.pending.clone(),
            combat: match &self.combat {
                None => None,
                Some(combat) => Some(CombatState::from_creature(
                    combat.identity.clone(),
                    combat.creature.restore(content, sources).map_err(|error| {
                        crate::checkpoint::PersistError::CreatureRecord { actor, error }
                    })?,
                )),
            },
            body: self.body.clone(),
            motion: self.motion.clone(),
            location: self.location,
            orientation: self.orientation,
            turn_ticks: self.turn_ticks,
            ready_at: self.ready_at,
            visited: self.visited.clone(),
            knowledge: self.knowledge.clone(),
            asset: self.asset.clone(),
        })
    }
}
