//! Sculpted surfaces: shapes described as signed distances, blended into one
//! continuous skin with smooth unions and carved with smooth subtractions,
//! then meshed with surface nets. Used where stacked primitives read as a
//! doll, above all for faces: cheekbones flow into the jaw, the brow shades
//! the eye sockets, the nose grows out of the face.

use crate::props::Builder;
use bevy::prelude::*;

/// One primitive of a sculpt.
#[derive(Clone, Copy)]
pub enum Shape {
    /// An ellipsoid: centre and radii.
    Ellipsoid(Vec3, Vec3),
    /// A capsule from a to b with radii at each end.
    Capsule(Vec3, Vec3, f32, f32),
    /// Everything beyond a plane: points whose distance along the unit
    /// normal exceeds the offset. Carving one trims a shape flat.
    Beyond(Vec3, f32),
}

impl Shape {
    fn distance(self, p: Vec3) -> f32 {
        match self {
            Shape::Ellipsoid(c, r) => {
                // A good approximation of the distance to an ellipsoid.
                let q = (p - c) / r;
                let k0 = q.length();
                let k1 = ((p - c) / (r * r)).length();
                if k1 < 1e-6 {
                    -r.min_element()
                } else {
                    k0 * (k0 - 1.0) / k1
                }
            }
            Shape::Capsule(a, b, ra, rb) => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                p.distance(a + ab * t) - (ra + (rb - ra) * t)
            }
            Shape::Beyond(n, o) => o - p.dot(n),
        }
    }
}

/// A step of a sculpt: add clay or carve it away, blended over `k`.
#[derive(Clone, Copy)]
pub enum Step {
    Add(Shape, f32),
    Carve(Shape, f32),
}

fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
    -smooth_min(-a, -b, k)
}

/// A sculpture: an ordered list of additions and carvings.
#[derive(Default, Clone)]
pub struct Sculpt {
    pub steps: Vec<Step>,
}

impl Sculpt {
    pub fn add(&mut self, shape: Shape, k: f32) -> &mut Self {
        self.steps.push(Step::Add(shape, k));
        self
    }

    pub fn carve(&mut self, shape: Shape, k: f32) -> &mut Self {
        self.steps.push(Step::Carve(shape, k));
        self
    }

    pub fn distance(&self, p: Vec3) -> f32 {
        let mut d = f32::MAX;
        for step in &self.steps {
            match *step {
                Step::Add(s, k) => d = if d == f32::MAX { s.distance(p) } else { smooth_min(d, s.distance(p), k) },
                Step::Carve(s, k) => d = smooth_max(d, -s.distance(p), k),
            }
        }
        d
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
            p += dir * d.max(-0.01).min(0.02);
        }
        p
    }

    /// How open the surface is at `p`: 1 in the open, lower in creases.
    fn occlusion(&self, p: Vec3, n: Vec3) -> f32 {
        let mut occ = 0.0;
        for (h, w) in [(0.008f32, 1.0f32), (0.018, 0.6), (0.035, 0.35)] {
            occ += (h - self.distance(p + n * h)).max(0.0) / h * w;
        }
        // A little darker facing down, as under a warm key light from above.
        let below = (-n.y).max(0.0) * 0.12;
        (1.0 - occ * 0.32 - below).clamp(0.55, 1.0)
    }

    fn normal(&self, p: Vec3) -> Vec3 {
        let e = 0.0015;
        Vec3::new(
            self.distance(p + Vec3::X * e) - self.distance(p - Vec3::X * e),
            self.distance(p + Vec3::Y * e) - self.distance(p - Vec3::Y * e),
            self.distance(p + Vec3::Z * e) - self.distance(p - Vec3::Z * e),
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

    pub fn mesh(&self, b: &mut Builder, min: Vec3, max: Vec3, step: f32, paint: &dyn Fn(Vec3, Vec3) -> Vec3) {
        let size = ((max - min) / step).ceil().as_uvec3() + UVec3::ONE;
        let (nx, ny, nz) = (size.x as usize, size.y as usize, size.z as usize);
        let at = |i: usize, j: usize, k: usize| min + Vec3::new(i as f32, j as f32, k as f32) * step;
        let index = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;
        let mut field = vec![0.0f32; nx * ny * nz];
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    field[index(i, j, k)] = self.distance(at(i, j, k));
                }
            }
        }
        // One vertex per cell the surface passes through, at the mean of its
        // edge crossings.
        let cells = (nx - 1, ny - 1, nz - 1);
        let cell = |i: usize, j: usize, k: usize| (k * cells.1 + j) * cells.0 + i;
        let mut vertex: Vec<Option<Vec3>> = vec![None; cells.0 * cells.1 * cells.2];
        const CORNERS: [(usize, usize, usize); 8] = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)];
        const EDGES: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
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
                    vertex[cell(i, j, k)] = Some(sum / n);
                }
            }
        }
        // A quad across every grid edge that crosses the surface, joining the
        // four cells around it.
        let mut quad = |cs: [(usize, usize, usize); 4], flip: bool| {
            let vs: Option<Vec<Vec3>> = cs.iter().map(|&(i, j, k)| vertex[cell(i, j, k)]).collect();
            let Some(mut vs) = vs else {
                return;
            };
            if flip {
                vs.reverse();
            }
            for tri in [[vs[0], vs[1], vs[2]], [vs[0], vs[2], vs[3]]] {
                let normals = tri.map(|p| self.normal(p));
                // Soft occlusion baked into the colour: creases, sockets and
                // the undersides of forms darken gently, as a painted model's do.
                let colors = std::array::from_fn(|q| paint(tri[q], normals[q]) * self.occlusion(tri[q], normals[q]));
                b.tri_smooth(tri, normals, colors);
            }
        };
        for k in 1..cells.2 {
            for j in 1..cells.1 {
                for i in 0..cells.0 {
                    // Edge along x from (i, j, k).
                    let (a, c) = (field[index(i, j, k)], field[index(i + 1, j, k)]);
                    if (a > 0.0) != (c > 0.0) {
                        quad([(i, j - 1, k - 1), (i, j, k - 1), (i, j, k), (i, j - 1, k)], a > 0.0);
                    }
                }
            }
        }
        for k in 1..cells.2 {
            for j in 0..cells.1 {
                for i in 1..cells.0 {
                    let (a, c) = (field[index(i, j, k)], field[index(i, j + 1, k)]);
                    if (a > 0.0) != (c > 0.0) {
                        quad([(i - 1, j, k - 1), (i - 1, j, k), (i, j, k), (i, j, k - 1)], a > 0.0);
                    }
                }
            }
        }
        for k in 0..cells.2 {
            for j in 1..cells.1 {
                for i in 1..cells.0 {
                    let (a, c) = (field[index(i, j, k)], field[index(i, j, k + 1)]);
                    if (a > 0.0) != (c > 0.0) {
                        quad([(i - 1, j - 1, k), (i, j - 1, k), (i, j, k), (i - 1, j, k)], a > 0.0);
                    }
                }
            }
        }
    }
}
