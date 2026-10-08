use std::collections::{BTreeMap, BTreeSet};

use crate::{Extent, Material, Position, Shared, Terrain};

#[path = "world_checkpoint.rs"]
pub mod checkpoint;
#[path = "region_slice.rs"]
mod region_slice;
pub use region_slice::RegionSlice;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct RegionId(pub u64);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub region: RegionId,
    pub position: Position,
}

/// Directions in the current region's coordinate system; z increases upward.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
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
    EastUp,
    WestUp,
    NorthUp,
    SouthUp,
    EastDown,
    WestDown,
    NorthDown,
    SouthDown,
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

    #[inline]
    pub fn components(self) -> Option<(Self, Self)> {
        match self {
            Self::NorthEast => Some((Self::North, Self::East)),
            Self::SouthEast => Some((Self::South, Self::East)),
            Self::SouthWest => Some((Self::South, Self::West)),
            Self::NorthWest => Some((Self::North, Self::West)),
            Self::EastUp => Some((Self::East, Self::Up)),
            Self::WestUp => Some((Self::West, Self::Up)),
            Self::NorthUp => Some((Self::North, Self::Up)),
            Self::SouthUp => Some((Self::South, Self::Up)),
            Self::EastDown => Some((Self::East, Self::Down)),
            Self::WestDown => Some((Self::West, Self::Down)),
            Self::NorthDown => Some((Self::North, Self::Down)),
            Self::SouthDown => Some((Self::South, Self::Down)),
            _ => None,
        }
    }

    #[inline]
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
            Self::EastUp => (1, 0, 1),
            Self::WestUp => (-1, 0, 1),
            Self::NorthUp => (0, -1, 1),
            Self::SouthUp => (0, 1, 1),
            Self::EastDown => (1, 0, -1),
            Self::WestDown => (-1, 0, -1),
            Self::NorthDown => (0, -1, -1),
            Self::SouthDown => (0, 1, -1),
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

    #[inline]
    pub fn from_delta(v: [i64; 3]) -> Option<Self> {
        Some(match v {
            [0, -1, 0] => Self::North,
            [1, 0, 0] => Self::East,
            [0, 1, 0] => Self::South,
            [-1, 0, 0] => Self::West,
            [0, 0, 1] => Self::Up,
            [0, 0, -1] => Self::Down,
            [1, -1, 0] => Self::NorthEast,
            [1, 1, 0] => Self::SouthEast,
            [-1, 1, 0] => Self::SouthWest,
            [-1, -1, 0] => Self::NorthWest,
            [1, 0, 1] => Self::EastUp,
            [-1, 0, 1] => Self::WestUp,
            [0, -1, 1] => Self::NorthUp,
            [0, 1, 1] => Self::SouthUp,
            [1, 0, -1] => Self::EastDown,
            [-1, 0, -1] => Self::WestDown,
            [0, -1, -1] => Self::NorthDown,
            [0, 1, -1] => Self::SouthDown,
            _ => return None,
        })
    }
    #[inline]
    pub fn rotated(self, turns: u8) -> Self {
        if turns == 0 {
            return self;
        }
        let (x, y, z) = self.delta();
        Self::from_delta(crate::rotate_vector(
            turns,
            [i64::from(x), i64::from(y), i64::from(z)],
        ))
        .expect("cube direction")
    }
}

/// A bounded rectangular volume with region-local coordinates.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
    pub bounds: Extent,
}

/// A directed, single-cell connection. Horizontal apertures are at boundaries;
/// explicit vertical links can represent stairs within a room. Rotation metadata
/// is validated by `World::connect`. A passage need not contain a door entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    #[serde(with = "crate::checkpoint_map::shared")]
    doors: Shared<BTreeMap<Location, Door>>,
    regions: Shared<BTreeMap<RegionId, Region>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    passages: Shared<BTreeMap<(Location, Direction), Passage>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    rotations: Shared<BTreeMap<(Location, Direction), u8>>,
    physical_vertical: Shared<BTreeSet<(Location, Direction)>>,
    region_gravity: Shared<BTreeMap<RegionId, [i32; 3]>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    cell_gravity: Shared<BTreeMap<Location, [i32; 3]>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    terrain: Shared<BTreeMap<Location, Terrain>>,
    /// Carved interior extents also locate join apertures; storage includes a shell.
    chambers: Shared<BTreeMap<RegionId, Extent>>,
    /// Authored place hints, each with its authored name ("" when it has
    /// none).
    #[serde(with = "crate::checkpoint_map::shared")]
    place_hints: Shared<BTreeMap<Location, String>>,
    /// Metadata of detached regions, whose content is held elsewhere as a
    /// [`RegionSlice`]. Omitted from saves while empty.
    #[serde(default, skip_serializing_if = "no_absent_regions")]
    absent: Shared<BTreeMap<RegionId, Region>>,
    /// Derived: never saved, and ignored by equality.
    #[serde(skip)]
    pub(crate) sight: crate::sight_cache::SightCache,
}

/// A barrier entity, unrelated to portal identity. It is stored at its base
/// cell and occupies `height` cells straight up from it, filling its opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Door {
    pub id: u64,
    pub open: bool,
    pub height: u8,
}

/// Doors are at most this many cells tall.
pub const MAX_DOOR_HEIGHT: u8 = 8;

fn no_absent_regions(absent: &Shared<BTreeMap<RegionId, Region>>) -> bool {
    absent.is_empty()
}

fn raised(location: Location, cells: i32) -> Option<Location> {
    Some(Location {
        position: Position {
            z: location.position.z.checked_add(cells)?,
            ..location.position
        },
        ..location
    })
}

impl World {
    /// Invalidates derived geometry mappings when any topology or region changes.
    /// Tokens only select cache reuse; they never affect world behavior.
    pub fn geometry_snapshot(&self) -> crate::GeometrySnapshot {
        self.sight.geometry_snapshot()
    }
    /// Authored portal endpoints must remain clear of solid terrain. Closed
    /// doors are ordinary gameplay state and do not invalidate a package.
    pub fn authored_links_clear(&self) -> bool {
        self.passages
            .values()
            .all(|p| !self.is_wall(p.from) && !self.is_wall(p.to))
    }
    /// Structural validation for backend checkpoint restoration.
    pub fn checkpoint_valid(&self, next_door_id: u64) -> bool {
        let mut ids = BTreeSet::new();
        if !self
            .region_gravity
            .iter()
            .all(|(r, g)| self.region(*r).is_some() && valid_gravity(*g))
            || !self
                .cell_gravity
                .iter()
                .all(|(at, g)| self.contains(*at) && valid_gravity(*g))
        {
            return false;
        }
        if self.physical_vertical.iter().any(|(at, d)| {
            !matches!(d, Direction::Up | Direction::Down) || self.passage(*at, *d).is_none()
        }) {
            return false;
        }
        if !self.absent.iter().all(|(id, region)| {
            *id == region.id && !self.regions.contains_key(id) && {
                let (x, y, z) = region.bounds.dimensions();
                x > 0 && y > 0 && z > 0
            }
        }) {
            return false;
        }
        self.regions.iter().all(|(id, region)| {
            *id == region.id && {
                let (x, y, z) = region.bounds.dimensions();
                x > 0 && y > 0 && z > 0
            }
        }) && self.doors.iter().all(|(location, door)| {
            (1..=MAX_DOOR_HEIGHT).contains(&door.height)
                && door.id > 0
                && door.id < next_door_id
                && ids.insert(door.id)
                && self.door_cells(*location).all(|cell| {
                    // Every cell exists and belongs to this door alone.
                    self.contains(cell)
                        && self.door_entry(cell).map(|(base, _)| base) == Some(*location)
                })
                && self.door_cells(*location).count() == usize::from(door.height)
        }) && self.terrain.keys().all(|location| self.contains(*location))
            && self
                .place_hints
                .keys()
                .all(|location| self.contains(*location))
            && self.chambers.keys().all(|id| self.regions.contains_key(id))
            && self.passages.iter().all(|((from, direction), passage)| {
                // A link may lead into a detached region.
                *from == passage.from
                    && *direction == passage.direction
                    && self.contains(*from)
                    && self.knows(passage.to)
            })
            && self
                .rotations
                .iter()
                .all(|(key, rotation)| *rotation < 24 && self.passages.contains_key(key))
    }

    /// The base cell and door occupying `location`, if any. Door cells differ
    /// only in z, the last key component, so one short range query finds it.
    fn door_entry(&self, location: Location) -> Option<(Location, Door)> {
        let lowest = raised(location, -i32::from(MAX_DOOR_HEIGHT - 1)).unwrap_or(Location {
            position: Position {
                z: i32::MIN,
                ..location.position
            },
            ..location
        });
        self.doors
            .range(lowest..=location)
            .next_back()
            .filter(|(base, door)| {
                i64::from(location.position.z) - i64::from(base.position.z) < i64::from(door.height)
            })
            .map(|(base, door)| (*base, *door))
    }
    /// The door occupying `location`, whichever of its cells that is.
    pub fn door(&self, location: Location) -> Option<Door> {
        self.door_entry(location).map(|(_, door)| door)
    }
    /// A door's base cell.
    pub fn door_location(&self, id: u64) -> Option<Location> {
        self.doors
            .iter()
            .find_map(|(location, door)| (door.id == id).then_some(*location))
    }
    /// The cells of the door based at `base`, from the bottom up.
    pub fn door_cells(&self, base: Location) -> impl Iterator<Item = Location> + '_ {
        let height = self.doors.get(&base).map_or(0, |door| door.height);
        (0..i32::from(height)).filter_map(move |cells| raised(base, cells))
    }
    fn door_free(&self, cell: Location) -> bool {
        self.contains(cell) && !self.is_wall(cell) && self.door(cell).is_none()
    }
    /// How tall a door based at `location` could be: the run of contained,
    /// non-solid, door-free cells straight up from it, at most
    /// [`MAX_DOOR_HEIGHT`].
    pub fn door_clearance(&self, location: Location) -> u8 {
        (0..MAX_DOOR_HEIGHT)
            .take_while(|&cells| {
                raised(location, i32::from(cells)).is_some_and(|cell| self.door_free(cell))
            })
            .count() as u8
    }
    /// Whether a door based at `base`, `height` cells tall, leaves its doorway
    /// open above it: the cell above its top is open and, like its top cell,
    /// walled on both sides along the same horizontal axis. Anything can then
    /// be seen over the door. A door in a low wall under open space isn't
    /// flagged, and a doorway at a region join, which has no walls beside it
    /// in the room, can't be judged this way.
    pub fn doorway_open_above(&self, base: Location, height: u8) -> bool {
        let walled = |cell: Location, dx: i32, dy: i32| {
            [-1, 1].into_iter().all(|sign| {
                let side = Location {
                    position: Position {
                        x: cell.position.x.saturating_add(sign * dx),
                        y: cell.position.y.saturating_add(sign * dy),
                        ..cell.position
                    },
                    ..cell
                };
                !self.contains(side) || self.is_wall(side)
            })
        };
        let (Some(top), Some(above)) = (
            raised(base, i32::from(height) - 1),
            raised(base, i32::from(height)),
        ) else {
            return false;
        };
        self.door_free(above)
            && [(1, 0), (0, 1)]
                .into_iter()
                .any(|(dx, dy)| walled(top, dx, dy) && walled(above, dx, dy))
    }
    /// Place a door `height` cells tall, whose cells must all be free.
    pub fn place_door(
        &mut self,
        location: Location,
        id: u64,
        open: bool,
        height: u8,
    ) -> Result<(), WorldError> {
        if !(1..=MAX_DOOR_HEIGHT).contains(&height)
            || self.door_clearance(location) < height
            || self.door_location(id).is_some()
        {
            return Err(WorldError::InvalidEndpoint);
        }
        self.doors.insert(location, Door { id, open, height });
        self.sight.region_changed(location.region);
        Ok(())
    }
    /// Open or close the door occupying `location`.
    pub fn set_door(&mut self, location: Location, open: bool) {
        let (base, _) = self.door_entry(location).expect("validated door");
        self.doors.get_mut(&base).expect("validated door").open = open;
        self.sight.region_changed(base.region);
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
            doors: Shared::new(BTreeMap::new()),
            regions: Shared::new(BTreeMap::new()),
            passages: Shared::new(BTreeMap::new()),
            rotations: Shared::new(BTreeMap::new()),
            physical_vertical: Shared::default(),
            region_gravity: Shared::default(),
            cell_gravity: Shared::default(),
            terrain: Shared::new(BTreeMap::new()),
            chambers: Shared::new(BTreeMap::new()),
            place_hints: Shared::new(BTreeMap::new()),
            absent: Shared::default(),
            sight: Default::default(),
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
        self.sight.topology_changed();
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
        self.sight.topology_changed();
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
        if quarter_turns >= 24 {
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
        self.sight.topology_changed();
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
                if turns >= 24 {
                    return Err(WorldError::InvalidRotation);
                }
                let [rx, ry, rz] =
                    crate::rotate_vector(turns, [i64::from(x), i64::from(y), i64::from(z)])
                        .map(|v| v as i32);
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
                        to: offset(anchor.to, rx, ry, rz)?,
                    },
                    turns,
                )?;
            }
        }
        *self = candidate;
        Ok(())
    }

    /// Physical apertures are distinct from explicit stair traversal links.
    pub fn connect_portal_area(
        &mut self,
        anchor: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), WorldError> {
        if anchor
            .direction
            .offset(anchor.from.position)
            .is_some_and(|position| {
                self.within_aperture_bounds(Location {
                    position,
                    ..anchor.from
                })
            })
        {
            return Err(WorldError::NotBoundaryExit);
        }
        let mut candidate = self.clone();
        candidate.connect_area(anchor, turns, width, height)?;
        if matches!(anchor.direction, Direction::Up | Direction::Down) {
            for u in 0..i32::from(width) {
                for v in 0..i32::from(height) {
                    let mut at = anchor.from;
                    at.position.x += u;
                    at.position.y += v;
                    candidate.physical_vertical.insert((at, anchor.direction));
                    candidate.sight.topology_changed();
                }
            }
        }
        *self = candidate;
        Ok(())
    }
    pub fn physics_neighbor(&self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        if !matches!(direction, Direction::Up | Direction::Down)
            || self.physical_vertical.contains(&(from, direction))
        {
            return self.movement_neighbor(from, direction);
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }
    pub fn is_stair(&self, from: Location, direction: Direction) -> bool {
        matches!(direction, Direction::Up | Direction::Down)
            && self.passage(from, direction).is_some()
            && !self.physical_vertical.contains(&(from, direction))
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
        if !self.contains(location) || (wall && self.door(location).is_some()) {
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
        self.sight.region_changed(location.region);
        Ok(())
    }

    /// Unnamed spatial anchors, independent of region boundaries and topology.
    /// Terrain edits retain authored hints; solid cells suppress their disclosure.
    pub fn set_place_hint(&mut self, location: Location, present: bool) -> Result<(), WorldError> {
        if !self.contains(location) {
            return Err(WorldError::InvalidEndpoint);
        }
        if present {
            // An existing hint keeps its authored name.
            self.place_hints.entry(location).or_default();
        } else {
            self.place_hints.remove(&location);
        }
        Ok(())
    }

    /// A place hint with an authored name the character learns on seeing it.
    pub fn set_named_place_hint(
        &mut self,
        location: Location,
        name: &str,
    ) -> Result<(), WorldError> {
        if !self.contains(location) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.place_hints.insert(location, name.to_owned());
        Ok(())
    }

    pub fn has_place_hint(&self, location: Location) -> bool {
        self.place_hints.contains_key(&location) && self.walkable(location)
    }

    /// The authored name of the place hint here, if it has one.
    pub fn place_name(&self, location: Location) -> Option<&str> {
        self.place_hints
            .get(&location)
            .map(String::as_str)
            .filter(|name| !name.is_empty())
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
            let [dx, dy, dz] = crate::rotate_vector(turns, [dx, dy, dz]);
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

    pub fn gravity(&self, at: Location) -> Option<[i32; 3]> {
        self.cell_gravity
            .get(&at)
            .or_else(|| self.region_gravity.get(&at.region))
            .copied()
    }
    pub fn has_gravity(&self) -> bool {
        !self.region_gravity.is_empty() || !self.cell_gravity.is_empty()
    }
    pub fn set_gravity(&mut self, region: RegionId, vector: [i32; 3]) -> Result<(), WorldError> {
        if self.region(region).is_none() || !valid_gravity(vector) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.region_gravity.insert(region, vector);
        Ok(())
    }
    pub fn set_cell_gravity(&mut self, at: Location, vector: [i32; 3]) -> Result<(), WorldError> {
        if !self.walkable(at) || !valid_gravity(vector) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.cell_gravity.insert(at, vector);
        Ok(())
    }
    pub fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.get(&id)
    }

    pub fn contains(&self, location: Location) -> bool {
        self.region(location.region)
            .is_some_and(|region| region.bounds.contains(location.position))
    }

    pub fn exits(&self, region: RegionId) -> impl Iterator<Item = &Passage> {
        let start = Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        let end = Location {
            region,
            position: Position {
                x: i32::MAX,
                y: i32::MAX,
                z: i32::MAX,
            },
        };
        self.passages
            .range((start, Direction::North)..=(end, Direction::Down))
            .map(|(_, passage)| passage)
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
            Some((to, crate::compose_rotation(r1, r2)))
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

#[cfg(test)]
mod sharing_tests {
    use super::*;
    #[test]
    fn door_edits_share_large_geometry_and_keep_the_previous_boundary_unchanged() {
        let regions = (1..=256)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room-{id}"),
                bounds: Extent::new(9, 9, 2).unwrap(),
            })
            .collect();
        let mut world = World::new(regions, vec![]).unwrap();
        let at = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 2, z: 0 },
        };
        world.place_door(at, 1, false, 2).unwrap();
        let original = world.clone();
        world.set_door(at, true);
        assert!(!original.door(at).unwrap().open);
        assert!(world.door(at).unwrap().open);
        assert!(!world.doors.shares_storage(&original.doors));
        assert!(world.regions.shares_storage(&original.regions));
        assert!(world.passages.shares_storage(&original.passages));
        assert!(world.terrain.shares_storage(&original.terrain));
        assert!(world.rotations.shares_storage(&original.rotations));
    }
}

fn valid_gravity(g: [i32; 3]) -> bool {
    g.iter().filter(|v| **v != 0).count() <= 1 && g.iter().all(|v| (-1024..=1024).contains(v))
}

#[cfg(test)]
mod gravity_sharing_tests {
    use super::*;
    #[test]
    fn gravity_tables_share_geometry_and_detach_only_on_edit() {
        let region = RegionId(1);
        let mut original = World::new(
            vec![Region {
                id: region,
                name: "field".into(),
                bounds: Extent::new(2, 2, 2).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let at = Location {
            region,
            position: Position { x: 0, y: 0, z: 0 },
        };
        original.set_gravity(region, [0, 0, -1]).unwrap();
        original.set_cell_gravity(at, [0; 3]).unwrap();
        let mut edited = original.clone();
        assert!(edited
            .region_gravity
            .shares_storage(&original.region_gravity));
        assert!(edited.cell_gravity.shares_storage(&original.cell_gravity));
        edited.set_cell_gravity(at, [0, 0, 1]).unwrap();
        assert_eq!(original.gravity(at), Some([0; 3]));
        assert_eq!(edited.gravity(at), Some([0, 0, 1]));
        assert!(edited
            .region_gravity
            .shares_storage(&original.region_gravity));
    }
}
