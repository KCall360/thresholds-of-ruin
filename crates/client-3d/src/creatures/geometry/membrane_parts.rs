//! Closed stretched skin around a fan of articulated supports.
//! Recipes own landmarks, facing, scallop depth, tension and physical thickness.
use super::*;

pub(super) struct MembraneFan<'a> {
    pub center: Vec3,
    pub rim: &'a [Vec3],
    pub outward: Vec3,
    pub scallop: f32,
    pub camber: f32,
    pub support_bow: f32,
    pub thickness: f32,
}

struct Surface {
    points: Vec<Vec3>,
    triangles: Vec<[usize; 3]>,
    boundary: Vec<usize>,
    normal: Vec3,
}

const RADIAL: usize = 6;
const EDGE: usize = 8;

impl MembraneFan<'_> {
    fn surface(&self) -> Surface {
        assert!(self.rim.len() >= 2);
        let normal = self.outward.normalize_or(Vec3::Y);
        let angular = (self.rim.len() - 1) * EDGE;
        let mut points = vec![self.center];
        let index = |r: usize, j: usize| 1 + (r - 1) * (angular + 1) + j;
        for r in 1..=RADIAL {
            let reach = r as f32 / RADIAL as f32;
            for j in 0..=angular {
                let panel = (j / EDGE).min(self.rim.len() - 2);
                let t = (j - panel * EDGE) as f32 / EDGE as f32;
                let bay = (PI * t).sin().max(0.0);
                let edge = self.rim[panel]
                    .lerp(self.rim[panel + 1], t)
                    .lerp(self.center, self.scallop * bay);
                let bow = 4.0 * reach * (1.0 - reach) * (self.support_bow + self.camber * bay);
                points.push(self.center.lerp(edge, reach) + normal * bow);
            }
        }
        let mut triangles = Vec::new();
        for j in 0..angular {
            triangles.push([0, index(1, j), index(1, j + 1)]);
        }
        for r in 1..RADIAL {
            for j in 0..angular {
                triangles.push([index(r, j), index(r + 1, j), index(r + 1, j + 1)]);
                triangles.push([index(r, j), index(r + 1, j + 1), index(r, j + 1)]);
            }
        }
        let first = triangles[0].map(|i| points[i]);
        let reverse = (first[1] - first[0]).cross(first[2] - first[0]).dot(normal) < 0.0;
        if reverse {
            for tri in &mut triangles {
                tri.swap(1, 2);
            }
        }
        let mut boundary = vec![0];
        boundary.extend((1..=RADIAL).map(|r| index(r, 0)));
        boundary.extend((1..=angular).map(|j| index(RADIAL, j)));
        boundary.extend((1..RADIAL).rev().map(|r| index(r, angular)));
        if reverse {
            boundary.reverse();
        }
        Surface {
            points,
            triangles,
            boundary,
            normal,
        }
    }

    pub(super) fn emit(&self, b: &mut Builder, color: Vec3) {
        let surface = self.surface();
        let half = surface.normal * self.thickness * 0.5;
        let mut normals = vec![Vec3::ZERO; surface.points.len()];
        for tri in &surface.triangles {
            let p = tri.map(|i| surface.points[i]);
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            for i in tri {
                normals[*i] += n;
            }
        }
        for n in &mut normals {
            *n = n.normalize_or(surface.normal);
        }
        for tri in &surface.triangles {
            let p = tri.map(|i| surface.points[i] + half);
            let n = tri.map(|i| normals[i]);
            b.tri_smooth(p, n, [color; 3]);
            let back = [tri[0], tri[2], tri[1]];
            b.tri_smooth(
                back.map(|i| surface.points[i] - half),
                back.map(|i| -normals[i]),
                [color * 0.82; 3],
            );
        }
        for k in 0..surface.boundary.len() {
            let a = surface.points[surface.boundary[k]];
            let c = surface.points[surface.boundary[(k + 1) % surface.boundary.len()]];
            b.tri([a + half, a - half, c - half], color * 0.76);
            b.tri([a + half, c - half, c + half], color * 0.76);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membrane_fan_keeps_support_anchors_and_curves_the_free_edge() {
        let rim = [
            Vec3::new(0.6, 0.0, 0.1),
            Vec3::new(0.5, 0.0, -0.25),
            Vec3::new(0.2, 0.0, -0.4),
        ];
        let fan = MembraneFan {
            center: Vec3::ZERO,
            rim: &rim,
            outward: Vec3::Y,
            scallop: 0.16,
            camber: -0.018,
            support_bow: 0.01,
            thickness: 0.006,
        };
        let surface = fan.surface();
        let stride = (rim.len() - 1) * EDGE + 1;
        let outside = 1 + (RADIAL - 1) * stride;
        for (k, anchor) in rim.iter().enumerate() {
            assert!(
                surface.points[outside + k * EDGE].distance(*anchor) < 1e-6,
                "finger anchor moved"
            );
        }
        let middle = surface.points[outside + EDGE / 2];
        assert!(
            middle.length() < rim[0].lerp(rim[1], 0.5).length() - 0.04,
            "free edge should scallop inward"
        );
        assert!(
            surface.points.iter().any(|p| p.y < -0.005),
            "skin has shallow tension between bowed supports"
        );
    }

    #[test]
    fn membrane_fan_mirrors_geometry_while_preserving_outer_facing() {
        let rim = [
            Vec3::new(0.02, 0.10, -0.46),
            Vec3::new(0.07, -0.14, -0.60),
            Vec3::new(0.13, -0.32, -0.50),
            Vec3::new(0.17, -0.42, -0.26),
        ];
        let make = |rim: &[Vec3], normal| {
            MembraneFan {
                center: Vec3::ZERO,
                rim,
                outward: normal,
                scallop: 0.16,
                camber: -0.018,
                support_bow: 0.01,
                thickness: 0.006,
            }
            .surface()
        };
        let right = make(&rim, Vec3::X);
        let mirrored = rim.map(|p| p * Vec3::new(-1.0, 1.0, 1.0));
        let left = make(&mirrored, Vec3::NEG_X);
        assert_eq!(right.points.len(), left.points.len());
        for (a, b) in right.points.iter().zip(&left.points) {
            assert!((*a * Vec3::new(-1.0, 1.0, 1.0)).distance(*b) < 1e-6);
        }
        for surface in [right, left] {
            for tri in surface.triangles {
                let p = tri.map(|i| surface.points[i]);
                assert!(
                    (p[1] - p[0]).cross(p[2] - p[0]).dot(surface.normal) > 0.0,
                    "outer skin normal reversed"
                );
            }
        }
    }
}
