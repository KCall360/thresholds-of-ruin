//! Soft regions answer their frame's motion and settle back to rest.
use super::*;
use crate::props::SoftBone;
use bevy::time::TimeUpdateStrategy;

fn bone(app: &mut App, bone: Entity) -> (Vec3, Vec3) {
    let world = app.world();
    let rest = world.get::<SoftBone>(bone).unwrap().rest;
    (world.get::<Transform>(bone).unwrap().translation, rest)
}

#[test]
fn soft_regions_bounce_with_the_stride_and_settle_at_rest() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Update, soften);
    let rest = Vec3::new(0.08, 0.34, 0.1);
    let frame = app.world_mut().spawn(Transform::default()).id();
    let soft = app
        .world_mut()
        .spawn((
            Transform::from_translation(rest),
            SoftBone {
                rest,
                give: 1.0,
                breath: 0.0,
                offset: Vec3::ZERO,
                velocity: Vec3::ZERO,
                history: None,
            },
            ChildOf(frame),
        ))
        .id();
    // A walking bob: three centimetres at two steps a second.
    let mut furthest: f32 = 0.0;
    for k in 0..90 {
        let t = k as f32 * 0.016;
        let bob = 0.03 * (t * 2.0 * std::f32::consts::TAU).sin();
        app.world_mut()
            .get_mut::<Transform>(frame)
            .unwrap()
            .translation
            .y = bob;
        app.update();
        let (at, rest) = bone(&mut app, soft);
        furthest = furthest.max(at.distance(rest));
    }
    assert!(
        furthest > 0.003,
        "the region answers the stride: {furthest:.4}"
    );
    assert!(
        furthest <= 0.0201,
        "and stays within its reach: {furthest:.4}"
    );
    for _ in 0..150 {
        app.update();
    }
    let (at, rest) = bone(&mut app, soft);
    assert!(
        at.distance(rest) < 2e-4,
        "it settles once the body is still: {:.5}",
        at.distance(rest)
    );
}

/// A woman in an unsupported leotard, jumping on the spot through the real
/// animation: her chest's bones must swing and then settle.
#[test]
fn a_jumping_woman_moves_her_bust_and_plate_holds_it() {
    let travel = |wardrobe: Wardrobe| {
        let look = Look::with_choice(
            CreatureModel::Human,
            5,
            Some(Variant {
                woman: true,
                bust: 3,
                ..default()
            }),
        )
        .dressed(wardrobe);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(16),
            ))
            .init_resource::<Assets<Mesh>>()
            .add_systems(Update, (animate, soften).chain());
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library =
            crate::props::PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let root = app
            .world_mut()
            .spawn((Anim::new(5, Vec3::ZERO), Transform::default()))
            .id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let (rig, meshes) = {
            let world = app.world_mut();
            let mut meshes = Assets::<Mesh>::default();
            let rig = spawn(
                &mut Commands::new(&mut queue, world),
                &mut library,
                &mut meshes,
                root,
                &look,
            );
            queue.apply(world);
            (rig, meshes)
        };
        app.insert_resource(meshes);
        app.world_mut().entity_mut(root).insert(rig);
        for _ in 0..30 {
            app.update();
        }
        app.world_mut()
            .get_mut::<Anim>(root)
            .unwrap()
            .act(Move::Jump);
        let mut furthest: f32 = 0.0;
        for _ in 0..90 {
            app.update();
            let mut bones = app.world_mut().query::<&SoftBone>();
            for bone in bones.iter(app.world()) {
                furthest = furthest.max(bone.offset.length() * bone.give);
            }
        }
        let count = app
            .world_mut()
            .query::<&SoftBone>()
            .iter(app.world())
            .count();
        (count, furthest)
    };
    let (bones, swing) = travel(Wardrobe::Leotard);
    assert_eq!(bones, 4, "a soft region each side of her bust and her seat");
    assert!(swing > 0.004, "the jump swings her bust: {swing:.4}");
    let (bones, _) = travel(Wardrobe::Knight);
    assert_eq!(bones, 0, "plate holds her bust and seat still");
}

#[test]
fn a_still_bust_rises_and_falls_gently_with_her_breath() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Update, soften);
    let rest = Vec3::new(0.08, 0.34, 0.1);
    let frame = app.world_mut().spawn(Transform::default()).id();
    let soft = app
        .world_mut()
        .spawn((
            Transform::from_translation(rest),
            SoftBone {
                rest,
                give: 1.0,
                breath: 1.0,
                offset: Vec3::ZERO,
                velocity: Vec3::ZERO,
                history: None,
            },
            ChildOf(frame),
        ))
        .id();
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for _ in 0..300 {
        app.update();
        let (at, _) = bone(&mut app, soft);
        low = low.min(at.y);
        high = high.max(at.y);
    }
    let swing = high - low;
    assert!(swing > 0.002, "her breath lifts the bust: {swing:.4}");
    assert!(swing < 0.006, "and only gently: {swing:.4}");
}
