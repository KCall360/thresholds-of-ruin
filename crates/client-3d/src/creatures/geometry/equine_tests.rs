use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

fn vertices(world: &World, rig: &Rig, meshes: &Assets<Mesh>, joint: Entity) -> Vec<Vec3> {
    let mut points = Vec::new();
    for surface in super::test_support::joint_meshes(world, rig, meshes, joint) {
        let Some(VertexAttributeValues::Float32x3(values)) =
            surface.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("equine mesh positions")
        };
        points.extend(
            values
                .iter()
                .map(|point| surface.transform.transform_point3((*point).into())),
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
                let body = vertices(&world, &rig, &meshes, rig.joint(J::Body).unwrap());
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
                    let foot = vertices(&world, &rig, &meshes, rig.joint(hoof).unwrap());
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
        let head = vertices(&world, &rig, &meshes, rig.joint(J::Snout).unwrap());
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
                rig,
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

#[test]
fn centaur_rider_arm_volume_tracks_build_without_changing_reach() {
    for woman in [false, true] {
        let mut widths = Vec::new();
        let mut reaches = Vec::new();
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Centaur,
                17,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (world, rig, _, meshes) = spawn_one(&look);
            let muscle: Vec<_> = vertices(&world, &rig, &meshes, rig.joint(J::ArmL).unwrap())
                .into_iter()
                .filter(|p| (-0.20..=-0.08).contains(&p.y))
                .collect();
            assert!(
                !muscle.is_empty(),
                "inspect actual upper-arm muscle surfaces"
            );
            let low = muscle.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
            let high = muscle.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
            widths.push(high - low);
            let forearm = vertices(&world, &rig, &meshes, rig.joint(J::ForeL).unwrap());
            let fingertips = forearm.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let grip = rig.rest[J::HandL as usize];
            assert!(grip.y > fingertips + 0.005 && grip.y < fingertips * 0.75,
                "woman={woman}, build={build}: the grip belongs in the terminal fist, not at the elbow");
            reaches.push((rig.rest[J::ForeL as usize], grip));
        }
        assert!(
            widths[0] > 0.08,
            "positive witness: substantial organic arm mass"
        );
        assert!(widths[1] > widths[0] * 1.04 && widths[2] > widths[1] * 1.04,
            "woman={woman}: slender/average/sturdy must change local muscle volume, widths={widths:?}");
        assert!(
            reaches.windows(2).all(|pair| pair[0] == pair[1]),
            "body fullness must not move the elbow or weapon grip"
        );
    }
}

#[test]
fn centaur_rider_deltoid_caps_belong_to_the_animated_arms() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Centaur,
                17,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (world, rig, _, meshes) = spawn_one(&look);
            let torso = vertices(&world, &rig, &meshes, rig.joint(J::Torso).unwrap());
            for joint in [J::ArmL, J::ArmR] {
                let root = rig.rest[joint as usize];
                assert!(!vertices(&world, &rig, &meshes, rig.joint(joint).unwrap()).is_empty());
                // The torso may support the clavicle, but a broad lateral cap
                // must rotate with its arm rather than stay behind on the chest.
                let retained_cap = torso.iter().any(|p| {
                    (root.y - 0.06..=root.y + 0.06).contains(&p.y)
                        && p.x * root.x.signum() > root.x.abs() + 0.04
                });
                assert!(!retained_cap,
                    "woman={woman}, build={build}, {joint:?}: torso retains a static deltoid beyond the shoulder root");
            }
        }
    }
}
