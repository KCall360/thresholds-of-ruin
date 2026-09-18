//! Symmetric shadowcasting with exact slopes and a bounded observer scene.
//! Geometry follows Ford's point-floor / diamond-wall model, described at
//! <https://www.albertford.com/shadowcasting/> (independent implementation).
use crate::{Direction, Location, Position, SightCell, World};

#[derive(Clone, Copy)]
struct Slope {
    n: i32,
    d: i32,
}
impl Slope {
    fn edge(column: i32, depth: i32) -> Self {
        Self {
            n: 2 * column - 1,
            d: 2 * depth,
        }
    }
    fn first(self, depth: i32) -> i32 {
        (2 * depth * self.n + self.d).div_euclid(2 * self.d)
    }
    fn last(self, depth: i32) -> i32 {
        -(self.d - 2 * depth * self.n).div_euclid(2 * self.d)
    }
}

struct Scan<'a> {
    world: &'a World,
    origin: Location,
    orientation: u8,
    radius: i32,
    // Each offset is resolved at most once, including quadrant boundary duplicates.
    cells: Vec<Option<Option<SightCell>>>,
    visible: Vec<bool>,
}
impl Scan<'_> {
    fn index(&self, x: i32, y: i32) -> usize {
        ((y + self.radius) * (2 * self.radius + 1) + x + self.radius) as usize
    }
    fn cell(&mut self, x: i32, y: i32) -> Option<SightCell> {
        let index = self.index(x, y);
        if let Some(cached) = self.cells[index] {
            return cached;
        }
        let cell = self
            .world
            .geometry_ray(self.origin, self.orientation, x, y)
            .map(|(location, rotation)| SightCell {
                location,
                rotation,
                offset: Position { x, y, z: 0 },
                wall: self.world.is_wall(location),
            });
        self.cells[index] = Some(cell);
        cell
    }
    fn row(&mut self, quadrant: u8, depth: i32, mut start: Slope, end: Slope) {
        if depth > self.radius {
            return;
        }
        let mut previous = None;
        // The Manhattan range is convex: an out-of-range cell cannot shadow
        // an in-range cell farther along a ray. Avoid resolving those cells.
        let remaining = self.radius - depth;
        for column in start.first(depth).max(-remaining)..=end.last(depth).min(remaining) {
            let (x, y) = match quadrant {
                0 => (column, -depth),
                1 => (depth, column),
                2 => (column, depth),
                _ => (-depth, column),
            };
            let cell = self.cell(x, y);
            let opaque = cell.is_none_or(|c| self.world.opaque(c.location));
            let symmetric = column * start.d >= depth * start.n && column * end.d <= depth * end.n;
            if cell.is_some() && (opaque || symmetric) && x.abs() + y.abs() <= self.radius {
                let index = self.index(x, y);
                self.visible[index] = true;
            }
            match (previous, opaque) {
                (Some(true), false) => start = Slope::edge(column, depth),
                (Some(false), true) => {
                    self.row(quadrant, depth + 1, start, Slope::edge(column, depth))
                }
                _ => {}
            }
            previous = Some(opaque);
        }
        if previous == Some(false) {
            self.row(quadrant, depth + 1, start, end);
        }
    }
}
impl World {
    /// Symmetric shadowcasting on bounded observer-relative topology. Retains
    /// Manhattan range and explicit vertical links. Missing/ambiguous geometry
    /// casts a shadow but is never disclosed as an invented wall.
    pub fn shadow_scene(&self, origin: Location, orientation: u8, radius: u8) -> Vec<SightCell> {
        if !self.walkable(origin) {
            return vec![];
        }
        let radius = i32::from(radius.min(16));
        let size = ((2 * radius + 1) * (2 * radius + 1)) as usize;
        let mut scan = Scan {
            world: self,
            origin,
            orientation,
            radius,
            cells: vec![None; size],
            visible: vec![false; size],
        };
        scan.cell(0, 0);
        let center = scan.index(0, 0);
        scan.visible[center] = true;
        for quadrant in 0..4 {
            scan.row(quadrant, 1, Slope { n: -1, d: 1 }, Slope { n: 1, d: 1 });
        }
        let mut cells: Vec<_> = scan
            .cells
            .into_iter()
            .zip(scan.visible)
            .filter_map(|(cell, visible)| if visible { cell.flatten() } else { None })
            .collect();
        // Vertical perception intentionally retains the established shaft rules.
        for (direction, sign) in [(Direction::Up, 1), (Direction::Down, -1)] {
            let mut current = origin;
            for distance in 1..=radius {
                let Some(passage) = self.passage(current, direction) else {
                    break;
                };
                current = passage.to;
                cells.push(SightCell {
                    location: current,
                    offset: Position {
                        x: 0,
                        y: 0,
                        z: sign * distance,
                    },
                    rotation: orientation % 4,
                    wall: self.is_wall(current),
                });
                if self.opaque(current) {
                    break;
                }
            }
        }
        cells.sort_by_key(|c| c.offset);
        cells
    }
}
