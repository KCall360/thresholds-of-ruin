//! Actor knowledge is refreshed only at authoritative perception boundaries.
//! Planning never reads current terrain or undiscovered topology.
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Navigation {
    cells: BTreeMap<Location, bool>,
    edges: BTreeMap<(Location, Direction), (Location, u8)>,
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
            let knowledge = self.navigation.entry(id).or_default();
            let visible: BTreeSet<_> = scene.iter().map(|c| c.location).collect();
            knowledge
                .edges
                .retain(|(from, _), (to, _)| !(visible.contains(from) && visible.contains(to)));
            for cell in &scene {
                knowledge
                    .cells
                    .insert(cell.location, self.world.opaque(cell.location));
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
                        knowledge.edges.insert((cell.location, local), (to, turns));
                    }
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
