//! Aquator's broad amphibian masses must be sculpted, not painted onto a barrel.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

fn direct_points(world: &World, meshes: &Assets<Mesh>, joint: Entity) -> Vec<Vec3> {
    let mut points = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let local = world.get::<Transform>(child).unwrap().compute_affine();
        points.extend(vertices.iter().map(|p| local.transform_point3((*p).into())));
    }
    assert!(!points.is_empty() && points.iter().all(|p| p.is_finite()));
    points
}

#[test]
fn aquator_has_shoulder_and_haunch_masses_around_a_narrower_midriff() {
    let mut failures = Vec::new();
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Aquator,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        let points = direct_points(&world, &meshes, rig.joint(J::Body).unwrap());
        let breadth = |low, high| {
            let mut left = f32::INFINITY;
            let mut right = f32::NEG_INFINITY;
            for p in &points {
                if p.z > low && p.z < high {
                    left = left.min(p.x);
                    right = right.max(p.x);
                }
            }
            assert!(left.is_finite() && right.is_finite());
            right - left
        };
        let hip = breadth(-0.30, -0.16);
        let shoulder = breadth(0.13, 0.25);
        let waist = breadth(-0.035, 0.035);
        if hip < waist * 1.15 || shoulder < waist * 1.02 || hip < 0.74 {
            failures.push(format!(
                "build={build}: hip/shoulder/waist breadth={hip:.4}/{shoulder:.4}/{waist:.4}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn aquator_has_a_broad_deep_skull_around_its_eye_and_tendril_roots() {
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Aquator,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        let points = direct_points(&world, &meshes, rig.joint(J::Head).unwrap());
        let (low, high) = points.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(a, b), p| (a.min(*p), b.max(*p)),
        );
        let size = high - low;
        assert!(
            size.x > 0.62 && size.y > 0.32 && size.z > 0.36,
            "build={build}: skull, excluding lower jaw and tendrils, is a shallow pad: {size:?}"
        );
        // The cheek roots have to occupy the same connected head silhouette.
        for side in [-1.0, 1.0] {
            let count = points
                .iter()
                .filter(|p| {
                    p.x * side > 0.17 && p.x * side < 0.25 && p.y.abs() < 0.08 && p.z > 0.07
                })
                .count();
            assert!(
                count > 20,
                "build={build} side={side}: missing cheek volume around tendril root"
            );
        }
    }
}
