//! Accelerated exact 3D sight. It must return exactly what
//! [`World::eye_scene_reference`] returns; see `docs/sight-3d.md`.
//!
//! The model is unchanged. The speed comes from resolving plain routes
//! directly, stepping plain cells without topology lookups, caching each
//! cell's state and exposed faces once per scene, walking only the cells along
//! each sight line, and testing a blocker's cube before its bevels.
use crate::sight3d::Fraction;
use crate::{compose_rotation, Direction, Location, Position, RegionId, SightCell, Terrain, World};

const UNRESOLVED: u8 = 0;
const MISSING: u8 = 1;
const EMPTY: u8 = 2;
/// Opaque but not a wall: a closed door.
const BARRIER: u8 = 3;
const WALL: u8 = 4;
/// Set in `faces` once a cell's exposed faces are known.
const FACES_KNOWN: u8 = 0x40;

/// A region's storage bounds and exit planes. A step is plain (an ordinary
/// neighbour in the same region) unless it leaves storage or starts on an
/// exit plane in that exit's direction. Only those steps can be redirected by
/// a passage or by rim projection, so only they need topology lookups.
struct RegionInfo {
    id: RegionId,
    lo: [i64; 3],
    hi: [i64; 3],
    /// Axis, sign, and plane coordinate of each exit.
    exits: Vec<(usize, i64, i64)>,
    /// Where the exits lead. Rim projection reads terrain there even when the
    /// step stays in this region, so a scene depends on these regions too.
    neighbours: Vec<RegionId>,
}

impl RegionInfo {
    fn new(world: &World, id: RegionId) -> Option<Self> {
        let bounds = world.region(id)?.bounds;
        let (w, d, h) = bounds.dimensions();
        let lo = [bounds.origin.x, bounds.origin.y, bounds.origin.z].map(i64::from);
        let mut neighbours: Vec<_> = world.exits(id).map(|p| p.to.region).collect();
        neighbours.sort();
        neighbours.dedup();
        Some(Self {
            id,
            lo,
            hi: [
                lo[0] + i64::from(w) - 1,
                lo[1] + i64::from(d) - 1,
                lo[2] + i64::from(h) - 1,
            ],
            exits: world
                .exits(id)
                .filter_map(|p| {
                    let (axis, sign) = axis_sign(p.direction)?;
                    let from = p.from.position;
                    Some((axis, sign, i64::from([from.x, from.y, from.z][axis])))
                })
                .collect(),
            neighbours,
        })
    }

    fn plain(&self, at: [i64; 3], axis: usize, sign: i64) -> bool {
        let next = at[axis] + sign;
        next >= self.lo[axis]
            && next <= self.hi[axis]
            && !self
                .exits
                .iter()
                .any(|&(a, s, c)| a == axis && s == sign && c == at[axis])
    }
}

fn axis_sign(direction: Direction) -> Option<(usize, i64)> {
    let (dx, dy, dz) = direction.delta();
    let delta = [dx, dy, dz];
    let axis = delta.iter().position(|v| *v != 0)?;
    (delta.iter().filter(|v| **v != 0).count() == 1).then_some((axis, i64::from(delta[axis])))
}

struct Scene<'a> {
    world: &'a World,
    eye: Location,
    origin: [i64; 3],
    frame: u8,
    reach: i32,
    side: usize,
    state: Vec<u8>,
    faces: Vec<u8>,
    slot: Vec<u32>,
    resolved: Vec<(Location, u8)>,
    /// Index 0 is the eye's region.
    regions: Vec<RegionInfo>,
}

impl Scene<'_> {
    fn index(&self, o: [i32; 3]) -> Option<usize> {
        if o.iter().any(|v| v.abs() > self.reach) {
            return None;
        }
        let [x, y, z] = o.map(|v| (v + self.reach) as usize);
        Some((z * self.side + y) * self.side + x)
    }

    fn state(&mut self, o: [i32; 3]) -> u8 {
        let Some(i) = self.index(o) else {
            return MISSING;
        };
        if self.state[i] == UNRESOLVED {
            self.state[i] = self.resolve(i, o);
        }
        self.state[i]
    }

    /// Same result as [`World::geometry_route`]. A route inside the eye's
    /// region whose bounding box spans no exit plane is a plain offset and is
    /// computed directly. Other routes are walked, with topology lookups only
    /// for steps that could be redirected.
    fn route(&mut self, o: [i32; 3]) -> Option<(Location, u8)> {
        let eye = &self.regions[0];
        let v = crate::rotate_vector(self.frame, o.map(i64::from));
        let target = [0, 1, 2].map(|a| self.origin[a] + v[a]);
        let span = |a: usize| (self.origin[a].min(target[a]), self.origin[a].max(target[a]));
        let clear = eye.exits.iter().all(|&(a, sign, c)| {
            let (lo, hi) = span(a);
            !(lo <= c.min(c + sign) && hi >= c.max(c + sign))
        });
        if clear {
            // The eye is inside its region, so a target outside it means the
            // route leaves the region's storage and the geometry is missing.
            let inside = (0..3).all(|a| target[a] >= eye.lo[a] && target[a] <= eye.hi[a]);
            if !inside {
                return None;
            }
            let position = Position {
                x: target[0] as i32,
                y: target[1] as i32,
                z: target[2] as i32,
            };
            return Some((
                Location {
                    position,
                    ..self.eye
                },
                self.frame,
            ));
        }
        self.walk(o)
    }

    /// [`World::geometry_route`] with plain steps taken arithmetically.
    fn walk(&mut self, offset: [i32; 3]) -> Option<(Location, u8)> {
        let length = offset.map(|v| i64::from(v.abs()));
        let mut progress = [0i64; 3];
        let (mut at, mut frame) = (self.eye, self.frame);
        while progress != length {
            // Next boundary crossing: minimal (1 + 2p) / n over pending axes.
            let key = |i: usize, progress: &[i64; 3]| (1 + 2 * progress[i], length[i]);
            let best = (0..3)
                .filter(|&i| progress[i] < length[i])
                .min_by(|&a, &b| {
                    let (na, da) = key(a, &progress);
                    let (nb, db) = key(b, &progress);
                    (na * db).cmp(&(nb * da))
                })?;
            let (nbest, dbest) = key(best, &progress);
            let mut delta = [0; 3];
            for (i, step) in delta.iter_mut().enumerate() {
                let (n, d) = key(i, &progress);
                if progress[i] < length[i] && n * dbest == nbest * d {
                    *step = offset[i].signum();
                }
            }
            for i in 0..3 {
                if delta[i] != 0 {
                    progress[i] += 1;
                }
            }
            (at, frame) = self.route_step(at, frame, delta)?;
        }
        Some((at, frame))
    }

    /// Every axis ordering of a tied step must agree, as in the reference.
    fn route_step(&mut self, at: Location, frame: u8, delta: [i32; 3]) -> Option<(Location, u8)> {
        let mut result = None;
        for axis in 0..3 {
            if delta[axis] == 0 {
                continue;
            }
            let mut unit = [0i64; 3];
            unit[axis] = i64::from(delta[axis]);
            let direction = Direction::from_delta(crate::rotate_vector(frame, unit))?;
            let (next, turns) = self.unit_step(at, direction)?;
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

    fn unit_step(&mut self, at: Location, direction: Direction) -> Option<(Location, u8)> {
        let (axis, sign) = axis_sign(direction)?;
        let region = match self.regions.iter().position(|r| r.id == at.region) {
            Some(index) => index,
            None => {
                self.regions.push(RegionInfo::new(self.world, at.region)?);
                self.regions.len() - 1
            }
        };
        let p = at.position;
        if self.regions[region].plain([p.x, p.y, p.z].map(i64::from), axis, sign) {
            let position = direction.offset(p)?;
            return Some((Location { position, ..at }, 0));
        }
        if self.world.is_stair(at, direction) {
            let next = Location {
                position: direction.offset(p)?,
                ..at
            };
            return self.world.contains(next).then_some((next, 0));
        }
        self.world.geometry_step(at, direction)
    }

    fn resolve(&mut self, i: usize, o: [i32; 3]) -> u8 {
        let Some((location, rotation)) = self.route(o) else {
            return MISSING;
        };
        self.slot[i] = self.resolved.len() as u32;
        self.resolved.push((location, rotation));
        if matches!(self.world.terrain(location), Some(Terrain::Solid(_))) {
            WALL
        } else if self.world.door(location).is_some_and(|d| !d.open) {
            BARRIER
        } else {
            EMPTY
        }
    }

    /// Bit `2 * axis + side` is set when that face is exposed: the resolved
    /// neighbour across it exists and is not opaque.
    fn exposed_faces(&mut self, o: [i32; 3]) -> u8 {
        let i = self.index(o).expect("blockers lie within reach");
        if self.faces[i] & FACES_KNOWN == 0 {
            let mut bits = FACES_KNOWN;
            for axis in 0..3 {
                for (side, sign) in [-1, 1].into_iter().enumerate() {
                    let mut n = o;
                    n[axis] += sign;
                    if self.state(n) == EMPTY {
                        bits |= 1 << (2 * axis + side);
                    }
                }
            }
            self.faces[i] = bits;
        }
        self.faces[i]
    }

    /// Whether the segment from the eye centre to `to` passes through the
    /// interior of the blocker at `cell`. Missing geometry is an unbeveled cube.
    fn cuts(&mut self, cell: [i32; 3], missing: bool, to: [i64; 3]) -> bool {
        let centre = cell.map(|v| 2 * i64::from(v));
        let mut lower = Fraction { n: 0, d: 1 };
        let mut upper = Fraction { n: 1, d: 1 };
        // Constraint a·(p - centre) < 1 along p = t·to; k = a·(-centre).
        let mut constrain = |k: i64, m: i64| -> bool {
            match m.cmp(&0) {
                std::cmp::Ordering::Equal => k < 1,
                std::cmp::Ordering::Greater => {
                    let bound = Fraction { n: 1 - k, d: m };
                    if bound.less(upper) {
                        upper = bound;
                    }
                    true
                }
                std::cmp::Ordering::Less => {
                    let bound = Fraction { n: k - 1, d: -m };
                    if lower.less(bound) {
                        lower = bound;
                    }
                    true
                }
            }
        };
        for axis in 0..3 {
            if !constrain(centre[axis], -to[axis]) || !constrain(-centre[axis], to[axis]) {
                return false;
            }
        }
        if !lower.less(upper) {
            return false;
        }
        if missing {
            return true;
        }
        let bits = self.exposed_faces(cell);
        let mut constrain = |k: i64, m: i64| -> bool {
            match m.cmp(&0) {
                std::cmp::Ordering::Equal => k < 1,
                std::cmp::Ordering::Greater => {
                    let bound = Fraction { n: 1 - k, d: m };
                    if bound.less(upper) {
                        upper = bound;
                    }
                    true
                }
                std::cmp::Ordering::Less => {
                    let bound = Fraction { n: k - 1, d: -m };
                    if lower.less(bound) {
                        lower = bound;
                    }
                    true
                }
            }
        };
        for i in 0..3 {
            for j in i + 1..3 {
                for (si, sign_i) in [-1i64, 1].into_iter().enumerate() {
                    for (sj, sign_j) in [-1i64, 1].into_iter().enumerate() {
                        if bits & (1 << (2 * i + si)) != 0 && bits & (1 << (2 * j + sj)) != 0 {
                            let k = -(sign_i * centre[i] + sign_j * centre[j]);
                            let m = sign_i * to[i] + sign_j * to[j];
                            if !constrain(k, m) {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        lower.less(upper)
    }

    /// Walks the cells whose closed cube meets the segment, slab by slab along
    /// its major axis. That is a superset of the cells whose interior it can
    /// enter, so the exact `cuts` test decides every case.
    fn sees(&mut self, to: [i64; 3]) -> bool {
        let major = (0..3).max_by_key(|&i| to[i].abs()).expect("three axes");
        let d = to[major].abs();
        if d == 0 {
            return true;
        }
        let (b, c) = ((major + 1) % 3, (major + 2) % 3);
        let floor = |n: i64, m: i64| n.div_euclid(m);
        let ceil = |n: i64, m: i64| -(-n).div_euclid(m);
        let (lo, hi) = (to[major].min(0), to[major].max(0));
        for k in ceil(lo - 1, 2)..=floor(hi + 1, 2) {
            // The parameter range, scaled by d, over which the segment lies in
            // this slab [2k - 1, 2k + 1], clamped to the segment.
            let (enter, leave) = if to[major] > 0 {
                (2 * k - 1, 2 * k + 1)
            } else {
                (-(2 * k + 1), -(2 * k - 1))
            };
            let (na, nb) = (enter.max(0), leave.min(d));
            // Cells whose closed cube meets [p, q] / d on another axis.
            let span = |axis: usize| {
                let (p, q) = (to[axis] * na, to[axis] * nb);
                ceil(p.min(q) - d, 2 * d)..=floor(p.max(q) + d, 2 * d)
            };
            for y in span(b) {
                for z in span(c) {
                    let mut cell = [0; 3];
                    cell[major] = k as i32;
                    cell[b] = y as i32;
                    cell[c] = z as i32;
                    let state = self.state(cell);
                    if state != EMPTY && self.cuts(cell, state == MISSING, to) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl World {
    /// Exact 3D sight from the centre of `eye`, with offsets relative to the eye
    /// cell in the observer's `frame`. Stair landings are not included; abstract
    /// stair links are traversal, not geometry. Identical to
    /// [`World::eye_scene_reference`], only faster. A scene is reused while
    /// the regions it read are unchanged, so it's also identical to
    /// [`World::eye_scene_uncached`].
    pub fn eye_scene(&self, eye: Location, frame: u8, radius: u8) -> Vec<SightCell> {
        if let Some(cells) = self.sight.get(eye, frame, radius) {
            return cells;
        }
        let (cells, regions) = self.build_eye_scene(eye, frame, radius);
        if let Some(regions) = regions {
            self.sight.insert(eye, frame, radius, regions, &cells);
        }
        cells
    }

    /// [`World::eye_scene`] without reuse, for tests and measurements.
    pub fn eye_scene_uncached(&self, eye: Location, frame: u8, radius: u8) -> Vec<SightCell> {
        self.build_eye_scene(eye, frame, radius).0
    }

    /// Whether [`World::eye_scene`] would reuse a scene. Diagnostic only.
    pub fn eye_scene_cached(&self, eye: Location, frame: u8, radius: u8) -> bool {
        self.sight.contains(eye, frame, radius)
    }

    /// The scene, and every region whose terrain or doors it read. There are
    /// no regions to report when the eye's region is missing.
    fn build_eye_scene(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
    ) -> (Vec<SightCell>, Option<Vec<RegionId>>) {
        if !self.walkable(eye) {
            return (vec![], Some(vec![eye.region]));
        }
        let radius = i32::from(radius.min(16));
        let reach = radius + 2;
        let side = (2 * reach + 1) as usize;
        let Some(eye_region) = RegionInfo::new(self, eye.region) else {
            return (vec![], None);
        };
        let p = eye.position;
        let mut scene = Scene {
            world: self,
            eye,
            origin: [p.x, p.y, p.z].map(i64::from),
            frame,
            reach,
            side,
            state: vec![UNRESOLVED; side * side * side],
            faces: vec![0; side * side * side],
            slot: vec![0; side * side * side],
            resolved: Vec::new(),
            regions: vec![eye_region],
        };
        let mut cells = Vec::new();
        for z in -radius..=radius {
            let rz = radius - z.abs();
            for y in -rz..=rz {
                let ry = rz - y.abs();
                for x in -ry..=ry {
                    let o = [x, y, z];
                    let state = scene.state(o);
                    if state == MISSING {
                        continue;
                    }
                    let centre = o.map(|v| 2 * i64::from(v));
                    let visible = if state == EMPTY {
                        scene.sees(centre)
                    } else {
                        let bits = scene.exposed_faces(o);
                        (0..6).any(|face| {
                            bits & (1 << face) != 0 && {
                                let mut point = centre;
                                point[face / 2] += if face % 2 == 0 { -1 } else { 1 };
                                scene.sees(point)
                            }
                        })
                    };
                    if visible {
                        let i = scene.index(o).expect("targets lie within reach");
                        let (location, rotation) = scene.resolved[scene.slot[i] as usize];
                        cells.push(SightCell {
                            location,
                            offset: Position { x, y, z },
                            rotation,
                            wall: state == WALL,
                        });
                    }
                }
            }
        }
        // Offsets are generated in (z, y, x) order; SightCell sorts by offset.
        cells.sort_by_key(|c| c.offset);
        // Every step starts in a region in `regions` and ends in it or in one
        // of its neighbours, so these cover every cell the scene resolved.
        let mut regions: Vec<_> = scene
            .regions
            .iter()
            .flat_map(|r| std::iter::once(r.id).chain(r.neighbours.iter().copied()))
            .collect();
        regions.sort();
        regions.dedup();
        (cells, Some(regions))
    }
}
