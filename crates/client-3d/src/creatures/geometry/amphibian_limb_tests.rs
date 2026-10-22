//! Squat supporting limbs need visible muscle, bent landmarks and broad soles.
//! These contracts inspect native spawned meshes, including their child scale;
//! they do not prescribe the generator's profile rings or exact bone vectors.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

const CHAINS: [(J, J, J, f32); 4] = [
    (J::FL, J::FLs, J::HoofFL, 1.0),
    (J::FR, J::FRs, J::HoofFR, -1.0),
    (J::BL, J::BLs, J::HoofBL, 1.0),
    (J::BR, J::BRs, J::HoofBR, -1.0),
];

// A segment's own rendered surface, excluding the next articulated segment.
fn segment_points(world: &World, meshes: &Assets<Mesh>, joint: Entity) -> Vec<Vec3> {
    let mut points = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("native limb mesh has no positions");
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        points.extend(
            vertices
                .iter()
                .map(|p| transform.transform_point3((*p).into())),
        );
    }
    assert!(!points.is_empty() && points.iter().all(|p| p.is_finite()));
    points
}

fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    assert!(!points.is_empty());
    points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    )
}

fn belly_radius(points: &[Vec3], bone: Vec3) -> f32 {
    let axis = bone.normalize();
    let mut radius = 0.0f32;
    let mut samples = 0;
    for p in points {
        let along = p.dot(axis);
        let t = along / bone.length();
        if (0.08..=0.62).contains(&t) {
            radius = radius.max((*p - axis * along).length());
            samples += 1;
        }
    }
    assert!(samples >= 8, "missing muscle-envelope samples: {samples}");
    radius
}

#[test]
fn amphibian_limbs_have_splayed_bent_support_landmarks() {
    let mut failures = Vec::new();
    for model in [CreatureModel::Aquator, CreatureModel::Xeroc] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (_, rig, _, _) = spawn_one(&look);
            for (upper, lower, foot, side) in CHAINS {
                let root = rig.rest[J::Body as usize] + rig.rest[upper as usize];
                let a = rig.rest[lower as usize];
                let b = rig.rest[foot as usize];
                let ankle = root + a + b;
                let bend = a.angle_between(b);
                let outward = (ankle.x - root.x) * side;
                // A real elbow/knee and a sole outside its root distinguish
                // the approved low crawl from four vertical cylindrical posts.
                if bend < 0.45 || outward < 0.07 || root.y - ankle.y > 0.36 {
                    failures.push(format!("{model:?} build={build} {upper:?}: bend={bend:.4}, outward={outward:.4}, root/ankle={root:?}/{ankle:?}"));
                }
                assert!(rig.supports(foot), "{model:?} must keep all four supports");
                assert!(
                    (ankle.y - (rig.ankle_floor - 0.008)).abs() < 1e-4,
                    "{model:?} {foot:?} support root no longer reaches its sole"
                );
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn amphibian_limbs_emit_substantial_haunches_and_tapered_supports() {
    let mut failures = Vec::new();
    for model in [CreatureModel::Aquator, CreatureModel::Xeroc] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (world, rig, _, meshes) = spawn_one(&look);
            let mut upper_radii = [0.0; 4];
            for (i, (upper, lower, foot, _)) in CHAINS.into_iter().enumerate() {
                let a = rig.rest[lower as usize];
                let b = rig.rest[foot as usize];
                let upper_points = segment_points(&world, &meshes, rig.joint(upper).unwrap());
                let lower_points = segment_points(&world, &meshes, rig.joint(lower).unwrap());
                let muscle = belly_radius(&upper_points, a);
                let distal = belly_radius(&lower_points, b);
                upper_radii[i] = muscle;
                // A substantial proximal mass has observable breadth relative
                // to its bone, then narrows into the supporting lower segment.
                let min_radius = if model == CreatureModel::Aquator && i >= 2 {
                    0.13
                } else {
                    0.105
                };
                if muscle < min_radius || muscle < distal * 1.20 || muscle < a.length() * 0.45 {
                    failures.push(format!("{model:?} build={build} {upper:?}: muscle={muscle:.4}, lower={distal:.4}, bone={:.4}", a.length()));
                }
            }
            if model == CreatureModel::Aquator {
                for side in 0..2 {
                    if upper_radii[side + 2] < upper_radii[side] * 1.20 {
                        failures.push(format!(
                            "Aquator build={build} side={side}: rear/front muscle={:.4}/{:.4}",
                            upper_radii[side + 2],
                            upper_radii[side]
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn amphibian_limbs_end_in_broad_low_grounded_feet() {
    let mut failures = Vec::new();
    for model in [CreatureModel::Aquator, CreatureModel::Xeroc] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (world, rig, _, meshes) = spawn_one(&look);
            for (upper, lower, foot, _) in CHAINS {
                let points = segment_points(&world, &meshes, rig.joint(foot).unwrap());
                let (low, high) = bounds(&points);
                let size = high - low;
                let sole_points: Vec<_> = points
                    .iter()
                    .copied()
                    .filter(|p| p.y < low.y + size.y * 0.55)
                    .collect();
                let (sole_low, sole_high) = bounds(&sole_points);
                let sole = sole_high - sole_low;
                let ankle = rig.rest[J::Body as usize]
                    + rig.rest[upper as usize]
                    + rig.rest[lower as usize]
                    + rig.rest[foot as usize];
                let floor = ankle.y + low.y;
                // Width is required at the low sole itself: a bulbous ankle
                // or a lone lateral spike cannot substitute for a broad pad.
                if sole.x < 0.22
                    || sole.z < 0.17
                    || size.x < size.y * 2.5
                    || !(-0.018..=0.018).contains(&floor)
                {
                    failures.push(format!("{model:?} build={build} {foot:?}: sole={sole:?}, whole foot={size:?}, lowest floor={floor:.4}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
