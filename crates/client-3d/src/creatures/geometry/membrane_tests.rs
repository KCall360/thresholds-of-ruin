use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::BTreeMap;

#[test]
fn membrane_emission_has_closed_thickness_on_both_wing_sides() {
    for side in [-1.0, 1.0] {
        let mut parts = Parts::default();
        rogue::membrane(
            &mut parts.matte,
            Vec3::ZERO,
            &[
                Vec3::new(side * 0.6, 0.0, 0.1),
                Vec3::new(side * 0.5, 0.0, -0.25),
                Vec3::new(side * 0.2, 0.0, -0.4),
            ],
            Vec3::Y,
            Vec3::new(0.4, 0.1, 0.3),
        );
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let material = crate::props::PropMaterials::new(&mut materials);
        let _model = parts.finish(&mut meshes, &material);
        let mut edges = BTreeMap::new();
        let mut volume = 0.0;
        for (_, mesh) in meshes.iter() {
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions")
            };
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            for tri in indices.as_chunks::<3>().0 {
                let p = tri.map(|i| Vec3::from(points[i]));
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
        }
        assert!(
            volume > 0.0001,
            "membrane must have actual enclosed thickness: {volume}"
        );
        assert!(
            edges.values().all(|v| *v == (2, 0)),
            "one closed volume, without doubled coplanar faces or internal walls"
        );
    }
}
