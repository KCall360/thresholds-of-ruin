use std::collections::{BTreeMap, BTreeSet};

use crate::{Extent, Position};

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
    Up,
    Down,
}

impl Direction {
    pub(crate) fn offset(self, position: Position) -> Option<Position> {
        let (x, y, z) = match self {
            Self::North => (0, -1, 0),
            Self::East => (1, 0, 0),
            Self::South => (0, 1, 0),
            Self::West => (-1, 0, 0),
            Self::Up => (0, 0, 1),
            Self::Down => (0, 0, -1),
        };
        Some(Position {
            x: position.x.checked_add(x)?,
            y: position.y.checked_add(y)?,
            z: position.z.checked_add(z)?,
        })
    }

    pub fn rotated(self, turns: u8) -> Self {
        let directions = [Self::North, Self::East, Self::South, Self::West];
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
    regions: BTreeMap<RegionId, Region>,
    passages: BTreeMap<(Location, Direction), Passage>,
    rotations: BTreeMap<(Location, Direction), u8>,
    walls: BTreeSet<Location>,
}

impl World {
    pub fn new(regions: Vec<Region>, passages: Vec<Passage>) -> Result<Self, WorldError> {
        let mut world = Self {
            regions: BTreeMap::new(),
            passages: BTreeMap::new(),
            rotations: BTreeMap::new(),
            walls: BTreeSet::new(),
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

    /// Clockwise quarter turns about z transform sight after crossing. Connections
    /// are directed: callers must explicitly construct and validate reverse links.
    pub fn connect(&mut self, passage: Passage, quarter_turns: u8) -> Result<(), WorldError> {
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
                    self.contains(Location {
                        position,
                        ..passage.from
                    })
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
        self.rotations.get(&(from, direction)).copied().unwrap_or(0)
    }

    pub fn set_wall(&mut self, location: Location, wall: bool) -> Result<(), WorldError> {
        if !self.contains(location) {
            return Err(WorldError::InvalidEndpoint);
        }
        if wall {
            self.walls.insert(location);
        } else {
            self.walls.remove(&location);
        }
        Ok(())
    }

    pub fn is_wall(&self, location: Location) -> bool {
        self.walls.contains(&location)
    }

    pub fn walkable(&self, location: Location) -> bool {
        self.contains(location) && !self.is_wall(location)
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
        if let Some(passage) = self.passage(from, direction) {
            return Some((passage.to, self.rotations[&(from, direction)]));
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

    /// Resolve geometry only. Occupancy and action costs belong to simulation.
    pub fn step(&self, from: Location, direction: Direction) -> Option<Location> {
        let (to, _) = self.sight_step(from, direction)?;
        self.walkable(to).then_some(to)
    }
}
