use super::*;
use bevy::asset::{AssetEvent, AssetPlugin};

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
