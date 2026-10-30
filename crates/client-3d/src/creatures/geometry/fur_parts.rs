//! Continuous underfur and staggered closed locks for body coats, caps and cuffs.
use super::super::outfit::Fringe;
use super::*;

fn variation(index: usize) -> f32 {
    ((index as f32 * 2.399_963).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// Two closed cross-sections and a pointed tip. Frames follow the surface,
/// rather than choosing thickness from a fixed world axis.
struct Lock {
    path: [Vec3; 3],
    normal: Vec3,
    widths: Vec2,
    depth: f32,
    sides: usize,
    colors: [Vec3; 2],
}
impl Lock {
    fn emit(&self, b: &mut Builder) {
        let direction = (self.path[2] - self.path[0]).normalize();
        let across = direction.cross(self.normal).normalize();
        let normal = across.cross(direction);
        let ring = |at: Vec3, width: f32| {
            std::array::from_fn::<_, 8, _>(|i| {
                if i >= self.sides {
                    return Vec3::ZERO;
                }
                let angle = i as f32 * TAU / self.sides as f32;
                at + across * angle.cos() * width + normal * angle.sin() * width * self.depth
            })
        };
        let [root, middle, tip] = self.path;
        let low = ring(root, self.widths.x);
        let middle = ring(middle, self.widths.y);
        for i in 0..self.sides {
            let next = (i + 1) % self.sides;
            b.tri([root, low[i], low[next]], self.colors[0]);
            b.tri([low[i], middle[next], low[next]], self.colors[0]);
            b.tri([low[i], middle[i], middle[next]], self.colors[0]);
            b.tri([middle[i], tip, middle[next]], self.colors[1]);
        }
    }
}

/// Existing trim locks retain their original frame and silhouette.
fn lock(b: &mut Builder, root: Vec3, direction: Vec3, size: Vec2, colors: [Vec3; 2]) {
    let direction = direction.normalize_or(Vec3::NEG_Y);
    let across = direction.cross(Vec3::Y).normalize_or(Vec3::X);
    let normal = across.cross(direction).normalize();
    Lock {
        path: [
            root,
            root + direction * size.x * 0.42 + normal * size.y * 0.35,
            root + direction * size.x,
        ],
        normal,
        widths: Vec2::new(size.y, size.y * 0.8),
        depth: 0.45,
        sides: 5,
        colors,
    }
    .emit(b);
}

/// A continuous undercoat with overlapping, combed surface locks. Dimensions
/// belong to the body recipe; density follows lock length, with bounded counts.
/// This component can cover a torso, limb or other ellipsoidal body mass.
#[derive(Clone, Copy)]
pub(super) struct Coat {
    pub center: Vec3,
    pub radii: Vec3,
    pub length: f32,
    pub flow: Vec3,
    pub color: Vec3,
    pub seed: u64,
}
impl Coat {
    /// Joint-local limbs hang along -Y. Resolve their coat from the actual
    /// attachment reach, with a small overlap beyond each end for bent poses.
    pub(super) fn hanging_limb(
        reach: f32,
        girth: Vec2,
        length: f32,
        color: Vec3,
        seed: u64,
    ) -> Self {
        Self {
            center: Vec3::NEG_Y * (reach * 0.5),
            radii: Vec3::new(girth.x, reach * 0.58, girth.y),
            length,
            flow: Vec3::NEG_Y,
            color,
            seed,
        }
    }
    fn locks(&self) -> impl Iterator<Item = Lock> + '_ {
        let rows = (self.radii.y * 2.0 / (self.length * 0.85))
            .ceil()
            .clamp(2.0, 16.0) as u32;
        let girth = TAU * ((self.radii.x.powi(2) + self.radii.z.powi(2)) * 0.5).sqrt();
        (0..rows).flat_map(move |row| {
            let latitude = (row as f32 + 0.5) * PI / rows as f32;
            let count = (girth * latitude.sin() / (self.length * 0.62))
                .ceil()
                .clamp(4.0, 48.0) as u32;
            (0..count).map(move |column| {
                let noise = rand(self.seed, row * 64 + column);
                let latitude = latitude
                    + (rand(self.seed, row * 64 + column + 2048) - 0.5) * PI / rows as f32 * 0.6;
                let angle = (column as f32 + row as f32 * 0.43 + noise * 0.24) * TAU / count as f32;
                let unit = Vec3::new(
                    latitude.sin() * angle.cos(),
                    latitude.cos(),
                    latitude.sin() * angle.sin(),
                );
                let normal = (unit / self.radii).normalize();
                let tangent = (self.flow - normal * self.flow.dot(normal))
                    .normalize_or(normal.any_orthonormal_vector());
                let length = self.length * (0.88 + noise * 0.24);
                // Project the crest and tip back to the body. Fur follows its
                // curve instead of forming straight spikes at each latitude.
                let at = |distance: f32, lift: f32| {
                    let destination = (unit + tangent * distance / self.radii).normalize();
                    let destination_normal = (destination / self.radii).normalize();
                    self.center + destination * self.radii + destination_normal * lift
                };
                let width = length * (0.30 + noise * 0.06);
                let loft = (width * 0.45).min(self.radii.min_element() * 0.09);
                Lock {
                    path: [
                        self.center + unit * self.radii * 0.94,
                        at(length * 0.43, loft * 0.35),
                        at(length, loft),
                    ],
                    normal,
                    widths: Vec2::new(width * 0.58, width),
                    depth: (loft * 0.6 / width).min(0.22),
                    sides: 8,
                    colors: [
                        self.color * (0.84 + noise * 0.12),
                        self.color * (0.96 + noise * 0.06),
                    ],
                }
            })
        })
    }

    pub(super) fn emit(self, p: &mut Parts) {
        assert!(
            self.radii.is_finite()
                && self.radii.min_element() > 0.0
                && self.center.is_finite()
                && self.length.is_finite()
                && self.length > 0.0
                && self.flow.is_finite()
                && self.flow.length_squared() > 0.0,
            "fur coat needs finite dimensions and a nonzero comb direction"
        );
        // The denser undercoat keeps embedded roots inside the faceted surface.
        // Restore the caller's resolution: neighboring pieces own their detail.
        let resolution = p.matte.res;
        p.matte.res = (12, 24);
        p.matte
            .blob(self.center, self.radii, self.seed, 0.0, self.color * 0.88);
        p.matte.res = resolution;
        for lock in self.locks() {
            lock.emit(&mut p.matte);
        }
    }
}

pub(super) fn rim(p: &mut Parts, fringe: Fringe, center: Vec3, radii: Vec2, scale: f32) {
    // Dense underfur hides the roots without filling a cuff's opening.
    let path: Vec<_> = (0..=48)
        .map(|i| {
            let a = i as f32 * TAU / 48.0;
            center + Vec3::new(a.sin() * radii.x, -0.012 * scale, a.cos() * radii.y)
        })
        .collect();
    tube(
        &mut p.matte,
        &path,
        &vec![(0.008 * scale, 0.012 * scale); path.len()],
        fringe.color * 0.9,
    );
    for row in 0..4 {
        for k in 0..64 {
            let sample = row * 64 + k;
            let noise = variation(sample);
            let angle = (k as f32 + row as f32 * 0.37 + noise * 0.28) * TAU / 64.0;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            let tangent = Vec3::new(out.z, 0.0, -out.x);
            let root = center
                + Vec3::new(
                    out.x * (radii.x + 0.006 * scale),
                    -(row as f32 * 0.018 + noise * 0.009) * scale,
                    out.z * (radii.y + 0.006 * scale),
                );
            let length = fringe.length * scale * (0.34 + noise * 0.48);
            let width = radii.min_element() * (0.048 + noise * 0.024);
            lock(
                &mut p.matte,
                root,
                out * 0.32 - Vec3::Y + tangent * (noise - 0.5) * 0.45,
                Vec2::new(length, width),
                [
                    fringe.color * (0.82 + noise * 0.18),
                    fringe.color.lerp(fringe.accent, 0.16 + noise * 0.24),
                ],
            );
        }
    }
}

pub(super) fn cap(p: &mut Parts, fringe: Fringe, center: Vec3, radii: Vec2, scale: f32) {
    let height = 0.035 * scale;
    p.matte.blob(
        center,
        Vec3::new(radii.x, height, radii.y),
        71,
        0.0,
        fringe.color * 0.88,
    );
    // Tufts grow from the dome and comb outward into the hanging edge.
    for row in 0..5 {
        let radial = (row as f32 + 0.2) / 5.0;
        let count = 8 + row * 12;
        for k in 0..count {
            let noise = variation(row * 61 + k);
            let a = (k as f32 + row as f32 * 0.43) * TAU / count as f32;
            let out = Vec3::new(a.sin(), 0.0, a.cos());
            let root = center
                + Vec3::new(
                    out.x * radii.x * radial,
                    height * (1.0 - radial * radial).sqrt(),
                    out.z * radii.y * radial,
                );
            lock(
                &mut p.matte,
                root,
                out - Vec3::Y * (0.15 + radial * 0.35),
                Vec2::new(
                    (0.025 + noise * 0.025) * scale,
                    (0.004 + noise * 0.003) * scale,
                ),
                [
                    fringe.color * (0.85 + noise * 0.15),
                    fringe.color.lerp(fringe.accent, 0.20 + noise * 0.25),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_coats_embed_roots_and_comb_tips_over_every_side() {
        for radii in [
            Vec3::new(0.3, 0.34, 0.24),
            Vec3::new(0.1, 0.16, 0.1),
            Vec3::new(0.06, 0.2, 0.08),
        ] {
            for flow in [
                Vec3::X,
                Vec3::Y,
                Vec3::Z,
                Vec3::NEG_X,
                Vec3::NEG_Y,
                Vec3::NEG_Z,
            ] {
                let coat = Coat {
                    center: Vec3::new(0.4, -0.2, 0.7),
                    radii,
                    length: 0.1,
                    flow,
                    color: Vec3::ONE,
                    seed: 17,
                };
                let locks: Vec<_> = coat.locks().collect();
                assert!(!locks.is_empty() && locks.len() <= 16 * 48);
                let mut sides = [false; 6];
                for lock in locks {
                    let root = (lock.path[0] - coat.center) / radii;
                    let tip = (lock.path[2] - coat.center) / radii;
                    assert!(
                        root.length() > 0.9 && root.length() < 1.0,
                        "roots must overlap the undercoat"
                    );
                    assert!(
                        tip.length() > 1.0 && tip.length() < 1.15,
                        "tips follow the curved surface instead of projecting spikes"
                    );
                    let normal = (root / radii).normalize();
                    let tangent = flow - normal * flow.dot(normal);
                    assert!(
                        (lock.path[2] - lock.path[0]).dot(tangent) > 0.0,
                        "fur follows the authored comb direction"
                    );
                    for (axis, sign) in [(0, root.x), (1, root.y), (2, root.z)] {
                        sides[axis * 2 + usize::from(sign > 0.0)] = true;
                    }
                    assert!(lock.path.iter().all(|p| p.is_finite()));
                }
                assert!(
                    sides.into_iter().all(|covered| covered),
                    "surface coverage includes back and flanks"
                );
            }
        }
    }

    #[test]
    fn hanging_coats_cover_both_ends_of_short_and_long_limbs() {
        for reach in [0.2, 0.3, 0.3875, 0.44] {
            let coat = Coat::hanging_limb(reach, Vec2::new(0.085, 0.08), 0.12, Vec3::ONE, 4);
            assert!(coat.center.y + coat.radii.y > 0.0);
            assert!(coat.center.y - coat.radii.y < -reach);
            let roots: Vec<_> = coat.locks().map(|lock| lock.path[0].y).collect();
            assert!(
                roots.iter().any(|y| *y > -reach * 0.1),
                "coat reaches the proximal joint"
            );
            assert!(
                roots.iter().any(|y| *y < -reach * 0.9),
                "coat reaches the distal joint"
            );
        }
    }

    #[test]
    fn resized_surface_coats_scale_the_complete_lock_path() {
        let coat = Coat {
            center: Vec3::new(0.1, 0.2, -0.3),
            radii: Vec3::new(0.1, 0.18, 0.08),
            length: 0.09,
            flow: Vec3::NEG_Y,
            color: Vec3::ONE,
            seed: 4,
        };
        let original: Vec<_> = coat.locks().collect();
        for scale in [0.5, 2.0] {
            let resized = Coat {
                center: coat.center * scale,
                radii: coat.radii * scale,
                length: coat.length * scale,
                ..coat
            };
            let resized: Vec<_> = resized.locks().collect();
            assert_eq!(
                original.len(),
                resized.len(),
                "resizing preserves detail density"
            );
            for (before, after) in original.iter().zip(resized) {
                for (index, (before, after)) in before.path.iter().zip(after.path).enumerate() {
                    assert!(
                        after.distance(*before * scale) < 1e-5,
                        "resized fur must scale the entire path: scale={scale} section={index}"
                    );
                }
                assert!(after.widths.distance(before.widths * scale) < 1e-5);
            }
        }
    }

    #[test]
    fn coat_emission_is_closed_bounded_and_restores_neighbor_detail() {
        let coat = Coat {
            center: Vec3::ZERO,
            radii: Vec3::new(0.1, 0.18, 0.08),
            length: 0.09,
            flow: Vec3::NEG_Y,
            color: Vec3::ONE,
            seed: 4,
        };
        let mut parts = Parts::default();
        let resolution = parts.matte.res;
        coat.emit(&mut parts);
        assert_eq!(parts.matte.res, resolution);
        assert_closed(parts);
    }

    #[test]
    #[should_panic(expected = "fur coat needs finite dimensions")]
    fn coat_rejects_zero_radius_before_emitting_geometry() {
        Coat {
            center: Vec3::ZERO,
            radii: Vec3::new(0.0, 0.2, 0.1),
            length: 0.1,
            flow: Vec3::NEG_Y,
            color: Vec3::ONE,
            seed: 0,
        }
        .emit(&mut Parts::default());
    }

    #[test]
    fn fur_locks_are_closed_with_outward_facing_triangles() {
        for direction in [Vec3::NEG_Y, Vec3::new(1.0, -0.3, 0.4)] {
            let mut parts = Parts::default();
            lock(
                &mut parts.matte,
                Vec3::ZERO,
                direction,
                Vec2::new(0.08, 0.008),
                [Vec3::ONE; 2],
            );
            assert_closed(parts);
        }
    }
    fn assert_closed(parts: Parts) {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        let mut volume = 0.0;
        let mut edges = std::collections::HashMap::new();
        for (handle, _) in &model.0 {
            let mesh = meshes.get(handle).unwrap();
            let bevy::render::mesh::VertexAttributeValues::Float32x3(positions) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!("positions")
            };
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            for triangle in indices.as_chunks::<3>().0 {
                let points = triangle.map(|i| Vec3::from(positions[i]));
                volume += points[0].dot(points[1].cross(points[2])) / 6.0;
                let keys = points.map(|p| p.to_array().map(|v| (v * 1e7).round() as i32));
                for i in 0..3 {
                    let a = keys[i];
                    let b = keys[(i + 1) % 3];
                    let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                    let entry = edges.entry(key).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += sign;
                }
            }
        }
        assert!(
            volume > 1e-7,
            "outward winding gives positive volume, got {volume}"
        );
        assert!(
            edges
                .values()
                .all(|&(count, balance)| count == 2 && balance == 0),
            "each closed edge needs two oppositely directed faces"
        );
    }
}
