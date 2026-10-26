use super::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

fn lineup_app() -> App {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.add_plugins((MinimalPlugins, crate::review_clock::ReviewClockPlugin))
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

#[test]
fn every_bestiary_creature_has_a_model_and_families_filter_the_line_ups() {
    let names = everyone();
    assert_eq!(names.len(), 34, "people, beasts and both Rogue rows");
    for name in &names {
        assert_ne!(
            crate::showcase::creature(name),
            tor_client_3d_model::CreatureModel::Placeholder,
            "{name}"
        );
    }
    for (index, name) in names.iter().enumerate() {
        let (row, i) = row_of(index);
        assert_eq!(ROWS[row][i], *name);
        assert_eq!(row_start(row) + i, index);
    }
    let diagnostics = Diagnostics {
        families: vec!["venus flytrap".into(), "dragon".into()],
        ..default()
    };
    assert_eq!(roster(&diagnostics), vec!["dragon", "venus flytrap"]);
    assert_eq!(roster(&Diagnostics::default()), names);
}

#[test]
fn lineup_capture_deadlines_preserve_motion_frames_on_slow_updates() {
    assert_eq!(lineup_capture_deadline(3.9, 2.0, 0.0, Some(0.5)), 2.5);
    assert_eq!(lineup_capture_deadline(4.8, 2.0, 3.0, Some(0.5)), 3.0);
    // A coincident named shot leaves the same motion deadline for the next
    // free update rather than issuing two readbacks for one render target.
    assert_eq!(lineup_capture_deadline(5.8, 2.0, 4.8, Some(0.5)), 4.8);
    assert_eq!(lineup_capture_deadline(3.9, 2.0, 0.0, None), 3.9);
    assert_eq!(lineup_capture_deadline(3.9, 2.0, 0.0, Some(f32::NAN)), 3.9);
}

#[test]
fn automatic_redressing_waits_for_the_presented_lineup_clock() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(10)))
        .insert_resource(Diagnostics {
            lineups: true,
            wardrobe_cycle: Some(1.5),
            ..default()
        })
        .insert_resource(PropLibrary::new(PropMaterials::new(&mut materials)))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Lineup>()
        .insert_resource(Dress {
            wardrobe: creatures::Wardrobe::Ranger,
            since: 0.0,
        })
        .add_systems(Update, redress);
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<Dress>().wardrobe,
        creatures::Wardrobe::Ranger,
        "GPU readback stalls must not cycle outfits on unpresented frames"
    );
    app.world_mut().resource_mut::<Lineup>().elapsed = 1.5;
    app.update();
    assert_eq!(
        app.world().resource::<Dress>().wardrobe,
        creatures::Wardrobe::Knight
    );
    app.update();
    assert_eq!(
        app.world().resource::<Dress>().wardrobe,
        creatures::Wardrobe::Knight,
        "one presented deadline triggers only one wardrobe change"
    );
    app.world_mut().resource_mut::<Lineup>().elapsed = 3.0;
    app.update();
    assert_eq!(
        app.world().resource::<Dress>().wardrobe,
        creatures::Wardrobe::Courtier
    );
}

#[test]
fn pending_lineup_capture_holds_the_actual_action_pose_and_resumes_after_readback() {
    let mut app = lineup_app();
    app.add_systems(Update, creatures::animate.after(lineup));
    app.update();
    let actor = app.world().resource::<Lineup>().group[0];
    app.world_mut()
        .get_mut::<Anim>(actor)
        .unwrap()
        .act(Move::Jump);
    app.update();
    screenshot(&mut app);
    let phase = app.world().get::<Anim>(actor).unwrap().action.unwrap().1;
    let transforms = |app: &mut App| {
        app.world_mut()
            .query::<(Entity, &Transform)>()
            .iter(app.world())
            .map(|(id, transform)| (id, *transform))
            .collect::<Vec<_>>()
    };
    let held = transforms(&mut app);
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(
        app.world().get::<Anim>(actor).unwrap().action.unwrap().1,
        phase,
        "GPU readback latency must not age the captured action"
    );
    assert_eq!(
        transforms(&mut app),
        held,
        "all posed joints and hanging cloth stay frozen"
    );
    let pending = screenshot(&mut app);
    app.world_mut().despawn(pending);
    app.update();
    assert!(
        app.world().get::<Anim>(actor).unwrap().action.unwrap().1 > phase,
        "animation resumes once readback completes"
    );
}
