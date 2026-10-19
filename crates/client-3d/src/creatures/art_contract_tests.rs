use super::anatomy::HumanoidBody;
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
                let p = geometry::drape_point(top, bottom, length, 8, 0.0, 0.2, 1.0, a, false);
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
        let waist = geometry::drape_point(top, bottom, 0.4, 8, 0.0, 0.2, 0.0, a, false);
        assert!((waist.y - 0.04).abs() < 1e-5);
    }
}
