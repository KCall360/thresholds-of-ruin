//! Accelerated exact 3D sight. It must return exactly what
//! [`World::eye_scene_reference`] returns; see `docs/sight-3d.md`.
//!
//! The model is unchanged. The speed comes from resolving plain routes
//! directly, stepping plain cells without topology lookups, caching each
//! cell's state and exposed faces once per scene, walking only the cells along
//! each sight line, and testing a blocker's cube before its bevels.
use crate::geometry::{axis_sign, RegionInfo};
use crate::sight3d::Fraction;
use crate::sight_cache::SightMode;
use crate::{compose_rotation, Direction, Location, Position, RegionId, SightCell, Terrain, World};

const UNRESOLVED: u8 = 0;
const MISSING: u8 = 1;
const EMPTY: u8 = 2;
/// Opaque but not a wall: a closed door.
const BARRIER: u8 = 3;
const WALL: u8 = 4;
/// Set in `faces` once a cell's exposed faces are known.
const FACES_KNOWN: u8 = 0x40;

struct Scene<'a> {
    world: &'a World,
    eye: Location,
    origin: [i64; 3],
    frame: u8,
    reach: i32,
    side: usize,
    state: Vec<u8>,
    unoccluded: Vec<bool>,
    faces: Vec<u8>,
    slot: Vec<u32>,
    resolved: Vec<(Location, u8)>,
    /// Index 0 is the eye's region.
    regions: Vec<RegionInfo>,
}

struct BuiltScene {
    cells: Vec<SightCell>,
    /// Regions whose resolved terrain is needed for exact disclosure.
    read: Option<Vec<RegionId>>,
    /// Additional geometry witnesses for a successful acceleration proof.
    proof: Vec<RegionId>,
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
    fn route(&mut self, o: [i32; 3]) -> Option<(Location, u8, bool)> {
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
                true,
            ));
        }
        self.walk(o)
            .map(|(location, frame)| (location, frame, false))
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
        let Some((location, rotation, plain)) = self.route(o) else {
            return MISSING;
        };
        // A straight segment wholly inside a convex transparent box cannot
        // meet a blocker. Portal routes and solid surfaces retain exact rays.
        self.unoccluded[i] =
            plain && self.regions[0].unoccluded(self.eye.position, location.position);
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
    /// Lit targets use the same occlusion rule; unlit cells remain transparent.
    pub fn illuminated_eye_scene(&self, eye: Location, frame: u8, radius: u8) -> Vec<SightCell> {
        if let Some(cells) = self
            .sight
            .get_mode(eye, frame, radius, SightMode::Illuminated)
        {
            return cells;
        }
        let BuiltScene {
            cells,
            read: regions,
            proof,
        } = self.build_eye_scene(eye, frame, radius, true);
        if let Some(regions) = regions {
            self.sight.insert_mode(
                (eye, frame, radius, SightMode::Illuminated),
                regions,
                &cells,
                &proof,
            );
        }
        cells
    }
    pub fn illuminated_eye_scene_regions(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
    ) -> Option<Vec<RegionId>> {
        if let Some(regions) = self
            .sight
            .regions_mode(eye, frame, radius, SightMode::Illuminated)
        {
            return Some(regions);
        }
        self.illuminated_eye_scene(eye, frame, radius);
        self.sight
            .regions_mode(eye, frame, radius, SightMode::Illuminated)
    }
    /// Unoccluded local awareness, resolved through geometry rather than movement.
    pub fn neighborhood_scene(&self, at: Location, frame: u8) -> Vec<SightCell> {
        if let Some(cells) = self.sight.get_mode(at, frame, 1, SightMode::Neighborhood) {
            return cells;
        }
        let mut cells = Vec::new();
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    if let Some((location, rotation)) = self.geometry_route(at, frame, [x, y, z]) {
                        if self.contains(location) {
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
        }
        let regions: std::collections::BTreeSet<_> = std::iter::once(at.region)
            .chain(cells.iter().map(|c| c.location.region))
            .flat_map(|id| std::iter::once(id).chain(self.linked_regions(id)))
            .collect();
        self.sight.insert_mode(
            (at, frame, 1, SightMode::Neighborhood),
            regions,
            &cells,
            &[],
        );
        cells
    }

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
        let BuiltScene {
            cells,
            read: regions,
            proof,
        } = self.build_eye_scene(eye, frame, radius, false);
        if let Some(regions) = regions {
            self.sight
                .insert(eye, frame, radius, regions, &cells, &proof);
        }
        cells
    }

    /// Every region [`World::eye_scene`] reads for this eye: the regions it
    /// entered and every region linked from them. The scene is exact only
    /// while all of them are loaded. `None` when the eye's region isn't.
    pub fn eye_scene_regions(&self, eye: Location, frame: u8, radius: u8) -> Option<Vec<RegionId>> {
        if let Some(regions) = self.sight.regions(eye, frame, radius) {
            return Some(regions);
        }
        let BuiltScene {
            cells,
            read: regions,
            proof,
        } = self.build_eye_scene(eye, frame, radius, false);
        if let Some(regions) = &regions {
            self.sight
                .insert(eye, frame, radius, regions.iter().copied(), &cells, &proof);
        }
        regions
    }

    /// The regions [`World::eye_scene`]'s visible cells are in, in order,
    /// without copying the scene when it's cached.
    pub fn eye_scene_visible_regions(&self, eye: Location, frame: u8, radius: u8) -> Vec<RegionId> {
        if let Some(regions) = self.sight.visible(eye, frame, radius) {
            return regions;
        }
        let mut regions: Vec<_> = self
            .eye_scene(eye, frame, radius)
            .iter()
            .map(|cell| cell.location.region)
            .collect();
        regions.sort();
        regions.dedup();
        regions
    }

    /// [`World::eye_scene`] without reuse, for tests and measurements.
    pub fn eye_scene_uncached(&self, eye: Location, frame: u8, radius: u8) -> Vec<SightCell> {
        self.build_eye_scene(eye, frame, radius, false).cells
    }

    /// Fresh illumination-filtered geometry, for diagnostics and equivalence checks.
    pub fn illuminated_eye_scene_uncached(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
    ) -> Vec<SightCell> {
        self.build_eye_scene(eye, frame, radius, true).cells
    }

    /// Whether [`World::eye_scene`] would reuse a scene. Diagnostic only.
    pub fn eye_scene_cached(&self, eye: Location, frame: u8, radius: u8) -> bool {
        self.sight.contains(eye, frame, radius)
    }

    /// A bounded physical component can prove that no route leaves its height
    /// slab. Rotated or translated joins that change height, missing regions,
    /// and larger components fall back to the general resolver. The cap keeps
    /// this proof independent of total world size; all inspected regions are
    /// dependencies when the proof succeeds.
    fn sight_slab(
        &self,
        eye: Location,
        frame: u8,
    ) -> (Option<(i64, i64)>, std::collections::BTreeSet<RegionId>) {
        const MAX_REGIONS: usize = 32;
        let up = crate::rotate_vector(frame, [0, 0, 1]);
        let axis = up.iter().position(|v| *v != 0).unwrap();
        let sign = up[axis];
        let mut pending = vec![eye.region];
        let mut read = std::collections::BTreeSet::new();
        let mut slab = None;
        while let Some(id) = pending.pop() {
            if !read.insert(id) {
                continue;
            }
            if read.len() > MAX_REGIONS {
                return (None, read);
            }
            let Some(region) = self.region(id) else {
                // Geometry routes cannot enter an unloaded region. End this
                // branch, retaining its presence witness so attachment retries
                // the proof before newly reachable geometry can be disclosed.
                continue;
            };
            let origin = region.bounds.origin;
            let lo = i64::from([origin.x, origin.y, origin.z][axis]);
            let (w, d, h) = region.bounds.dimensions();
            let bounds = (lo, lo + i64::from([w, d, h][axis]) - 1);
            if slab.is_some_and(|previous| previous != bounds) {
                return (None, read);
            }
            slab = Some(bounds);
            for passage in self.exits(id) {
                if self.is_stair(passage.from, passage.direction) {
                    continue;
                }
                let from = passage.from.position;
                let to = passage.to.position;
                let delta = passage.direction.delta();
                let turns = self.crossing_rotation(passage.from, passage.direction);
                if [delta.0, delta.1, delta.2][axis] != 0
                    || [from.x, from.y, from.z][axis] != [to.x, to.y, to.z][axis]
                    || crate::rotate_vector(turns, up) != up
                {
                    return (None, read);
                }
                pending.push(passage.to.region);
            }
        }
        let origin = i64::from([eye.position.x, eye.position.y, eye.position.z][axis]);
        let bounds = slab.map(|(lo, hi)| {
            let a = (lo - origin) * sign;
            let b = (hi - origin) * sign;
            (a.min(b), a.max(b))
        });
        (bounds, read)
    }

    /// The scene, and every region whose terrain or doors it read. There are
    /// no regions to report when the eye's region is missing.
    fn build_eye_scene(
        &self,
        eye: Location,
        frame: u8,
        radius: u8,
        illuminated: bool,
    ) -> BuiltScene {
        if !self.walkable(eye) {
            return BuiltScene {
                cells: vec![],
                read: Some(vec![eye.region]),
                proof: vec![],
            };
        }
        let radius = i32::from(radius.min(16));
        let reach = radius + 2;
        let side = (2 * reach + 1) as usize;
        let Some(eye_region) = RegionInfo::new(self, eye.region) else {
            return BuiltScene {
                cells: vec![],
                read: None,
                proof: vec![],
            };
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
            unoccluded: vec![false; side * side * side],
            faces: vec![0; side * side * side],
            slot: vec![0; side * side * side],
            resolved: Vec::new(),
            regions: vec![eye_region],
        };
        let (slab, slab_regions) = self.sight_slab(eye, frame);
        let mut cells = Vec::new();
        for z in -radius..=radius {
            if slab.is_some_and(|(lo, hi)| i64::from(z) < lo || i64::from(z) > hi) {
                continue;
            }
            let rz = radius - z.abs();
            for y in -rz..=rz {
                let ry = rz - y.abs();
                for x in -ry..=ry {
                    let o = [x, y, z];
                    let state = scene.state(o);
                    if state == MISSING {
                        continue;
                    }
                    let index = scene.index(o).expect("target lies within reach");
                    let (target, _) = scene.resolved[scene.slot[index] as usize];
                    if illuminated && self.is_lit(target) != Some(true) {
                        continue;
                    }
                    let centre = o.map(|v| 2 * i64::from(v));
                    let visible = if scene.unoccluded[index] {
                        true
                    } else if state == EMPTY {
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
        let proof = if slab.is_some() {
            slab_regions
                .into_iter()
                .filter(|region| regions.binary_search(region).is_err())
                .collect()
        } else {
            vec![]
        };
        BuiltScene {
            cells,
            read: Some(regions),
            proof,
        }
    }
}

#[cfg(test)]
mod slab_tests {
    use super::*;

    #[test]
    fn missing_regions_end_height_proof_branches_until_attached() {
        let at = |region, x| Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        };
        let mut world = World::new(
            vec![
                crate::Region {
                    id: RegionId(1),
                    name: "near".into(),
                    bounds: crate::Extent::new(3, 3, 2).unwrap(),
                },
                crate::Region {
                    id: RegionId(2),
                    name: "taller".into(),
                    bounds: crate::Extent::new(3, 3, 5).unwrap(),
                },
            ],
            vec![crate::Passage {
                from: at(1, 2),
                direction: Direction::East,
                to: at(2, 0),
            }],
        )
        .unwrap();
        let eye = at(1, 1);
        assert!(world.sight_slab(eye, 0).0.is_none());
        let detached = world.detach_region(RegionId(2)).unwrap();
        let (bounds, witnesses) = world.sight_slab(eye, 0);
        assert_eq!(bounds, Some((0, 1)));
        assert!(witnesses.contains(&RegionId(2)));
        assert_eq!(
            world.eye_scene(eye, 0, 16),
            world.eye_scene_reference(eye, 0, 16)
        );
        assert!(world.sight.get(eye, 0, 16).is_some());
        world.attach_region(detached).unwrap();
        assert!(world.sight.get(eye, 0, 16).is_none());
        assert!(world.sight_slab(eye, 0).0.is_none());
        assert_eq!(
            world.eye_scene(eye, 0, 16),
            world.eye_scene_reference(eye, 0, 16)
        );
    }
}
