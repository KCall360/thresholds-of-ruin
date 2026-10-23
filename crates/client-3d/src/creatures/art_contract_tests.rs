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

#[test]
fn art_contract_heads_sit_close_on_the_shoulders() {
    // As on the concept sheets, the head sits on a short neck just above
    // the shoulder line, never raised on a stalk.
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Orc,
        CreatureModel::Leprechaun,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&appearance(model, woman, build));
                let rise = body.head.y - body.shoulder(1.0).y;
                assert!(
                    (0.0..=0.04).contains(&rise),
                    "{model:?} woman {woman} build {build}: the head rises {rise:.3} over the shoulders"
                );
            }
        }
    }
}

#[test]
fn art_contract_spawned_fitted_legs_are_lean_tapered_and_keep_build_identity() {
    use bevy::render::mesh::VertexAttributeValues;

    // A unique fitted-leg pigment isolates actual anatomy triangles from
    // boots, wrappings and costume pieces. Preserve the emitted mesh scale;
    // normalizing by resolved build removes species girth, not profile shape.
    let measure = |model, woman, build| {
        let mut look = appearance(model, woman, build).dressed(Wardrobe::Commoner);
        look.outfit.as_mut().unwrap().legs = outfit::Legs::Fitted(Vec3::new(0.0, 0.0, 0.73));
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
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
        let triangles = |joint| {
            let entity = rig.joint(joint).unwrap();
            let joint_transform = world.get::<Transform>(entity).unwrap();
            let mut triangles = Vec::new();
            for child in world.get::<Children>(entity).unwrap().iter() {
                let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
                    continue;
                };
                let Some(VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("{model:?} woman {woman} build {build}: leg positions")
                };
                let Some(VertexAttributeValues::Float32x4(colors)) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                else {
                    panic!("{model:?} woman {woman} build {build}: leg pigments")
                };
                let transform = joint_transform.compute_affine()
                    * world.get::<Transform>(child).unwrap().compute_affine();
                let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
                for tri in indices.as_chunks::<3>().0 {
                    if tri.iter().all(|&i| {
                        let c = colors[i];
                        c[0].abs() < 1e-6 && c[1].abs() < 1e-6 && c[2] > 0.01
                    }) {
                        triangles.push(tri.map(|i| {
                            transform.transform_point3(Vec3::from(positions[i]))
                                - joint_transform.translation
                        }));
                    }
                }
            }
            assert!(
                !triangles.is_empty(),
                "{model:?} woman {woman} build {build} {joint:?}: no fitted-leg triangles"
            );
            triangles
        };
        // Intersect triangles rather than selecting nearby vertices, whose
        // grid spacing can hide the narrow knee or favor one body build.
        let section = |triangles: &[[Vec3; 3]], y: f32, axis: usize| {
            let (mut low, mut high, mut hits) = (f32::INFINITY, f32::NEG_INFINITY, 0);
            for tri in triangles {
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let (a, b) = (tri[a], tri[b]);
                    if (a.y - b.y).abs() < 1e-7 {
                        continue;
                    }
                    let t = (y - a.y) / (b.y - a.y);
                    if (0.0..=1.0).contains(&t) {
                        let x = a.lerp(b, t)[axis];
                        low = low.min(x);
                        high = high.max(x);
                        hits += 1;
                    }
                }
            }
            assert!(
                hits >= 12 && low < 0.0 && high > 0.0,
                "{model:?} woman {woman} build {build}: section y={y:.4}, hits={hits}, x={low:.4}..{high:.4}"
            );
            (high - low) * 0.5 / body.build
        };
        let radius = |triangles: &[[Vec3; 3]], y| section(triangles, y, 0);
        let thigh = triangles(J::LegL);
        let shin = triangles(J::ShinL);
        let right_thigh = triangles(J::LegR);
        // Joint placement supplies the bilateral mirror. The anatomical
        // cross-sections must match on both emitted legs, including depth.
        for fraction in [0.15, 0.35, 0.65, 0.9] {
            for axis in [0, 2] {
                let y = -body.thigh.length * fraction;
                assert!(
                    (section(&thigh, y, axis) - section(&right_thigh, y, axis)).abs() < 0.0005,
                    "{model:?} woman {woman} build {build}: mismatched bilateral section {fraction} axis {axis}"
                );
            }
        }
        let knee = world
            .get::<Transform>(rig.joint(J::ShinL).unwrap())
            .unwrap()
            .translation;
        let ankle = world
            .get::<Transform>(rig.joint(J::FootL).unwrap())
            .unwrap()
            .translation;
        let upper = [0.15, 0.25, 0.35, 0.45, 0.55]
            .map(|t| radius(&thigh, knee.y * t))
            .into_iter()
            .fold(0.0, f32::max);
        let distal = radius(&thigh, knee.y * 0.78);
        let at_knee = radius(&thigh, knee.y * 0.98).max(radius(&shin, ankle.y * 0.02));
        let calf = radius(&shin, ankle.y * 0.30);
        let combined: Vec<_> = thigh
            .iter()
            .copied()
            .chain(shin.iter().map(|tri| tri.map(|point| point + knee)))
            .collect();
        let approach = radius(&combined, knee.y * 0.88);
        let knee_envelope = [0.94, 1.0, 1.04]
            .map(|fraction| radius(&combined, knee.y * fraction))
            .into_iter()
            .fold(0.0f32, f32::max);
        let bridge_ratio = radius(&thigh, knee.y * 0.19)
            / radius(&thigh, knee.y * 0.06).min(radius(&thigh, knee.y * 0.35));
        let proximal_aspect = section(&thigh, knee.y * 0.28, 2) / section(&thigh, knee.y * 0.28, 0);
        let distal_aspect = section(&thigh, knee.y * 0.82, 2) / section(&thigh, knee.y * 0.82, 0);
        (
            upper,
            distal,
            at_knee,
            calf,
            knee,
            ankle,
            proximal_aspect,
            distal_aspect,
            approach,
            knee_envelope,
            bridge_ratio,
        )
    };

    let mut failures = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Scout,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Zombie,
        CreatureModel::Orc,
    ] {
        let mut envelopes = [[0.0; 3]; 2];
        for build in 0..3 {
            for woman in [false, true] {
                let (
                    upper,
                    distal,
                    knee,
                    calf,
                    knee_joint,
                    ankle_joint,
                    proximal_aspect,
                    distal_aspect,
                    approach,
                    knee_envelope,
                    bridge_ratio,
                ) = measure(model, woman, build);
                envelopes[usize::from(woman)][build as usize] = upper;
                if !(upper > distal * 1.12 && distal > knee * 1.03 && calf > knee * 1.03) {
                    failures.push(format!(
                        "{model:?} woman {woman} build {build}: taper/calf recovery upper={upper:.5} distal={distal:.5} knee={knee:.5} calf={calf:.5}"
                    ));
                }
                // The female thigh's upper muscle and distal/knee region
                // need distinct front/side contours, rather than a circular
                // tube with a uniformly smaller radius. These are emitted
                // triangle-plane sections, including the actual mesh scale.
                if woman && proximal_aspect - distal_aspect < 0.08 {
                    failures.push(format!(
                        "{model:?} woman {woman} build {build}: tubular side contour; proximal depth/width={proximal_aspect:.4}, distal={distal_aspect:.4}"
                    ));
                }
                if woman && bridge_ratio < 0.94 {
                    failures.push(format!(
                        "{model:?} woman {woman} build {build}: pinched upper-thigh bridge ratio={bridge_ratio:.4}"
                    ));
                }
                // Inspect the combined thigh and shin around the actual
                // joint: a circular shin cap must not make an isolated bead
                // after the distal thigh narrows. Include both sides of it.
                if woman && knee_envelope > approach * 1.08 {
                    failures.push(format!(
                        "{model:?} woman {woman} build {build}: bulbous combined knee approach={approach:.5}, envelope={knee_envelope:.5}, ratio={:.4}", knee_envelope / approach
                    ));
                }
                // These are the accepted joint landmarks, independent of
                // girth and clothing; the new lean shape must not stretch legs.
                if knee_joint.distance(Vec3::Y * -0.44) > 1e-6
                    || ankle_joint.distance(Vec3::Y * -0.4) > 1e-6
                {
                    failures.push(format!(
                        "{model:?} woman {woman} build {build}: shifted knee={knee_joint:?} ankle={ankle_joint:?}"
                    ));
                }
            }
            let male = envelopes[0][build as usize];
            let female = envelopes[1][build as usize];
            if female > male * 1.10 {
                failures.push(format!(
                    "{model:?} build {build}: inflated female peak={female:.5}, male={male:.5}, ratio={:.4} exceeds 1.10",
                    female / male
                ));
            }
        }
        for (sex, widths) in envelopes.iter().enumerate() {
            if !(widths[1] > widths[0] * 1.01 && widths[2] > widths[1] * 1.01) {
                failures.push(format!(
                    "{model:?} woman {}: normalized slim/sturdy/muscular envelopes lack distinct growth: {widths:?}",
                    sex == 1
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn art_contract_humanoid_pelvis_is_moderate_and_sex_shape_survives() {
    let mut failures = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Scout,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Zombie,
        CreatureModel::Orc,
    ] {
        for build in 0..3 {
            let her = HumanoidBody::resolve(&appearance(model, true, build));
            let him = HumanoidBody::resolve(&appearance(model, false, build));
            for body in [her, him] {
                // Compare the supported pelvis after its actual mesh scale
                // with the shoulder joint span, not with garment padding.
                let half_width = body.pelvis_radii().x * storybook_part(Plan::Biped, J::Hips).x;
                let shoulder = body.shoulder(1.0).x;
                if half_width > shoulder * 0.98 {
                    failures.push(format!(
                        "{model:?} woman {} build {build}: pelvis halfwidth={half_width:.5}, shoulder={shoulder:.5}, ratio={:.4} exceeds 0.98",
                        body.woman,
                        half_width / shoulder
                    ));
                }
            }
            // Independent existing sex-shape witnesses remain: the leaner
            // thigh direction does not remove the finer ankle or rounder seat.
            let per = |v: f32, body: &HumanoidBody| v / body.build;
            let (her_ankle, his_ankle) = (
                per(her.shin.radius_bottom, &her),
                per(him.shin.radius_bottom, &him),
            );
            let (her_depth, his_depth) = (
                per(her.pelvis_radii().z, &her),
                per(him.pelvis_radii().z, &him),
            );
            if her_ankle >= his_ankle * 0.92 || her_depth <= his_depth * 1.05 {
                failures.push(format!(
                    "{model:?} build {build}: normalized female/male ankle={her_ankle:.5}/{his_ankle:.5}, pelvis depth={her_depth:.5}/{his_depth:.5}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn art_contract_adult_brows_have_substance_taper_and_species_expression() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Medusa,
        CreatureModel::Vampire,
        CreatureModel::Orc,
    ] {
        let her = geometry::brow_profile(&appearance(model, true, 1), 1.0);
        let him = geometry::brow_profile(&appearance(model, false, 1), 1.0);
        let widest = |b: &[(Vec3, (f32, f32))]| b.iter().map(|(_, r)| r.0).fold(0.0, f32::max);
        let ratio = widest(&her) / widest(&him);
        assert!(
            (0.5..=0.8).contains(&ratio),
            "{model:?}: adult brow substance: {ratio}"
        );
        let peak = her.iter().position(|(_, r)| r.0 == widest(&her)).unwrap();
        assert!(
            peak > 0 && peak < 3,
            "{model:?}: hers swells within, not at an end"
        );
        assert!(
            her[3].1 .0 < widest(&her) * 0.4,
            "{model:?}: and tapers outward"
        );
        let rise = her.iter().map(|(at, _)| at.y).fold(f32::MIN, f32::max) - her[0].0.y;
        let span = her[3].0.x - her[0].0.x;
        assert!(
            rise > 0.0 && rise / span <= 0.22,
            "{model:?}: controlled arch {rise}/{span}"
        );
        let left = geometry::brow_profile(&appearance(model, true, 1), -1.0);
        for (right, left) in her.iter().zip(left) {
            assert_eq!(right.0 * Vec3::new(-1.0, 1.0, 1.0), left.0);
            assert_eq!(right.1, left.1);
        }
    }
    let slope = |model| {
        let brow = geometry::brow_profile(&appearance(model, true, 1), 1.0);
        brow[0].0.y - brow[3].0.y
    };
    assert!(slope(CreatureModel::Orc) < slope(CreatureModel::Medusa));
    assert!(slope(CreatureModel::Medusa) < slope(CreatureModel::Human));
}
