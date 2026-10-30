//! Sculpted surfaces: shapes described as signed distances, blended into one
//! continuous skin with smooth unions and carved with smooth subtractions,
//! then meshed with surface nets. Used where stacked primitives read as a
//! doll, above all for faces: cheekbones flow into the jaw, the brow shades
//! the eye sockets, the nose grows out of the face.

mod contact_field;
mod differential;
mod partition;
mod refinement;
use crate::props::Builder;
use bevy::prelude::*;
pub(crate) use contact_field::ContactField;

/// One primitive of a sculpt.
#[derive(Clone, Copy)]
pub enum Shape {
    /// An ellipsoid: centre and radii.
    Ellipsoid(Vec3, Vec3),
    /// A capsule from a to b with radii at each end.
    Capsule(Vec3, Vec3, f32, f32),
    /// A horizontal rounded ring: center, major/minor radii and Z stretch.
    Torus(Vec3, Vec2, f32),
    /// Everything beyond a plane: points whose distance along the unit
    /// normal exceeds the offset. Carving one trims a shape flat.
    Beyond(Vec3, f32),
}

impl Shape {
    pub(crate) fn distance(self, p: Vec3) -> f32 {
        self.evaluate::<false>(p).distance
    }

    /// The range this shape's distance can take anywhere within `radius` of
    /// `center`, when it can be bounded. A capsule's distance changes no
    /// faster than one plus its taper; a plane's exactly as fast as the
    /// point moves; the ellipsoid's approximation no faster than its
    /// longest radius over its shortest.
    fn bounds(self, center: Vec3, radius: f32) -> Option<(f32, f32)> {
        let slope = match self {
            Shape::Capsule(a, b, ra, rb) => 1.0 + (rb - ra).abs() / a.distance(b).max(1e-6),
            Shape::Beyond(..) => 1.0,
            Shape::Ellipsoid(_, r) => r.max_element() / r.min_element().max(1e-6),
            Shape::Torus(_, _, depth) => 1.0f32.max(1.0 / depth),
        };
        let d = self.distance(center);
        Some((d - slope * radius, d + slope * radius))
    }
}

/// A step of a sculpt: add clay or carve it away, blended over `k`.
#[derive(Clone)]
pub enum Step {
    Add(Shape, f32),
    Carve(Shape, f32),
    /// Grow everything sculpted so far outward by a distance: a garment
    /// laid over the form it covers, its small hollows filled.
    Inflate(f32),
    /// Join a complete layer, preserving its own order of growth and cuts.
    Union(Sculpt, f32),
}

/// Distance and derivative weight of the existing (left) source. Surface
/// provenance uses the exact same blend as geometry, including ordered unions.
#[inline]
pub(crate) fn smooth_min_with_weight(a: f32, b: f32, k: f32) -> (f32, f32) {
    if k <= 0.0 {
        return (a.min(b), if a <= b { 1.0 } else { 0.0 });
    }
    if b - a >= k {
        return (a, 1.0);
    }
    if a - b >= k {
        return (b, 0.0);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    (b + (a - b) * h - k * h * (1.0 - h), h)
}

/// A sculpture: an ordered list of additions and carvings.
#[derive(Default, Clone)]
pub struct Sculpt {
    pub steps: Vec<Step>,
}

#[derive(Clone, Copy)]
struct Sampling {
    step: f32,
    placement: VertexPlacement,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VertexPlacement {
    Average,
    Analytic,
    Symmetric,
}

// Keep a quarter of the one-millimeter physical error allowance for the
// nonlinear difference between the local field estimate and actual distance.
const PROJECTED_ERROR: f32 = 0.00075;

impl Sculpt {
    pub fn add(&mut self, shape: Shape, k: f32) -> &mut Self {
        self.steps.push(Step::Add(shape, k));
        self
    }

    pub fn carve(&mut self, shape: Shape, k: f32) -> &mut Self {
        self.steps.push(Step::Carve(shape, k));
        self
    }

    pub fn inflate(&mut self, amount: f32) -> &mut Self {
        self.steps.push(Step::Inflate(amount));
        self
    }

    /// Combine independently constructed layers without letting a child's
    /// clipping planes trim earlier geometry.
    pub fn union(&mut self, other: Sculpt, k: f32) -> &mut Self {
        if !other.steps.is_empty() {
            self.steps.push(Step::Union(other, k));
        }
        self
    }

    pub fn distance(&self, p: Vec3) -> f32 {
        self.distance_among(p, self.steps.iter())
    }

    /// The field at `p` from these steps, in order: all of them, or only
    /// those that can shape the field near `p`.
    fn distance_among<'a>(&self, p: Vec3, steps: impl Iterator<Item = &'a Step>) -> f32 {
        self.evaluate_among::<false>(p, steps).distance
    }

    /// The steps that can shape the field anywhere within `radius` of
    /// `center`, written to `out` in order, and the range the field takes
    /// there. The field is followed as an interval through the steps: an
    /// addition that stays at least its blend beyond the field's highest
    /// value there, or a carving whose inside stays at least its blend below
    /// the field's lowest, changes nothing and is left out; the field
    /// computed from the rest is identical.
    fn steps_near(&self, center: Vec3, radius: f32, out: &mut Vec<usize>) -> (f32, f32) {
        out.clear();
        let (mut lo, mut hi) = (f32::INFINITY, f32::INFINITY);
        let mut started = false;
        for (index, step) in self.steps.iter().enumerate() {
            match *step {
                Step::Add(shape, k) => {
                    let bounds = shape.bounds(center, radius);
                    if !started {
                        started = true;
                        (lo, hi) = bounds.unwrap_or((f32::NEG_INFINITY, f32::INFINITY));
                    } else if let Some((s_lo, s_hi)) = bounds {
                        if s_lo >= hi + k.max(0.0) {
                            continue;
                        }
                        hi = hi.min(s_hi);
                        lo = lo.min(s_lo) - k.max(0.0) * 0.25;
                    } else {
                        // A smooth union never rises above the field it joins.
                        lo = f32::NEG_INFINITY;
                    }
                }
                Step::Carve(shape, k) => {
                    if started {
                        if let Some((s_lo, s_hi)) = shape.bounds(center, radius) {
                            if -s_lo <= lo - k.max(0.0) {
                                continue;
                            }
                            lo = lo.max(-s_hi);
                            hi = hi.max(-s_lo) + k.max(0.0) * 0.25;
                        } else {
                            // A smooth carving never sinks below the field it cuts.
                            hi = f32::INFINITY;
                        }
                    }
                }
                Step::Inflate(amount) => {
                    lo -= amount;
                    hi -= amount;
                }
                Step::Union(ref other, k) => {
                    let (s_lo, s_hi) = other.steps_near(center, radius, &mut Vec::new());
                    if !started {
                        started = true;
                        (lo, hi) = (s_lo, s_hi);
                    } else {
                        if s_lo >= hi + k.max(0.0) {
                            continue;
                        }
                        hi = hi.min(s_hi);
                        lo = lo.min(s_lo) - k.max(0.0) * 0.25;
                    }
                }
            }
            out.push(index);
        }
        (lo, hi)
    }

    /// The point on the surface straight ahead of `from` along `dir`, found
    /// by marching in; used to seat buttons, laces and straps on clothing.
    pub fn onto(&self, from: Vec3, dir: Vec3) -> Vec3 {
        let mut p = from;
        for _ in 0..64 {
            let d = self.distance(p);
            if d.abs() < 0.0005 {
                break;
            }
            p += dir * d.clamp(-0.01, 0.02);
        }
        p
    }

    /// How open the surface is at `p`: 1 in the open, lower in creases.
    fn occlusion(&self, p: Vec3, n: Vec3, near: &[usize]) -> f32 {
        let distance = |q: Vec3| self.distance_among(q, near.iter().map(|&i| &self.steps[i]));
        let mut occ = 0.0;
        for (h, w) in [(0.008f32, 1.0f32), (0.018, 0.6), (0.035, 0.35)] {
            occ += (h - distance(p + n * h)).max(0.0) / h * w;
        }
        // Runtime lighting supplies directional shade; bake only local creases.
        (1.0 - occ * 0.32).clamp(0.55, 1.0)
    }

    fn normal(&self, p: Vec3, near: &[usize]) -> Vec3 {
        let distance = |q: Vec3| self.distance_among(q, near.iter().map(|&i| &self.steps[i]));
        let e = 0.0015;
        Vec3::new(
            distance(p + Vec3::X * e) - distance(p - Vec3::X * e),
            distance(p + Vec3::Y * e) - distance(p - Vec3::Y * e),
            distance(p + Vec3::Z * e) - distance(p - Vec3::Z * e),
        )
        .normalize_or(Vec3::Y)
    }

    /// Mesh the surface inside `min`..`max` at `step` spacing with surface
    /// nets, painting each vertex with `paint(point, normal)`.
    /// A fine woven texture for cloth: crossed threads as a faint light and
    /// dark lattice over the colour.
    pub fn weave(at: Vec3) -> f32 {
        let u = (at.x * 260.0 + at.z * 90.0).sin();
        let v = (at.y * 260.0).sin();
        1.0 + 0.035 * u * v + 0.02 * (at.y * 31.0 + at.x * 17.0).sin()
    }

    /// The field sampled on a grid of `size` points from `min` at `step`
    /// spacing, x fastest. Near the surface every sample is exact; away from
    /// it a sample may hold a bound of the right sign instead.
    fn field(&self, min: Vec3, step: f32, size: (usize, usize, usize)) -> Vec<f32> {
        let (nx, ny, nz) = size;
        let at =
            |i: usize, j: usize, k: usize| min + Vec3::new(i as f32, j as f32, k as f32) * step;
        let index = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;
        let mut field = vec![0.0f32; nx * ny * nz];
        // Surface nets read only the field's sign away from the surface. A
        // small block of the grid wholly outside (or inside) the sculpt by
        // more than two cells takes its bound instead, as no crossing can lie
        // within reach of it; any other block evaluates exactly, from only
        // the steps that can shape the field within it.
        const BLOCK: usize = 4;
        let blocks = (nx.div_ceil(BLOCK), ny.div_ceil(BLOCK), nz.div_ceil(BLOCK));
        let half = step * BLOCK as f32 * 0.5;
        let clear = 2.0 * step;
        let mut steps = Vec::new();
        for bk in 0..blocks.2 {
            for bj in 0..blocks.1 {
                for bi in 0..blocks.0 {
                    let center = at(bi * BLOCK, bj * BLOCK, bk * BLOCK) + Vec3::splat(half);
                    let (lo, hi) = self.steps_near(center, half * 3f32.sqrt(), &mut steps);
                    let settled = if lo > clear {
                        Some(lo)
                    } else if hi < -clear {
                        Some(hi)
                    } else {
                        None
                    };
                    for k in bk * BLOCK..((bk + 1) * BLOCK).min(nz) {
                        for j in bj * BLOCK..((bj + 1) * BLOCK).min(ny) {
                            for i in bi * BLOCK..((bi + 1) * BLOCK).min(nx) {
                                field[index(i, j, k)] = settled.unwrap_or_else(|| {
                                    self.distance_among(
                                        at(i, j, k),
                                        steps.iter().map(|&s| &self.steps[s]),
                                    )
                                });
                            }
                        }
                    }
                }
            }
        }
        field
    }

    fn project_in_cell(
        &self,
        mut p: Vec3,
        low: Vec3,
        step: f32,
        near: &[usize],
        placement: VertexPlacement,
    ) -> Vec3 {
        let distance = |q| self.distance_among(q, near.iter().map(|&i| &self.steps[i]));
        let high = low + Vec3::splat(step);
        for _ in 0..8 {
            let steps = near.iter().map(|&i| &self.steps[i]);
            let sample = match placement {
                VertexPlacement::Analytic => self.sample_among(p, steps),
                VertexPlacement::Symmetric => {
                    self.sample_symmetric_among(p, 0.001f32.min(step * 0.25), steps)
                }
                VertexPlacement::Average => unreachable!("cell averages do not project"),
            };
            let gap = sample.distance;
            if !gap.is_finite() || gap.abs() < 1e-6 {
                break;
            }
            let mut gradient = sample.gradient;
            // Once a cell face blocks an axis, solve along the remaining
            // axes instead of repeatedly pushing into that same constraint.
            for axis in 0..3 {
                let movement = -gap * gradient[axis];
                if (p[axis] <= low[axis] && movement < 0.0)
                    || (p[axis] >= high[axis] && movement > 0.0)
                {
                    gradient[axis] = 0.0;
                }
            }
            let norm = gradient.length_squared();
            if !norm.is_finite() || norm < 1e-12 {
                break;
            }
            let correction = (gradient * (gap / norm)).clamp_length_max(step * 1.75);
            let mut fraction = 1.0;
            let mut improved = false;
            for _ in 0..6 {
                let candidate = (p - correction * fraction).clamp(low, high);
                let next_gap = distance(candidate);
                if candidate.is_finite() && next_gap.is_finite() && next_gap.abs() < gap.abs() {
                    p = candidate;
                    improved = true;
                    break;
                }
                fraction *= 0.5;
            }
            if !improved {
                break;
            }
        }
        p
    }

    pub fn mesh(
        &self,
        b: &mut Builder,
        min: Vec3,
        max: Vec3,
        step: f32,
        paint: &dyn Fn(Vec3, Vec3) -> Vec3,
    ) {
        self.mesh_partitioned(b, min, max, step, paint, None);
    }

    /// Preserve a hard coverage edge instead of interpolating its pigments.
    #[allow(clippy::too_many_arguments)]
    pub fn mesh_partitioned(
        &self,
        b: &mut Builder,
        min: Vec3,
        max: Vec3,
        step: f32,
        paint: &dyn Fn(Vec3, Vec3) -> Vec3,
        coverage: Option<&dyn Fn(Vec3) -> bool>,
    ) {
        self.mesh_sampled(
            b,
            min,
            max,
            Sampling {
                step,
                placement: VertexPlacement::Average,
            },
            paint,
            coverage,
        );
    }

    /// Closed masses can use a wider grid when crossing averages are projected
    /// toward the exact surface. Constrained placement starts in each cell,
    /// with a small bounded correction for stalled vertices. Fitted shells
    /// and hard painted cut edges retain their authored mesher.
    pub fn mesh_projected(
        &self,
        b: &mut Builder,
        min: Vec3,
        max: Vec3,
        step: f32,
        paint: &dyn Fn(Vec3, Vec3) -> Vec3,
    ) {
        self.mesh_sampled(
            b,
            min,
            max,
            Sampling {
                step,
                placement: VertexPlacement::Analytic,
            },
            paint,
            None,
        );
    }

    /// A flattened mass declares a narrow crest at its center plane. Put that
    /// plane on the sampling grid so a coarse cell cannot step over the crest.
    /// The grid only expands the supplied bounds; legacy sampling is unchanged.
    fn feature_sampling(&self, mut min: Vec3, requested: f32) -> (Vec3, f32) {
        fn visit(sculpt: &Sculpt, anchors: &mut [Option<(f32, f32)>; 3], curvature: &mut f32) {
            for shape in &sculpt.steps {
                match shape {
                    Step::Add(Shape::Ellipsoid(center, radii), _) => {
                        for axis in 0..3 {
                            let ratio =
                                radii[axis] / radii[(axis + 1) % 3].min(radii[(axis + 2) % 3]);
                            if ratio < 1.0 / 3.0 {
                                // A flat ellipsoid's rim curvature is much
                                // tighter than its minor radius suggests.
                                *curvature =
                                    curvature.min(radii[axis].powi(2) / radii.max_element());
                                if anchors[axis].is_none_or(|(best, _)| ratio < best) {
                                    anchors[axis] = Some((ratio, center[axis]));
                                }
                            }
                        }
                    }
                    Step::Add(Shape::Torus(center, radii, _), _) => {
                        let ratio = radii.y / (radii.x + radii.y);
                        if anchors[1].is_none_or(|(best, _)| ratio < best) {
                            anchors[1] = Some((ratio, center.y));
                        }
                    }
                    Step::Union(child, _) => visit(child, anchors, curvature),
                    _ => {}
                }
            }
        }
        let mut anchors = [None; 3];
        let mut curvature = f32::INFINITY;
        visit(self, &mut anchors, &mut curvature);
        let step = requested.min((0.0024 * curvature).sqrt());
        for (axis, anchor) in anchors.into_iter().enumerate() {
            if let Some((_, center)) = anchor {
                min[axis] = (center + ((min[axis] - center) / step).floor() * step).min(min[axis]);
            }
        }
        (min, step)
    }

    #[allow(clippy::too_many_arguments)]
    fn mesh_sampled(
        &self,
        b: &mut Builder,
        min: Vec3,
        max: Vec3,
        sampling: Sampling,
        paint: &dyn Fn(Vec3, Vec3) -> Vec3,
        coverage: Option<&dyn Fn(Vec3) -> bool>,
    ) {
        // Align once. A symmetric retry reuses the exact same grid rather than
        // rounding its already aligned bounds a second time.
        let (min, step) = if sampling.placement == VertexPlacement::Analytic {
            self.feature_sampling(min, sampling.step)
        } else {
            (min, sampling.step)
        };
        let profile_started = crate::props::profile_builds().then(std::time::Instant::now);
        let size = ((max - min) / step).ceil().as_uvec3() + UVec3::ONE;
        let (nx, ny, nz) = (size.x as usize, size.y as usize, size.z as usize);
        let at =
            |i: usize, j: usize, k: usize| min + Vec3::new(i as f32, j as f32, k as f32) * step;
        let index = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;
        let field = self.field(min, step, (nx, ny, nz));
        // Normals and occlusion sample within a few centimetres of each
        // surface vertex: the steps near each block that holds surface,
        // gathered once per block.
        const REACH: f32 = 0.04;
        const NORMAL_REACH: f32 = 0.002;
        const SURFACE_BLOCK: usize = 4;
        let surface_blocks = (
            nx.div_ceil(SURFACE_BLOCK),
            ny.div_ceil(SURFACE_BLOCK),
            nz.div_ceil(SURFACE_BLOCK),
        );
        let surface_half = step * SURFACE_BLOCK as f32 * 0.5;
        let mut near: Vec<Option<(Vec<usize>, Vec<usize>)>> =
            vec![None; surface_blocks.0 * surface_blocks.1 * surface_blocks.2];
        let field_done = profile_started.map(|time| time.elapsed());
        // One vertex per cell the surface passes through, at the mean of its
        // edge crossings.
        let cells = (nx - 1, ny - 1, nz - 1);
        let cell = |i: usize, j: usize, k: usize| (k * cells.1 + j) * cells.0 + i;
        // Empty cells carry only an index. Positions and their exact painted
        // attributes live once in a dense surface list, shared by every face.
        // A surface-net vertex normally belongs to several triangles; repeating
        // six normal samples and three occlusion samples for each corner
        // repeats identical distance work during first-use construction.
        #[derive(Clone, Copy)]
        struct SurfaceVertex {
            position: Vec3,
            normal: Vec3,
            pigment: Vec3,
            shade: f32,
        }
        let mut grid_vertex = vec![usize::MAX; cells.0 * cells.1 * cells.2];
        let mut vertex: Vec<SurfaceVertex> = Vec::new();
        const CORNERS: [(usize, usize, usize); 8] = [
            (0, 0, 0),
            (1, 0, 0),
            (0, 1, 0),
            (1, 1, 0),
            (0, 0, 1),
            (1, 0, 1),
            (0, 1, 1),
            (1, 1, 1),
        ];
        const EDGES: [(usize, usize); 12] = [
            (0, 1),
            (2, 3),
            (4, 5),
            (6, 7),
            (0, 2),
            (1, 3),
            (4, 6),
            (5, 7),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        for k in 0..cells.2 {
            for j in 0..cells.1 {
                for i in 0..cells.0 {
                    let d = CORNERS.map(|(a, b2, c)| field[index(i + a, j + b2, k + c)]);
                    if d.iter().all(|v| *v > 0.0) || d.iter().all(|v| *v <= 0.0) {
                        continue;
                    }
                    let mut sum = Vec3::ZERO;
                    let mut n = 0.0;
                    for (a, c) in EDGES {
                        if (d[a] > 0.0) != (d[c] > 0.0) {
                            let t = d[a] / (d[a] - d[c]);
                            let pa = at(i + CORNERS[a].0, j + CORNERS[a].1, k + CORNERS[a].2);
                            let pc = at(i + CORNERS[c].0, j + CORNERS[c].1, k + CORNERS[c].2);
                            sum += pa.lerp(pc, t);
                            n += 1.0;
                        }
                    }
                    let position = sum / n;
                    let (bi, bj, bk) = (i / SURFACE_BLOCK, j / SURFACE_BLOCK, k / SURFACE_BLOCK);
                    let steps = near[(bk * surface_blocks.1 + bj) * surface_blocks.0 + bi]
                        .get_or_insert_with(|| {
                            let center =
                                at(bi * SURFACE_BLOCK, bj * SURFACE_BLOCK, bk * SURFACE_BLOCK)
                                    + Vec3::splat(surface_half);
                            // Normals sample within a couple of millimetres;
                            // occlusion reaches out a few centimetres.
                            let reach = surface_half * 3f32.sqrt();
                            let (mut close, mut wide) = (Vec::new(), Vec::new());
                            self.steps_near(center, reach + NORMAL_REACH, &mut close);
                            self.steps_near(center, reach + REACH, &mut wide);
                            (close, wide)
                        });
                    let position = if sampling.placement != VertexPlacement::Average {
                        self.project_in_cell(
                            position,
                            at(i, j, k),
                            step,
                            &steps.0,
                            sampling.placement,
                        )
                    } else {
                        position
                    };
                    let normal = self.normal(position, &steps.0);
                    let pigment = paint(position, normal);
                    let shade = self.occlusion(position, normal, &steps.1);
                    grid_vertex[cell(i, j, k)] = vertex.len();
                    vertex.push(SurfaceVertex {
                        position,
                        normal,
                        pigment,
                        shade,
                    });
                }
            }
        }
        // Includes crossing extraction, normals, pigment and sculpt occlusion.
        let surface_done = profile_started.map(|time| time.elapsed());
        // A quad across every grid edge that crosses the surface, joining the
        // four cells around it.
        let mut faces = Vec::new();
        let mut quad = |cs: [(usize, usize, usize); 4], flip: bool| {
            let indices = cs.map(|(i, j, k)| grid_vertex[cell(i, j, k)]);
            if indices.contains(&usize::MAX) {
                return;
            }
            let mut indices = indices;
            if flip {
                indices.reverse();
            }
            faces.extend([
                [indices[0], indices[1], indices[2]],
                [indices[0], indices[2], indices[3]],
            ]);
        };
        for k in 1..cells.2 {
            for j in 1..cells.1 {
                for i in 0..cells.0 {
                    // Edge along x from (i, j, k).
                    let (a, c) = (field[index(i, j, k)], field[index(i + 1, j, k)]);
                    if (a > 0.0) != (c > 0.0) {
                        quad(
                            [(i, j - 1, k - 1), (i, j, k - 1), (i, j, k), (i, j - 1, k)],
                            a > 0.0,
                        );
                    }
                }
            }
        }
        for k in 1..cells.2 {
            for j in 0..cells.1 {
                for i in 1..cells.0 {
                    let (a, c) = (field[index(i, j, k)], field[index(i, j + 1, k)]);
                    if (a > 0.0) != (c > 0.0) {
                        quad(
                            [(i - 1, j, k - 1), (i - 1, j, k), (i, j, k), (i, j, k - 1)],
                            a > 0.0,
                        );
                    }
                }
            }
        }
        for k in 0..cells.2 {
            for j in 1..cells.1 {
                for i in 1..cells.0 {
                    let (a, c) = (field[index(i, j, k)], field[index(i, j, k + 1)]);
                    if (a > 0.0) != (c > 0.0) {
                        quad(
                            [(i - 1, j - 1, k), (i, j - 1, k), (i, j, k), (i - 1, j, k)],
                            a > 0.0,
                        );
                    }
                }
            }
        }
        if sampling.placement != VertexPlacement::Average {
            let mut points: Vec<_> = vertex.iter().map(|v| v.position).collect();
            let all: Vec<_> = (0..self.steps.len()).collect();
            let error = |p: Vec3| {
                let sample = match sampling.placement {
                    VertexPlacement::Analytic => self.sample(p),
                    VertexPlacement::Symmetric => {
                        self.sample_symmetric_among(p, 0.001, self.steps.iter())
                    }
                    VertexPlacement::Average => unreachable!("cell averages do not refine"),
                };
                let gap = sample.distance.abs();
                if !gap.is_finite() {
                    return f32::INFINITY;
                }
                if gap <= 1e-6 {
                    return 0.0;
                }
                let norm = sample.gradient.length();
                if !norm.is_finite() || norm < 1e-6 {
                    f32::INFINITY
                } else {
                    gap / norm
                }
            };
            // A flat crest can stall against a cell face. Correct only inaccurate
            // representatives, within a quarter-cell neighbourhood.
            for point in &mut points {
                if error(*point) > PROJECTED_ERROR {
                    let margin = step * 0.25;
                    let candidate = self.project_in_cell(
                        *point,
                        *point - Vec3::splat(margin),
                        margin * 2.0,
                        &all,
                        sampling.placement,
                    );
                    if error(candidate) < error(*point) {
                        *point = candidate;
                    }
                }
            }
            refinement::repair_quad_diagonals(&points, &mut faces, |p| self.normal(p, &all));
            let mut converged =
                refinement::refine(&mut points, &mut faces, PROJECTED_ERROR, error, |p| {
                    self.project_in_cell(
                        p,
                        p - Vec3::splat(step),
                        step * 2.0,
                        &all,
                        sampling.placement,
                    )
                });
            let outward = faces.iter().all(|face| {
                let [a, b, c] = face.map(|index| points[index]);
                let outward = (b - a).cross(c - a);
                outward.is_finite() && outward.dot(self.normal((a + b + c) / 3.0, &all)) > 0.0
            });
            if !outward && crate::props::profile_builds() {
                eprintln!("3d_sdf_refinement_failed reason=face_orientation");
            }
            converged &= outward;
            if !converged {
                if sampling.placement == VertexPlacement::Analytic {
                    if crate::props::profile_builds() {
                        eprintln!("3d_sdf_symmetric_retry step={step:.5} bounds={min:?}..{max:?}");
                    }
                    // A numerical slope can resolve a hard seam that makes
                    // analytic placement fold a face. Try the same grid once.
                    self.mesh_sampled(
                        b,
                        min,
                        max,
                        Sampling {
                            step,
                            placement: VertexPlacement::Symmetric,
                        },
                        paint,
                        coverage,
                    );
                } else {
                    if crate::props::profile_builds() {
                        eprintln!(
                            "3d_sdf_refinement_fallback step={step:.5} bounds={min:?}..{max:?}"
                        );
                    }
                    // Two projected attempts at most, then a nonprojected fine
                    // mesh. Failed attempts never emit partial geometry.
                    self.mesh_partitioned(b, min, max, step * 0.25, paint, coverage);
                }
                return;
            }
            for (index, &position) in points.iter().enumerate() {
                if index < vertex.len() && position == vertex[index].position {
                    continue;
                }
                let normal = self.normal(position, &all);
                let sampled = SurfaceVertex {
                    position,
                    normal,
                    pigment: paint(position, normal),
                    shade: self.occlusion(position, normal, &all),
                };
                if index < vertex.len() {
                    vertex[index] = sampled;
                } else {
                    vertex.push(sampled);
                }
            }
        }
        for indices in faces {
            let tri = indices.map(|index| vertex[index]);
            if let Some(region) = coverage {
                let input = tri.map(|v| partition::Vertex {
                    position: v.position,
                    sample: v.position,
                    normal: v.normal,
                    shade: v.shade,
                });
                if let Some(pieces) = partition::split(input, region) {
                    for face in pieces {
                        b.tri_smooth_shaded(
                            face.map(|v| v.position),
                            face.map(|v| v.normal),
                            face.map(|v| paint(v.sample, v.normal)),
                            face.map(|v| v.shade),
                        );
                    }
                    continue;
                }
            }
            // Preserve the existing triangle order and winding. Pigment
            // remains sRGB and occlusion remains linear until upload.
            b.tri_smooth_shaded(
                tri.map(|v| v.position),
                tri.map(|v| v.normal),
                tri.map(|v| v.pigment),
                tri.map(|v| v.shade),
            );
        }
        if let (Some(started), Some(field_done), Some(surface_done)) =
            (profile_started, field_done, surface_done)
        {
            let elapsed = started.elapsed();
            eprintln!(
                "3d_sdf_mesh grid={nx}x{ny}x{nz} samples={} steps={} surface_vertices={} step={step:.5} field_ms={:.3} surface_ms={:.3} emission_ms={:.3} total_ms={:.3}",
                field.len(),
                self.steps.len(),
                vertex.len(),
                field_done.as_secs_f64() * 1000.0,
                (surface_done - field_done).as_secs_f64() * 1000.0,
                (elapsed - surface_done).as_secs_f64() * 1000.0,
                elapsed.as_secs_f64() * 1000.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_ring_fields_preserve_extents_and_conservative_bounds() {
        let center = Vec3::new(0.11, 0.2, -0.03);
        for depth in [0.75, 1.0, 1.04] {
            let ring = Shape::Torus(center, Vec2::new(0.08, 0.013), depth);
            for p in [
                Vec3::X * 0.093,
                Vec3::X * 0.067,
                Vec3::new(0.08, 0.013, 0.0),
                Vec3::Z * (0.093 * depth),
            ] {
                assert!(ring.distance(center + p).abs() < 1e-6);
            }
            assert!((ring.distance(center + Vec3::X * 0.08) + 0.013).abs() < 1e-6);
            assert!(
                ring.distance(center) > 0.0,
                "the ring retains its central opening"
            );
            let at = center + Vec3::new(0.04, 0.02, -0.03);
            let (low, high) = ring.bounds(at, 0.017).unwrap();
            for direction in [
                Vec3::X,
                -Vec3::X,
                Vec3::Y,
                -Vec3::Y,
                Vec3::Z,
                -Vec3::Z,
                Vec3::ONE.normalize(),
            ] {
                let distance = ring.distance(at + direction * 0.017);
                assert!(distance >= low - 1e-6 && distance <= high + 1e-6);
            }
        }
    }

    #[test]
    fn complete_layers_keep_their_clipping_local_to_the_child() {
        let mut body = Sculpt::default();
        body.add(
            Shape::Ellipsoid(Vec3::Y * 0.25, Vec3::new(0.2, 0.3, 0.15)),
            0.0,
        );
        let mut vest = body.clone();
        vest.inflate(0.02).carve(Shape::Beyond(Vec3::Y, 0.4), 0.0);
        let mut layers = body.clone();
        layers.union(vest.clone(), 0.0);
        let shoulder = Vec3::Y * 0.48;
        assert!(body.distance(shoulder) < 0.0 && vest.distance(shoulder) > 0.0);
        assert_eq!(
            layers.distance(shoulder),
            body.distance(shoulder),
            "vest cut must not remove the tunic shoulder"
        );
        let chest = Vec3::new(0.0, 0.25, 0.16);
        assert!(body.distance(chest) > 0.0 && vest.distance(chest) < 0.0);
        assert_eq!(
            layers.distance(chest),
            vest.distance(chest),
            "outer vest remains the exposed layer"
        );
        let mut alone = Sculpt::default();
        alone.union(vest.clone(), 0.0).union(Sculpt::default(), 0.0);
        assert_eq!(alone.distance(chest), vest.distance(chest));
        layers.inflate(0.005);
        assert_eq!(layers.distance(chest), vest.distance(chest) - 0.005);
    }

    #[test]
    fn blocked_fields_preserve_independent_layer_unions() {
        let mut body = Sculpt::default();
        body.add(
            Shape::Ellipsoid(Vec3::Y * 0.2, Vec3::new(0.19, 0.28, 0.16)),
            0.0,
        );
        let mut vest = body.clone();
        vest.inflate(0.02)
            .carve(Shape::Beyond(Vec3::Y, 0.35), 0.004);
        let mut far = Sculpt::default();
        far.add(Shape::Ellipsoid(Vec3::X * 1.8, Vec3::splat(0.1)), 0.0);
        let mut layers = body;
        layers.union(vest, 0.006).union(far, 0.0).inflate(0.005);
        let min = Vec3::new(-0.3, -0.15, -0.25);
        let (step, size) = (0.025, (24, 32, 22));
        let field = layers.field(min, step, size);
        for k in 0..size.2 {
            for j in 0..size.1 {
                for i in 0..size.0 {
                    let at = min + Vec3::new(i as f32, j as f32, k as f32) * step;
                    let exact = layers.distance(at);
                    let sampled = field[(k * size.1 + j) * size.0 + i];
                    if exact.abs() <= 2.0 * step {
                        assert_eq!(sampled, exact, "layer field near surface {at}");
                    } else {
                        assert_eq!(sampled > 0.0, exact > 0.0, "layer field sign {at}");
                    }
                }
            }
        }
        let mut steps = Vec::new();
        layers.steps_near(Vec3::Y * 0.2, 0.02, &mut steps);
        assert!(
            steps.len() < layers.steps.len(),
            "distant child layers are still culled"
        );
    }

    #[test]
    fn culled_fields_match_full_evaluation_near_the_surface() {
        // A sculpt with every kind of step: a crowd of overlapping tapered
        // capsules (as in hair), ellipsoids, carvings by plane and capsule,
        // and an inflation. Wherever the field could place the surface, the
        // blocked evaluation equals evaluating every step; elsewhere it
        // keeps the sign.
        let mut form = Sculpt::default();
        form.add(
            Shape::Ellipsoid(Vec3::new(0.0, 0.2, 0.0), Vec3::new(0.1, 0.09, 0.11)),
            0.0,
        );
        for k in 0..14 {
            let a = k as f32 * 0.45;
            let root = Vec3::new(a.sin() * 0.09, 0.22, a.cos() * 0.09);
            let tip = Vec3::new(a.sin() * 0.14, -0.15 + 0.01 * k as f32, a.cos() * 0.12);
            form.add(Shape::Capsule(root, tip, 0.03, 0.008), 0.03);
        }
        form.carve(
            Shape::Beyond(Vec3::new(0.0, 0.6, 0.8).normalize(), 0.09),
            0.01,
        )
        .add(
            Shape::Ellipsoid(Vec3::new(0.05, 0.0, 0.0), Vec3::new(0.04, 0.06, 0.03)),
            0.02,
        )
        .carve(
            Shape::Capsule(
                Vec3::new(-0.2, 0.1, 0.0),
                Vec3::new(0.2, 0.1, 0.0),
                0.012,
                0.012,
            ),
            0.008,
        )
        .inflate(0.004)
        .add(
            Shape::Capsule(
                Vec3::new(0.0, -0.2, 0.05),
                Vec3::new(0.0, -0.3, 0.08),
                0.02,
                0.01,
            ),
            0.015,
        );
        let (min, step) = (Vec3::new(-0.22, -0.34, -0.2), 0.0075);
        let size = (60, 90, 54);
        let field = form.field(min, step, size);
        let mut settled = 0;
        for k in 0..size.2 {
            for j in 0..size.1 {
                for i in 0..size.0 {
                    let at = min + Vec3::new(i as f32, j as f32, k as f32) * step;
                    let got = field[(k * size.1 + j) * size.0 + i];
                    let exact = form.distance(at);
                    if exact.abs() <= 2.0 * step {
                        assert_eq!(got, exact, "near the surface at {at}");
                    } else {
                        assert_eq!(got > 0.0, exact > 0.0, "sign at {at}");
                        settled += usize::from(got != exact);
                    }
                }
            }
        }
        assert!(settled > 0, "blocks away from the surface are settled");
    }

    #[test]
    fn shape_distances_change_no_faster_than_their_bounds_allow() {
        // The slopes `bounds` relies on, over anisotropic ellipsoids and
        // tapered capsules, between nearby points inside, outside and across
        // their surfaces.
        let shapes = [
            Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.1, 0.09, 0.11)),
            Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.2, 0.02, 0.05)),
            Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.01, 0.15, 0.03)),
            Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.3, 0.3, 0.004)),
            Shape::Capsule(Vec3::ZERO, Vec3::Y * -0.3, 0.06, 0.01),
            Shape::Capsule(Vec3::ZERO, Vec3::X * 0.05, 0.04, 0.002),
        ];
        let mut seed = 0x2545_f491u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32 * 2.0 - 1.0
        };
        for shape in shapes {
            let (_, slope_bound) = shape.bounds(Vec3::ZERO, 1.0).unwrap();
            let slope = slope_bound - shape.distance(Vec3::ZERO);
            for _ in 0..20_000 {
                let p = Vec3::new(next(), next(), next()) * 0.4;
                let q = p + Vec3::new(next(), next(), next()) * 0.03;
                let change = (shape.distance(p) - shape.distance(q)).abs();
                assert!(
                    change <= slope * p.distance(q) * 1.0001 + 1e-6,
                    "{change} over {} at {p}",
                    p.distance(q)
                );
            }
        }
    }
}

#[cfg(test)]
#[test]
fn painted_cut_edges_do_not_interpolate_between_skin_and_cloth() {
    use crate::props::{Parts, PropMaterials};
    use bevy::render::mesh::VertexAttributeValues;
    let mut form = Sculpt::default();
    form.add(Shape::Ellipsoid(Vec3::ZERO, Vec3::splat(0.1)), 0.0);
    let mut parts = Parts::default();
    form.mesh_partitioned(
        &mut parts.matte,
        Vec3::splat(-0.15),
        Vec3::splat(0.15),
        0.025,
        &|at, _| if at.y < 0.023 { Vec3::X } else { Vec3::Z },
        Some(&|at| at.y < 0.023),
    );
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let model = parts.finish(&mut meshes, &PropMaterials::new(&mut materials));
    let mesh = meshes.get(&model.0[0].0).unwrap();
    let VertexAttributeValues::Float32x4(colors) = mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
    else {
        panic!("colors")
    };
    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
    assert!(indices.len() > 60);
    for tri in indices.as_chunks::<3>().0 {
        let cloth = colors[tri[0]][0] > colors[tri[0]][2];
        assert!(
            tri.iter().all(|&i| (colors[i][0] > colors[i][2]) == cloth),
            "one triangle bridges the skin/cloth cut and blends pigment past the intended edge"
        );
    }
}
