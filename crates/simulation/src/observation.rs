use std::collections::BTreeSet;
use tor_world::{Direction, Location, Position, Region, RegionId};

use crate::{ActorId, Game, GameError, ItemId, ItemLocation};

/// Manhattan sight range in cells, measured from the eye cell.
const SIGHT_RANGE: u8 = 8;

/// Where the scene places the landing of the abstract stair the actor stands
/// on: straight up or down, just beyond any physically visible offset, so it
/// never collides with a real cell. Travel learns stair links from it.
pub(crate) fn stair_landing_offset(body: &crate::BodySpec, direction: Direction) -> Position {
    let [x, y, z] = body.eye;
    let beyond = i32::from(SIGHT_RANGE) + x.abs() + y.abs() + z.abs() + 1;
    Position {
        x: 0,
        y: 0,
        z: if direction == Direction::Down {
            -beyond
        } else {
            beyond
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundItemView {
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPlace {
    pub id: RegionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorView {
    pub name: String,
    pub description: &'static str,
    pub id: ActorId,
    pub location: Location,
}

/// An observable exit, without the unexplored destination's identity or contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitView {
    pub location: Location,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellView {
    pub frame: u8,
    pub door: Option<tor_world::Door>,
    pub door_reachable: bool,
    pub door_approaches: Vec<Location>,
    pub material: &'static str,
    pub location: Location,
    pub wall: bool,
    pub place_hint: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionView {
    pub velocity: [i64; 3],
    pub displaced: bool,
    pub impacted: bool,
}

/// Disclosed facts for one actor, separate from the authoritative game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub combat: Option<crate::combat::CombatView>,
    pub motion: Option<MotionView>,
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
    /// A free read using bounded symmetric shadowcasting through rotated portals.
    /// Neither querying nor seeing through a portal counts as visiting a place.
    pub fn observe(&self, id: ActorId) -> Result<Observation, GameError> {
        self.observe_scene(id).map(|(view, _)| view)
    }

    /// Construct disclosure and its exact projected scene together. Callers can
    /// reuse the scene without repeating shadowcasting or door-approach queries.
    pub fn observe_scene(
        &self,
        id: ActorId,
    ) -> Result<(Observation, Vec<tor_world::SightCell>), GameError> {
        let scene = self.scene(id)?;
        let view = self.observe_in_scene(id, &scene)?;
        Ok((view, scene))
    }

    fn observe_in_scene(
        &self,
        id: ActorId,
        scene: &[tor_world::SightCell],
    ) -> Result<Observation, GameError> {
        crate::diagnostics::observation();
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let cells = scene
            .iter()
            .map(|cell| cell.location)
            .collect::<BTreeSet<_>>();
        let visible = |location: Location| cells.contains(&location);
        let door_base = |location: Location| {
            self.world
                .door(location)
                .is_some_and(|door| self.world.door_location(door.id) == Some(location))
        };
        // A location can occur in several frames and at several distances;
        // each (location, frame) pair is disclosed once. Floors and ceilings
        // are ordinary seen solid cells, derived by clients.
        let occurrences: BTreeSet<_> = scene.iter().map(|c| (c.location, c.rotation)).collect();
        let mut ground_items = Vec::new();
        let mut inventory = Vec::new();
        for (&item_id, item) in self.items.iter() {
            crate::diagnostics::item_view(item.spec.concealed);
            let identified = !item.spec.concealed || actor.knowledge.contains(&item.spec.identity);
            let name = if identified {
                &item.spec.name
            } else {
                &item.spec.appearance
            };

            match item.location {
                ItemLocation::Ground(location) if visible(location) => {
                    ground_items.push(GroundItemView {
                        description: item_description(name),
                        id: item_id,
                        name: name.clone(),
                        quantity: item.quantity,
                        appearance: item.spec.appearance.clone(),
                        identified,
                        location,
                    });
                }
                ItemLocation::Carried(owner) if owner == id => {
                    inventory.push(ItemView {
                        description: item_description(name),
                        id: item_id,
                        name: name.clone(),
                        quantity: item.quantity,
                        appearance: item.spec.appearance.clone(),
                        identified,
                    });
                }
                _ => {}
            }
        }
        Ok(Observation {
            combat: self.combat_view(id, &visible),
            motion: self.motion_view_active(id).then_some(MotionView {
                velocity: actor.motion.velocity,
                displaced: self.physics.displaced.contains(&id),
                impacted: self
                    .physics
                    .impacts
                    .iter()
                    .any(|e| e.entity == crate::PhysicsEntity::Actor(id)),
            }),
            actor: id,
            tick: self.tick,
            location: actor.location,
            region: self
                .world
                .region(actor.location.region)
                .expect("validated actor location")
                .clone(),
            ground_items,
            visible_cells: occurrences
                .iter()
                .map(|&(location, frame)| CellView {
                    frame,
                    door: self.world.door(location),
                    // A tall door is one door: only its base cell is offered
                    // for interaction, so it isn't reachable twice.
                    door_reachable: door_base(location)
                        && self.door_reachable_from(actor.location, location),
                    door_approaches: if door_base(location) {
                        self.disclosed_door_approaches(scene, location)
                    } else {
                        vec![]
                    },
                    material: match self.world.terrain(location) {
                        Some(tor_world::Terrain::Solid(material)) => material.name(),
                        _ if self.world.is_chamber(location.region) => "",
                        _ => "stone", // Cosmetic fallback for raw diagnostic regions.
                    },
                    location,
                    wall: self.world.is_wall(location),
                    place_hint: self.world.has_place_hint(location),
                })
                .collect(),
            inventory,
            visible_actors: self
                .actors
                .iter()
                .filter(|(_, a)| a.alive())
                .flat_map(|(&other_id, other)| {
                    self.body_cells(other.location, other.orientation, &other.body)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(move |(at, _)| {
                            visible(*at) && (other_id != id || *at != actor.location)
                        })
                        .map(move |(location, _)| ActorView {
                            name: if other_id == id {
                                "yourself".into()
                            } else {
                                other
                                    .combat
                                    .as_ref()
                                    .map_or_else(|| "figure".into(), |c| c.spec.name.clone())
                            },
                            description: "An unremarkable figure is here.",
                            id: other_id,
                            location,
                        })
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
                .filter(|exit| {
                    !self.world.is_wall(exit.from)
                        && (!matches!(exit.direction, Direction::Up | Direction::Down)
                            || self.world.is_stair(exit.from, exit.direction))
                })
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

    fn disclosed_door_approaches(
        &self,
        scene: &[tor_world::SightCell],
        door: Location,
    ) -> Vec<Location> {
        let mut approaches = BTreeSet::new();
        for from in scene {
            if !self.world.walkable(from.location) {
                continue;
            }
            for direction in Direction::HORIZONTAL {
                let (dx, dy, _) = direction.delta();
                let local = direction.rotated(from.rotation);
                let reach = if let Some((a, b)) = direction.components() {
                    self.world.diagonal_reach(from.location, local, |side| {
                        !self.occupied(side)
                            && [a, b].into_iter().any(|first| {
                                let Some((next, turns)) = self
                                    .world
                                    .movement_neighbor(from.location, first.rotated(from.rotation))
                                else {
                                    return false;
                                };
                                let (sx, sy, _) = first.delta();
                                next == side
                                    && scene.iter().any(|seen| {
                                        seen.location == side
                                            && seen.rotation
                                                == tor_world::compose_rotation(from.rotation, turns)
                                            && seen.offset
                                                == Position {
                                                    x: from.offset.x + sx,
                                                    y: from.offset.y + sy,
                                                    z: from.offset.z,
                                                }
                                    })
                            })
                    })
                } else {
                    self.reach(from.location, local)
                };
                let Some((to, turns)) = reach else {
                    continue;
                };
                if to != door {
                    continue;
                }
                let rotation = tor_world::compose_rotation(from.rotation, turns);
                if scene.iter().any(|target| {
                    target.location == door
                        && target.rotation == rotation
                        && target.offset
                            == Position {
                                x: from.offset.x + dx,
                                y: from.offset.y + dy,
                                z: from.offset.z,
                            }
                }) {
                    approaches.insert(from.location);
                }
            }
        }
        approaches.into_iter().collect()
    }

    /// Backend-resolved view occurrences. A location may be seen at several
    /// offsets. Sight starts at the centre of the body's eye cell; offsets are
    /// relative to the actor's reference cell in its body frame.
    pub fn scene(&self, id: ActorId) -> Result<Vec<tor_world::SightCell>, GameError> {
        if !self.alive(id) && self.combat.selected != Some(id) && self.actors.contains_key(&id) {
            return Ok(Vec::new());
        }
        crate::diagnostics::scene();
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let body = &actor.body;
        let eye_index = body
            .cells
            .iter()
            .position(|cell| *cell == body.eye)
            .expect("validated body contains its eye");
        let Some((eye, eye_frame)) = self
            .body_cells(actor.location, actor.orientation, body)
            .map(|cells| cells[eye_index])
        else {
            return Ok(Vec::new());
        };
        // The body frame is carried across any portal inside the body, so
        // eye-frame offsets are body-frame offsets from the eye cell.
        let [ex, ey, ez] = body.eye;
        let mut scene = self.world.eye_scene(eye, eye_frame, SIGHT_RANGE);
        for cell in &mut scene {
            cell.offset.x += ex;
            cell.offset.y += ey;
            cell.offset.z += ez;
        }
        // Abstract stair links are traversal, not geometry: standing on one
        // discloses its landing as a separate occurrence beyond physical sight.
        for direction in [Direction::Up, Direction::Down] {
            if self.world.is_stair(actor.location, direction) {
                let passage = self
                    .world
                    .passage(actor.location, direction)
                    .expect("stair link");
                scene.push(tor_world::SightCell {
                    location: passage.to,
                    rotation: tor_world::compose_rotation(
                        actor.orientation,
                        self.world.crossing_rotation(actor.location, direction),
                    ),
                    offset: stair_landing_offset(body, direction),
                    wall: self.world.is_wall(passage.to),
                });
            }
        }
        scene.sort_by_key(|c| c.offset);
        Ok(scene)
    }
}

/// Initial authored appearance catalog. These cosmetic stubs do not add item rules.
fn item_description(name: &str) -> String {
    match name {
        "copper token" => "A small copper disc, stamped with a worn spiral.",
        "silver token" => "A small silver disc, stamped with a worn spiral.",
        "iron token" => "A small iron disc, stamped with a worn spiral.",
        "stone tablet" => "A weathered slab of stone. Shallow marks run across its surface.",
        _ => "You notice no further distinguishing details.",
    }
    .into()
}
