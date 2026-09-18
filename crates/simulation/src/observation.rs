use std::collections::BTreeSet;
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
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPlace {
    pub id: RegionId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorView {
    pub id: ActorId,
    pub location: Location,
}

/// An observable exit, without the unexplored destination's identity or contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitView {
    pub location: Location,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellView {
    pub location: Location,
    pub wall: bool,
    pub place_hint: bool,
}

/// Disclosed facts for one actor, separate from the authoritative game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub actor: ActorId,
    pub tick: u64,
    pub location: Location,
    pub region: Region,
    pub visible_cells: Vec<CellView>,
    pub ground_items: Vec<GroundItemView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub exits: Vec<ExitView>,
    pub known_places: Vec<KnownPlace>,
}

impl Game {
    /// A free read of current perception. New games use bounded cell-centre rays,
    /// including rotated portals. Legacy saves retain whole-room perception.
    /// Neither querying nor seeing through a portal counts as visiting a place.
    pub fn observe(&self, id: ActorId) -> Result<Observation, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let cells = if self.legacy_perception {
            let (width, depth, _) = self
                .world
                .region(actor.location.region)
                .expect("actor region")
                .bounds
                .dimensions();
            (0..width)
                .flat_map(|x| {
                    (0..depth).map(move |y| Location {
                        region: actor.location.region,
                        position: Position {
                            x,
                            y,
                            z: actor.location.position.z,
                        },
                    })
                })
                .collect::<BTreeSet<_>>()
        } else {
            if self.scene_rules {
                self.world
                    .scene(actor.location, actor.orientation, 8)
                    .into_iter()
                    .map(|cell| cell.location)
                    .collect()
            } else {
                self.world.visible_cells(actor.location, 4)
            }
        };
        let visible = |location: Location| cells.contains(&location);
        let mut ground_items = Vec::new();
        let mut inventory = Vec::new();
        for (&item_id, item) in &self.items {
            match item.location {
                ItemLocation::Ground(location) if visible(location) => {
                    ground_items.push(GroundItemView {
                        id: item_id,
                        name: item.name.clone(),
                        location,
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
            visible_cells: cells
                .iter()
                .map(|&location| CellView {
                    location,
                    wall: self.world.is_wall(location),
                    place_hint: self.world.has_place_hint(location),
                })
                .collect(),
            inventory,
            visible_actors: self
                .actors
                .iter()
                .filter(|(other_id, other)| **other_id != id && visible(other.location))
                .map(|(&id, other)| ActorView {
                    id,
                    location: other.location,
                })
                .collect(),
            exits: cells
                .iter()
                .flat_map(|&location| {
                    [
                        Direction::North,
                        Direction::East,
                        Direction::South,
                        Direction::West,
                        Direction::Up,
                        Direction::Down,
                    ]
                    .into_iter()
                    .filter_map(move |direction| self.world.passage(location, direction))
                })
                .filter(|exit| !self.world.is_wall(exit.from))
                .map(|exit| ExitView {
                    location: exit.from,
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

    /// Backend-resolved view occurrences. A location may be seen at several offsets.
    pub fn scene(&self, id: ActorId) -> Result<Vec<tor_world::SightCell>, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if self.legacy_perception {
            return Ok(self
                .observe(id)?
                .visible_cells
                .into_iter()
                .map(|cell| tor_world::SightCell {
                    location: cell.location,
                    offset: Position {
                        x: cell.location.position.x - actor.location.position.x,
                        y: cell.location.position.y - actor.location.position.y,
                        z: 0,
                    },
                    rotation: 0,
                    wall: cell.wall,
                })
                .collect());
        }
        Ok(self.world.scene(
            actor.location,
            actor.orientation,
            if self.scene_rules { 8 } else { 4 },
        ))
    }
}
