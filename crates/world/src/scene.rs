use crate::{Direction, Location, Position, World};

/// Backend-only visible occurrence. Multiple offsets may reference one location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SightCell {
    pub location: Location,
    pub offset: Position,
    pub rotation: u8,
    pub wall: bool,
}

impl World {
    /// Resolve topology into an observer-relative scene. No global embedding is
    /// assumed. Radius and path lengths are bounded even for cyclic connections.
    pub fn scene(&self, origin: Location, orientation: u8, radius: u8) -> Vec<SightCell> {
        if !self.walkable(origin) {
            return Vec::new();
        }
        let radius = i32::from(radius.min(16));
        let mut cells = Vec::new();
        for x in -radius..=radius {
            for y in -radius..=radius {
                if x.abs() + y.abs() > radius {
                    continue;
                }
                if let Some((location, rotation)) = self.scene_ray(origin, orientation, x, y) {
                    cells.push(SightCell {
                        location,
                        offset: Position { x, y, z: 0 },
                        rotation,
                        wall: self.is_wall(location),
                    });
                }
            }
        }
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
                    rotation: orientation,
                    wall: self.is_wall(current),
                });
                if self.is_wall(current) {
                    break;
                }
            }
        }
        cells.sort_by_key(|cell| cell.offset);
        cells
    }

    fn scene_ray(
        &self,
        origin: Location,
        orientation: u8,
        dx: i32,
        dy: i32,
    ) -> Option<(Location, u8)> {
        let (nx, ny) = (dx.abs(), dy.abs());
        let horizontal = if dx >= 0 {
            Direction::East
        } else {
            Direction::West
        };
        let vertical = if dy >= 0 {
            Direction::South
        } else {
            Direction::North
        };
        let (mut ix, mut iy) = (0, 0);
        let (mut current, mut rotation) = (origin, orientation % 4);
        while ix < nx || iy < ny {
            if self.is_wall(current) {
                return None;
            }
            let tx = (1 + 2 * ix) * ny;
            let ty = (1 + 2 * iy) * nx;
            let x = horizontal.rotated(rotation);
            let y = vertical.rotated(rotation);
            let (next, turns) = if tx == ty {
                // Both routes through a corner must agree. This keeps an invisible
                // partition invisible while still forbidding blocked corner cuts.
                let (sx, rx) = self.sight_step(current, x)?;
                let (sy, ry) = self.sight_step(current, y)?;
                if !self.walkable(sx) || !self.walkable(sy) {
                    return None;
                }
                let (a, ra) = self.sight_step(sx, y.rotated(rx))?;
                let (b, rb) = self.sight_step(sy, x.rotated(ry))?;
                if a != b || (rx + ra) % 4 != (ry + rb) % 4 {
                    return None;
                }
                ix += 1;
                iy += 1;
                (a, (rx + ra) % 4)
            } else if tx < ty {
                ix += 1;
                self.sight_step(current, x)?
            } else {
                iy += 1;
                self.sight_step(current, y)?
            };
            current = next;
            rotation = (rotation + turns) % 4;
        }
        Some((current, rotation))
    }
}
