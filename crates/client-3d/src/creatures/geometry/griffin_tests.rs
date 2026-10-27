//! Broad Griffin anatomy, measured from emitted surfaces apart from its crest.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
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
fn griffin_compact_supports_keep_substantial_talons_and_grounded_soles() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Griffin,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (world, rig, _, meshes) = spawn_one(&look);
            let height = rig.rest[J::Body as usize].y;
            let links = rig.rest[J::FLs as usize].length() + rig.rest[J::HoofFL as usize].length();
            assert!(
                links / height < 0.69,
                "short supports beneath the breast: {links}/{height}"
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
                let floor = points
                    .iter()
                    .map(|p| p.y + ankle.y)
                    .fold(f32::INFINITY, f32::min);
                assert!(floor.abs() < 0.018, "{foot:?} actual sole height {floor}");
                if matches!(foot, J::HoofFL | J::HoofFR) {
                    let width = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
                        - points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                    assert!(width > 0.18, "broad fore talon span {width}");
                }
            }
        }
    }
}

#[test]
fn griffin_anatomical_skull_and_beak_remain_large_without_the_crest() {
    let (world, rig, _, meshes) = spawn_one(&Look::of(CreatureModel::Griffin, 42));
    let points = direct(&world, &meshes, rig.joint(J::Head).unwrap());
    let skull: Vec<_> = points
        .iter()
        .filter(|p| p.z < 0.11 && p.z > -0.10 && p.y < 0.15 && p.y > -0.09)
        .collect();
    let width = skull.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
        - skull.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    assert!(width > 0.27, "anatomical cranium/cheek width {width}");
    let beak: Vec<_> = points.iter().filter(|p| p.z > 0.17).collect();
    let width = beak.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
        - beak.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    assert!(width > 0.09, "substantial hooked beak breadth {width}");
}
