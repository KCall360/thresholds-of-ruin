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
    pub(crate) fn geometry_ray(
        &self,
        origin: Location,
        orientation: u8,
        dx: i32,
        dy: i32,
    ) -> Option<(Location, u8)> {
        let (x, y) = match orientation % 4 {
            0 => (dx, dy),
            1 => (-dy, dx),
            2 => (-dx, -dy),
            _ => (dy, -dx),
        };
        let direct = Location {
            position: Position {
                x: origin.position.x.checked_add(x)?,
                y: origin.position.y.checked_add(y)?,
                z: origin.position.z,
            },
            ..origin
        };
        if self.within_aperture_bounds(origin) && self.within_aperture_bounds(direct) {
            return Some((direct, orientation % 4));
        }
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
            let tx = (1 + 2 * ix) * ny;
            let ty = (1 + 2 * iy) * nx;
            let x = horizontal.rotated(rotation);
            let y = vertical.rotated(rotation);
            let (next, turns) = if tx == ty {
                // Ambiguous topology is not a line of sight. Opacity is evaluated
                // separately by shadowcasting, including at exact corners.
                let (sx, rx) = self.geometry_step(current, x)?;
                let (sy, ry) = self.geometry_step(current, y)?;
                let (a, ra) = self.geometry_step(sx, y.rotated(rx))?;
                let (b, rb) = self.geometry_step(sy, x.rotated(ry))?;
                if a != b || (rx + ra) % 4 != (ry + rb) % 4 {
                    return None;
                }
                ix += 1;
                iy += 1;
                (a, (rx + ra) % 4)
            } else if tx < ty {
                ix += 1;
                self.geometry_step(current, x)?
            } else {
                iy += 1;
                self.geometry_step(current, y)?
            };
            current = next;
            rotation = (rotation + turns) % 4;
        }
        Some((current, rotation))
    }
}
