use super::*;
use bevy::{
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
    },
    time::TimeUpdateStrategy,
};
use std::time::Duration;

struct Fixture {
    app: App,
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("tor-capture-{}", uuid::Uuid::new_v4()));
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_event::<AppExit>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
            .insert_resource(Diagnostics {
                capture: Some(directory.clone()),
                capture_every: Some(0.4),
                ..default()
            })
            .add_systems(Update, capture);
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(1));
        Self { app, directory }
    }

    fn screenshots(&mut self) -> Vec<Entity> {
        self.app
            .world_mut()
            .query_filtered::<Entity, With<Screenshot>>()
            .iter(self.app.world())
            .collect()
    }

    fn request_first(&mut self) -> Entity {
        for _ in 0..8 {
            self.app.update();
            if let Some(entity) = self.screenshots().first() {
                return *entity;
            }
        }
        panic!("The fixture must reach an actual screenshot request");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.directory.exists() {
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }
}

#[test]
fn stalled_gpu_readback_keeps_only_one_screenshot_request() {
    let mut fixture = Fixture::new();
    let first = fixture.request_first();
    // Without a renderer, the genuine screenshot entity remains pending.
    // Keep driving the real capture system past many scheduled intervals.
    for _ in 0..64 {
        fixture.app.update();
    }
    assert_eq!(fixture.screenshots(), vec![first]);
    let diagnostics = fixture.app.world().resource::<Diagnostics>();
    assert_eq!(diagnostics.pending_shots, 1);
    assert_eq!(diagnostics.shots, 1);
}

#[test]
fn completed_readback_saves_the_image_and_allows_the_next_capture() {
    let mut fixture = Fixture::new();
    let first = fixture.request_first();
    let image = Image::new_fill(
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[96, 128, 160, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD,
    );
    fixture
        .app
        .world_mut()
        .trigger_targets(ScreenshotCaptured(image), first);
    fixture.app.world_mut().entity_mut(first).despawn();
    assert_eq!(
        fixture.app.world().resource::<Diagnostics>().pending_shots,
        0
    );
    let png = std::fs::read(fixture.directory.join("shot-01.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    for _ in 0..64 {
        fixture.app.update();
    }
    assert_eq!(fixture.screenshots().len(), 1);
    let diagnostics = fixture.app.world().resource::<Diagnostics>();
    assert_eq!(diagnostics.shots, 2);
    assert_eq!(diagnostics.pending_shots, 1);
    assert!(!diagnostics.capture_failed);
}

#[test]
fn stalled_readback_still_reaches_the_existing_failure_deadline() {
    let mut fixture = Fixture::new();
    fixture.request_first();
    let limit = fixture.app.world().resource::<Time<Real>>().elapsed_secs() + 1.0;
    fixture
        .app
        .world_mut()
        .resource_mut::<Diagnostics>()
        .quit_after = Some(limit);
    for _ in 0..16 {
        fixture.app.update();
    }
    assert!(fixture
        .app
        .world_mut()
        .resource_mut::<Events<AppExit>>()
        .drain()
        .any(|event| event.is_error()));
    assert_eq!(fixture.screenshots().len(), 1);
}

#[test]
fn wall_clock_capture_delay_is_independent_of_animation_clamping() {
    let mut fixture = Fixture::new();
    fixture
        .app
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_millis(250));
    fixture
        .app
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(5)));
    fixture.app.update();
    fixture.app.update();
    assert!(fixture.app.world().resource::<Time<Real>>().elapsed_secs() >= 5.0);
    assert!(
        fixture
            .app
            .world()
            .resource::<Time<Virtual>>()
            .elapsed_secs()
            < 3.0
    );
    assert_eq!(fixture.screenshots().len(), 1);
}

#[test]
fn wall_clock_quit_deadline_is_independent_of_animation_clamping() {
    let mut fixture = Fixture::new();
    fixture
        .app
        .world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_millis(250));
    fixture
        .app
        .world_mut()
        .resource_mut::<Diagnostics>()
        .quit_after = Some(1.0);
    fixture
        .app
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(5)));
    fixture.app.update();
    fixture.app.update();
    assert!(fixture
        .app
        .world_mut()
        .resource_mut::<Events<AppExit>>()
        .drain()
        .any(|event| event.is_success()));
    assert!(fixture.screenshots().is_empty());
}
