//! Small shared equine forms, authored in their articulated joint frames.
use super::*;

/// A broad hoof in ankle-pivot-local coordinates, toe toward +Z. The shallow
/// sole bevel preserves a large planar contact patch at y = -0.078.
pub(super) fn hoof(b: &mut Builder, color: Vec3) {
    let outline = [
        Vec2::new(-0.055, -0.055),
        Vec2::new(0.055, -0.055),
        Vec2::new(0.085, -0.025),
        Vec2::new(0.085, 0.055),
        Vec2::new(0.060, 0.110),
        Vec2::new(-0.060, 0.110),
        Vec2::new(-0.085, 0.055),
        Vec2::new(-0.085, -0.025),
    ];
    let root = outline.map(|p| Vec3::new(p.x * 0.62, 0.015, p.y * 0.55 - 0.005));
    let wall = outline.map(|p| Vec3::new(p.x, -0.063, p.y));
    let sole = outline.map(|p| Vec3::new(p.x * 0.97, -0.078, p.y * 0.98));
    let inside = Vec3::new(0.0, -0.03, 0.02);
    for i in 0..outline.len() {
        let next = (i + 1) % outline.len();
        b.quad_out([root[i], wall[i], wall[next], root[next]], inside, color);
        b.quad_out(
            [wall[i], sole[i], sole[next], wall[next]],
            inside,
            color * 0.8,
        );
        b.tri_out(
            [Vec3::new(0.0, 0.015, 0.01), root[i], root[next]],
            inside,
            color,
        );
        b.tri_out(
            [Vec3::new(0.0, -0.078, 0.02), sole[i], sole[next]],
            inside,
            color * 0.58,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;
    use std::collections::BTreeMap;

    fn emitted() -> Vec<([Vec3; 3], [Vec3; 3])> {
        let mut parts = Parts::default();
        hoof(&mut parts.matte, Vec3::splat(0.3));
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let materials = crate::props::PropMaterials::new(&mut materials);
        let _model = parts.finish(&mut meshes, &materials);
        let mut faces = Vec::new();
        for (_, mesh) in meshes.iter() {
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions")
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("colors")
            };
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            for tri in indices.as_chunks::<3>().0 {
                faces.push((
                    tri.map(|i| points[i].into()),
                    tri.map(|i| Vec3::new(colors[i][0], colors[i][1], colors[i][2])),
                ));
            }
        }
        assert!(!faces.is_empty());
        faces
    }

    #[test]
    fn equine_hoof_has_closed_welded_topology_and_outward_winding() {
        let mut edges = BTreeMap::new();
        let mut volume = 0.0;
        for (p, _) in emitted() {
            assert!(p.iter().all(|v| v.is_finite()));
            assert!((p[1] - p[0]).cross(p[2] - p[0]).length() > 1e-7);
            volume += p[0].dot(p[1].cross(p[2])) / 6.0;
            let keys = p.map(|v| v.to_array().map(|x| (x * 1_000_000.0).round() as i32));
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (key, sign) = if keys[a] < keys[b] {
                    ((keys[a], keys[b]), 1)
                } else {
                    ((keys[b], keys[a]), -1)
                };
                let entry = edges.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += sign;
            }
        }
        assert!(
            edges.values().all(|v| *v == (2, 0)),
            "each welded edge needs two opposite incident faces"
        );
        assert!(
            volume > 0.0008 && volume < 0.003,
            "closed hoof volume={volume}"
        );
    }

    #[test]
    fn equine_hoof_has_a_substantial_planar_dark_sole() {
        let mut area = 0.0;
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for (p, colors) in emitted() {
            assert!(
                p.iter().all(|v| v.y >= -0.078001),
                "geometry below sole plane"
            );
            if !p.iter().all(|v| (v.y + 0.078).abs() < 1e-6) {
                continue;
            }
            let normal = (p[1] - p[0]).cross(p[2] - p[0]);
            assert!(normal.y < 0.0, "sole must face down");
            area += normal.length() * 0.5;
            assert!(
                colors.iter().all(|c| c.max_element() < 0.04),
                "sole must be darker than hoof wall"
            );
            for v in p {
                low = low.min(v);
                high = high.max(v);
            }
        }
        assert!(area > 0.018, "contact area={area}");
        assert!(
            high.x - low.x > 0.15 && high.z - low.z > 0.15,
            "sole bounds={low:?}..{high:?}"
        );
    }

    #[test]
    fn equine_hoof_has_a_sloped_toe_and_distinct_heel() {
        let points: Vec<_> = emitted().into_iter().flat_map(|(p, _)| p).collect();
        let root: Vec<_> = points.iter().filter(|p| p.y > 0.014).collect();
        let bottom: Vec<_> = points.iter().filter(|p| p.y < -0.06).collect();
        assert!(!root.is_empty() && !bottom.is_empty());
        let root_toe = root.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max);
        let toe = bottom.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max);
        let heel = bottom.iter().map(|p| p.z).fold(f32::INFINITY, f32::min);
        assert!(
            toe > 0.10 && toe - root_toe > 0.045,
            "toe/root={toe}/{root_toe}"
        );
        assert!(
            heel < -0.05 && toe > -heel * 1.6,
            "toe/heel silhouette={toe}/{heel}"
        );
    }
}
