//! Compact Dragon anatomy and correlated articulated head geometry.
use super::*;
use crate::creatures::rogue_tests::{animate_look, spawn_one};
use bevy::render::mesh::VertexAttributeValues;

fn direct(world: &World, meshes: &Assets<Mesh>, joint: Entity) -> Vec<Vec3> {
    let mut out = Vec::new();
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
        out.extend(points.iter().map(|p| at.transform_point3((*p).into())));
    }
    assert!(!out.is_empty());
    out
}

#[test]
fn dragon_short_bent_supports_have_broad_grounded_claw_feet() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Dragon,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (world, rig, _, meshes) = spawn_one(&look);
            let upper = rig.rest[J::FLs as usize];
            let lower = rig.rest[J::HoofFL as usize];
            assert!(
                (upper.length() + lower.length()) / rig.rest[J::Body as usize].y < 0.72,
                "compact fore support lengths"
            );
            assert!(
                upper.z < -0.03 && rig.rest[J::BLs as usize].z > 0.08,
                "distinct elbow and hind knee/hock silhouette"
            );
            for (root, knee, foot) in [
                (J::FL, J::FLs, J::HoofFL),
                (J::FR, J::FRs, J::HoofFR),
                (J::BL, J::BLs, J::HoofBL),
                (J::BR, J::BRs, J::HoofBR),
            ] {
                let ankle = rig.rest[J::Body as usize]
                    + rig.rest[root as usize]
                    + rig.rest[knee as usize]
                    + rig.rest[foot as usize];
                let points = direct(&world, &meshes, rig.joint(foot).unwrap());
                let low = points
                    .iter()
                    .map(|p| p.y + ankle.y)
                    .fold(f32::INFINITY, f32::min);
                let width = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
                    - points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                assert!(low.abs() < 0.018, "{foot:?} actual sole height {low}");
                assert!(width > 0.22, "substantial weight-bearing claws: {width}");
            }
        }
    }
}

#[test]
fn dragon_skull_is_substantial_apart_from_horns() {
    let (world, rig, _, meshes) = spawn_one(&Look::of(CreatureModel::Dragon, 42));
    let points = direct(&world, &meshes, rig.joint(J::Head).unwrap());
    let skull: Vec<_> = points
        .iter()
        .filter(|p| p.z > 0.0 && p.z < 0.12 && p.y > -0.02 && p.y < 0.12)
        .collect();
    let width = skull.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
        - skull.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    assert!(
        width > 0.28,
        "anatomical skull breadth excluding horns: {width}"
    );
}

#[test]
fn dragon_lower_jaw_matches_the_rendered_upper_muzzle() {
    let (world, rig, _, meshes) = spawn_one(&Look::of(CreatureModel::Dragon, 42));
    let upper = direct(&world, &meshes, rig.joint(J::Head).unwrap());
    let front = upper.iter().map(|p| p.z).fold(f32::NEG_INFINITY, f32::max);
    let section = front * 0.6;
    let width = |joint: Entity, offset: Vec3| {
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
                let p = tri.map(|i| at.transform_point3(points[i].into()) + offset);
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let da = p[a].z - section;
                    let db = p[b].z - section;
                    if da.abs() < 1e-6 {
                        cuts.push(p[a].x);
                    }
                    if da * db < 0.0 {
                        cuts.push(p[a].lerp(p[b], da / (da - db)).x);
                    }
                }
            }
        }
        assert!(!cuts.is_empty(), "actual muzzle/jaw triangle section");
        cuts.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - cuts.iter().copied().fold(f32::INFINITY, f32::min)
    };
    let lower = width(rig.joint(J::Jaw).unwrap(), rig.rest[J::Jaw as usize]);
    let upper_width = width(rig.joint(J::Head).unwrap(), Vec3::ZERO);
    let ratio = lower / upper_width;
    assert!(
        (0.76..1.1).contains(&ratio),
        "lower/upper muzzle breadth ratio {ratio}: {lower}/{upper_width}"
    );
}

#[test]
fn dragon_fallen_barrel_clears_the_floor_across_builds() {
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Dragon,
            42,
            Some(Variant { build, ..default() }),
        );
        let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
        app.world_mut().get_mut::<Anim>(root).unwrap().dead = Some(1.0);
        for _ in 0..100 {
            app.update();
        }
        let world = app.world();
        let rig = world.get::<Rig>(root).unwrap();
        let at = world
            .get::<Transform>(rig.joint(J::Lift).unwrap())
            .unwrap()
            .compute_affine()
            * world
                .get::<Transform>(rig.joint(J::Body).unwrap())
                .unwrap()
                .compute_affine();
        let floor = direct(
            world,
            world.resource::<Assets<Mesh>>(),
            rig.joint(J::Body).unwrap(),
        )
        .iter()
        .map(|p| at.transform_point3(*p).y)
        .fold(f32::INFINITY, f32::min);
        assert!(floor > -0.015, "fallen barrel floor {floor}");
    }
}
