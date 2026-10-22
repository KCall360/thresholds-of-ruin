use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

fn vertices(world: &World, meshes: &Assets<Mesh>, joint: Entity) -> Vec<Vec3> {
    let mut points = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world
            .get::<Mesh3d>(child)
            .and_then(|mesh| meshes.get(&mesh.0))
        else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(values)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("equine mesh positions")
        };
        let at = world.get::<Transform>(child).unwrap().compute_affine();
        points.extend(
            values
                .iter()
                .map(|point| at.transform_point3((*point).into())),
        );
    }
    assert!(!points.is_empty());
    points
}

#[test]
fn equine_masses_and_fore_hind_links_share_compact_support_proportions() {
    for model in [
        CreatureModel::Quagga,
        CreatureModel::BlackUnicorn,
        CreatureModel::Centaur,
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
                );
                let (world, rig, _, meshes) = spawn_one(&look);
                let upper = rig.rest[J::FLs as usize];
                let lower = rig.rest[J::HoofFL as usize];
                assert!(
                    lower.length() / upper.length() < 0.84,
                    "{model:?}: cannon should be visibly shorter than the upper support"
                );
                assert!(
                    upper.distance(rig.rest[J::BLs as usize]) > 0.06,
                    "fore knee and hind hock need distinct landmarks"
                );
                let height = rig.rest[J::Body as usize].y;
                assert!(
                    (0.72..0.92).contains(&height),
                    "compact support height: {height}"
                );
                let body = vertices(&world, &meshes, rig.joint(J::Body).unwrap());
                let low = body.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                let high = body.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    (high - low) / height > 0.65,
                    "ribcage/haunch mass must balance the supporting lengths"
                );
                for (root, knee, hoof) in [
                    (J::FL, J::FLs, J::HoofFL),
                    (J::FR, J::FRs, J::HoofFR),
                    (J::BL, J::BLs, J::HoofBL),
                    (J::BR, J::BRs, J::HoofBR),
                ] {
                    let ankle = rig.rest[J::Body as usize]
                        + rig.rest[root as usize]
                        + rig.rest[knee as usize]
                        + rig.rest[hoof as usize];
                    let foot = vertices(&world, &meshes, rig.joint(hoof).unwrap());
                    let floor = foot
                        .iter()
                        .map(|p| p.y + ankle.y)
                        .fold(f32::INFINITY, f32::min);
                    assert!(
                        floor.abs() < 0.008,
                        "{model:?} {hoof:?} rests on its actual sole: {floor}"
                    );
                    let sole: Vec<_> = foot.iter().filter(|p| p.y + ankle.y < 0.004).collect();
                    let width = sole.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
                        - sole.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                    assert!(width > 0.16, "broad weight-bearing hoof sole: {width}");
                }
            }
        }
    }
}

#[test]
fn equine_muzzle_has_a_substantial_bridge_and_jaw_instead_of_a_button_tip() {
    for model in [CreatureModel::Quagga, CreatureModel::BlackUnicorn] {
        let (world, rig, _, meshes) = spawn_one(&Look::of(model, 42));
        let head = vertices(&world, &meshes, rig.joint(J::Snout).unwrap());
        let muzzle: Vec<_> = head
            .iter()
            .filter(|p| p.z > 0.33 && p.y < 0.0 && p.y > -0.18)
            .collect();
        let width = muzzle.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
            - muzzle.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        assert!(
            width > 0.18,
            "{model:?}: substantial anatomical muzzle width {width}"
        );
    }
}

#[test]
fn equine_fallen_barrel_clears_the_floor_at_each_build() {
    use crate::creatures::rogue_tests::animate_look;
    for model in [
        CreatureModel::Quagga,
        CreatureModel::BlackUnicorn,
        CreatureModel::Centaur,
    ] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
            app.world_mut().get_mut::<Anim>(root).unwrap().dead = Some(1.0);
            for _ in 0..100 {
                app.update();
            }
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let lift = world
                .get::<Transform>(rig.joint(J::Lift).unwrap())
                .unwrap()
                .compute_affine();
            let body = world
                .get::<Transform>(rig.joint(J::Body).unwrap())
                .unwrap()
                .compute_affine();
            let at = lift * body;
            let points = vertices(
                world,
                world.resource::<Assets<Mesh>>(),
                rig.joint(J::Body).unwrap(),
            );
            let floor = points
                .iter()
                .map(|p| at.transform_point3(*p).y)
                .fold(f32::INFINITY, f32::min);
            assert!(
                floor > -0.015,
                "{model:?} build {build}: fallen barrel cuts through the floor at {floor}"
            );
        }
    }
}
