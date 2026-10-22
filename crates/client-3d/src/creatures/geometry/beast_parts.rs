//! Small anatomical construction pieces, independent of species selection.
//! Recipes choose proportions and placement; these pieces supply real volume.
use super::*;

/// A fleshy pointed leaf with a raised middle, rolled rim and curved spine.
/// Root rosettes and stalk growths choose their own proportions and attitude.
pub(super) struct LeafBlade {
    pub root: Vec3,
    pub direction: Vec3,
    pub length: f32,
    pub half_width: f32,
    pub arch: f32,
    pub droop: f32,
    pub color: Vec3,
    pub edge: Vec3,
}

impl LeafBlade {
    fn faces(&self) -> Vec<([Vec3; 3], Vec3)> {
        let dir = self.direction.normalize_or(Vec3::Z);
        let across = Vec3::Y.cross(dir).normalize_or(Vec3::X);
        let normal = dir.cross(across);
        let spine = |t: f32| {
            self.root
                + dir * self.length * t
                + normal * self.length * (self.arch * (PI * t).sin() - self.droop * t * t)
        };
        const PROFILE: [(f32, f32); 7] = [
            (0.0, 0.06),
            (0.12, 0.45),
            (0.32, 1.0),
            (0.60, 0.94),
            (0.80, 0.62),
            (0.94, 0.18),
            (1.0, 0.0),
        ];
        const SECTION: [(f32, f32); 10] = [
            (-1.0, 0.0),
            (-0.86, 0.22),
            (-0.07, 1.0),
            (0.07, 1.0),
            (0.86, 0.22),
            (1.0, 0.0),
            (0.86, -0.22),
            (0.07, -0.7),
            (-0.07, -0.7),
            (-0.86, -0.22),
        ];
        const ROWS: usize = 12;
        let rings: Vec<_> = (0..=ROWS)
            .map(|i| {
                let t = i as f32 / ROWS as f32;
                let span = PROFILE.windows(2).find(|s| t <= s[1].0).unwrap();
                let f = ease((t - span[0].0) / (span[1].0 - span[0].0));
                let w = self.half_width * (span[0].1 + (span[1].1 - span[0].1) * f);
                SECTION.map(|(x, y)| spine(t) + across * w * x + normal * w * 0.14 * y)
            })
            .collect();
        let mut faces = Vec::new();
        let mut add = |mut face: [Vec3; 3], inside: Vec3, color: Vec3| {
            let n = (face[1] - face[0]).cross(face[2] - face[0]);
            if n.length_squared() < 1e-20 {
                return;
            }
            if n.dot((face[0] + face[1] + face[2]) / 3.0 - inside) < 0.0 {
                face.swap(1, 2);
            }
            faces.push((face, color));
        };
        for row in 0..ROWS {
            let t = (row as f32 + 0.5) / ROWS as f32;
            for col in 0..10 {
                let next = (col + 1) % 10;
                let color = if matches!(col, 0 | 4 | 5 | 9) {
                    self.edge
                } else if col == 2 {
                    self.color.lerp(BONE, 0.14)
                } else {
                    self.color * if col > 5 { 0.82 } else { 1.0 }
                };
                let q = [
                    rings[row][col],
                    rings[row + 1][col],
                    rings[row + 1][next],
                    rings[row][next],
                ];
                add([q[0], q[1], q[2]], spine(t), color);
                add([q[0], q[2], q[3]], spine(t), color);
            }
        }
        for col in 0..10 {
            add(
                [self.root, rings[0][col], rings[0][(col + 1) % 10]],
                spine(0.01),
                self.color,
            );
        }
        faces
    }

    pub fn emit(&self, mesh: &mut Builder) {
        for (face, color) in self.faces() {
            mesh.tri(face, color);
        }
    }
}

/// An overlapping scute with a broad crown, bevelled perimeter and closed back.
/// `forward` lies along the surface; `normal` points out of the supporting body.
pub(super) struct ShellPlate {
    pub center: Vec3,
    pub normal: Vec3,
    pub forward: Vec3,
    pub half_size: Vec2,
    pub rise: f32,
    pub thickness: f32,
}

impl ShellPlate {
    /// Seat a scute on a convex body. Its tangent plane keeps the supporting
    /// body below the plate instead of cutting holes through its visible face.
    pub fn on_ellipsoid(
        center: Vec3,
        radii: Vec3,
        direction: Vec3,
        half_size: Vec2,
        rise: f32,
        thickness: f32,
    ) -> Self {
        let unit = direction.normalize_or(Vec3::Y);
        let normal = (unit / radii).normalize();
        Self {
            center: center + unit * radii + normal * 0.002,
            normal,
            forward: Vec3::Z,
            half_size,
            rise,
            thickness,
        }
    }

    fn faces(&self) -> Vec<[Vec3; 3]> {
        let normal = self.normal.normalize_or(Vec3::Y);
        let forward = (self.forward - normal * self.forward.dot(normal))
            .normalize_or(crate::props::basis(normal).0);
        let across = forward.cross(normal).normalize_or(Vec3::X);
        let outline = [
            Vec2::new(0.0, 1.0),
            Vec2::new(0.75, 0.62),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.7, -0.65),
            Vec2::new(0.0, -1.0),
            Vec2::new(-0.7, -0.65),
            Vec2::new(-1.0, 0.0),
            Vec2::new(-0.75, 0.62),
        ];
        let ring = outline.map(|v| {
            self.center + across * v.x * self.half_size.x + forward * v.y * self.half_size.y
        });
        let crown = self.center + normal * self.rise;
        let back = self.center - normal * self.thickness;
        let inside = self.center + normal * (self.rise - self.thickness) * 0.25;
        let mut faces = Vec::with_capacity(32);
        for i in 0..8 {
            let j = (i + 1) % 8;
            let a = ring[i];
            let b = ring[j];
            let c = b - normal * self.thickness;
            let d = a - normal * self.thickness;
            for mut triangle in [[crown, a, b], [back, c, d], [a, d, c], [a, c, b]] {
                let outward = (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]);
                if outward.dot((triangle[0] + triangle[1] + triangle[2]) / 3.0 - inside) < 0.0 {
                    triangle.swap(1, 2);
                }
                faces.push(triangle);
            }
        }
        faces
    }

    pub fn emit(&self, mesh: &mut Builder, color: Vec3) {
        for face in self.faces() {
            mesh.tri(face, color);
        }
    }
}

/// Teeth rooted along an elliptical lip, in a freely oriented mouth frame.
/// Growth is authored independently of the lip so upper/lower jaws share it.
pub(super) struct FangArc {
    pub frame: Transform,
    pub half_width: f32,
    pub rise: f32,
    pub depth: f32,
    pub growth: Vec3,
    pub length: f32,
    pub radius: f32,
    pub count: usize,
}

impl FangArc {
    pub fn emit(&self, mesh: &mut Builder, color: Vec3) {
        self.emit_offset(mesh, color, 0.0);
    }

    /// Stagger opposing lip rows without changing their authored mouth frame.
    pub fn emit_offset(&self, mesh: &mut Builder, color: Vec3, phase: f32) {
        let growth = self.growth.normalize_or(Vec3::NEG_Y);
        let (right, up) = crate::props::basis(growth);
        for i in 0..self.count {
            let a = (i as f32 / (self.count - 1).max(1) as f32 - 0.5) * 2.6 + phase;
            let root = Vec3::new(
                a.sin() * self.half_width,
                a.cos() * self.rise,
                a.cos() * self.depth,
            );
            let length = self.length * (0.82 + 0.18 * a.cos());
            let tip = root + growth * length;
            let ring: [Vec3; 5] = std::array::from_fn(|j| {
                let a = j as f32 / 5.0 * TAU;
                root + (right * a.cos() + up * a.sin()) * self.radius
            });
            let inside = self.frame.transform_point(root + growth * length * 0.25);
            for j in 0..5 {
                for face in [
                    [root, ring[j], ring[(j + 1) % 5]],
                    [ring[j], tip, ring[(j + 1) % 5]],
                ] {
                    mesh.tri_out(face.map(|v| self.frame.transform_point(v)), inside, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn leaf_blades_remain_closed_thick_solids_at_small_and_large_scales() {
        for scale in [0.4, 1.0, 2.0] {
            let blade = LeafBlade {
                root: Vec3::ZERO,
                direction: Vec3::new(0.1, 0.25, 1.0),
                length: 0.48 * scale,
                half_width: 0.16 * scale,
                arch: 0.20,
                droop: 0.14,
                color: Vec3::new(0.3, 0.6, 0.2),
                edge: Vec3::new(0.5, 0.2, 0.1),
            };
            let mut edges = BTreeMap::new();
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            for (tri, _) in blade.faces() {
                let ids = tri.map(|p| {
                    p.to_array()
                        .map(|v| (v / scale * 1_000_000.0).round() as i32)
                });
                for p in tri {
                    assert!(p.is_finite());
                    low = low.min(p / scale);
                    high = high.max(p / scale);
                }
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let edge = if ids[a] < ids[b] {
                        (ids[a], ids[b])
                    } else {
                        (ids[b], ids[a])
                    };
                    *edges.entry(edge).or_insert(0) += 1;
                }
            }
            assert!(
                edges.values().all(|n| *n == 2),
                "leaf must have closed edges and inserted root"
            );
            let size = high - low;
            assert!(
                size.x > 0.30 && size.y > 0.11 && size.z > 0.44,
                "leaf needs width, arch and length: {size:?}"
            );
        }
    }

    #[test]
    fn scute_tangent_planes_clear_the_body_on_crown_flanks_and_endcaps() {
        let center = Vec3::new(0.0, 0.08, -0.02);
        let radii = Vec3::new(0.34, 0.2, 0.4);
        for direction in [
            Vec3::Y,
            Vec3::new(0.7, 0.6, 0.4),
            Vec3::new(-0.7, 0.6, -0.4),
            Vec3::Z,
            Vec3::NEG_Z,
        ] {
            let plate = ShellPlate::on_ellipsoid(
                center,
                radii,
                direction,
                Vec2::new(0.15, 0.18),
                0.035,
                0.018,
            );
            let surface = center + direction.normalize() * radii;
            assert!(
                (plate.center - surface).length() < plate.thickness,
                "back roots into its support"
            );
            for face in plate.faces() {
                for p in face {
                    assert!(p.is_finite());
                    if (p - plate.center).dot(plate.normal) >= -1e-6 {
                        assert!(
                            ((p - center) / radii).length() > 1.0,
                            "the body cannot protrude through a scute face"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fang_rows_scale_in_the_mouth_frame_and_point_into_both_jaws() {
        for growth in [Vec3::Y, Vec3::NEG_Y] {
            let mut reference = None;
            for scale in [0.5, 1.0, 2.0] {
                let offset = Vec3::new(0.3, -0.2, 0.8);
                let mut mesh = Builder::default();
                FangArc {
                    frame: Transform::from_translation(offset).with_scale(Vec3::splat(scale)),
                    half_width: 0.2,
                    rise: 0.1,
                    depth: 0.05,
                    growth,
                    length: 0.08,
                    radius: 0.012,
                    count: 9,
                }
                .emit(&mut mesh, BONE);
                let (low, high) = mesh.bounds();
                let normalized = ((low - offset) / scale, (high - offset) / scale);
                if let Some((a, b)) = reference {
                    assert!(normalized.0.distance(a) < 1e-6 && normalized.1.distance(b) < 1e-6);
                } else {
                    reference = Some(normalized);
                }
                assert!(normalized.1.x - normalized.0.x > 0.38);
                assert!(normalized.1.y - normalized.0.y > 0.1);
                if growth.y > 0.0 {
                    assert!(normalized.1.y > 0.17);
                } else {
                    assert!(normalized.0.y < -0.035);
                }
            }
        }
    }

    #[test]
    fn shell_plates_are_closed_outward_solids_on_rotated_small_and_large_bodies() {
        for scale in [0.4, 1.0, 2.5] {
            for rotation in [
                Quat::IDENTITY,
                Quat::from_rotation_z(0.8),
                Quat::from_rotation_x(-0.6),
            ] {
                let center = Vec3::new(-0.3, 0.4, 0.7);
                let normal = rotation * Vec3::Y;
                let plate = ShellPlate {
                    center,
                    normal,
                    forward: rotation * Vec3::Z,
                    half_size: Vec2::new(0.13, 0.2) * scale,
                    rise: 0.04 * scale,
                    thickness: 0.016 * scale,
                };
                let mut edges = BTreeMap::new();
                let mut min_height = f32::INFINITY;
                let mut max_height = f32::NEG_INFINITY;
                for triangle in plate.faces() {
                    let n = (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]);
                    assert!(n.length() > 1e-8);
                    let inside = center + normal * (plate.rise - plate.thickness) * 0.25;
                    assert!(n.dot((triangle[0] + triangle[1] + triangle[2]) / 3.0 - inside) > 0.0);
                    let ids =
                        triangle.map(|p| p.to_array().map(|x| (x * 1_000_000.0).round() as i32));
                    for p in triangle {
                        let h = (p - center).dot(normal) / scale;
                        min_height = min_height.min(h);
                        max_height = max_height.max(h);
                    }
                    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                        let pair = if ids[a] < ids[b] {
                            (ids[a], ids[b])
                        } else {
                            (ids[b], ids[a])
                        };
                        *edges.entry(pair).or_insert(0) += 1;
                    }
                }
                assert!(
                    edges.values().all(|n| *n == 2),
                    "no open or duplicate edges"
                );
                assert!((min_height + 0.016).abs() < 1e-6);
                assert!((max_height - 0.04).abs() < 1e-6);
            }
        }
    }
}
