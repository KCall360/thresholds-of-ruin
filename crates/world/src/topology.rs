use std::collections::BTreeMap;

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
    fn offset(self, position: Position) -> Option<Position> {
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
}

/// An unobstructed rectangular volume. Interior terrain is a later extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
    pub bounds: Extent,
}

/// A directed, single-cell boundary connection.
///
/// This first slice supports translated endpoints, not rotated orientation or
/// multi-cell portal apertures. A doorway need not contain a door entity.
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
}

/// Validated region topology with stable iteration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    regions: BTreeMap<RegionId, Region>,
    passages: BTreeMap<(Location, Direction), Passage>,
}

impl World {
    pub fn new(regions: Vec<Region>, passages: Vec<Passage>) -> Result<Self, WorldError> {
        let mut world = Self {
            regions: BTreeMap::new(),
            passages: BTreeMap::new(),
        };
        for region in regions {
            if world.regions.insert(region.id, region).is_some() {
                return Err(WorldError::DuplicateRegion);
            }
        }
        for passage in passages {
            if !world.contains(passage.from) || !world.contains(passage.to) {
                return Err(WorldError::InvalidEndpoint);
            }
            if passage
                .direction
                .offset(passage.from.position)
                .is_some_and(|position| {
                    world.contains(Location {
                        position,
                        ..passage.from
                    })
                })
            {
                return Err(WorldError::NotBoundaryExit);
            }
            if world
                .passages
                .insert((passage.from, passage.direction), passage)
                .is_some()
            {
                return Err(WorldError::DuplicateExit);
            }
        }
        Ok(world)
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
        if !self.contains(from) {
            return None;
        }
        if let Some(passage) = self.passages.get(&(from, direction)) {
            return Some(passage.to);
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some(to)
    }
}
