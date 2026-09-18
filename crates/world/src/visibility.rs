use crate::{Direction, Location, World};
use std::collections::BTreeSet;

impl World {
    /// Cell-centre rays with integer arithmetic and a Manhattan distance budget.
    /// Rays rotate when crossing single-cell apertures. Tied diagonal crossings
    /// require both side cells to be clear in the same region (no corner peeking).
    /// The radius is capped at 16; cycles consume distance and always terminate.
    pub fn visible_cells(&self, origin: Location, radius: u8) -> BTreeSet<Location> {
        let mut seen = BTreeSet::new();
        if !self.contains(origin) {
            return seen;
        }
        seen.insert(origin);
        let radius = i32::from(radius.min(16));
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                let nx = dx.abs();
                let ny = dy.abs();
                if nx + ny > radius {
                    continue;
                }
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
                let mut current = origin;
                let mut rotation = 0;
                while ix < nx || iy < ny {
                    let tx = (1 + 2 * ix) * ny;
                    let ty = (1 + 2 * iy) * nx;
                    let x = horizontal.rotated(rotation);
                    let y = vertical.rotated(rotation);
                    if tx == ty {
                        // A ray exactly through a cell corner never selects an
                        // arbitrary portal or leaks past either opaque side.
                        let ordinary = |from: Location, direction: Direction| {
                            if self.passage(from, direction).is_some() {
                                return None;
                            }
                            self.step(from, direction)
                        };
                        let Some(side_x) = ordinary(current, x) else {
                            break;
                        };
                        let Some(_) = ordinary(current, y) else {
                            break;
                        };
                        let Some(position) = y.offset(side_x.position) else {
                            break;
                        };
                        let next = Location { position, ..side_x };
                        if !self.contains(next) {
                            break;
                        }
                        current = next;
                        ix += 1;
                        iy += 1;
                    } else {
                        let direction = if tx < ty {
                            ix += 1;
                            x
                        } else {
                            iy += 1;
                            y
                        };
                        let Some((next, turns)) = self.sight_step(current, direction) else {
                            break;
                        };
                        current = next;
                        rotation = (rotation + turns) % 4;
                    }
                    seen.insert(current);
                    if self.opaque(current) {
                        break;
                    }
                }
            }
        }
        // Vertical visibility follows explicit stair/shaft links only, never
        // reveals another floor merely because it shares x/y coordinates.
        for direction in [Direction::Up, Direction::Down] {
            let mut current = origin;
            for _ in 0..radius {
                if self.opaque(current) {
                    break;
                }
                let Some(passage) = self.passage(current, direction) else {
                    break;
                };
                current = passage.to;
                seen.insert(current);
            }
        }
        seen
    }
}
