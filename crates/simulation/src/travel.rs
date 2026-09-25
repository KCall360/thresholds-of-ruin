//! Actor knowledge is refreshed only at authoritative perception boundaries.
//! Planning never reads current terrain or undiscovered topology.
use crate::navigation_map::RegionMap;
use crate::{movement_cost, ActorId, Game, GameError};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Direction, Location, Position};

const DIRECTIONS: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Navigation {
    pub(super) cells: RegionMap<Location, bool>,
    pub(super) edges: RegionMap<(Location, Direction), (Location, u8)>,
}

impl Navigation {
    pub(crate) fn checkpoint_valid(&self, world: &tor_world::World) -> bool {
        self.cells.keys().all(|location| world.contains(*location))
            && self.edges.iter().all(|((from, _), (to, turns))| {
                *turns < 4 && self.cells.contains_key(from) && self.cells.contains_key(to)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TravelStep {
    pub direction: Direction,
    pub destination: Location,
}

impl Game {
    /// Capture only connections whose two ends appear adjacent in the perceived
    /// scene. Merely knowing two cells never reveals an unseen link between them.
    pub fn refresh_navigation(&mut self) {
        for id in self.actors.keys().copied().collect::<Vec<_>>() {
            let scene = self.scene(id).expect("existing actor");
            self.refresh_navigation_scene(id, &scene);
        }
    }

    /// Reuse a scene produced by this game at the current decision boundary.
    /// This is backend-only; caller-supplied protocol data must never enter here.
    pub fn refresh_navigation_scene(&mut self, id: ActorId, scene: &[tor_world::SightCell]) {
        let knowledge = self.navigation.entry(id).or_default();
        let visible: BTreeSet<_> = scene.iter().map(|c| c.location).collect();
        let projected: BTreeSet<_> = scene
            .iter()
            .filter(|c| !c.wall)
            .map(|c| (c.location, c.offset, c.rotation))
            .collect();
        // Inspect only edges originating in the visible scene, never all remembered
        // topology. Preserve stale edges when either end is undisclosed.
        let mut edges = BTreeMap::new();
        for &from in &visible {
            for direction in DIRECTIONS {
                if knowledge
                    .edges
                    .get(&(from, direction))
                    .is_some_and(|(to, _)| visible.contains(to))
                {
                    edges.insert((from, direction), None);
                }
            }
        }
        let mut cells = BTreeMap::new();
        for cell in scene {
            let opaque = self.world.opaque(cell.location);
            if knowledge.cells.get(&cell.location) != Some(&opaque) {
                cells.insert(cell.location, opaque);
            }
            if opaque {
                continue;
            }
            for direction in DIRECTIONS {
                let local = direction.rotated(cell.rotation);
                if matches!(local, Direction::Up | Direction::Down)
                    && self.world.passage(cell.location, local).is_none()
                {
                    continue;
                }
                let Some(to) = self.world.step(cell.location, local) else {
                    continue;
                };
                let turns = self.world.crossing_rotation(cell.location, local);
                let (dx, dy, dz) = direction.delta();
                let offset = Position {
                    x: cell.offset.x + dx,
                    y: cell.offset.y + dy,
                    z: cell.offset.z + dz,
                };
                if projected.contains(&(to, offset, (cell.rotation + turns) % 4)) {
                    edges.insert((cell.location, local), Some((to, turns)));
                }
            }
        }
        edges.retain(|key, value| knowledge.edges.get(key) != value.as_ref());
        if !cells.is_empty() || !edges.is_empty() {
            // Copy-on-write detaches only when knowledge actually changes.
            let knowledge = &mut **knowledge;
            knowledge.cells.extend(cells);
            for (key, value) in edges {
                if let Some(value) = value {
                    knowledge.edges.insert(key, value);
                } else {
                    knowledge.edges.remove(&key);
                }
            }
        }
    }

    pub fn known_cells(&self, actor: ActorId) -> impl Iterator<Item = Location> + '_ {
        self.navigation
            .get(&actor)
            .into_iter()
            .flat_map(|n| n.cells.keys().copied())
    }

    /// Stable minimum-tick search in remembered topology, including orientation.
    pub fn travel_route(
        &self,
        actor: ActorId,
        destination: Location,
    ) -> Result<Vec<TravelStep>, GameError> {
        let actor_state = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let knowledge = self.navigation.get(&actor).ok_or(GameError::Blocked)?;
        if knowledge.cells.get(&destination) != Some(&false) {
            return Err(GameError::Blocked);
        }
        let start = (actor_state.location, actor_state.orientation);
        let mut queue = BTreeSet::from([(0u128, 0u64, start)]);
        let mut distances = BTreeMap::from([(start, 0u128)]);
        let mut order = 0u64;
        let mut previous = BTreeMap::new();
        while let Some((cost, _, node)) = queue.pop_first() {
            if distances[&node] != cost {
                continue;
            }
            if node.0 == destination {
                let mut route = Vec::new();
                let mut cursor = node;
                while cursor != start {
                    let (parent, direction) = previous[&cursor];
                    route.push(TravelStep {
                        direction,
                        destination: cursor.0,
                    });
                    cursor = parent;
                }
                route.reverse();
                return Ok(route);
            }
            for direction in DIRECTIONS.into_iter().chain(
                Direction::HORIZONTAL
                    .into_iter()
                    .filter(|d| d.components().is_some()),
            ) {
                let local = direction.rotated(node.1);
                let edge = if let Some((a, b)) = local.components() {
                    let path = |first, second: Direction| {
                        let &(side, r1) = knowledge.edges.get(&(node.0, first))?;
                        if knowledge.cells.get(&side) != Some(&false) {
                            return None;
                        }
                        let &(to, r2) = knowledge.edges.get(&(side, second.rotated(r1)))?;
                        Some((to, (r1 + r2) % 4))
                    };
                    match (path(a, b), path(b, a)) {
                        (Some(a), Some(b)) if a == b => Some(a),
                        (Some(a), None) | (None, Some(a)) => Some(a),
                        _ => None,
                    }
                } else {
                    knowledge.edges.get(&(node.0, local)).copied()
                };
                if let Some((to, rotation)) = edge {
                    if knowledge.cells.get(&to) != Some(&false) {
                        continue;
                    }
                    let next = (to, (node.1 + rotation) % 4);
                    let Ok(duration) = movement_cost(actor_state.turn_ticks.get(), direction)
                    else {
                        continue;
                    };
                    let next_cost = cost + u128::from(duration);
                    if distances.get(&next).is_none_or(|old| next_cost < *old) {
                        distances.insert(next, next_cost);
                        previous.insert(next, (node, direction));
                        order += 1;
                        queue.insert((next_cost, order, next));
                    }
                }
            }
        }
        Err(GameError::Blocked)
    }
}

#[cfg(test)]
impl Game {
    fn reference_refresh_navigation(&mut self) {
        for id in self.actors.keys().copied().collect::<Vec<_>>() {
            let scene = self.scene(id).expect("existing actor");
            let knowledge = self.navigation.entry(id).or_default();
            let mut cells: BTreeMap<_, _> = knowledge.cells.iter().map(|(k, v)| (*k, *v)).collect();
            let mut edges: BTreeMap<_, _> = knowledge.edges.iter().map(|(k, v)| (*k, *v)).collect();
            let visible: BTreeSet<_> = scene.iter().map(|c| c.location).collect();
            edges.retain(|(from, _), (to, _)| !(visible.contains(from) && visible.contains(to)));
            for cell in &scene {
                cells.insert(cell.location, self.world.opaque(cell.location));
                if self.world.opaque(cell.location) {
                    continue;
                }
                for direction in DIRECTIONS {
                    let local = direction.rotated(cell.rotation);
                    if matches!(local, Direction::Up | Direction::Down)
                        && self.world.passage(cell.location, local).is_none()
                    {
                        continue;
                    }
                    let Some(to) = self.world.step(cell.location, local) else {
                        continue;
                    };
                    let turns = self.world.crossing_rotation(cell.location, local);
                    let (dx, dy, dz) = direction.delta();
                    let offset = Position {
                        x: cell.offset.x + dx,
                        y: cell.offset.y + dy,
                        z: cell.offset.z + dz,
                    };
                    if scene.iter().any(|other| {
                        other.location == to
                            && !other.wall
                            && other.offset == offset
                            && other.rotation == (cell.rotation + turns) % 4
                    }) {
                        edges.insert((cell.location, local), (to, turns));
                    }
                }
            }
            knowledge.cells = RegionMap::default();
            knowledge.cells.extend(cells);
            knowledge.edges = RegionMap::default();
            knowledge.edges.extend(edges);
        }
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    use crate::Action;
    use std::num::NonZeroU64;
    use tor_world::RegionId;

    #[test]
    fn local_refresh_matches_original_full_scan_with_stale_edges_and_door_changes() {
        let mut game = Game::two_room_in_stone(42);
        let location = |region, x, y| Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        };
        let actor = game
            .spawn_actor(location(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let compare = |game: &mut Game| {
            let mut reference = game.clone();
            reference.reference_refresh_navigation();
            game.refresh_navigation();
            assert_eq!(game.navigation, reference.navigation);
            for destination in game.known_cells(actor) {
                assert_eq!(
                    game.travel_route(actor, destination),
                    reference.travel_route(actor, destination)
                );
            }
        };
        compare(&mut game);
        for _ in 0..4 {
            game.act(actor, Action::Move(Direction::East)).unwrap();
            compare(&mut game);
        }
        game.act(actor, Action::Move(Direction::East)).unwrap();
        compare(&mut game);
        game.act(actor, Action::Move(Direction::East)).unwrap();
        compare(&mut game);
        game.teleport(actor, location(1, 4, 1)).unwrap();
        compare(&mut game);
        game.act(
            actor,
            Action::SetDoor {
                door: 1,
                open: false,
            },
        )
        .unwrap();
        compare(&mut game);
        game.teleport(actor, location(2, 4, 1)).unwrap();
        compare(&mut game);
        game.set_wall(location(2, 3, 1), true).unwrap();
        compare(&mut game);
        game.set_wall(location(2, 3, 1), false).unwrap();
        compare(&mut game);
        game.teleport(actor, location(1, 4, 1)).unwrap();
        compare(&mut game);
        game.act(
            actor,
            Action::SetDoor {
                door: 1,
                open: true,
            },
        )
        .unwrap();
        compare(&mut game);
    }
}
