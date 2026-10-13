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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    #[serde(deserialize_with = "crate::public_plan::read_recognized_abilities")]
    recognized_abilities: Shared<BTreeSet<crate::content_ids::AbilityId>>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    public_plan: Option<Shared<crate::public_plan::PublicPlanObservation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset: Option<String>,
}
impl<'de> Deserialize<'de> for ActorRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            active_since: u64,
            anatomy: Shared<AnatomySpec>,
            equipment: BTreeMap<EquipmentSlotId, ItemId>,
            pending: Option<crate::combat::Preparation>,
            combat: Option<CombatantRecord>,
            body: Shared<BodySpec>,
            motion: MotionState,
            location: Location,
            orientation: u8,
            turn_ticks: NonZeroU64,
            ready_at: u64,
            visited: BTreeSet<RegionId>,
            knowledge: Shared<BTreeSet<String>>,
            #[serde(deserialize_with = "crate::public_plan::read_recognized_abilities")]
            recognized_abilities: Shared<BTreeSet<crate::content_ids::AbilityId>>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            public_plan: Option<Shared<crate::public_plan::PublicPlanObservation>>,
            #[serde(default)]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            active_since: fields.active_since,
            anatomy: fields.anatomy,
            equipment: fields.equipment,
            pending: fields.pending,
            combat: fields.combat,
            body: fields.body,
            motion: fields.motion,
            location: fields.location,
            orientation: fields.orientation,
            turn_ticks: fields.turn_ticks,
            ready_at: fields.ready_at,
            visited: fields.visited,
            knowledge: fields.knowledge,
            recognized_abilities: fields.recognized_abilities,
            public_plan: fields.public_plan,
            asset: fields.asset,
        })
    }
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
            recognized_abilities: actor.recognized_abilities.clone(),
            public_plan: actor.public_plan.clone(),
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
            recognized_abilities: self.recognized_abilities.clone(),
            public_plan: self.public_plan.clone(),
            asset: self.asset.clone(),
        })
    }
}
#[cfg(test)]
mod public_plan_record_tests {
    use super::*;
    #[test]
    fn observed_actor_record_requires_named_current_fields_from_real_bytes() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut game = crate::Game::two_room_in_stone(42, build.content().clone());
        let id = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: "observer".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        let record = ActorRecord::capture(game.actors.get(&id).unwrap());
        let bytes = serde_json::to_vec(&record).unwrap();
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(
            serde_json::from_slice::<ActorRecord>(&bytes).unwrap(),
            record
        );
        let fields = [
            "active_since",
            "anatomy",
            "equipment",
            "pending",
            "combat",
            "body",
            "motion",
            "location",
            "orientation",
            "turn_ticks",
            "ready_at",
            "visited",
            "knowledge",
            "recognized_abilities",
            "public_plan",
            "asset",
        ];
        let positional =
            serde_json::Value::Array(fields.iter().map(|key| value[*key].clone()).collect());
        assert!(
            serde_json::from_slice::<ActorRecord>(&serde_json::to_vec(&positional).unwrap())
                .is_err(),
            "current actor wrapper must reject positional aliases"
        );
        assert!(serde_json::from_value::<ActorRecord>(positional).is_err());
        for key in ["recognized_abilities", "public_plan"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_slice::<ActorRecord>(&serde_json::to_vec(&missing).unwrap())
                    .is_err()
            );
            assert!(serde_json::from_value::<ActorRecord>(missing).is_err());
        }
        let text = std::str::from_utf8(&bytes).unwrap();
        let duplicate = text.replace(
            "\"public_plan\":null",
            "\"public_plan\":null,\"public_plan\":null",
        );
        assert!(serde_json::from_slice::<ActorRecord>(duplicate.as_bytes()).is_err());
        let mut duplicate_knowledge = value;
        duplicate_knowledge["recognized_abilities"] =
            serde_json::json!(["basic_melee", "basic_melee"]);
        assert!(serde_json::from_slice::<ActorRecord>(
            &serde_json::to_vec(&duplicate_knowledge).unwrap()
        )
        .is_err());
    }
}
