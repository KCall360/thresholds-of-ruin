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
