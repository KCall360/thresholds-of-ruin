//! `--frame-stats SECS`: live frame-time measurement. Once the world first
//! appears it waits until the scene has run SETTLE_SECS without a stall,
//! noting the longest frame meanwhile (creature construction stalls show
//! there), then times every frame for SECS and prints one JSON report of
//! frame times, animation work, per-pass render times and what was drawn.
use super::*;
use bevy::render::{mesh::skinning::SkinnedMesh, view::VisibleEntities};

/// Seconds without a stall before frames are timed.
pub(crate) const SETTLE_SECS: f32 = 5.0;
/// A frame this long restarts the settling wait.
pub(crate) const STALL_MS: f32 = 250.0;

/// Frame timing for one run.
#[derive(Resource)]
pub(crate) struct FrameStats {
    measure_secs: f32,
    /// When the scene last stalled (or first appeared), in real seconds.
    settling_since: Option<f32>,
    /// When timing began.
    timing_since: Option<f32>,
    worst_settling_ms: f32,
    frames_ms: Vec<f32>,
    frustum: FrustumWindow,
    /// Animation work summed over the timed frames: pose/joints, robe
    /// fitting, drapes/segments and hair/contact fitting, then hair's own
    /// control, deformation, constraint and mesh-write stages.
    motion_ms: [f64; 4],
    hair_ms: [f64; 4],
    rigs: usize,
}

impl FrameStats {
    pub(crate) fn new(measure_secs: f32) -> Self {
        Self {
            measure_secs,
            settling_since: None,
            timing_since: None,
            worst_settling_ms: 0.0,
            frames_ms: Vec::new(),
            frustum: FrustumWindow::default(),
            motion_ms: [0.0; 4],
            hair_ms: [0.0; 4],
            rigs: 0,
        }
    }
}

/// Mean, percentiles and extremes of a set of frame times in milliseconds.
#[derive(Debug, PartialEq)]
pub(crate) struct Summary {
    pub frames: usize,
    pub mean_ms: f32,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
}

pub(crate) fn summarize(frames_ms: &[f32]) -> Option<Summary> {
    if frames_ms.is_empty() {
        return None;
    }
    let mut sorted = frames_ms.to_vec();
    sorted.sort_by(f32::total_cmp);
    // Nearest-rank percentile.
    let pick =
        |q: f32| sorted[((q * sorted.len() as f32).ceil() as usize).clamp(1, sorted.len()) - 1];
    Some(Summary {
        frames: sorted.len(),
        mean_ms: sorted.iter().sum::<f32>() / sorted.len() as f32,
        p50_ms: pick(0.5),
        p95_ms: pick(0.95),
        p99_ms: pick(0.99),
        max_ms: sorted[sorted.len() - 1],
    })
}

type ShapedBody = (
    With<Mesh3d>,
    With<creatures::BodySurface>,
    Without<picking::LooseBounds>,
);
type UnfinishedModel = Or<(
    With<creatures::loading::PendingModel>,
    With<creatures::loading::LoadFailed>,
)>;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Drawn<'w, 's> {
    unfinished: Query<'w, 's, (), UnfinishedModel>,
    bodies: Query<'w, 's, &'static ActorBody>,
    meshes: Query<'w, 's, (), With<Mesh3d>>,
    skinned: Query<'w, 's, (), With<SkinnedMesh>>,
    lamps: Query<'w, 's, &'static PointLight>,
    views: Query<
        'w,
        's,
        (&'static Camera, &'static VisibleEntities),
        crate::portrait::ExplorationCamera,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    shaped: Query<'w, 's, (), ShapedBody>,
}

impl Drawn<'_, '_> {
    /// Distinct bodies in the sole active exploration camera's frustum.
    /// Unavailable/ambiguous cameras cannot establish a count.
    fn frustum_creatures(&self) -> Option<usize> {
        let mut views = self.views.iter().filter(|(camera, _)| camera.is_active);
        let (_, visible) = views.next()?;
        if views.next().is_some() {
            return None;
        }
        let mut bodies = std::collections::HashSet::new();
        for &mesh in visible.get(std::any::TypeId::of::<Mesh3d>()) {
            if !self.shaped.contains(mesh) {
                continue;
            }
            let mut ancestor = mesh;
            loop {
                if self.bodies.contains(ancestor) {
                    bodies.insert(ancestor);
                    break;
                }
                let Ok(parent) = self.parents.get(ancestor) else {
                    break;
                };
                ancestor = parent.parent();
            }
        }
        Some(bodies.len())
    }
}

/// A count is qualified only if it was available on every timed frame.
#[derive(Default)]
struct FrustumWindow {
    frames: usize,
    unavailable: usize,
    range: Option<[usize; 2]>,
}

impl FrustumWindow {
    fn record(&mut self, count: Option<usize>) {
        self.frames += 1;
        if let Some(count) = count {
            self.range = Some(match self.range {
                Some([min, max]) => [min.min(count), max.max(count)],
                None => [count; 2],
            });
        } else {
            self.unavailable += 1;
        }
    }

    fn complete_range(&self) -> Option<[usize; 2]> {
        (self.unavailable == 0).then_some(self.range).flatten()
    }
}

pub(crate) fn frame_stats(
    time: Res<Time<Real>>,
    game: Res<Game>,
    stats: Option<ResMut<FrameStats>>,
    motion: Option<Res<creatures::MotionProfile>>,
    drawn: Drawn,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut exit: EventWriter<AppExit>,
) {
    let Some(mut stats) = stats else {
        return;
    };
    let now = time.elapsed_secs();
    let frame_ms = time.delta_secs() * 1000.0;
    if game.view.is_none() {
        return;
    }
    if !drawn.unfinished.is_empty() {
        let worst = stats.worst_settling_ms.max(frame_ms);
        *stats = FrameStats::new(stats.measure_secs);
        stats.worst_settling_ms = worst;
        return;
    }
    if stats.timing_since.is_none() {
        let since = *stats.settling_since.get_or_insert(now);
        stats.worst_settling_ms = stats.worst_settling_ms.max(frame_ms);
        if frame_ms > STALL_MS {
            stats.settling_since = Some(now);
        } else if now >= since + SETTLE_SECS {
            stats.timing_since = Some(now);
        }
        return;
    }
    let timing_since = stats.timing_since.expect("timing began");
    if now < timing_since + stats.measure_secs {
        stats.frames_ms.push(frame_ms);
        stats.frustum.record(drawn.frustum_creatures());
        if let Some(motion) = motion {
            for (sum, phase) in stats.motion_ms.iter_mut().zip(motion.phases) {
                *sum += phase.as_secs_f64() * 1000.0;
            }
            for (sum, phase) in stats.hair_ms.iter_mut().zip(motion.hair_phases) {
                *sum += phase.as_secs_f64() * 1000.0;
            }
            stats.rigs = motion.rigs;
        }
        return;
    }
    let per_frame = |sums: [f64; 4]| sums.map(|sum| sum / stats.frames_ms.len().max(1) as f64);
    let Some(summary) = summarize(&stats.frames_ms) else {
        return;
    };
    println!(
        "{}",
        serde_json::json!({
            "type": "3d_frame_stats",
            "frames": summary.frames,
            "mean_ms": summary.mean_ms,
            "fps": 1000.0 / summary.mean_ms,
            "p50_ms": summary.p50_ms,
            "p95_ms": summary.p95_ms,
            "p99_ms": summary.p99_ms,
            "max_ms": summary.max_ms,
            "worst_settling_ms": stats.worst_settling_ms,
            "creatures": drawn.bodies.iter().count(),
            "frustum_creatures": {
                "min": stats.frustum.complete_range().map(|range| range[0]),
                "max": stats.frustum.complete_range().map(|range| range[1]),
                "sampled_frames": stats.frustum.frames,
                "unavailable_frames": stats.frustum.unavailable,
            },
            "mesh_entities": drawn.meshes.iter().count(),
            "skinned_meshes": drawn.skinned.iter().count(),
            "point_lights": drawn.lamps.iter().count(),
            "shadowed_lights": drawn.lamps.iter().filter(|lamp| lamp.shadows_enabled).count(),
            "posed_rigs": stats.rigs,
            "motion_ms_per_frame": {
                "pose": per_frame(stats.motion_ms)[0], "robes": per_frame(stats.motion_ms)[1],
                "drapes": per_frame(stats.motion_ms)[2], "hair": per_frame(stats.motion_ms)[3],
            },
            "hair_ms_per_frame": per_frame(stats.hair_ms),
            "passes_ms": passes(&diagnostics),
        })
    );
    exit.write(AppExit::Success);
}

/// Recent average GPU and CPU time of each render pass, from Bevy's render
/// diagnostics; empty where the device cannot time passes.
fn passes(
    diagnostics: &bevy::diagnostic::DiagnosticsStore,
) -> serde_json::Map<String, serde_json::Value> {
    let mut passes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| {
            let path = d.path().as_str();
            let pass = path.strip_prefix("render/")?;
            Some((pass.to_owned(), d.average()?))
        })
        .collect();
    passes.sort_by(|a, b| a.0.cmp(&b.0));
    passes
        .into_iter()
        .map(|(pass, ms)| (pass, serde_json::json!((ms * 100.0).round() / 100.0)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Observed(Option<usize>);

    fn observe(drawn: Drawn, mut observed: ResMut<Observed>) {
        observed.0 = drawn.frustum_creatures();
    }

    fn actor(world: &mut World) -> Entity {
        let label = world.spawn_empty().id();
        let ring = world.spawn_empty().id();
        world
            .spawn(ActorBody {
                appearance_seed: None,
                appearance_key: "fixture".into(),
                model: CreatureModel::Placeholder,
                wearing: Default::default(),
                hostile: false,
                goal: Vec3::ZERO,
                label,
                ring,
                name: String::new(),
                height: 1.0,
            })
            .id()
    }

    fn camera(world: &mut World, meshes: &[Entity], active: bool) -> Entity {
        let mut visible = VisibleEntities::default();
        for &entity in meshes {
            visible.push(entity, std::any::TypeId::of::<Mesh3d>());
        }
        world
            .spawn((
                Camera3d::default(),
                Camera {
                    is_active: active,
                    ..default()
                },
                visible,
            ))
            .id()
    }

    #[test]
    fn camera_body_count_deduplicates_parts_and_excludes_offscreen_and_loose_bounds() {
        let mut app = App::new();
        app.init_resource::<Observed>().add_systems(Update, observe);
        let world = app.world_mut();
        let a = actor(world);
        let b = actor(world);
        let offscreen = actor(world);
        let loose_only = actor(world);
        let joint = world.spawn(ChildOf(a)).id();
        let part = world
            .spawn((Mesh3d::default(), creatures::BodySurface, ChildOf(joint)))
            .id();
        let other_part = world
            .spawn((Mesh3d::default(), creatures::BodySurface, ChildOf(a)))
            .id();
        world
            .entity_mut(b)
            .insert((Mesh3d::default(), creatures::BodySurface));
        let loose = world
            .spawn((
                Mesh3d::default(),
                creatures::BodySurface,
                ChildOf(loose_only),
                picking::LooseBounds,
            ))
            .id();
        let ring = world
            .spawn((Mesh3d::default(), ChildOf(loose_only), HostileRing))
            .id();
        let prop = world
            .spawn((Mesh3d::default(), creatures::BodySurface))
            .id();
        let held_prop = world.spawn((Mesh3d::default(), ChildOf(loose_only))).id();
        let label = world.spawn(ChildOf(loose_only)).id();
        let offscreen_part = world.spawn((Mesh3d::default(), ChildOf(offscreen))).id();
        // Global visibility can come from another camera or a shadow view.
        world
            .entity_mut(offscreen_part)
            .get_mut::<ViewVisibility>()
            .unwrap()
            .set();
        camera(
            world,
            &[part, other_part, b, loose, ring, prop, held_prop, label],
            true,
        );
        camera(world, &[offscreen_part], false);
        assert_eq!(world.query::<&ActorBody>().iter(world).count(), 4);
        app.update();
        assert_eq!(app.world().resource::<Observed>().0, Some(2));
    }

    #[test]
    fn portrait_camera_does_not_change_the_exploration_frustum_count() {
        let mut app = App::new();
        app.init_resource::<Observed>().add_systems(Update, observe);
        let body = actor(app.world_mut());
        let part = app
            .world_mut()
            .spawn((Mesh3d::default(), creatures::BodySurface, ChildOf(body)))
            .id();
        camera(app.world_mut(), &[part], true);
        app.update();
        assert_eq!(
            app.world().resource::<Observed>().0,
            Some(1),
            "positive world-camera fixture"
        );
        let portrait = camera(app.world_mut(), &[], true);
        app.world_mut()
            .entity_mut(portrait)
            .insert(crate::portrait::PortraitCamera);
        app.update();
        assert_eq!(
            app.world().resource::<Observed>().0,
            Some(1),
            "the portrait is not another exploration view"
        );
    }

    #[test]
    fn camera_body_count_distinguishes_zero_missing_and_ambiguous_views() {
        let mut app = App::new();
        app.init_resource::<Observed>().add_systems(Update, observe);
        app.update();
        assert_eq!(app.world().resource::<Observed>().0, None);
        let first = camera(app.world_mut(), &[], false);
        app.update();
        assert_eq!(app.world().resource::<Observed>().0, None);
        app.world_mut()
            .entity_mut(first)
            .get_mut::<Camera>()
            .unwrap()
            .is_active = true;
        app.update();
        assert_eq!(app.world().resource::<Observed>().0, Some(0));
        camera(app.world_mut(), &[], true);
        app.update();
        assert_eq!(app.world().resource::<Observed>().0, None);
    }

    #[test]
    fn camera_body_count_handles_one_hundred_composed_actors() {
        let mut app = App::new();
        app.init_resource::<Observed>().add_systems(Update, observe);
        let world = app.world_mut();
        let mut visible = Vec::new();
        for _ in 0..101 {
            let body = actor(world);
            let joint = world.spawn(ChildOf(body)).id();
            for _ in 0..3 {
                visible.push(
                    world
                        .spawn((Mesh3d::default(), creatures::BodySurface, ChildOf(joint)))
                        .id(),
                );
            }
        }
        camera(world, &visible, true);
        app.update();
        assert_eq!(app.world().resource::<Observed>().0, Some(101));
    }

    #[test]
    fn camera_window_retains_the_minimum_and_rejects_incomplete_samples() {
        let mut window = FrustumWindow::default();
        assert_eq!(window.complete_range(), None);
        for count in [101, 103, 99, 102] {
            window.record(Some(count));
        }
        assert_eq!(window.complete_range(), Some([99, 103]));
        assert_eq!(window.frames, 4);
        window.record(None);
        window.record(Some(101));
        assert_eq!(window.complete_range(), None);
        assert_eq!((window.frames, window.unavailable), (6, 1));
        let mut empty = FrustumWindow::default();
        empty.record(Some(0));
        assert_eq!(empty.complete_range(), Some([0, 0]));
    }

    #[test]
    fn timing_waits_for_models_and_restarts_if_loading_or_failure_interrupts_it() {
        use bevy::{ecs::system::SystemState, time::TimeUpdateStrategy};
        use std::time::Duration;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_event::<AppExit>()
            .init_resource::<Game>()
            .init_resource::<bevy::diagnostic::DiagnosticsStore>()
            .insert_resource(FrameStats::new(100.0))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .add_systems(Update, frame_stats);
        app.world_mut().resource_mut::<Game>().view = Some(crate::test_support::view(0));
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let parent = app.world_mut().spawn_empty().id();
        let mut state = SystemState::<Commands>::new(app.world_mut());
        creatures::loading::request(
            &mut state.get_mut(app.world_mut()),
            &mut library,
            &mut meshes,
            parent,
            &Look::of(CreatureModel::Human, 17),
        );
        state.apply(app.world_mut());
        for _ in 0..120 {
            app.update();
        }
        assert!(
            app.world().resource::<FrameStats>().timing_since.is_none(),
            "eggs cannot qualify steady-state model timing"
        );
        app.world_mut().entity_mut(parent).despawn();
        for _ in 0..90 {
            app.update();
        }
        assert!(
            !app.world().resource::<FrameStats>().frames_ms.is_empty(),
            "ready-scene positive control must reach timing"
        );
        app.world_mut().spawn(creatures::loading::LoadFailed);
        app.update();
        let stats = app.world().resource::<FrameStats>();
        assert!(stats.timing_since.is_none());
        assert!(
            stats.frames_ms.is_empty(),
            "a failed model cannot leave a qualified timing window"
        );
    }

    #[test]
    fn summaries_use_nearest_rank_percentiles() {
        let frames: Vec<f32> = (1..=100).map(|ms| ms as f32).collect();
        let summary = summarize(&frames).unwrap();
        assert_eq!(summary.frames, 100);
        assert_eq!(summary.mean_ms, 50.5);
        assert_eq!(
            (
                summary.p50_ms,
                summary.p95_ms,
                summary.p99_ms,
                summary.max_ms
            ),
            (50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!(summarize(&[7.0]).unwrap().p95_ms, 7.0);
        assert!(summarize(&[]).is_none());
    }
}
