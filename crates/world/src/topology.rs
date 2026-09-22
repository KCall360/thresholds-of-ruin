use std::collections::{BTreeMap, BTreeSet};

use crate::{Extent, Material, Position, Terrain};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub region: RegionId,
    pub position: Position,
}

/// Directions in the current region's coordinate system; z increases upward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}

impl Direction {
    pub const HORIZONTAL: [Self; 8] = [
        Self::North,
        Self::East,
        Self::South,
        Self::West,
        Self::NorthEast,
        Self::SouthEast,
        Self::SouthWest,
        Self::NorthWest,
    ];

    pub fn components(self) -> Option<(Self, Self)> {
        match self {
            Self::NorthEast => Some((Self::North, Self::East)),
            Self::SouthEast => Some((Self::South, Self::East)),
            Self::SouthWest => Some((Self::South, Self::West)),
            Self::NorthWest => Some((Self::North, Self::West)),
            _ => None,
        }
    }

    pub fn delta(self) -> (i32, i32, i32) {
        match self {
            Self::NorthEast => (1, -1, 0),
            Self::SouthEast => (1, 1, 0),
            Self::SouthWest => (-1, 1, 0),
            Self::NorthWest => (-1, -1, 0),
            Self::North => (0, -1, 0),
            Self::East => (1, 0, 0),
            Self::South => (0, 1, 0),
            Self::West => (-1, 0, 0),
            Self::Up => (0, 0, 1),
            Self::Down => (0, 0, -1),
        }
    }

    pub(crate) fn offset(self, position: Position) -> Option<Position> {
        let (x, y, z) = self.delta();
        Some(Position {
            x: position.x.checked_add(x)?,
            y: position.y.checked_add(y)?,
            z: position.z.checked_add(z)?,
        })
    }

    pub fn rotated(self, turns: u8) -> Self {
        let directions = if self.components().is_some() {
            [
                Self::NorthEast,
                Self::SouthEast,
                Self::SouthWest,
                Self::NorthWest,
            ]
        } else {
            [Self::North, Self::East, Self::South, Self::West]
        };
        match directions.iter().position(|d| *d == self) {
            Some(index) => directions[(index + usize::from(turns)) % 4],
            None => self,
        }
    }
}

/// A bounded rectangular volume with region-local coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
    pub bounds: Extent,
}

/// A directed, single-cell connection. Horizontal apertures are at boundaries;
/// explicit vertical links can represent stairs within a room. Rotation metadata
/// is validated by `World::connect`. A passage need not contain a door entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passage {
    pub from: Location,
    pub direction: Direction,
    pub to: Location,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldError {
    DuplicateRegion,
    InvalidEndpoint,
    NotBoundaryExit,
    DuplicateExit,
    InvalidRotation,
}

/// Validated region topology with stable iteration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    doors: BTreeMap<Location, Door>,
    regions: BTreeMap<RegionId, Region>,
    passages: BTreeMap<(Location, Direction), Passage>,
    rotations: BTreeMap<(Location, Direction), u8>,
    terrain: BTreeMap<Location, Terrain>,
    /// Carved interior extents also locate join apertures; storage includes a shell.
    chambers: BTreeMap<RegionId, Extent>,
    place_hints: BTreeSet<Location>,
}

/// A cell-sized barrier entity, unrelated to portal identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Door {
    pub id: u64,
    pub open: bool,
}

impl World {
    pub fn door(&self, location: Location) -> Option<Door> {
        self.doors.get(&location).copied()
    }
    pub fn door_location(&self, id: u64) -> Option<Location> {
        self.doors
            .iter()
            .find_map(|(location, door)| (door.id == id).then_some(*location))
    }
    pub fn place_door(
        &mut self,
        location: Location,
        id: u64,
        open: bool,
    ) -> Result<(), WorldError> {
        if !self.walkable(location)
            || self.doors.contains_key(&location)
            || self.door_location(id).is_some()
        {
            return Err(WorldError::InvalidEndpoint);
        }
        self.doors.insert(location, Door { id, open });
        Ok(())
    }
    pub fn set_door(&mut self, location: Location, open: bool) {
        self.doors.get_mut(&location).expect("validated door").open = open;
    }
    pub fn opaque(&self, location: Location) -> bool {
        self.is_wall(location) || self.door(location).is_some_and(|d| !d.open)
    }
    /// The perceived adjacent cell, including a closed barrier at the destination.
    pub fn adjacent(&self, from: Location, direction: Direction) -> Option<Location> {
        if direction.components().is_some() {
            return self
                .diagonal_reach(from, direction, |_| true)
                .map(|(to, _)| to);
        }
        self.sight_step(from, direction).map(|(to, _)| to)
    }

    pub fn new(regions: Vec<Region>, passages: Vec<Passage>) -> Result<Self, WorldError> {
        let mut world = Self {
            doors: BTreeMap::new(),
            regions: BTreeMap::new(),
            passages: BTreeMap::new(),
            rotations: BTreeMap::new(),
            terrain: BTreeMap::new(),
            chambers: BTreeMap::new(),
            place_hints: BTreeSet::new(),
        };
        for region in regions {
            if world.regions.insert(region.id, region).is_some() {
                return Err(WorldError::DuplicateRegion);
            }
        }
        for passage in passages {
            world.connect(passage, 0)?;
        }
        Ok(world)
    }

    pub fn add_region(&mut self, region: Region) -> Result<(), WorldError> {
        if self.regions.contains_key(&region.id) {
            return Err(WorldError::DuplicateRegion);
        }
        self.regions.insert(region.id, region);
        Ok(())
    }

    /// Allocate finite stone and carve the requested interior. Joins override
    /// adjacent shell cells, so a storage partition does not create a barrier.
    pub fn add_chamber(&mut self, mut region: Region) -> Result<(), WorldError> {
        let interior = region.bounds;
        region.bounds = interior.with_shell().ok_or(WorldError::InvalidEndpoint)?;
        let id = region.id;
        self.add_region(region)?;
        self.chambers.insert(id, interior);
        Ok(())
    }

    pub fn terrain(&self, location: Location) -> Option<Terrain> {
        if !self.contains(location) {
            return None;
        }
        Some(self.terrain.get(&location).copied().unwrap_or_else(|| {
            if self
                .chambers
                .get(&location.region)
                .is_some_and(|bounds| !bounds.contains(location.position))
            {
                Terrain::Solid(Material::Stone)
            } else {
                Terrain::Empty
            }
        }))
    }

    pub fn is_chamber(&self, region: RegionId) -> bool {
        self.chambers.contains_key(&region)
    }

    /// Conservative vertical surface probe in allocated space. It follows no
    /// stair teleport, stops at the first obstruction, and never invents a shell.
    pub fn vertical_surface(
        &self,
        from: Location,
        direction: Direction,
        range: u32,
    ) -> Option<(Material, u32)> {
        if !matches!(direction, Direction::Up | Direction::Down) || !self.walkable(from) {
            return None;
        }
        let mut location = from;
        for distance in 1..=range.min(16) {
            location.position = direction.offset(location.position)?;
            match self.terrain(location)? {
                Terrain::Solid(material) => return Some((material, distance)),
                Terrain::Empty if self.opaque(location) => return None,
                Terrain::Empty => {}
            }
        }
        None
    }

    pub(crate) fn within_aperture_bounds(&self, location: Location) -> bool {
        self.chambers
            .get(&location.region)
            .or_else(|| self.region(location.region).map(|r| &r.bounds))
            .is_some_and(|bounds| bounds.contains(location.position))
    }

    /// Clockwise quarter turns about z transform sight after crossing. Connections
    /// are directed: callers must explicitly construct and validate reverse links.
    pub fn connect(&mut self, passage: Passage, quarter_turns: u8) -> Result<(), WorldError> {
        if passage.direction.components().is_some() {
            return Err(WorldError::InvalidEndpoint);
        }
        if quarter_turns > 3
            || (matches!(passage.direction, Direction::Up | Direction::Down) && quarter_turns != 0)
        {
            return Err(WorldError::InvalidRotation);
        }
        if !self.walkable(passage.from) || !self.walkable(passage.to) {
            return Err(WorldError::InvalidEndpoint);
        }
        if !matches!(passage.direction, Direction::Up | Direction::Down)
            && passage
                .direction
                .offset(passage.from.position)
                .is_some_and(|position| {
                    self.chambers
                        .get(&passage.from.region)
                        .or_else(|| self.region(passage.from.region).map(|r| &r.bounds))
                        .is_some_and(|bounds| bounds.contains(position))
                })
        {
            return Err(WorldError::NotBoundaryExit);
        }
        let key = (passage.from, passage.direction);
        if self.passages.contains_key(&key) {
            return Err(WorldError::DuplicateExit);
        }
        self.passages.insert(key, passage);
        self.rotations.insert(key, quarter_turns);
        Ok(())
    }

    /// Atomically glue a rectangular aperture with a single affine transform.
    /// Width follows +y for east/west faces and +x otherwise; height follows
    /// +z for horizontal exits and +y for vertical exits. Destination offsets
    /// are rotated by the same transform as the continuing direction.
    pub fn connect_area(
        &mut self,
        anchor: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), WorldError> {
        if width == 0 || height == 0 || u32::from(width) * u32::from(height) > 1024 {
            return Err(WorldError::InvalidEndpoint);
        }
        let mut candidate = self.clone();
        for u in 0..i32::from(width) {
            for v in 0..i32::from(height) {
                let (x, y, z) = match anchor.direction {
                    Direction::East | Direction::West => (0, u, v),
                    Direction::North | Direction::South => (u, 0, v),
                    Direction::Up | Direction::Down => (u, v, 0),
                    _ => return Err(WorldError::InvalidEndpoint),
                };
                let (rx, ry) = match turns {
                    0 => (x, y),
                    1 => (-y, x),
                    2 => (-x, -y),
                    3 => (y, -x),
                    _ => return Err(WorldError::InvalidRotation),
                };
                let offset =
                    |location: Location, x: i32, y: i32, z: i32| -> Result<Location, WorldError> {
                        Ok(Location {
                            position: Position {
                                x: location
                                    .position
                                    .x
                                    .checked_add(x)
                                    .ok_or(WorldError::InvalidEndpoint)?,
                                y: location
                                    .position
                                    .y
                                    .checked_add(y)
                                    .ok_or(WorldError::InvalidEndpoint)?,
                                z: location
                                    .position
                                    .z
                                    .checked_add(z)
                                    .ok_or(WorldError::InvalidEndpoint)?,
                            },
                            ..location
                        })
                    };
                candidate.connect(
                    Passage {
                        from: offset(anchor.from, x, y, z)?,
                        direction: anchor.direction,
                        to: offset(anchor.to, rx, ry, z)?,
                    },
                    turns,
                )?;
            }
        }
        *self = candidate;
        Ok(())
    }

    pub fn crossing_rotation(&self, from: Location, direction: Direction) -> u8 {
        if direction.components().is_some() {
            return self
                .diagonal_reach(from, direction, |_| true)
                .map_or(0, |(_, turns)| turns);
        }
        self.rotations.get(&(from, direction)).copied().unwrap_or(0)
    }

    pub fn set_wall(&mut self, location: Location, wall: bool) -> Result<(), WorldError> {
        if !self.contains(location) || (wall && self.doors.contains_key(&location)) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.terrain.insert(
            location,
            if wall {
                Terrain::Solid(Material::Stone)
            } else {
                Terrain::Empty
            },
        );
        Ok(())
    }

    /// Unnamed spatial anchors, independent of region boundaries and topology.
    /// Terrain edits retain authored hints; solid cells suppress their disclosure.
    pub fn set_place_hint(&mut self, location: Location, present: bool) -> Result<(), WorldError> {
        if !self.contains(location) {
            return Err(WorldError::InvalidEndpoint);
        }
        if present {
            self.place_hints.insert(location);
        } else {
            self.place_hints.remove(&location);
        }
        Ok(())
    }

    pub fn has_place_hint(&self, location: Location) -> bool {
        self.place_hints.contains(&location) && self.walkable(location)
    }

    pub fn is_wall(&self, location: Location) -> bool {
        matches!(self.terrain(location), Some(Terrain::Solid(_)))
    }

    pub fn walkable(&self, location: Location) -> bool {
        self.contains(location) && !self.opaque(location)
    }

    pub fn passage(&self, from: Location, direction: Direction) -> Option<&Passage> {
        self.passages.get(&(from, direction))
    }

    pub(crate) fn sight_step(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if !self.walkable(from) {
            return None;
        }
        self.geometry_step(from, direction)
    }

    /// Topology only: opaque cells still have geometric neighbors.
    pub(crate) fn geometry_step(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if let Some(passage) = self.passage(from, direction) {
            return Some((passage.to, self.rotations[&(from, direction)]));
        }
        if self.chambers.contains_key(&from.region) {
            return self.rim_step(from, direction);
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }

    /// Resolve a consistent transform across the solid portion of a chamber
    /// face. A solid/floor rim is as important as a solid/solid rim: the former
    /// occurs at narrow doors. This never creates an unadvertised floor/floor
    /// opening. Several incompatible transforms leave the geometry unresolved.
    fn rim_step(&self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        let mut projected = None;
        for passage in self.exits(from.region).filter(|p| p.direction == direction) {
            let a = passage.from.position;
            let b = from.position;
            let same_plane = match direction {
                Direction::East | Direction::West => a.x == b.x,
                Direction::North | Direction::South => a.y == b.y,
                Direction::Up | Direction::Down => a.z == b.z,
                _ => return None,
            };
            if !same_plane {
                continue;
            }
            let dx = i64::from(b.x) - i64::from(a.x);
            let dy = i64::from(b.y) - i64::from(a.y);
            let dz = i64::from(b.z) - i64::from(a.z);
            let turns = self.crossing_rotation(passage.from, direction);
            let (dx, dy) = match turns {
                0 => (dx, dy),
                1 => (-dy, dx),
                2 => (-dx, -dy),
                _ => (dy, -dx),
            };
            let to = Location {
                region: passage.to.region,
                position: Position {
                    x: i32::try_from(i64::from(passage.to.position.x) + dx).ok()?,
                    y: i32::try_from(i64::from(passage.to.position.y) + dy).ok()?,
                    z: i32::try_from(i64::from(passage.to.position.z) + dz).ok()?,
                },
            };
            let candidate = (to, turns);
            if projected.is_some_and(|old| old != candidate) {
                return None;
            }
            projected = Some(candidate);
        }
        if let Some((to, turns)) = projected {
            if self.contains(to) && (self.is_wall(from) || self.is_wall(to)) {
                return Some((to, turns));
            }
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }

    pub fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.get(&id)
    }

    pub fn contains(&self, location: Location) -> bool {
        self.region(location.region)
            .is_some_and(|region| region.bounds.contains(location.position))
    }

    pub fn exits(&self, region: RegionId) -> impl Iterator<Item = &Passage> {
        self.passages
            .values()
            .filter(move |passage| passage.from.region == region)
    }

    /// Actual movement topology, excluding sight-only material rim projection.
    pub fn movement_neighbor(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if direction.components().is_some() || !self.walkable(from) {
            return None;
        }
        if let Some(passage) = self.passage(from, direction) {
            return Some((passage.to, self.crossing_rotation(from, direction)));
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }

    /// Resolve diagonal reach through at least one clear side. Destination may
    /// contain a closed door; callers validate destination occupancy separately.
    pub fn diagonal_reach(
        &self,
        from: Location,
        direction: Direction,
        clear: impl Fn(Location) -> bool,
    ) -> Option<(Location, u8)> {
        let (a, b) = direction.components()?;
        let route = |first, second: Direction| {
            let (side, r1) = self.movement_neighbor(from, first)?;
            if !self.walkable(side) || !clear(side) {
                return None;
            }
            let (to, r2) = self.movement_neighbor(side, second.rotated(r1))?;
            if self.is_wall(to) {
                return None;
            }
            Some((to, (r1 + r2) % 4))
        };
        match (route(a, b), route(b, a)) {
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        }
    }

    /// Resolve geometry only. Occupancy and action costs belong to simulation.
    pub fn step(&self, from: Location, direction: Direction) -> Option<Location> {
        let to = self.adjacent(from, direction)?;
        self.walkable(to).then_some(to)
    }
}
