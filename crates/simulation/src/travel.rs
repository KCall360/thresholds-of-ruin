//! Actor knowledge is refreshed only at authoritative perception boundaries.
//! Planning never reads current terrain or undiscovered topology.
use crate::{ActorId, Game, GameError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
                knowledge.cells.insert(cell.location, cell.wall);
                if cell.wall {
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
                    let (dx, dy, dz) = match direction {
                        Direction::North => (0, -1, 0),
                        Direction::East => (1, 0, 0),
                        Direction::South => (0, 1, 0),
                        Direction::West => (-1, 0, 0),
                        Direction::Up => (0, 0, 1),
                        Direction::Down => (0, 0, -1),
                    };
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

    /// Stable breadth-first search in remembered topology, including orientation.
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
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::from([start]);
        let mut previous = BTreeMap::new();
        while let Some(node) = queue.pop_front() {
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
            for direction in DIRECTIONS {
                let local = direction.rotated(node.1);
                if let Some(&(to, rotation)) = knowledge.edges.get(&(node.0, local)) {
                    if knowledge.cells.get(&to) != Some(&false) {
                        continue;
                    }
                    let next = (to, (node.1 + rotation) % 4);
                    if seen.insert(next) {
                        previous.insert(next, (node, direction));
                        queue.push_back(next);
                    }
                }
            }
        }
        Err(GameError::Blocked)
    }
}
