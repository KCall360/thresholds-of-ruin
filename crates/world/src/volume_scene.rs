//! Bounded voxel sight for bodies and gravity scenarios. Exact integer rays
//! reject ambiguous corner topology and never traverse opaque intermediate cells.
use crate::{compose_rotation, Direction, Location, Position, SightCell, World};

impl World {
    pub fn volume_scene(&self, origin: Location, frame: u8, radius: u8) -> Vec<SightCell> {
        let radius = i32::from(radius.min(16));
        // Preserve the established beveled-corner visibility on the observer's
        // main plane. Height slices use conservative voxel rays.
        let mut cells: Vec<_> = self
            .shadow_scene(origin, frame, radius as u8)
            .into_iter()
            .filter(|c| c.offset.z == 0)
            .collect();
        for z in -radius..=radius {
            if z == 0 {
                continue;
            }
            for y in -radius..=radius {
                for x in -radius..=radius {
                    let offset = [x, y, z];
                    if x.abs() + y.abs() + z.abs() > radius {
                        continue;
                    }
                    if let Some((location, rotation)) = self.volume_ray(origin, frame, offset) {
                        cells.push(SightCell {
                            location,
                            rotation,
                            offset: Position { x, y, z },
                            wall: self.is_wall(location),
                        });
                    }
                }
            }
        }
        // Abstract stair landings occupy separate panels, outside physical slices.
        for (direction, sign) in [(Direction::Up, 1), (Direction::Down, -1)] {
            if self.is_stair(origin, direction) {
                let passage = self.passage(origin, direction).expect("stair");
                cells.push(SightCell {
                    location: passage.to,
                    rotation: compose_rotation(frame, self.crossing_rotation(origin, direction)),
                    offset: Position {
                        x: 0,
                        y: 0,
                        z: sign * (radius + 1),
                    },
                    wall: self.is_wall(passage.to),
                });
            }
        }
        cells.sort_by_key(|c| c.offset);
        cells
    }
    fn volume_ray(
        &self,
        mut at: Location,
        mut frame: u8,
        offset: [i32; 3],
    ) -> Option<(Location, u8)> {
        let length = offset.map(i32::abs);
        let mut progress = [0; 3];
        while progress != length {
            if self.opaque(at) {
                return None;
            }
            let axis = (0..3)
                .filter(|i| progress[*i] < length[*i])
                .min_by(|a, b| {
                    ((1 + 2 * progress[*a]) * length[*b])
                        .cmp(&((1 + 2 * progress[*b]) * length[*a]))
                })?;
            let tied: Vec<_> = (0..3)
                .filter(|i| {
                    progress[*i] < length[*i]
                        && (1 + 2 * progress[*i]) * length[axis]
                            == (1 + 2 * progress[axis]) * length[*i]
                })
                .collect();
            let mut delta = [0; 3];
            for i in tied {
                progress[i] += 1;
                delta[i] = offset[i].signum();
            }
            (at, frame) = self.volume_step(at, frame, delta)?;
        }
        Some((at, frame))
    }
    fn volume_step(&self, at: Location, frame: u8, delta: [i32; 3]) -> Option<(Location, u8)> {
        let mut result = None;
        for axis in 0..3 {
            if delta[axis] == 0 {
                continue;
            }
            let mut unit = [0; 3];
            unit[axis] = i64::from(delta[axis]);
            let direction = Direction::from_delta(crate::rotate_vector(frame, unit))?;
            let (next, rotation) = if self.is_stair(at, direction) {
                // Stairs disclose a landing only; they are not a physical ray.
                let (x, y, z) = direction.delta();
                let next = Location {
                    position: Position {
                        x: at.position.x.checked_add(x)?,
                        y: at.position.y.checked_add(y)?,
                        z: at.position.z.checked_add(z)?,
                    },
                    ..at
                };
                self.contains(next).then_some((next, 0))?
            } else {
                self.geometry_step(at, direction)?
            };
            let next_frame = compose_rotation(frame, rotation);
            let mut rest = delta;
            rest[axis] = 0;
            let candidate = if rest == [0; 3] {
                (next, next_frame)
            } else {
                if self.opaque(next) {
                    return None;
                }
                self.volume_step(next, next_frame, rest)?
            };
            if result.is_some_and(|old| old != candidate) {
                return None;
            }
            result = Some(candidate);
        }
        result
    }
}
