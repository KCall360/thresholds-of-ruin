use super::*;
use bevy::asset::{AssetEvent, AssetPlugin};

#[test]
fn inventory_framing_centers_the_actor_in_the_uncovered_region_without_turning_the_camera() {
    let perspective = PerspectiveProjection {
        aspect_ratio: 1.6,
        ..default()
    };
    let matrix = Mat4::perspective_rh(
        perspective.fov,
        perspective.aspect_ratio,
        perspective.near,
        perspective.far,
    );
    let projection = Projection::Perspective(perspective);
    let closed = framed_camera(Vec3::ZERO, Vec3::NEG_Z, 14., &projection, 0.);
    let open = framed_camera(Vec3::ZERO, Vec3::NEG_Z, 14., &projection, 0.54);
    let screen_x = |camera: &Transform| {
        let clip = matrix * camera.compute_matrix().inverse() * Vec3::ZERO.extend(1.);
        clip.x / clip.w * 0.5 + 0.5
    };
    assert!((screen_x(&closed) - 0.5).abs() < 1e-5);
    assert!((screen_x(&open) - 0.23).abs() < 1e-5);
    assert!((closed.rotation.dot(open.rotation).abs() - 1.).abs() < 1e-5);
    assert!(
        (closed.translation - Vec3::new(0., 14. * 0.95f32.sin(), 14. * 0.95f32.cos())).length()
            < 1e-5
    );
}

fn fixture() -> (App, Handle<CaveMaterial>, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<CaveMaterial>()
        .init_resource::<CameraRig>()
        .add_systems(Update, update_cave_params);
    let material = app
        .world_mut()
        .resource_mut::<Assets<CaveMaterial>>()
        .add(CaveMaterial {
            base: StandardMaterial::default(),
            extension: CaveExtension {
                params: CaveParams::default(),
            },
        });
    app.insert_resource(Palette {
        ring: default(),
        cave_mat: material.clone(),
        crystal_mat: default(),
        hostile_ring: default(),
    });
    let camera = app
        .world_mut()
        .spawn((Camera3d::default(), Transform::from_xyz(3.0, 10.0, 8.0)))
        .id();
    app.update();
    modifications(&mut app, &material);
    (app, material, camera)
}

fn modifications(app: &mut App, material: &Handle<CaveMaterial>) -> usize {
    app.world_mut()
        .resource_mut::<Events<AssetEvent<CaveMaterial>>>()
        .drain()
        .filter(|event| matches!(event, AssetEvent::Modified { id } if *id == material.id()))
        .count()
}

#[test]
fn portrait_camera_never_drives_the_cave_cutaway() {
    let (mut app, material, world_camera) = fixture();
    let pose = Transform::from_xyz(-100., -50., -80.);
    let portrait = app
        .world_mut()
        .spawn((Camera3d::default(), crate::portrait::PortraitCamera, pose))
        .id();
    app.world_mut()
        .entity_mut(world_camera)
        .get_mut::<Transform>()
        .unwrap()
        .translation = Vec3::new(5., 12., 4.);
    app.update();
    let params = &app
        .world()
        .resource::<Assets<CaveMaterial>>()
        .get(&material)
        .unwrap()
        .extension
        .params;
    assert_eq!(params.camera, Vec3::new(5., 12., 4.).extend(1.55));
    assert_eq!(*app.world().get::<Transform>(portrait).unwrap(), pose);
}

#[test]
fn actor_following_uses_the_world_camera_even_with_an_offscreen_camera() {
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(std::time::Duration::from_secs_f32(1. / 60.));
    app.insert_resource(time).add_systems(Update, follow_actors);
    app.world_mut()
        .spawn((Camera3d::default(), Transform::from_xyz(0., 10., 8.)));
    let portrait_pose = Transform::from_xyz(-99., -99., -99.);
    let portrait = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            crate::portrait::PortraitCamera,
            portrait_pose,
        ))
        .id();
    let label = app
        .world_mut()
        .spawn((Label, Node::default(), Visibility::Hidden))
        .id();
    let ring = app.world_mut().spawn_empty().id();
    let body = app
        .world_mut()
        .spawn((
            ActorBody {
                appearance_seed: None,
                appearance_key: "fixture".into(),
                model: CreatureModel::Placeholder,
                wearing: default(),
                hostile: false,
                goal: Vec3::new(2., 0., 3.),
                label,
                ring,
                name: "Actor".into(),
                height: 1.8,
            },
            Transform::default(),
        ))
        .id();
    app.update();
    assert!(
        app.world()
            .get::<Transform>(body)
            .unwrap()
            .translation
            .length()
            > 0.
    );
    assert_eq!(
        *app.world().get::<Transform>(portrait).unwrap(),
        portrait_pose
    );
}

#[test]
fn stationary_view_does_not_emit_cave_material_modifications() {
    let (mut app, material, _) = fixture();
    let mut count = 0;
    for _ in 0..64 {
        app.update();
        count += modifications(&mut app, &material);
    }
    assert_eq!(
        count, 0,
        "An unchanged view must not reprepare GPU material bindings"
    );
}

#[test]
fn moving_focus_and_camera_publish_current_cutaway_then_settle() {
    let (mut app, material, camera) = fixture();
    let focus = Vec3::new(2.0, 3.0, 4.0);
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    app.update();
    assert_eq!(modifications(&mut app, &material), 1);
    let params = &app
        .world()
        .resource::<Assets<CaveMaterial>>()
        .get(&material)
        .unwrap()
        .extension
        .params;
    assert_eq!(
        params.player,
        (focus + Vec3::Y * 0.1).extend(focus.y - 0.65)
    );
    let eye = Vec3::new(7.0, 11.0, 9.0);
    app.world_mut()
        .entity_mut(camera)
        .get_mut::<Transform>()
        .unwrap()
        .translation = eye;
    app.update();
    assert_eq!(modifications(&mut app, &material), 1);
    let params = &app
        .world()
        .resource::<Assets<CaveMaterial>>()
        .get(&material)
        .unwrap()
        .extension
        .params;
    assert_eq!(params.camera, eye.extend(1.55));
    app.update();
    assert_eq!(modifications(&mut app, &material), 0);
}
