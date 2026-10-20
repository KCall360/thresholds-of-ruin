use super::anatomy::{storybook_part, HumanoidBody};
use super::*;

fn appearance(model: CreatureModel, woman: bool, build: u8) -> Look {
    Look::with_choice(
        model,
        42,
        Some(Variant {
            woman,
            build,
            ..default()
        }),
    )
}

#[test]
fn art_contract_profiles_change_mass_distribution_without_lengthening_bones() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Scout,
        CreatureModel::Vampire,
    ] {
        for woman in [false, true] {
            let bodies =
                [0, 1, 2].map(|build| HumanoidBody::resolve(&appearance(model, woman, build)));
            assert!(
                bodies[0].shoulders < bodies[1].shoulders
                    && bodies[1].shoulders < bodies[2].shoulders
            );
            assert!(
                bodies[0].hips_width < bodies[1].hips_width
                    && bodies[1].hips_width < bodies[2].hips_width
            );
            for body in bodies {
                assert_eq!(body.knee(), bodies[1].knee());
                assert_eq!(body.ankle(), bodies[1].ankle());
                assert_eq!(body.elbow(), bodies[1].elbow());
                assert_eq!(body.hand, bodies[1].hand);
                assert!(body.shoulder(1.0).x > body.leg_root(1.0).x);
                if woman {
                    assert!(body.hips_width > body.waist * 1.4);
                } else {
                    assert!(body.shoulders > body.waist);
                }
            }
        }
    }
}

#[test]
fn art_contract_constructed_and_furred_families_keep_separate_profiles() {
    for model in [CreatureModel::Guardian, CreatureModel::Yeti] {
        for build in 0..3 {
            let body = HumanoidBody::resolve(&appearance(model, false, build));
            assert_eq!(body.profile.shoulders, 1.0);
            assert_eq!(body.profile.hips, 1.0);
            assert_eq!(body.profile.muscle, 1.0);
        }
    }
}

#[test]
fn art_contract_spawned_joints_use_the_same_dimensions_as_body_fitting() {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let look = appearance(CreatureModel::Mage, true, 2);
    let parent = world.spawn_empty().id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = spawn(
        &mut Commands::new(&mut queue, &world),
        &mut library,
        &mut meshes,
        parent,
        &look,
    );
    queue.apply(&mut world);
    let body = rig.humanoid.unwrap();
    for (joint, expected) in [
        (J::Hips, body.hips),
        (J::ArmL, body.shoulder(1.0)),
        (J::LegR, body.leg_root(-1.0)),
        (J::ShinR, body.knee()),
        (J::FootR, body.ankle()),
    ] {
        assert_eq!(rig.rest[joint as usize], expected);
        assert_eq!(
            world
                .get::<Transform>(rig.joint(joint).unwrap())
                .unwrap()
                .translation,
            expected
        );
    }
    assert!(
        rig.robe.is_some(),
        "the actual caster recipe must own fitted cloth"
    );
}

/// Every surface point of `body` within `heights` lies inside `cloth` by at
/// least `clearance`, all the way round the limb.
fn assert_covers(
    body: &crate::sdf::Sculpt,
    cloth: &crate::sdf::Sculpt,
    heights: &[f32],
    clearance: f32,
    what: &str,
) {
    for &y in heights {
        for k in 0..12 {
            let a = k as f32 / 12.0 * std::f32::consts::TAU;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            let skin = body.onto(Vec3::new(0.0, y, 0.0) + out * 0.3, -out);
            assert!(
                body.distance(skin).abs() < 0.002,
                "{what}: no surface at {y}"
            );
            let inside = cloth.distance(skin);
            assert!(
                inside < -clearance,
                "{what} at height {y}, angle {a:.2}: {inside}"
            );
        }
    }
}

#[test]
fn art_contract_sleeves_and_trousers_stand_off_the_limbs_they_cover() {
    for (build, muscle) in [(0.9, 0.94), (1.0, 1.0), (1.1, 1.08)] {
        assert_covers(
            &geometry::upper_arm_form(1.0, build, 1.0, muscle, 1.0),
            &geometry::upper_sleeve_form(1.0, build, 1.0, muscle),
            &[-0.06, -0.12, -0.18, -0.24],
            0.003 * build,
            "upper sleeve",
        );
        assert_covers(
            &geometry::forearm_form(build, 1.0, muscle),
            &geometry::forearm_sleeve_form(build, 1.0, muscle),
            &[-0.03, -0.08, -0.14, -0.2],
            0.003 * build,
            "forearm sleeve",
        );
        // The elbow gathers: cloth stands further off there than the ease.
        let arm = geometry::upper_arm_form(1.0, build, 1.0, muscle, 1.0);
        let sleeve = geometry::upper_sleeve_form(1.0, build, 1.0, muscle);
        let elbow = arm.onto(Vec3::new(0.0, -0.245, 0.3), Vec3::NEG_Z);
        assert!(sleeve.distance(elbow) < -(geometry::SLEEVE_EASE * build + 0.003));
    }
    for build in 0..3 {
        let body = HumanoidBody::resolve(&appearance(CreatureModel::Human, build == 1, build));
        let ease = geometry::TROUSER_EASE * body.build;
        for (shape, knee) in [(body.thigh, false), (body.shin, true)] {
            let length = shape.length;
            assert_covers(
                &geometry::limb_form(shape),
                &geometry::cloth_over(geometry::limb_form(shape), ease, &[]),
                &[-0.15 * length, -0.5 * length, -0.85 * length],
                0.005 * body.build,
                if knee {
                    "trouser shin"
                } else {
                    "trouser thigh"
                },
            );
        }
    }
}

#[test]
fn art_contract_long_hems_hang_free_while_short_edges_stay_neat() {
    let (top, bottom) = (Vec2::new(0.18, 0.135), Vec2::new(0.22, 0.16));
    let hem_range = |length: f32| {
        let heights: Vec<f32> = (0..64)
            .map(|k| {
                let a = k as f32 / 64.0 * std::f32::consts::TAU;
                let p = geometry::drape_point(
                    geometry::SkirtCut::straight(top, bottom),
                    length,
                    8,
                    0.0,
                    0.2,
                    1.0,
                    a,
                    false,
                );
                assert!(p.is_finite());
                p.y
            })
            .collect();
        let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let high = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        high - low
    };
    assert!(
        hem_range(0.4) > 0.01,
        "a long skirt's hem must not be a level ring"
    );
    assert!(hem_range(0.045) < 0.003, "a cowl keeps its neat edge");
    // The waist stays where the garment is fitted.
    for k in 0..16 {
        let a = k as f32 / 16.0 * std::f32::consts::TAU;
        let waist = geometry::drape_point(
            geometry::SkirtCut::straight(top, bottom),
            0.4,
            8,
            0.0,
            0.2,
            0.0,
            a,
            false,
        );
        assert!((waist.y - 0.04).abs() < 1e-5);
    }
}

#[test]
fn art_contract_plate_lames_overlap_like_shingles() {
    for (count, length, top_r, bottom_r) in [(3, 0.055, 0.064, 0.058), (4, 0.085, 0.104, 0.075)] {
        // A limb tapering from the hip down.
        let mut limb = crate::sdf::Sculpt::default();
        limb.add(
            crate::sdf::Shape::Capsule(Vec3::ZERO, Vec3::Y * -0.4, top_r, bottom_r),
            0.0,
        );
        let bands = geometry::fitted_lames(&limb, -0.04, count, length, 0.007);
        assert_eq!(bands.len(), count);
        for pair in bands.windows(2) {
            let (upper, lower) = (pair[0], pair[1]);
            assert!(
                lower.top > upper.bottom,
                "plates must overlap, not open a gap"
            );
            assert!(
                lower.top < upper.top,
                "each plate starts below the one above"
            );
            // Where they overlap, the lower plate tucks inside the upper one.
            assert!(lower.radius_top < upper.radius_bottom);
        }
        for band in bands {
            assert!(band.top > band.bottom && band.radius_top > 0.0 && band.radius_bottom > 0.0);
        }
    }
}

/// The highest point of a sculpted form, found by sampling down from above.
fn crown(form: &crate::sdf::Sculpt) -> f32 {
    let mut top = f32::NEG_INFINITY;
    for i in -30..=30 {
        for k in -20..=20 {
            let (x, z) = (i as f32 * 0.005, k as f32 * 0.005);
            let mut y = 0.15;
            while y > -0.1 {
                if form.distance(Vec3::new(x, y, z)) < 0.0 {
                    top = top.max(y);
                    break;
                }
                y -= 0.002;
            }
        }
    }
    top
}

#[test]
fn art_contract_deltoids_round_off_level_with_the_shoulder_line() {
    // The top of the upper arm rounds off level with the clavicle that
    // ends at the shoulder pivot, rather than rising above it like a pad.
    for (build, muscle) in [(0.9, 0.9), (1.0, 1.0), (1.1, 1.08), (1.2, 1.2)] {
        for fullness in [0.94, 1.0] {
            let arm = geometry::upper_arm_form(1.0, build, 1.0, muscle, fullness);
            let top = crown(&arm);
            assert!(
                top < 0.02 * build,
                "build {build} muscle {muscle}: the arm rises {top:.3} over the shoulder"
            );
        }
    }
}

/// How far a limb's sculpted surface reaches from its axis at a height.
fn limb_reach(form: &crate::sdf::Sculpt, y: f32) -> f32 {
    (0..24)
        .filter_map(|k| {
            let a = k as f32 / 24.0 * std::f32::consts::TAU;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            let hit = form.onto(Vec3::new(0.0, y, 0.0) + out * 0.3, -out);
            (form.distance(hit).abs() < 0.002).then(|| Vec2::new(hit.x, hit.z).length())
        })
        .fold(0.0, f32::max)
}

#[test]
fn art_contract_plate_lames_cover_every_wearer() {
    // Any humanoid can be dressed in plate: the cuisses and rerebraces are
    // cut from the wearer's own thigh and upper arm, over every build and
    // figure, and still overlap like shingles.
    for model in [
        CreatureModel::Human,
        CreatureModel::Medusa,
        CreatureModel::Orc,
        CreatureModel::Troll,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&appearance(model, woman, build));
                let arm = geometry::upper_arm_form(
                    1.0,
                    body.build,
                    body.arm_length,
                    body.profile.muscle,
                    0.94,
                );
                for (what, form, bands) in [
                    (
                        "cuisse",
                        geometry::limb_form(body.thigh),
                        geometry::cuisse_lames(body),
                    ),
                    ("rerebrace", arm, geometry::rerebrace_lames(body)),
                ] {
                    for pair in bands.windows(2) {
                        assert!(
                            pair[1].radius_top < pair[0].radius_bottom,
                            "{model:?} woman {woman} build {build}: {what} lames must nest"
                        );
                    }
                    for (k, lame) in bands.iter().enumerate() {
                        for step in 0..=4 {
                            let s = step as f32 / 4.0;
                            let y = lame.top + (lame.bottom - lame.top) * s;
                            let plate =
                                lame.radius_top + (lame.radius_bottom - lame.radius_top) * s;
                            let reach = limb_reach(&form, y);
                            assert!(
                                plate > reach + 0.002,
                                "{model:?} woman {woman} build {build}: {what} lame {k} \
                                 {plate:.3} inside the limb {reach:.3} at {y:.3}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn art_contract_womens_faces_are_soft_with_full_lips() {
    // As on the concept sheets, her lower face is softly rounded and her
    // lips stand fuller than a man's. The undead keep
    // their gaunt hollows and are not held to it.
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Leprechaun,
    ] {
        for build in 0..3 {
            let her = geometry::face_shape(&appearance(model, true, build), true);
            let him = geometry::face_shape(&appearance(model, false, build), true);
            // Across the lower face her cheeks round away gently from the
            // mouth, rather than turning a flat front into flat sides.
            let depth = |x: f32, y: f32| her.form.onto(Vec3::new(x, y, 0.3), Vec3::NEG_Z).z;
            for y in [0.09f32, 0.1] {
                let fall = depth(0.03, y) - depth(0.06, y);
                assert!(
                    fall < 0.02,
                    "{model:?} build {build}: her face falls {fall:.4} across the cheek at {y}"
                );
            }
            // Beside and below the mouth her face stays full, rather than a
            // narrow muzzle standing out of cheeks that fall away behind it.
            let corner = depth(0.064, 0.078);
            assert!(
                corner > 0.04,
                "{model:?} build {build}: her face falls away beside the mouth: {corner:.4}"
            );
            let lips = |shape: &geometry::FaceShape| {
                // The lips' height, front to back: how far they stand over
                // the plane below them.
                let lip = shape.form.onto(Vec3::new(0.0, 0.087, 0.3), Vec3::NEG_Z).z;
                let below = shape.form.onto(Vec3::new(0.0, 0.074, 0.3), Vec3::NEG_Z).z;
                lip - below
            };
            assert!(
                lips(&her) > lips(&him) + 0.002,
                "{model:?} build {build}: her lips {:.4} against his {:.4}",
                lips(&her),
                lips(&him)
            );
        }
    }
}

#[test]
fn art_contract_the_neck_rises_from_the_shoulders_without_a_ring() {
    // Going up the neck all round, its skin emerges from the shoulders once
    // and stays clear: no rim of the torso's neck stub crosses it again
    // above, and it never rises out of a flat ledge (the top of a stub
    // wider than the neck), for either sex or any build. Each part carries its own
    // storybook proportion.
    let torso_scale = storybook_part(Plan::Biped, J::Torso);
    let head_scale = storybook_part(Plan::Biped, J::Head);
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&appearance(model, woman, build));
                let torso = geometry::torso_shape(body, None, true);
                let neck = geometry::neck_profile(body.build, geometry::neck_slim(woman));
                for k in 0..16 {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    let mut emerged = None;
                    for pair in neck.windows(2) {
                        for step in 0..8 {
                            let s = step as f32 / 8.0;
                            let at = pair[0].0.lerp(pair[1].0, s);
                            let radii = pair[0].1.lerp(pair[1].1, s);
                            // A point on the neck's skin, in the torso's own space.
                            let skin = (at + Vec3::new(a.cos() * radii.x, 0.0, a.sin() * radii.y))
                                * head_scale
                                + body.head;
                            let inner = skin / torso_scale;
                            let clear = torso.distance(inner) > -0.001;
                            match (emerged, clear) {
                                (None, true) => {
                                    emerged = Some(skin.y);
                                    let e = 0.002;
                                    let rise = torso.distance(inner + Vec3::Y * e)
                                        - torso.distance(inner - Vec3::Y * e);
                                    let slope = Vec3::new(
                                        torso.distance(inner + Vec3::X * e)
                                            - torso.distance(inner - Vec3::X * e),
                                        rise,
                                        torso.distance(inner + Vec3::Z * e)
                                            - torso.distance(inner - Vec3::Z * e),
                                    )
                                    .normalize();
                                    assert!(
                                        slope.y < 0.9,
                                        "{model:?} woman {woman} build {build}: the neck rises out of a flat ledge at {skin}"
                                    );
                                }
                                (Some(from), false) => panic!(
                                    "{model:?} woman {woman} build {build}: a rim crosses the neck at {skin}, above where it emerged at {from:.3}"
                                ),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
}
