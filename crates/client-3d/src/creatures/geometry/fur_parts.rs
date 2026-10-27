//! Continuous underfur and staggered tapered locks, shared by caps and cuffs.
use super::super::outfit::Fringe;
use super::*;

fn variation(index: usize) -> f32 {
    ((index as f32 * 2.399_963).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// A closed, narrow lock with a soft root and a pointed, lighter tip.
fn lock(b: &mut Builder, root: Vec3, direction: Vec3, size: Vec2, colors: [Vec3; 2]) {
    let direction = direction.normalize_or(Vec3::NEG_Y);
    let across = direction.cross(Vec3::Y).normalize_or(Vec3::X);
    let normal = across.cross(direction).normalize();
    let ring = |at: Vec3, width: f32| {
        std::array::from_fn::<_, 5, _>(|i| {
            let angle = i as f32 * TAU / 5.0;
            at + across * angle.cos() * width + normal * angle.sin() * width * 0.45
        })
    };
    let low = ring(root, size.y);
    let middle = ring(
        root + direction * size.x * 0.42 + normal * size.y * 0.35,
        size.y * 0.8,
    );
    let tip = root + direction * size.x;
    for i in 0..5 {
        let next = (i + 1) % 5;
        b.tri([root, low[i], low[next]], colors[0]);
        b.tri([low[i], middle[next], low[next]], colors[0]);
        b.tri([low[i], middle[i], middle[next]], colors[0]);
        b.tri([middle[i], tip, middle[next]], colors[1]);
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
}
