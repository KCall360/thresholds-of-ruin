//! Boot silhouette contracts measure the emitted triangle surface, including swaps.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

fn section_floor(world: &World, meshes: &Assets<Mesh>, joint: Entity, z: f32) -> f32 {
    let mut cuts = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let at = world.get::<Transform>(child).unwrap().compute_affine();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for tri in indices.as_chunks::<3>().0 {
            let p = tri.map(|i| at.transform_point3(points[i].into()));
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let da = p[a].z - z;
                let db = p[b].z - z;
                if da.abs() < 1e-6 {
                    cuts.push(p[a].y);
                }
                if da * db < 0.0 {
                    cuts.push(p[a].lerp(p[b], da / (da - db)).y);
                }
            }
        }
    }
    assert!(!cuts.is_empty());
    cuts.into_iter().fold(f32::INFINITY, f32::min)
}

#[test]
fn constructed_boots_have_a_raised_arch_only_when_the_outfit_has_a_heel() {
    for (model, wardrobe) in [
        (CreatureModel::Vampire, Wardrobe::Native),
        (CreatureModel::Medusa, Wardrobe::Native),
        (CreatureModel::Orc, Wardrobe::Ranger),
        (CreatureModel::Vampire, Wardrobe::Courtier),
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                )
                .dressed(wardrobe);
                let Feet::Boots(boots) = Outfit::worn(&look).feet else {
                    panic!("boot fixture")
                };
                let (world, rig, _, meshes) = spawn_one(&look);
                let body = rig.humanoid.unwrap();
                for foot in [J::FootL, J::FootR] {
                    let arch = section_floor(
                        &world,
                        &meshes,
                        rig.joint(foot).unwrap(),
                        0.03 * body.build * super::super::anatomy::FOOT_SCALE.z,
                    );
                    if boots.heel {
                        assert!(arch> -0.054,"{model:?} {wardrobe:?} woman {woman} build {build}: arch filled to {arch}");
                    } else {
                        assert!(arch < -0.073, "flat boots keep a planted midsole: {arch}");
                    }
                }
            }
        }
    }
}
