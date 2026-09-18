use tor_world::{Direction, Location, Position, Region, RegionId};

use crate::{ActorId, Game, GameError, ItemId, ItemLocation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub id: ItemId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundItemView {
    pub id: ItemId,
    pub name: String,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPlace {
    pub id: RegionId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorView {
    pub id: ActorId,
    pub position: Position,
}

/// An observable exit, without the unexplored destination's identity or contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitView {
    pub position: Position,
    pub direction: Direction,
}

/// Disclosed facts for one actor, separate from the authoritative game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub actor: ActorId,
    pub tick: u64,
    pub location: Location,
    pub region: Region,
    pub ground_items: Vec<GroundItemView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub exits: Vec<ExitView>,
    pub known_places: Vec<KnownPlace>,
}

impl Game {
    /// A free read of perceived state. The initial policy reveals the actor's
    /// fully lit room on the current elevation, plus visited place names and
    /// their own inventory. Portal sight, occlusion, and remembered items follow
    /// in the perception milestone. This query does not change knowledge or time.
    pub fn observe(&self, id: ActorId) -> Result<Observation, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let visible = |location: Location| {
            location.region == actor.location.region
                && location.position.z == actor.location.position.z
        };
        let mut ground_items = Vec::new();
        let mut inventory = Vec::new();
        for (&item_id, item) in &self.items {
            match item.location {
                ItemLocation::Ground(location) if visible(location) => {
                    ground_items.push(GroundItemView {
                        id: item_id,
                        name: item.name.clone(),
                        position: location.position,
                    });
                }
                ItemLocation::Carried(owner) if owner == id => {
                    inventory.push(ItemView {
                        id: item_id,
                        name: item.name.clone(),
                    });
                }
                _ => {}
            }
        }
        Ok(Observation {
            actor: id,
            tick: self.tick,
            location: actor.location,
            region: self
                .world
                .region(actor.location.region)
                .expect("validated actor location")
                .clone(),
            ground_items,
            inventory,
            visible_actors: self
                .actors
                .iter()
                .filter(|(other_id, other)| **other_id != id && visible(other.location))
                .map(|(&id, other)| ActorView {
                    id,
                    position: other.location.position,
                })
                .collect(),
            exits: self
                .world
                .exits(actor.location.region)
                .filter(|exit| visible(exit.from))
                .map(|exit| ExitView {
                    position: exit.from.position,
                    direction: exit.direction,
                })
                .collect(),
            known_places: actor
                .visited
                .iter()
                .map(|&id| KnownPlace {
                    id,
                    name: self
                        .world
                        .region(id)
                        .expect("visited region exists")
                        .name
                        .clone(),
                })
                .collect(),
        })
    }
}
