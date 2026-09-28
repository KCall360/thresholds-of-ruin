//! Exact three-dimensional sight: the reference implementation of the model in
//! `docs/sight-3d.md`. Correctness first; accelerated layers must match it.
//!
//! Geometry uses doubled coordinates relative to the eye cell: cell `o` has its
//! centre at `2o` and its faces on odd planes. Sight lines run from the eye
//! centre to empty-cell centres and to the centres of exposed faces of opaque
//! cells. Opaque cells block as cubes beveled only on convex exposed edges, and
//! only a segment through a blocker's interior is blocked.
use crate::{compose_rotation, Direction, Location, Position, SightCell, World};

#[derive(Clone, Copy)]
struct Resolved {
    location: Location,
    rotation: u8,
    opaque: bool,
}

/// Observer-relative unfolded space, resolved lazily and at most once per offset.
struct Volume<'a> {
    world: &'a World,
    eye: Location,
    frame: u8,
    reach: i32,
    cells: Vec<Option<Option<Resolved>>>,
}

/// Strictly bounded parameter interval along a segment, as exact fractions.
#[derive(Clone, Copy)]
pub(crate) struct Fraction {
    pub(crate) n: i64,
    pub(crate) d: i64,
}
impl Fraction {
    pub(crate) fn less(self, other: Self) -> bool {
        self.n * other.d < other.n * self.d
    }
}

impl Volume<'_> {
    fn index(&self, o: [i32; 3]) -> Option<usize> {
        if o.iter().any(|v| v.abs() > self.reach) {
            return None;
        }
        let side = (2 * self.reach + 1) as usize;
        let [x, y, z] = o.map(|v| (v + self.reach) as usize);
        Some((z * side + y) * side + x)
    }

    /// `None` is missing, unloaded, or ambiguous geometry.
    fn get(&mut self, o: [i32; 3]) -> Option<Resolved> {
        let index = self.index(o)?;
        if let Some(cached) = self.cells[index] {
            return cached;
        }
        let resolved =
            self.world
                .geometry_route(self.eye, self.frame, o)
                .map(|(location, rotation)| Resolved {
                    location,
                    rotation,
                    opaque: self.world.opaque(location),
                });
        self.cells[index] = Some(resolved);
        resolved
    }

    fn blocks(&mut self, o: [i32; 3]) -> bool {
        self.get(o).is_none_or(|c| c.opaque)
    }

    /// A face is exposed when the resolved neighbour across it exists and is
    /// not opaque. Neighbours come from the unfolded scene, not backend storage,
    /// so a split room bevels exactly like the unsplit room.
    fn exposed(&mut self, o: [i32; 3], axis: usize, sign: i32) -> bool {
        let mut n = o;
        n[axis] += sign;
        self.get(n).is_some_and(|c| !c.opaque)
    }

    /// Whether the segment `from -> to` passes through the interior of the
    /// blocker at `cell`. Missing geometry is an unbeveled cube.
    fn cuts(&mut self, cell: [i32; 3], from: [i64; 3], to: [i64; 3]) -> bool {
        let centre = cell.map(|v| 2 * i64::from(v));
        let beveled = self.get(cell).is_some();
        let mut faces = [[false; 2]; 3];
        if beveled {
            for (axis, sides) in faces.iter_mut().enumerate() {
                for (side, sign) in [-1, 1].into_iter().enumerate() {
                    sides[side] = self.exposed(cell, axis, sign);
                }
            }
        }
        let rel = [0, 1, 2].map(|i| from[i] - centre[i]);
        let dir = [0, 1, 2].map(|i| to[i] - from[i]);
        let mut lower = Fraction { n: 0, d: 1 };
        let mut upper = Fraction { n: 1, d: 1 };
        // Constraint a·(p - centre) < b along p = from + t·dir, t in [0, 1].
        let mut constrain = |a: [i64; 3], b: i64| -> bool {
            let k: i64 = (0..3).map(|i| a[i] * rel[i]).sum();
            let m: i64 = (0..3).map(|i| a[i] * dir[i]).sum();
            match m.cmp(&0) {
                std::cmp::Ordering::Equal => k < b,
                std::cmp::Ordering::Greater => {
                    let bound = Fraction { n: b - k, d: m };
                    if bound.less(upper) {
                        upper = bound;
                    }
                    true
                }
                std::cmp::Ordering::Less => {
                    let bound = Fraction { n: k - b, d: -m };
                    if lower.less(bound) {
                        lower = bound;
                    }
                    true
                }
            }
        };
        for axis in 0..3 {
            for sign in [-1, 1] {
                let mut a = [0; 3];
                a[axis] = sign;
                if !constrain(a, 1) {
                    return false;
                }
            }
        }
        for i in 0..3 {
            for j in i + 1..3 {
                for (si, sign_i) in [-1, 1].into_iter().enumerate() {
                    for (sj, sign_j) in [-1, 1].into_iter().enumerate() {
                        if faces[i][si] && faces[j][sj] {
                            let mut a = [0; 3];
                            a[i] = sign_i;
                            a[j] = sign_j;
                            if !constrain(a, 1) {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        lower.less(upper)
    }

    /// Whether a doubled-coordinate point is visible from the eye centre.
    fn sees(&mut self, to: [i64; 3]) -> bool {
        let from = [0; 3];
        // Cells whose open cube interval overlaps the segment's extent on each axis.
        let range = |axis: usize| {
            let (lo, hi) = (from[axis].min(to[axis]), from[axis].max(to[axis]));
            let first = (lo - 1).div_euclid(2) + 1;
            let last = -(-(hi + 1)).div_euclid(2) - 1;
            first as i32..=last as i32
        };
        for z in range(2) {
            for y in range(1) {
                for x in range(0) {
                    let cell = [x, y, z];
                    if self.blocks(cell) && self.cuts(cell, from, to) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl World {
    /// Reference exact 3D sight from the centre of `eye`, with offsets relative
    /// to the eye cell in the observer's `frame`. Stair landings are not
    /// included; abstract stair links are traversal, not geometry. This is the
    /// test oracle for the accelerated [`World::eye_scene`].
    pub fn eye_scene_reference(&self, eye: Location, frame: u8, radius: u8) -> Vec<SightCell> {
        if !self.walkable(eye) {
            return vec![];
        }
        let radius = i32::from(radius.min(16));
        // Blockers along in-range segments lie within radius + 1; face exposure
        // looks one cell further.
        let reach = radius + 2;
        let side = (2 * reach + 1) as usize;
        let mut volume = Volume {
            world: self,
            eye,
            frame,
            reach,
            cells: vec![None; side * side * side],
        };
        let mut cells = Vec::new();
        for z in -radius..=radius {
            let rz = radius - z.abs();
            for y in -rz..=rz {
                let ry = rz - y.abs();
                for x in -ry..=ry {
                    let o = [x, y, z];
                    let Some(cell) = volume.get(o) else {
                        continue;
                    };
                    let centre = o.map(|v| 2 * i64::from(v));
                    let visible = if cell.opaque {
                        let mut any = false;
                        'faces: for axis in 0..3 {
                            for sign in [-1, 1] {
                                if volume.exposed(o, axis, sign) {
                                    let mut point = centre;
                                    point[axis] += i64::from(sign);
                                    if volume.sees(point) {
                                        any = true;
                                        break 'faces;
                                    }
                                }
                            }
                        }
                        any
                    } else {
                        volume.sees(centre)
                    };
                    if visible {
                        cells.push(SightCell {
                            location: cell.location,
                            offset: Position { x, y, z },
                            rotation: cell.rotation,
                            wall: self.is_wall(cell.location),
                        });
                    }
                }
            }
        }
        cells.sort_by_key(|c| c.offset);
        cells
    }

    /// Opacity-free observer-relative topology: the location and frame reached
    /// by the unfolded straight route to `offset`. Exact ties must agree across
    /// every axis ordering, or the geometry is ambiguous. Abstract stair links
    /// are not followed; physical portals and joins are.
    pub(crate) fn geometry_route(
        &self,
        origin: Location,
        frame: u8,
        offset: [i32; 3],
    ) -> Option<(Location, u8)> {
        let length = offset.map(|v| i64::from(v.abs()));
        let mut progress = [0i64; 3];
        let (mut at, mut frame) = (origin, frame);
        while progress != length {
            let pending: Vec<_> = (0..3).filter(|&i| progress[i] < length[i]).collect();
            // Next boundary crossing: minimal (1 + 2p) / n over pending axes.
            let key = |i: usize| (1 + 2 * progress[i], length[i]);
            let best = pending.iter().copied().min_by(|&a, &b| {
                let (na, da) = key(a);
                let (nb, db) = key(b);
                (na * db).cmp(&(nb * da))
            })?;
            let (nbest, dbest) = key(best);
            let tied: Vec<_> = pending
                .iter()
                .copied()
                .filter(|&i| {
                    let (n, d) = key(i);
                    n * dbest == nbest * d
                })
                .collect();
            let mut delta = [0; 3];
            for i in tied {
                progress[i] += 1;
                delta[i] = offset[i].signum();
            }
            (at, frame) = self.route_step(at, frame, delta)?;
        }
        Some((at, frame))
    }

    fn route_step(&self, at: Location, frame: u8, delta: [i32; 3]) -> Option<(Location, u8)> {
        let mut result = None;
        for axis in 0..3 {
            if delta[axis] == 0 {
                continue;
            }
            let mut unit = [0i64; 3];
            unit[axis] = i64::from(delta[axis]);
            let direction = Direction::from_delta(crate::rotate_vector(frame, unit))?;
            let (next, turns) = if self.is_stair(at, direction) {
                let position = direction.offset(at.position)?;
                let next = Location { position, ..at };
                self.contains(next).then_some((next, 0))?
            } else {
                self.geometry_step(at, direction)?
            };
            let next_frame = compose_rotation(frame, turns);
            let mut rest = delta;
            rest[axis] = 0;
            let candidate = if rest == [0; 3] {
                (next, next_frame)
            } else {
                self.route_step(next, next_frame, rest)?
            };
            if result.is_some_and(|old| old != candidate) {
                return None;
            }
            result = Some(candidate);
        }
        result
    }
}
