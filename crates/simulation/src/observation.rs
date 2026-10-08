use std::collections::BTreeSet;
use tor_world::{Direction, Location, Position, Region, RegionId};

use crate::{ActorId, Game, GameError, ItemId, ItemLocation};

/// Manhattan sight range in cells, measured from the eye cell.
pub(crate) const SIGHT_RANGE: u8 = 8;

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
    pub class: crate::ItemClass,
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundItemView {
    pub class: crate::ItemClass,
    pub description: String,
    pub id: ItemId,
    pub name: String,
    pub quantity: u64,
    pub appearance: String,
    pub identified: bool,
    pub location: Location,
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPlace {
    pub id: RegionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorView {
    /// Empty when nothing names it.
    pub name: String,
    pub id: ActorId,
    pub location: Location,
    pub asset: Option<String>,
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
    pub interactions: Option<crate::interactions::InteractionView>,
    pub combat: Option<crate::combat::CombatView>,
    pub motion: Option<MotionView>,
    pub actor: ActorId,
    pub tick: u64,
    pub location: Location,
    pub region: Region,
    /// The observer's own asset, for views of itself from another angle.
    pub asset: Option<String>,
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
        let perceived = self.actors.perceived(&self.world, &cells);
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
        let mut item_interactions = Vec::new();
        for (item_id, item) in self.items.perceived(id, &cells) {
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
                        class: item.spec.class,
                        description: item_description(name),
                        id: item_id,
                        name: name.clone(),
                        quantity: item.quantity,
                        appearance: item.spec.appearance.clone(),
                        identified,
                        location,
                        asset: item.spec.asset.clone(),
                    });
                }
                ItemLocation::Carried(owner) if owner == id => {
                    if item.spec.equipment.is_some() || item.spec.consumable.is_some() {
                        item_interactions.push(crate::interactions::ItemInteractionView {
                            item: item_id,
                            slot: item.spec.equipment.as_ref().map(|equipment| equipment.slot),
                            equipped_slot: actor
                                .equipment
                                .iter()
                                .find_map(|(slot, item)| (*item == item_id).then_some(*slot)),
                            known_equipment: identified
                                .then(|| item.spec.equipment.clone())
                                .flatten(),
                            drinkable: item.spec.consumable.is_some(),
                        });
                    }
                    inventory.push(ItemView {
                        class: item.spec.class,
                        description: item_description(name),
                        id: item_id,
                        name: name.clone(),
                        quantity: item.quantity,
                        appearance: item.spec.appearance.clone(),
                        identified,
                        asset: item.spec.asset.clone(),
                    });
                }
                _ => {}
            }
        }
        let completed: Vec<_> = self
            .combat
            .events
            .iter()
            .filter_map(|event| match event {
                crate::combat::CombatEvent::ItemCompleted { actor, work, .. } if *actor == id => {
                    Some(*work)
                }
                _ => None,
            })
            .collect();
        Ok(Observation {
            interactions: (!actor.anatomy.slots.is_empty()
                || actor.pending.is_some()
                || !item_interactions.is_empty()
                || !completed.is_empty())
            .then(|| crate::interactions::InteractionView {
                completed,
                slots: actor.anatomy.slots.clone(),
                preparation: actor.pending.as_ref().map(|progress| {
                    crate::interactions::PreparationView {
                        work: progress.work,
                        remaining: if progress.active {
                            progress.remaining.saturating_sub(
                                self.actor_clock(id).saturating_sub(progress.started),
                            )
                        } else {
                            progress.remaining
                        },
                        active: progress.active,
                    }
                }),
                inventory: item_interactions,
            }),
            combat: self.combat_view(id, &visible, &perceived),
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
            asset: actor.asset.clone(),
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
            visible_actors: perceived
                .iter()
                .filter(|(other_id, _)| self.actors[other_id].alive())
                .flat_map(|(other_id, body)| {
                    let other_id = *other_id;
                    let other = &self.actors[&other_id];
                    body.iter()
                        .copied()
                        .filter(move |(at, _)| {
                            visible(*at) && (other_id != id || *at != actor.location)
                        })
                        .map(move |(location, _)| ActorView {
                            name: other
                                .combat
                                .as_ref()
                                .map_or_else(String::new, |c| c.spec.name.clone()),
                            id: other_id,
                            location,
                            asset: other.asset.clone(),
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
                        .known_region(id)
                        .expect("visited region is known")
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
        self.door_approaches_among(scene, door, true)
    }

    /// With `filtered`, only cells beside an occurrence of the door are
    /// considered. Tests compare that against considering every cell.
    fn door_approaches_among(
        &self,
        scene: &[tor_world::SightCell],
        door: Location,
        filtered: bool,
    ) -> Vec<Location> {
        let mut approaches = BTreeSet::new();
        // An approach needs the door seen one horizontal step away, so only
        // cells beside one of its occurrences can qualify. Checking that first
        // skips only candidates the final test below would reject.
        let doors: Vec<Position> = scene
            .iter()
            .filter(|cell| cell.location == door)
            .map(|cell| cell.offset)
            .collect();
        for from in scene {
            let beside = |x: i32, y: i32| {
                let target = Position {
                    x: from.offset.x + x,
                    y: from.offset.y + y,
                    z: from.offset.z,
                };
                doors.contains(&target)
            };
            if filtered && !(-1..=1).any(|x| (-1..=1).any(|y| (x, y) != (0, 0) && beside(x, y)))
                || !self.world.walkable(from.location)
            {
                continue;
            }
            for direction in Direction::HORIZONTAL {
                let (dx, dy, _) = direction.delta();
                if filtered && !beside(dx, dy) {
                    continue;
                }
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
        let Some((eye, eye_frame)) = self.eye(id) else {
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

    /// The loaded actor's eye cell and frame, if its body resolves.
    pub(crate) fn eye(&self, id: ActorId) -> Option<(Location, u8)> {
        let actor = self.actors.get(&id)?;
        let body = &actor.body;
        let eye_index = body
            .cells
            .iter()
            .position(|cell| *cell == body.eye)
            .expect("validated body contains its eye");
        self.body_cells(actor.location, actor.orientation, body)
            .map(|cells| cells[eye_index])
    }
}

/// Initial authored appearance catalog. These cosmetic stubs do not add item rules.
/// Empty when nothing is authored; clients decide how to say so.
fn item_description(name: &str) -> String {
    match name {
        "copper token" => "A small copper disc, stamped with a worn spiral.",
        "silver token" => "A small silver disc, stamped with a worn spiral.",
        "iron token" => "A small iron disc, stamped with a worn spiral.",
        "stone tablet" => "A weathered slab of stone. Shallow marks run across its surface.",
        _ => "",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Extent, Passage, World};

    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }

    #[test]
    fn door_approaches_ignore_only_cells_away_from_the_door() {
        let (mut compared, mut found) = (0, 0);
        for seed in 1..=24u64 {
            let mut state = 0x9E37_79B9_7F4A_7C15 ^ seed;
            let mut below = |n: u64| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state % n
            };
            let rooms = (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: String::new(),
                    bounds: Extent::new(7, 7, 1).unwrap(),
                })
                .collect();
            let mut world = World::new(rooms, vec![]).unwrap();
            // A straight join and a rotated one, as in the latency fixture.
            for (from, direction, to, back, turns) in [
                (
                    at(1, 6, 3),
                    Direction::East,
                    at(2, 0, 3),
                    Direction::West,
                    0,
                ),
                (
                    at(1, 3, 0),
                    Direction::North,
                    at(2, 0, 5),
                    Direction::West,
                    1,
                ),
            ] {
                world
                    .connect(
                        Passage {
                            from,
                            direction,
                            to,
                        },
                        turns,
                    )
                    .unwrap();
                world
                    .connect(
                        Passage {
                            from: to,
                            direction: back,
                            to: from,
                        },
                        (4 - turns) % 4,
                    )
                    .unwrap();
            }
            let cell = |below: &mut dyn FnMut(u64) -> u64| {
                at(1 + below(2), below(7) as i32, below(7) as i32)
            };
            for _ in 0..12 {
                let _ = world.set_wall(cell(&mut below), true);
            }
            let mut game = Game::new(world, seed);
            for _ in 0..8 {
                let _ = game.place_door(cell(&mut below), below(2) == 0, 1);
            }
            for _ in 0..4 {
                let _ = game.spawn_actor(cell(&mut below), NonZeroU64::new(100).unwrap());
            }
            for _ in 0..16 {
                let eye = cell(&mut below);
                let scene = game.world.eye_scene(eye, below(4) as u8, SIGHT_RANGE);
                let doors: BTreeSet<_> = scene
                    .iter()
                    .map(|c| c.location)
                    .filter(|&at| game.world.door(at).is_some())
                    .collect();
                for door in doors {
                    let approaches = game.door_approaches_among(&scene, door, true);
                    assert_eq!(
                        approaches,
                        game.door_approaches_among(&scene, door, false),
                        "seed {seed}: eye {eye:?}, door {door:?}"
                    );
                    compared += 1;
                    found += approaches.len();
                }
            }
        }
        assert!(
            compared > 200 && found > 200,
            "{compared} doors, {found} approaches"
        );
    }
}
