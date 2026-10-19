use super::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

fn lineup_app() -> App {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(10)))
        .insert_resource(Diagnostics {
            lineups: true,
            capture: Some(std::env::temp_dir().join("unused-lineup-capture")),
            capture_warmup: 2.0,
            ..default()
        })
        .insert_resource(library)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<CaveMaterial>>()
        .init_resource::<Lineup>()
        .add_systems(Update, lineup);
    app
}

#[test]
fn slow_lineup_frames_request_the_first_view_without_waiting_on_clamped_animation_time() {
    let mut app = lineup_app();
    for _ in 0..3 {
        app.update();
    }
    let state = app.world().resource::<Lineup>();
    assert_eq!(
        state.shown,
        Some(0),
        "slow startup must not skip the first family"
    );
    assert_eq!(state.group.len(), 5);
    assert_eq!(
        state.shot, 1,
        "slow frames must reach the first full-line capture"
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<Screenshot>>()
            .iter(app.world())
            .count(),
        1
    );
}

fn screenshot(app: &mut App) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<Screenshot>>()
        .single(world)
        .unwrap()
}

fn camera_x(app: &mut App) -> f32 {
    let world = app.world_mut();
    world
        .query_filtered::<&Transform, With<Camera3d>>()
        .single(world)
        .unwrap()
        .translation
        .x
}

#[test]
fn slow_lineup_readbacks_preserve_each_view_and_render_new_families_before_capture() {
    for warmup in [2.0, 0.1] {
        let mut app = lineup_app();
        app.world_mut().resource_mut::<Diagnostics>().capture_warmup = warmup;
        app.world_mut().spawn((
            Camera3d::default(),
            Projection::default(),
            Transform::default(),
        ));
        app.update();
        app.update();
        let first = screenshot(&mut app);
        assert_eq!(app.world().resource::<Lineup>().shot, 1);
        assert_eq!(camera_x(&mut app), 6.6);
        // An incomplete GPU readback must retain both the view and the family,
        // even when another 200 seconds of real frame time passes.
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(screenshot(&mut app), first);
        assert_eq!(app.world().resource::<Lineup>().shown, Some(0));
        assert_eq!(app.world().resource::<Lineup>().shot, 1);
        assert_eq!(camera_x(&mut app), 6.6);
        for (shot, x) in [(2, 3.4), (3, 5.0)] {
            let pending = screenshot(&mut app);
            app.world_mut().despawn(pending);
            app.update();
            assert_eq!(app.world().resource::<Lineup>().shot, shot);
            assert_eq!(app.world().resource::<Lineup>().shown, Some(0));
            assert_eq!(camera_x(&mut app), x);
            screenshot(&mut app);
        }
        let pending = screenshot(&mut app);
        app.world_mut().despawn(pending);
        app.update();
        assert_eq!(app.world().resource::<Lineup>().shown, Some(1));
        assert_eq!(app.world().resource::<Lineup>().shot, 3);
        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<Entity, With<Screenshot>>()
                .iter(world)
                .count(),
            0,
            "the new family needs a rendered frame before its first capture"
        );
        app.update();
        assert_eq!(app.world().resource::<Lineup>().shown, Some(1));
        assert_eq!(app.world().resource::<Lineup>().shot, 4);
        assert_eq!(camera_x(&mut app), 6.6);
        screenshot(&mut app);
    }
}
