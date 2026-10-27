//! Compact adult ratite proportions measured from the native rendered parts.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

#[derive(Clone, Copy)]
enum Pigment {
    Exact(Vec3),
    Sculpted(Vec3),
}

fn direct_points(
    world: &World,
    meshes: &Assets<Mesh>,
    joint: Entity,
    pigment: Option<Pigment>,
) -> Vec<Vec3> {
    let color = pigment.map(|p| {
        let (c, shaded) = match p {
            Pigment::Exact(c) => (c, false),
            Pigment::Sculpted(c) => (c, true),
        };
        let c = Color::srgb(c.x, c.y, c.z).to_linear();
        (Vec3::new(c.red, c.green, c.blue), shaded)
    });
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
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors")
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        for (point, c) in points.iter().zip(colors) {
            if color.is_none_or(|(wanted, shaded)| {
                let actual = Vec3::new(c[0], c[1], c[2]);
                if !shaded {
                    return wanted.distance(actual) < 1e-5;
                }
                // Sculpt occlusion multiplies decoded RGB by a neutral factor
                // in Builder::finish. Preserve exact chromaticity while
                // admitting that documented shading interval, not other hues.
                let shade = actual.dot(wanted) / wanted.length_squared();
                (0.54999..=1.00001).contains(&shade) && actual.distance(wanted * shade) < 1e-5
            }) {
                out.push(transform.transform_point3((*point).into()));
            }
        }
    }
    assert!(
        !out.is_empty() && out.iter().all(|p| p.is_finite()),
        "native part {joint:?} has no finite sampled surface"
    );
    out
}

fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    assert!(!points.is_empty());
    points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    )
}

fn looks() -> impl Iterator<Item = Look> {
    [false, true].into_iter().flat_map(|woman| {
        (0..3).map(move |build| {
            Look::with_choice(
                CreatureModel::Emu,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            )
        })
    })
}

#[test]
fn emu_compact_supports_and_body_height_resolve_to_grounded_soles() {
    let mut failures = Vec::new();
    for look in looks() {
        let (world, rig, _, meshes) = spawn_one(&look);
        let (body_low, body_high) = bounds(&direct_points(
            &world,
            &meshes,
            rig.joint(J::Body).unwrap(),
            None,
        ));
        let body_depth = body_high.y - body_low.y;
        let visible_support = rig.rest[J::Body as usize].y + body_low.y;
        for (leg, shin, foot) in [(J::LegL, J::ShinL, J::FootL), (J::LegR, J::ShinR, J::FootR)] {
            let support_length =
                rig.rest[shin as usize].length() + rig.rest[foot as usize].length();
            let (foot_low, _) = bounds(&direct_points(
                &world,
                &meshes,
                rig.joint(foot).unwrap(),
                None,
            ));
            let sole = rig.rest[J::Body as usize].y
                + rig.rest[leg as usize].y
                + rig.rest[shin as usize].y
                + rig.rest[foot as usize].y
                + foot_low.y;
            // The sheet's rounded feather mass stands on compact supports,
            // rather than gaining height from exposed stick-like legs. The
            // independent sole check prevents lowering only the body mesh.
            if support_length > body_depth * 1.25
                || visible_support > body_depth * 0.95
                || !(-0.018..=0.018).contains(&sole)
            {
                failures.push(format!("woman={} build={} {foot:?}: bones={support_length:.4}, feather depth={body_depth:.4}, visible support={visible_support:.4}, sole={sole:.4}", look.v.woman, look.v.build));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn emu_emits_substantial_hocks_and_tapered_load_bearing_shanks() {
    let mut failures = Vec::new();
    for look in looks() {
        let (world, rig, _, meshes) = spawn_one(&look);
        for (leg, shin, foot) in [(J::LegL, J::ShinL, J::FootL), (J::LegR, J::ShinR, J::FootR)] {
            let upper = direct_points(&world, &meshes, rig.joint(leg).unwrap(), None);
            let lower = direct_points(&world, &meshes, rig.joint(shin).unwrap(), None);
            let axis = rig.rest[foot as usize].normalize();
            let length = rig.rest[foot as usize].length();
            let mut hock = Vec::new();
            let mut middle = Vec::new();
            let mut distal = Vec::new();
            for p in lower {
                let t = p.dot(axis) / length;
                if (-0.12..=0.12).contains(&t) {
                    hock.push(p);
                }
                if (0.30..=0.65).contains(&t) {
                    middle.push(p);
                }
                if (0.75..=0.95).contains(&t) {
                    distal.push(p);
                }
            }
            let span = |points: &[Vec3]| {
                let (low, high) = bounds(points);
                high.x - low.x
            };
            let hock_width = span(&hock);
            let middle_width = span(&middle);
            let distal_width = span(&distal);
            let muscle_width = span(&upper);
            // Observable volume hierarchy: muscular attachment, readable
            // hock, substantial shank, then a narrower ankle above the toes.
            if hock_width < 0.10
                || middle_width < 0.075
                || middle_width < distal_width * 1.08
                || muscle_width < middle_width * 1.35
            {
                failures.push(format!("woman={} build={} {shin:?}: muscle/hock/mid/distal widths={muscle_width:.4}/{hock_width:.4}/{middle_width:.4}/{distal_width:.4}", look.v.woman, look.v.build));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn emu_has_broad_three_toed_feet_beneath_its_mass() {
    let mut failures = Vec::new();
    for look in looks() {
        let (world, rig, _, meshes) = spawn_one(&look);
        for foot in [J::FootL, J::FootR] {
            let points = direct_points(&world, &meshes, rig.joint(foot).unwrap(), None);
            let claws = direct_points(
                &world,
                &meshes,
                rig.joint(foot).unwrap(),
                Some(Pigment::Exact(DARK * 3.0)),
            );
            let (low, high) = bounds(&points);
            let size = high - low;
            let low_points: Vec<_> = points
                .into_iter()
                .filter(|p| p.y < low.y + size.y * 0.6)
                .collect();
            let (sole_low, sole_high) = bounds(&low_points);
            let sole_width = sole_high.x - sole_low.x;
            // Require three independently readable forward claw regions;
            // widening an ankle ball cannot impersonate a three-toed foot.
            for region in [-1, 0, 1] {
                assert!(
                    claws.iter().any(|p| p.z > 0.07
                        && match region {
                            -1 => p.x < -0.05,
                            0 => p.x.abs() < 0.025,
                            _ => p.x > 0.05,
                        }),
                    "woman={} build={} {foot:?}: missing claw region {region}",
                    look.v.woman,
                    look.v.build
                );
            }
            if sole_width < 0.22 || size.z < 0.20 || sole_width < size.z * 0.85 {
                failures.push(format!("woman={} build={} {foot:?}: low sole width={sole_width:.4}, full extent={size:?}", look.v.woman, look.v.build));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn emu_adult_skull_and_throat_read_above_the_feathered_breast() {
    let mut failures = Vec::new();
    for look in looks() {
        let (world, rig, _, meshes) = spawn_one(&look);
        // Skin chromaticity with baked neutral sculpt shading excludes crest,
        // beak and eyes: the skull itself must carry the adult expression.
        let skull = direct_points(
            &world,
            &meshes,
            rig.joint(J::Head).unwrap(),
            Some(Pigment::Sculpted(look.cloth)),
        );
        let neck = direct_points(
            &world,
            &meshes,
            rig.joint(J::Neck).unwrap(),
            Some(Pigment::Sculpted(look.cloth)),
        );
        let (skull_low, skull_high) = bounds(&skull);
        let (neck_low, neck_high) = bounds(&neck);
        let head_height = rig.rest[J::Head as usize].y;
        let top_neck: Vec<_> = neck
            .into_iter()
            .filter(|p| p.y > head_height * 0.72 && p.y < head_height * 0.95)
            .collect();
        let (top_low, top_high) = bounds(&top_neck);
        let skull_size = skull_high - skull_low;
        let top_width = top_high.x - top_low.x;
        // A larger cranium above a tapered neck, with a throat mass descending
        // into the breast rather than stopping at a bare tube's root disk.
        if skull_size.x < top_width * 1.85
            || skull_size.y < (neck_high.y - neck_low.y) * 0.34
            || neck_low.y > -0.075
        {
            failures.push(format!("woman={} build={}: skull={skull_size:?}, upper neck width={top_width:.4}, neck range={neck_low:?}..{neck_high:?}", look.v.woman, look.v.build));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
