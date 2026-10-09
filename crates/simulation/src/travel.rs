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
    pub(super) places: RegionMap<Location, crate::PlaceName>,
    pub(super) edges: RegionMap<(Location, Direction), (Location, u8)>,
}

impl Navigation {
    pub(crate) fn checkpoint_valid(&self, world: &tor_world::World) -> bool {
        self.places.iter().all(|(location, place)| {
            self.cells.contains_key(location) && crate::places::valid_name(&place.name)
        }) && self.cells.keys().all(|location| world.knows(*location))
            && self.edges.iter().all(|((from, _), (to, turns))| {
                *turns < 24 && self.cells.contains_key(from) && self.cells.contains_key(to)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TravelStep {
    pub direction: Direction,
    pub destination: Location,
}

type Node = (Location, u8);

type Offset = (i32, i32, i32);
type ProjectedCell = (Location, u8, bool);

/// Read-only chart lookup for one perception boundary. Compact scenes use a
/// bounded rectangular index; widely separated stair/portal offsets stay sparse.
struct ProjectedCells {
    values: Vec<ProjectedCell>,
    index: OffsetIndex,
}

enum OffsetIndex {
    Dense {
        origin: Offset,
        size: [usize; 3],
        slots: Vec<usize>,
    },
    Sparse(std::collections::HashMap<Offset, usize>),
}

impl OffsetIndex {
    fn new(offsets: &[Offset]) -> Self {
        let dense = offsets.first().and_then(|&(x, y, z)| {
            let mut lo = [x, y, z];
            let mut hi = lo;
            for &(x, y, z) in offsets {
                for (axis, value) in [x, y, z].into_iter().enumerate() {
                    lo[axis] = lo[axis].min(value);
                    hi[axis] = hi[axis].max(value);
                }
            }
            let size = [0, 1, 2].map(|axis| (i64::from(hi[axis]) - i64::from(lo[axis]) + 1) as u64);
            let volume = size.into_iter().try_fold(1u64, u64::checked_mul)?;
            // At most eight index slots per disclosed cell, capped at 2 MiB
            // on a 64-bit host. Sparse scenes never allocate their bounding box.
            if volume > offsets.len().saturating_mul(8).min(262_144) as u64 {
                return None;
            }
            Some(Self::Dense {
                origin: (lo[0], lo[1], lo[2]),
                size: size.map(|n| n as usize),
                slots: vec![usize::MAX; volume as usize],
            })
        });
        let Some(mut index) = dense else {
            return Self::Sparse(
                offsets
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(i, p)| (p, i))
                    .collect(),
            );
        };
        for (i, offset) in offsets.iter().enumerate() {
            let slot = index
                .dense_slot(offset)
                .expect("authored offset lies in its bounds");
            if let Self::Dense { slots, .. } = &mut index {
                slots[slot] = i;
            }
        }
        index
    }

    fn dense_slot(&self, &(x, y, z): &Offset) -> Option<usize> {
        let Self::Dense { origin, size, .. } = self else {
            return None;
        };
        let delta = [
            i64::from(x) - i64::from(origin.0),
            i64::from(y) - i64::from(origin.1),
            i64::from(z) - i64::from(origin.2),
        ];
        if (0..3).any(|axis| delta[axis] < 0 || delta[axis] >= size[axis] as i64) {
            return None;
        }
        let [x, y, z] = delta.map(|n| n as usize);
        Some((x * size[1] + y) * size[2] + z)
    }

    fn get(&self, offset: &Offset) -> Option<usize> {
        match self {
            Self::Dense { slots, .. } => {
                let value = slots[self.dense_slot(offset)?];
                (value != usize::MAX).then_some(value)
            }
            Self::Sparse(map) => map.get(offset).copied(),
        }
    }
}

impl ProjectedCells {
    fn new(scene: &[tor_world::SightCell], world: &tor_world::World) -> Self {
        let offsets: Vec<_> = scene
            .iter()
            .map(|c| (c.offset.x, c.offset.y, c.offset.z))
            .collect();
        Self {
            index: OffsetIndex::new(&offsets),
            values: scene
                .iter()
                .map(|c| (c.location, c.rotation, world.opaque(c.location)))
                .collect(),
        }
    }

    fn get(&self, offset: &Offset) -> Option<&ProjectedCell> {
        self.values.get(self.index.get(offset)?)
    }
}

/// One ordered search shared by targets in a single read-only decision.
pub(crate) struct RouteSearch<'a> {
    actor: &'a crate::Actor,
    knowledge: &'a Navigation,
    start: Node,
    queue: BTreeSet<(u128, u64, Node)>,
    distances: BTreeMap<Node, u128>,
    previous: BTreeMap<Node, (Node, Direction)>,
    settled: BTreeMap<Location, Node>,
    order: u64,
    started: bool,
}

impl<'a> RouteSearch<'a> {
    fn new(actor: &'a crate::Actor, knowledge: &'a Navigation) -> Self {
        let start = (actor.location, actor.orientation);
        Self {
            actor,
            knowledge,
            start,
            queue: BTreeSet::from([(0, 0, start)]),
            distances: BTreeMap::from([(start, 0)]),
            previous: BTreeMap::new(),
            settled: BTreeMap::new(),
            order: 0,
            started: false,
        }
    }

    pub(crate) fn route(&mut self, destination: Location) -> Result<Vec<TravelStep>, GameError> {
        if self.knowledge.cells.get(&destination) != Some(&false) {
            return Err(GameError::Blocked);
        }
        if !self.started {
            crate::diagnostics::route_search();
            self.started = true;
        }
        while !self.settled.contains_key(&destination) {
            let Some((cost, _, node)) = self.queue.pop_first() else {
                return Err(GameError::Blocked);
            };
            if self.distances[&node] != cost {
                continue;
            }
            self.settled.entry(node.0).or_insert(node);
            self.expand(cost, node);
        }
        let mut route = Vec::new();
        let mut cursor = self.settled[&destination];
        while cursor != self.start {
            let (parent, direction) = self.previous[&cursor];
            route.push(TravelStep {
                direction,
                destination: cursor.0,
            });
            cursor = parent;
        }
        route.reverse();
        Ok(route)
    }

    fn expand(&mut self, cost: u128, node: Node) {
        for direction in DIRECTIONS.into_iter().chain(
            Direction::HORIZONTAL
                .into_iter()
                .filter(|d| d.components().is_some()),
        ) {
            let local = direction.rotated(node.1);
            let edge = if let Some((a, b)) = local.components() {
                let path = |first, second: Direction| {
                    let &(side, r1) = self.knowledge.edges.get(&(node.0, first))?;
                    if self.knowledge.cells.get(&side) != Some(&false) {
                        return None;
                    }
                    let &(to, r2) = self.knowledge.edges.get(&(side, second.rotated(r1)))?;
                    Some((to, tor_world::compose_rotation(r1, r2)))
                };
                match (path(a, b), path(b, a)) {
                    (Some(a), Some(b)) if a == b => Some(a),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    _ => None,
                }
            } else {
                self.knowledge.edges.get(&(node.0, local)).copied()
            };
            if let Some((to, rotation)) = edge {
                if self.knowledge.cells.get(&to) != Some(&false) {
                    continue;
                }
                let next = (to, tor_world::compose_rotation(node.1, rotation));
                let Ok(duration) = movement_cost(self.actor.turn_ticks.get(), direction) else {
                    continue;
                };
                let next_cost = cost + u128::from(duration);
                if self.distances.get(&next).is_none_or(|old| next_cost < *old) {
                    self.distances.insert(next, next_cost);
                    self.previous.insert(next, (node, direction));
                    self.order += 1;
                    self.queue.insert((next_cost, self.order, next));
                }
            }
        }
    }
}

/// Where the far end of a link from `cell` must appear in the actor's scene for
/// the link to count as seen: the adjacent offset, or, for the abstract stair
/// the actor stands on, the landing occurrence beyond physical sight.
fn linked_offset(
    world: &tor_world::World,
    body: &crate::BodySpec,
    cell: &tor_world::SightCell,
    direction: Direction,
    local: Direction,
) -> Position {
    let origin = Position { x: 0, y: 0, z: 0 };
    if cell.offset == origin && world.is_stair(cell.location, local) {
        return crate::observation::stair_landing_offset(body, local);
    }
    let (dx, dy, dz) = direction.delta();
    Position {
        x: cell.offset.x + dx,
        y: cell.offset.y + dy,
        z: cell.offset.z + dz,
    }
}

impl Game {
    /// Whether the scene changes the inputs used to learn connections. The
    /// caller must separately account for geometry edits. Translating a chart
    /// preserves its disclosed adjacencies; abstract stairs are conservative
    /// because their landing offsets are anchored to the observer's origin.
    pub fn navigation_scene_changed(
        &self,
        before: &[tor_world::SightCell],
        after: &[tor_world::SightCell],
    ) -> bool {
        if before.len() != after.len() {
            return true;
        }
        let Some((first, next)) = before.first().zip(after.first()) else {
            return false;
        };
        let delta = |a: Position, b: Position| {
            [
                i64::from(b.x) - i64::from(a.x),
                i64::from(b.y) - i64::from(a.y),
                i64::from(b.z) - i64::from(a.z),
            ]
        };
        let translation = delta(first.offset, next.offset);
        if before.iter().zip(after).any(|(a, b)| {
            a.location != b.location
                || a.rotation != b.rotation
                || a.wall != b.wall
                || delta(a.offset, b.offset) != translation
        }) {
            return true;
        }
        translation != [0; 3]
            && after.iter().any(|cell| {
                self.world.is_stair(cell.location, Direction::Up)
                    || self.world.is_stair(cell.location, Direction::Down)
            })
    }

    /// Capture only connections whose two ends appear adjacent in the perceived
    /// scene. Merely knowing two cells never reveals an unseen link between them.
    pub fn refresh_navigation(&mut self) {
        for id in self.actors.keys().copied().collect::<Vec<_>>() {
            let scene = self.scene(id).expect("existing actor");
            self.refresh_navigation_scene(id, &scene);
        }
    }

    /// Reuse a scene produced by this game at the current decision boundary.
    /// The scene has unique offsets, as produced by `Game::scene`. This is
    /// backend-only; caller-supplied protocol data must never enter here.
    pub fn refresh_navigation_scene(&mut self, id: ActorId, scene: &[tor_world::SightCell]) {
        self.refresh_places(id, scene);
        let knowledge = self.navigation.entry(id).or_default();
        let mut sources: Vec<_> = scene.iter().collect();
        // Stable grouping retains the chart's last valid proposal for aliases.
        sources.sort_by_key(|cell| cell.location);
        let location_key =
            |at: Location| (at.region.0, at.position.x, at.position.y, at.position.z);
        let disclosed: std::collections::HashSet<_> = scene
            .iter()
            .map(|cell| location_key(cell.location))
            .collect();
        // Membership only: hashing never determines an authoritative order.
        let projected = ProjectedCells::new(scene, &self.world);
        let mut adjacency = self.world.adjacency();
        let mut edges = Vec::new();
        // Travel only ever uses known walkable cells, so a cell is remembered
        // once it has been seen walkable. It is then kept, marked opaque if it
        // closes, so places and links stay valid. Floors, ceilings and walls
        // that were never walkable aren't stored; routing treats them exactly
        // like unknown cells.
        let mut cells = BTreeMap::new();
        for group in sources.chunk_by(|a, b| a.location == b.location) {
            let from = group[0].location;
            let offset = group[0].offset;
            let opaque = projected
                .get(&(offset.x, offset.y, offset.z))
                .expect("source is disclosed")
                .2;
            match knowledge.cells.get(&from) {
                Some(&known) if known != opaque => {
                    cells.insert(from, opaque);
                }
                None if !opaque => {
                    cells.insert(from, false);
                }
                _ => {}
            }
            let mut proposed = [None; 6];
            for cell in group.iter().filter(|_| !opaque) {
                for direction in DIRECTIONS {
                    let local = direction.rotated(cell.rotation);
                    if matches!(local, Direction::Up | Direction::Down)
                        && self.world.passage(cell.location, local).is_none()
                        && !self.world.is_stair(cell.location, local)
                    {
                        continue;
                    }
                    let offset =
                        linked_offset(&self.world, &self.actors[&id].body, cell, direction, local);
                    let Some(&(seen, frame, target_opaque)) =
                        projected.get(&(offset.x, offset.y, offset.z))
                    else {
                        continue;
                    };
                    if target_opaque {
                        continue;
                    }
                    let Some((to, turns)) = adjacency.resolve(cell.location, local) else {
                        continue;
                    };
                    // Cached scene opacity validates admission; both ends appear in this
                    // scene, so no undisclosed topology enters navigation knowledge.
                    if seen == to && frame == tor_world::compose_rotation(cell.rotation, turns) {
                        let bit = DIRECTIONS
                            .iter()
                            .position(|direction| *direction == local)
                            .expect("rotations preserve cardinal directions");
                        proposed[bit] = Some((to, turns));
                    }
                }
            }
            // Compare each source once. Undisclosed old endpoints stay stale;
            // valid new proposals replace them. Avoid provisional deletions,
            // duplicate proposals and sorting the whole edge patch.
            let mut previous = [None; 6];
            if let Some(bucket) = knowledge.edges.region(from.region) {
                for (&(_, direction), &value) in
                    bucket.range((from, Direction::North)..=(from, Direction::Down))
                {
                    if let Some(bit) = DIRECTIONS.iter().position(|d| *d == direction) {
                        previous[bit] = Some(value);
                    }
                }
            }
            for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                let next = proposed[bit].or_else(|| {
                    previous[bit].filter(|(to, _)| !disclosed.contains(&location_key(*to)))
                });
                if next != previous[bit] {
                    edges.push(((from, direction), next));
                }
            }
        }
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
        self.route_search(actor)?.route(destination)
    }

    /// Borrowed decision-local frontier: it cannot outlive a mutation of this game.
    pub(crate) fn route_search(&self, actor: ActorId) -> Result<RouteSearch<'_>, GameError> {
        let actor_state = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let knowledge = self.navigation.get(&actor).ok_or(GameError::Blocked)?;
        Ok(RouteSearch::new(actor_state, knowledge))
    }
}

#[cfg(test)]
impl Game {
    /// Stable minimum-tick search in remembered topology, including orientation.
    fn reference_travel_route(
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
                        Some((to, tor_world::compose_rotation(r1, r2)))
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
                    let next = (to, tor_world::compose_rotation(node.1, rotation));
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
            self.refresh_places(id, &scene);
            let knowledge = self.navigation.entry(id).or_default();
            let mut cells: BTreeMap<_, _> = knowledge.cells.iter().map(|(k, v)| (*k, *v)).collect();
            let mut edges: BTreeMap<_, _> = knowledge.edges.iter().map(|(k, v)| (*k, *v)).collect();
            let visible: BTreeSet<_> = scene.iter().map(|c| c.location).collect();
            edges.retain(|(from, _), (to, _)| !(visible.contains(from) && visible.contains(to)));
            for cell in &scene {
                let opaque = self.world.opaque(cell.location);
                if !opaque || cells.contains_key(&cell.location) {
                    cells.insert(cell.location, opaque);
                }
                if opaque {
                    continue;
                }
                for direction in DIRECTIONS {
                    let local = direction.rotated(cell.rotation);
                    if matches!(local, Direction::Up | Direction::Down)
                        && self.world.passage(cell.location, local).is_none()
                        && !self.world.is_stair(cell.location, local)
                    {
                        continue;
                    }
                    let Some(to) = self.world.step(cell.location, local) else {
                        continue;
                    };
                    let turns = self.world.crossing_rotation(cell.location, local);
                    let offset =
                        linked_offset(&self.world, &self.actors[&id].body, cell, direction, local);
                    if (!self.world.opaque(to) || cells.contains_key(&to))
                        && scene.iter().any(|other| {
                            other.location == to
                                && !other.wall
                                && other.offset == offset
                                && other.rotation
                                    == tor_world::compose_rotation(cell.rotation, turns)
                        })
                    {
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
    fn translated_chart_reuse_matches_full_navigation_at_every_rotation() {
        let open = tor_world::World::new(
            vec![tor_world::Region {
                id: RegionId(1),
                name: "open".into(),
                bounds: tor_world::Extent::new(7, 5, 3).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        for (fixture, template) in [
            Game::new(open, 42),
            Game::region_corridor(42, 2),
            Game::two_room_in_stone(42),
        ]
        .into_iter()
        .enumerate()
        {
            for dark in [false, true] {
                for frame in 0..24 {
                    let mut game = template.clone();
                    game.set_region_light(RegionId(1), !dark).unwrap();
                    let actor = game
                        .spawn_actor(
                            Location {
                                region: RegionId(1),
                                position: Position { x: 2, y: 1, z: 0 },
                            },
                            NonZeroU64::new(100).unwrap(),
                        )
                        .unwrap();
                    game.actors.get_mut(&actor).unwrap().orientation = frame;
                    let before = game.scene(actor).unwrap();
                    game.refresh_navigation();
                    let destinations: Vec<_> = before
                        .iter()
                        .filter(|c| !c.wall)
                        .map(|c| c.location)
                        .take(8)
                        .collect();
                    for destination in destinations {
                        let mut moved = game.clone();
                        if moved.teleport(actor, destination).is_err() {
                            continue;
                        }
                        let after = moved.scene(actor).unwrap();
                        let mut reference = moved.clone();
                        reference.reference_refresh_navigation();
                        if moved.navigation_scene_changed(&before, &after) {
                            moved.refresh_navigation_scene(actor, &after);
                        }
                        assert_eq!(
                            moved.navigation, reference.navigation,
                            "fixture {fixture}, dark {dark}, frame {frame}, at {destination:?}"
                        );
                        let known: Vec<_> = reference.known_cells(actor).collect();
                        for index in [0, known.len() / 2, known.len().saturating_sub(1)] {
                            if let Some(&target) = known.get(index) {
                                assert_eq!(
                                    moved.travel_route(actor, target),
                                    reference.travel_route(actor, target)
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn chart_translation_handles_extreme_offsets_and_keeps_stairs_conservative() {
        let mut game = Game::region_corridor(42, 1);
        let location = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 1, z: 0 },
        };
        let cell = tor_world::SightCell {
            location,
            rotation: 0,
            wall: false,
            offset: Position {
                x: i32::MIN,
                y: 0,
                z: 0,
            },
        };
        let translated = tor_world::SightCell {
            offset: Position {
                x: i32::MAX,
                y: 0,
                z: 0,
            },
            ..cell
        };
        assert!(!game.navigation_scene_changed(&[cell], &[translated]));
        game.register_named_anchors(
            RegionId(1),
            BTreeMap::from([("landing".into(), Position { x: 4, y: 1, z: 0 })]),
        )
        .unwrap();
        game.connect_named_stair(
            location,
            Direction::Down,
            tor_world::NamedAnchor {
                region: RegionId(1),
                name: "landing".into(),
            },
        )
        .unwrap();
        assert!(game.navigation_scene_changed(&[cell], &[translated]));
        assert!(!game.navigation_scene_changed(&[cell], &[cell]));
        assert!(game.navigation_scene_changed(&[cell], &[]));
    }

    #[test]
    fn bounded_offset_index_matches_hash_lookup_for_dense_sparse_and_extreme_charts() {
        let dense: Vec<_> = (-2..=2)
            .flat_map(|x| (-2..=2).flat_map(move |y| (-2..=2).map(move |z| (x, y, z))))
            .collect();
        for (offsets, is_dense) in [
            (dense, true),
            (vec![(0, 0, 0), (2, 0, 0), (0, 0, 0)], true),
            (vec![(0, 0, 0), (1000, 1000, 1000)], false),
            (
                vec![
                    (i32::MIN, i32::MIN, i32::MIN),
                    (i32::MAX, i32::MAX, i32::MAX),
                ],
                false,
            ),
            (vec![], false),
        ] {
            let index = OffsetIndex::new(&offsets);
            assert_eq!(matches!(index, OffsetIndex::Dense { .. }), is_dense);
            let reference: std::collections::HashMap<_, _> = offsets
                .iter()
                .copied()
                .enumerate()
                .map(|(i, p)| (p, i))
                .collect();
            for offset in offsets
                .iter()
                .chain([(1, 0, 0), (3, 0, 0), (i32::MIN, 0, 0), (i32::MAX, 0, 0)].iter())
            {
                assert_eq!(index.get(offset), reference.get(offset).copied());
            }
            if let OffsetIndex::Dense { slots, .. } = index {
                assert!(slots.len() <= offsets.len() * 8 && slots.len() <= 262_144);
            }
        }
    }

    #[test]
    fn disclosed_membership_keeps_navigation_independent_of_scene_order() {
        let mut game = Game::two_room_in_stone(42);
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 4, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut scene = game.scene(actor).unwrap();
        let mut ordered = game.clone();
        ordered.refresh_navigation_scene(actor, &scene);
        scene.reverse();
        game.refresh_navigation_scene(actor, &scene);
        assert_eq!(game.navigation, ordered.navigation);
        for destination in game.known_cells(actor) {
            assert_eq!(
                game.travel_route(actor, destination),
                ordered.travel_route(actor, destination)
            );
        }
    }

    #[test]
    fn grouped_sources_match_full_scan_for_rotated_portal_aliases_and_stale_links() {
        let at = |x, y| Location {
            region: RegionId(1),
            position: Position { x, y, z: 0 },
        };
        let mut world = tor_world::World::new(
            vec![tor_world::Region {
                id: RegionId(1),
                name: "aliases".into(),
                bounds: tor_world::Extent::new(5, 3, 2).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        world
            .connect(
                tor_world::Passage {
                    from: at(4, 1),
                    direction: Direction::East,
                    to: at(2, 0),
                },
                1,
            )
            .unwrap();
        world
            .connect(
                tor_world::Passage {
                    from: at(2, 0),
                    direction: Direction::North,
                    to: at(4, 1),
                },
                3,
            )
            .unwrap();
        let mut game = Game::new(world, 42);
        let actor = game
            .spawn_actor(at(2, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let scene = game.scene(actor).unwrap();
        assert!(
            scene.len()
                > scene
                    .iter()
                    .map(|c| c.location)
                    .collect::<BTreeSet<_>>()
                    .len()
        );
        for frame in [0, 1, 5, 12, 23] {
            game.actors.get_mut(&actor).unwrap().orientation = frame;
            for dark in [false, true, false] {
                game.set_region_light(RegionId(1), !dark).unwrap();
                let mut reference = game.clone();
                reference.reference_refresh_navigation();
                game.refresh_navigation();
                assert_eq!(
                    game.navigation, reference.navigation,
                    "frame {frame}, dark {dark}"
                );
            }
        }
        game.set_wall(at(3, 1), true).unwrap();
        let mut reference = game.clone();
        reference.reference_refresh_navigation();
        game.refresh_navigation();
        assert_eq!(game.navigation, reference.navigation);
    }

    #[test]
    fn dense_rotated_navigation_matches_full_scan_and_preserves_old_boundaries() {
        for cells in [2, 8] {
            let mut world = tor_world::World::new(vec![], vec![]).unwrap();
            world
                .add_chamber(tor_world::Region {
                    id: RegionId(1),
                    name: "dense-navigation".into(),
                    bounds: tor_world::Extent::new(32, 8, 8).unwrap(),
                })
                .unwrap();
            let mut game = Game::new(world, 42);
            let actors: Vec<_> = (0..8)
                .map(|n| {
                    let actor = game
                        .spawn_actor(
                            Location {
                                region: RegionId(1),
                                position: Position {
                                    x: 2 + n * 3,
                                    y: 3,
                                    z: 3,
                                },
                            },
                            NonZeroU64::new(100).unwrap(),
                        )
                        .unwrap();
                    game.set_body(
                        actor,
                        crate::BodySpec {
                            cells: if cells == 2 {
                                vec![[0, 0, 0], [0, 0, 1]]
                            } else {
                                (0..2)
                                    .flat_map(|x| {
                                        (0..2).flat_map(move |y| (0..2).map(move |z| [x, y, z]))
                                    })
                                    .collect()
                            },
                            eye: [0, 0, 1],
                            mass: 80,
                        },
                    )
                    .unwrap();
                    actor
                })
                .collect();
            for frame in [0, 5, 12, 23] {
                let old = game.clone();
                let old_navigation = old.navigation.clone();
                for &actor in &actors {
                    game.actors.get_mut(&actor).unwrap().orientation = frame;
                }
                let mut reference = game.clone();
                reference.reference_refresh_navigation();
                game.refresh_navigation();
                assert_eq!(
                    game.navigation, reference.navigation,
                    "cells {cells}, frame {frame}"
                );
                assert_eq!(old.navigation, old_navigation);
                for &actor in &actors {
                    let destinations: Vec<_> = game.known_cells(actor).collect();
                    assert!(!destinations.is_empty());
                    for index in [0, destinations.len() / 2, destinations.len() - 1] {
                        let destination = destinations[index];
                        assert_eq!(
                            game.travel_route(actor, destination),
                            reference.travel_route(actor, destination),
                            "cells {cells}, frame {frame}, actor {actor:?}"
                        );
                    }
                }
                let before = game.navigation.clone();
                game.refresh_navigation();
                assert_eq!(
                    game.navigation, before,
                    "repeated refresh must be idempotent"
                );
            }
        }
    }

    #[test]
    fn expanded_navigation_refresh_preserves_dark_stale_edges_and_reveals_edits() {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_chamber(tor_world::Region {
                id: RegionId(1),
                name: "lighted knowledge".into(),
                bounds: tor_world::Extent::new(36, 5, 2).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42);
        let cell = |x| Location {
            region: RegionId(1),
            position: Position { x, y: 2, z: 0 },
        };
        let actor = game
            .spawn_actor(cell(2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let refresh = |game: &mut Game| {
            let mut reference = game.clone();
            reference.reference_refresh_navigation();
            game.refresh_navigation();
            assert_eq!(game.navigation, reference.navigation);
        };
        refresh(&mut game);
        assert_eq!(game.navigation[&actor].cells.get(&cell(14)), Some(&false));
        let old = game.clone();
        game.set_region_light(RegionId(1), false).unwrap();
        game.set_wall(cell(14), true).unwrap();
        refresh(&mut game);
        assert_eq!(game.navigation[&actor].cells.get(&cell(14)), Some(&false));
        assert_eq!(old.navigation[&actor].cells.get(&cell(14)), Some(&false));
        game.set_cell_light(cell(14), true).unwrap();
        refresh(&mut game);
        assert_eq!(game.navigation[&actor].cells.get(&cell(14)), Some(&true));
        game.set_wall(cell(14), false).unwrap();
        refresh(&mut game);
        assert_eq!(game.navigation[&actor].cells.get(&cell(14)), Some(&false));
    }

    #[test]
    fn shared_search_matches_fresh_routes_across_target_orders_and_frames() {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        for id in 1..=2 {
            world
                .add_region(tor_world::Region {
                    id: RegionId(id),
                    name: format!("remembered-{id}"),
                    bounds: tor_world::Extent::new(12, 3, 1).unwrap(),
                })
                .unwrap();
        }
        let mut game = Game::new(world, 42);
        let cells: Vec<_> = (0..24)
            .map(|n| Location {
                region: RegionId(1 + n / 12),
                position: Position {
                    x: (n % 12) as i32,
                    y: 1,
                    z: 0,
                },
            })
            .collect();
        let actor = game
            .spawn_actor(cells[0], NonZeroU64::new(100).unwrap())
            .unwrap();
        // Remembered edges may remain after hidden topology edits. Exercise
        // directed cycles, portal frames, equal-cost choices and unreachable cells.
        let mut knowledge = Navigation::default();
        knowledge.cells.extend(cells.iter().map(|&at| (at, false)));
        for n in 0..20 {
            for (offset, direction) in DIRECTIONS.into_iter().enumerate() {
                knowledge.edges.insert(
                    (cells[n], direction),
                    (cells[(n + offset + 1) % 20], ((n + offset) % 24) as u8),
                );
            }
        }
        knowledge.cells.insert(cells[23], true);
        game.navigation
            .insert(actor, tor_world::Shared::new(knowledge));
        for frame in [0, 1, 5, 12, 23] {
            game.actors.get_mut(&actor).unwrap().orientation = frame;
            for order in 0..3 {
                let mut targets = cells.clone();
                match order {
                    1 => targets.reverse(),
                    2 => targets.rotate_left(11),
                    _ => {}
                }
                let mut search = game.route_search(actor).unwrap();
                for destination in targets.into_iter().chain(cells.iter().copied()) {
                    assert_eq!(
                        search.route(destination),
                        game.reference_travel_route(actor, destination),
                        "frame {frame}, order {order}, destination {destination:?}"
                    );
                }
            }
        }
    }

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

    #[test]
    fn a_two_cell_body_walks_every_step_of_a_route_round_a_doorway() {
        // Regression: a two-cell body needed both sides of a diagonal clear,
        // so routes past a doorway's corner, planned with one clear side,
        // stopped "blocked" beside the wall.
        use tor_world::{Extent, Passage, Region, World};
        // Two 7x5 chambers joined by a gap in the middle of a wall, as in
        // the first dungeon.
        let at = |region, x, y| Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        };
        let mut world = World::new(vec![], vec![]).unwrap();
        for id in [1, 2] {
            world
                .add_chamber(Region {
                    id: RegionId(id),
                    name: format!("Room {id}"),
                    bounds: Extent::new(7, 5, 2).unwrap(),
                })
                .unwrap();
        }
        for (from, direction, to) in [
            (at(1, 6, 2), Direction::East, at(2, 0, 2)),
            (at(2, 0, 2), Direction::West, at(1, 6, 2)),
        ] {
            world
                .connect_area(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                    1,
                    2,
                )
                .unwrap();
        }
        let mut game = Game::new(world, 42);
        // Beside the wall, north of the gap.
        let actor = game
            .spawn_actor(at(1, 5, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        game.refresh_navigation();
        // Straight into the gap, past the wall's corner.
        let mut corner = game.clone();
        corner
            .act(actor, Action::Move(Direction::SouthEast))
            .unwrap();
        let destinations: Vec<Location> = game.known_cells(actor).collect();
        let mut routes = 0;
        for destination in destinations {
            let Ok(route) = game.travel_route(actor, destination) else {
                continue;
            };
            let mut walker = game.clone();
            for step in &route {
                walker
                    .act(actor, Action::Move(step.direction))
                    .unwrap_or_else(|e| panic!("{destination:?} {step:?}: {e:?}"));
            }
            routes += 1;
        }
        assert!(routes > 20, "{routes}");
    }
}
