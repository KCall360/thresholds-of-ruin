//! Offline script runner, sharing procedural composition and presentation animation.
pub(crate) mod live;
mod schema;
#[cfg(test)]
mod tests;
use crate::{
    creatures::{self, Anim, Look, Rig, J},
    props::{PropLibrary, PropMaterials},
};
use bevy::{
    ecs::schedule::ScheduleLabel,
    prelude::*,
    render::{
        mesh::RenderMesh,
        render_asset::RenderAssets,
        render_resource::{CachedPipelineState, PipelineCache},
        renderer::RenderAdapterInfo,
        view::screenshot::{Screenshot, ScreenshotCaptured},
        Render, RenderApp, RenderSet,
    },
    time::TimeSystem,
    window::WindowResolution,
};
use schema::*;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Apply,
    Pose,
    Camera,
    Pending,
    Advance,
    Finished,
    Failed,
}
#[derive(Resource)]
struct Run {
    script: Script,
    output: PathBuf,
    tick: u32,
    next_capture: usize,
    phase: Phase,
    clock: Time<Virtual>,
    deadline: Instant,
    manifest: Value,
    error: Option<String>,
}
#[derive(Resource, Clone, Default)]
struct Readiness(Arc<Mutex<ReadyState>>);
#[derive(Default)]
struct ReadyState {
    ready: bool,
    error: Option<String>,
    adapter: Value,
    meshes: Vec<AssetId<Mesh>>,
}
#[derive(Component)]
struct ActorState {
    spec: Actor,
    velocity: Vec3,
}
#[derive(Component)]
struct ReviewCamera;

pub(crate) fn run(path: &Path, output: &Path) -> Result<(), crate::Error> {
    if std::fs::metadata(path)?.len() > 1_048_576 {
        return Err("scene script exceeds 1 MiB".into());
    }
    let bytes = std::fs::read(path)?;
    let script = Script::parse(&bytes)?;
    // A new run never overwrites another run or follows an existing output symlink.
    std::fs::create_dir(output)?;
    std::fs::write(output.join("script.json"), &bytes)?;
    let manifest = json!({"version":1,"status":"running","script":script,
        "script_fingerprint_fnv1a64":fingerprint(&bytes),
        "executable":std::env::current_exe()?.to_string_lossy(),
        "captures":[],"provenance":"local executable; source revision not supplied"});
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let viewport = script.viewport;
    let readiness = Readiness::default();
    let mut app = App::new();
    app.add_plugins(
        crate::graphics::plugins().set(WindowPlugin {
            primary_window: Some(Window {
                title: "Procedural model script review".into(),
                resolution: WindowResolution::new(viewport[0] as f32, viewport[1] as f32)
                    .with_scale_factor_override(1.0),
                ..default()
            }),
            ..default()
        }),
    )
    .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
    .insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 180.0,
        ..default()
    })
    .insert_resource(readiness.clone())
    .insert_resource(Run {
        script,
        output: output.to_path_buf(),
        tick: 0,
        next_capture: 0,
        phase: Phase::Apply,
        clock: Time::default(),
        deadline: Instant::now() + Duration::from_secs(180),
        manifest,
        error: None,
    })
    .add_systems(Startup, setup)
    .add_systems(First, clock.after(TimeSystem))
    .add_systems(
        Update,
        (
            advance_actors,
            advance_animation,
            apply,
            refresh_animation,
            creatures::snuff,
            creatures::effects,
        )
            .chain(),
    )
    .add_systems(Last, capture);
    configure_animation(&mut app);
    app.sub_app_mut(RenderApp)
        .insert_resource(readiness)
        .add_systems(Render, renderer_ready.in_set(RenderSet::Cleanup));
    let exit = app.run();
    let mut final_manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("manifest.json"))?)?;
    if exit.is_success() && final_manifest["status"] == "complete" {
        Ok(())
    } else {
        if final_manifest["status"] != "failed" {
            final_manifest["status"] = json!("failed");
            final_manifest["error"] = json!("renderer exited before every capture completed");
            std::fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&final_manifest)?,
            )?;
        }
        Err("scene script failed; see manifest.json".into())
    }
}
fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}
fn renderer_ready(
    readiness: Res<Readiness>,
    pipelines: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    adapter: Res<RenderAdapterInfo>,
) {
    let error = pipelines.pipelines().find_map(|p| match &p.state {
        CachedPipelineState::Err(e) => Some(format!("{e:?}")),
        _ => None,
    });
    let mut state = readiness.0.lock().unwrap();
    let ready = error.is_none()
        && pipelines.waiting_pipelines().next().is_none()
        && !state.meshes.is_empty()
        && state.meshes.iter().all(|id| meshes.get(*id).is_some());
    state.ready = ready;
    state.error = error;
    state.adapter = json!({"name":adapter.name,"backend":format!("{:?}",adapter.backend),"driver":adapter.driver,"driver_info":adapter.driver_info});
}
fn clock(mut run: ResMut<Run>, mut virtual_time: ResMut<Time<Virtual>>, mut time: ResMut<Time>) {
    let delta = if run.phase == Phase::Advance {
        run.tick += 1;
        run.phase = Phase::Apply;
        Duration::from_secs_f64(1.0 / run.script.hz as f64)
    } else {
        Duration::ZERO
    };
    run.clock.advance_by(delta);
    *virtual_time = run.clock;
    *time = run.clock.as_generic();
}
fn setup(
    mut commands: Commands,
    run: Res<Run>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    readiness: Res<Readiness>,
) {
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    for spec in &run.script.actors {
        let at = Vec3::from_array(spec.position);
        let mut anim = Anim::new(spec.seed, at);
        anim.yaw = spec.yaw.to_radians();
        anim.target_yaw = Some(anim.yaw);
        let entity = commands
            .spawn((
                ActorState {
                    spec: spec.clone(),
                    velocity: Vec3::ZERO,
                },
                anim,
                Transform::from_translation(at),
                Visibility::default(),
            ))
            .id();
        let rig = creatures::spawn(
            &mut commands,
            &mut library,
            &mut meshes,
            entity,
            &spec.look().expect("validated actor"),
        );
        commands.entity(entity).insert(rig);
    }
    commands.insert_resource(library);
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(100.0, 100.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.21, 0.23),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.0, 5.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        ReviewCamera,
        Camera3d::default(),
        crate::graphics::camera_antialiasing(),
        Transform::from_xyz(0.0, 1.5, 4.0).looking_at(Vec3::Y * 0.5, Vec3::Y),
    ));
    readiness.0.lock().unwrap().meshes = meshes.ids().collect();
}
fn advance_actors(run: Res<Run>, mut actors: Query<(&ActorState, &mut Transform)>) {
    if run.phase == Phase::Apply && run.tick > 0 {
        for (actor, mut transform) in &mut actors {
            transform.translation += actor.velocity / run.script.hz as f32;
        }
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AdvanceReview;
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct RefreshReview;

/// Shared CPU pose/cloth schedule used by review playback and its benchmark.
pub(crate) fn animation_schedule() -> Schedule {
    let mut schedule = Schedule::new(AdvanceReview);
    schedule.add_systems(
        (
            creatures::animate,
            bevy::transform::systems::mark_dirty_trees,
            bevy::transform::systems::sync_simple_transforms,
            bevy::transform::systems::propagate_parent_transforms,
            creatures::soften,
        )
            .chain(),
    );
    schedule
}

fn configure_animation(app: &mut App) {
    app.add_schedule(animation_schedule());
    app.init_schedule(RefreshReview).add_systems(
        RefreshReview,
        (
            creatures::refresh_pose,
            bevy::transform::systems::mark_dirty_trees,
            bevy::transform::systems::sync_simple_transforms,
            bevy::transform::systems::propagate_parent_transforms,
        )
            .chain(),
    );
}
fn advance_animation(world: &mut World) {
    world.run_schedule(AdvanceReview);
}
fn refresh_animation(world: &mut World) {
    world.run_schedule(RefreshReview);
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    mut run: ResMut<Run>,
    mut actors: Query<(Entity, &mut ActorState, &mut Anim, &mut Transform, &Rig)>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    readiness: Res<Readiness>,
) {
    if run.phase != Phase::Apply {
        return;
    }
    for (entity, mut actor, mut anim, mut transform, rig) in &mut actors {
        let mut rebuild = false;
        for event in run.script.events.iter().filter(|e| e.tick == run.tick) {
            if actor.spec.id != event.actor {
                continue;
            }
            let ActorState { spec, velocity } = &mut *actor;
            rebuild |= apply_operation(spec, velocity, &mut anim, &mut transform, &event.operation);
            if matches!(
                event.operation,
                Operation::Action { .. } | Operation::Teleport { .. }
            ) {
                commands.entity(entity).insert(creatures::PoseRefresh);
            }
        }
        if rebuild {
            readiness.0.lock().unwrap().ready = false;
            if let Some(lift) = rig.joint(J::Lift) {
                commands.entity(lift).despawn();
            }
            let look: Look = actor.spec.look().expect("validated composition event");
            let rig = creatures::spawn(&mut commands, &mut library, &mut meshes, entity, &look);
            commands
                .entity(entity)
                .insert((rig, creatures::PoseRefresh));
        }
    }
    run.phase = Phase::Pose;
    readiness.0.lock().unwrap().meshes = meshes.ids().collect();
}
// Shared event semantics; rendering policies decide how to realize changed geometry.
fn apply_operation(
    spec: &mut Actor,
    velocity: &mut Vec3,
    anim: &mut Anim,
    transform: &mut Transform,
    operation: &Operation,
) -> bool {
    match operation {
        Operation::Action { name } => {
            if let Some(action) = schema::action(name).expect("validated action") {
                anim.act(action);
            } else {
                anim.action = None;
            }
        }
        Operation::Velocity { value } => *velocity = Vec3::from_array(*value),
        Operation::Teleport { position, yaw } => {
            transform.translation = Vec3::from_array(*position);
            *anim = Anim::new(spec.seed, transform.translation);
            anim.yaw = yaw.to_radians();
            anim.target_yaw = Some(anim.yaw);
        }
        Operation::Dress { outfit } => {
            if spec.outfit != *outfit {
                spec.outfit = outfit.clone();
                return true;
            }
        }
        Operation::Equipment { items } => {
            if spec.equipment != *items {
                spec.equipment = items.clone();
                return true;
            }
        }
    }
    false
}
fn save_manifest(run: &Run) -> std::io::Result<()> {
    let temp = run.output.join("manifest.pending.json");
    std::fs::write(&temp, serde_json::to_vec_pretty(&run.manifest)?)?;
    // Windows rename does not replace files. The manifest remains recoverable
    // as manifest.pending.json if replacement fails.
    let target = run.output.join("manifest.json");
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    std::fs::rename(temp, target)
}
#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    mut run: ResMut<Run>,
    readiness: Res<Readiness>,
    mut camera: Query<(&mut Transform, &mut Projection), With<ReviewCamera>>,
    actors: Query<(&ActorState, &Anim, &Transform, &Rig, &Children), Without<ReviewCamera>>,
    joints: Query<&GlobalTransform>,
    mut exit: EventWriter<AppExit>,
) {
    if run.phase == Phase::Finished {
        return;
    }
    let (ready, render_error, adapter) = {
        let state = readiness.0.lock().unwrap();
        (state.ready, state.error.clone(), state.adapter.clone())
    };
    if let Some(error) = render_error {
        run.error = Some(error);
        run.phase = Phase::Failed;
    }
    if Instant::now() > run.deadline {
        run.error = Some("scene or GPU readback timed out".into());
        run.phase = Phase::Failed;
    }
    if run.phase == Phase::Failed {
        run.manifest["status"] = json!("failed");
        run.manifest["error"] = json!(run.error);
        let _ = save_manifest(&run);
        run.phase = Phase::Finished;
        exit.write(AppExit::error());
        return;
    }
    if !ready || run.phase == Phase::Pending {
        return;
    }
    if run.next_capture == run.script.captures.len() {
        run.manifest["status"] = json!("complete");
        match save_manifest(&run) {
            Ok(()) => {
                run.phase = Phase::Finished;
                exit.write(AppExit::Success);
            }
            Err(e) => {
                run.error = Some(e.to_string());
                run.phase = Phase::Failed;
            }
        }
        return;
    }
    let shot = run.script.captures[run.next_capture].clone();
    if run.phase == Phase::Pose {
        if shot.tick > run.tick {
            run.phase = Phase::Advance;
            return;
        }
        let spec = run
            .script
            .cameras
            .iter()
            .find(|c| c.id == shot.camera)
            .unwrap();
        let Ok((mut transform, mut projection)) = camera.single_mut() else {
            return;
        };
        *transform = Transform::from_translation(Vec3::from_array(spec.position))
            .looking_at(Vec3::from_array(spec.target), Vec3::Y);
        *projection = Projection::Perspective(PerspectiveProjection {
            fov: spec.fov_degrees.to_radians(),
            near: 0.01,
            far: 2000.0,
            ..default()
        });
        // Camera changes in Last propagate next frame, while the pose is held.
        run.phase = Phase::Camera;
        return;
    }
    if run.phase != Phase::Camera {
        return;
    }
    let mut states:Vec<Value>=actors.iter().map(|(actor,anim,transform,rig,children)|json!({
        "id":actor.spec.id,"composition":actor.spec,"resolved_key":actor.spec.look().unwrap().key,
        "position":transform.translation.to_array(),"yaw":anim.yaw,"root_children":children.len(),
        "joints": ([J::Hips,J::Torso,J::Head,J::HandL,J::HandR,J::LegL,J::LegR,J::ShinL,J::ShinR,J::FootL,J::FootR].into_iter().filter_map(|joint| {
            let entity=rig.joint(joint)?;
            let matrix=joints.get(entity).ok()?.compute_matrix().to_cols_array();
            Some((format!("{joint:?}"),json!(matrix)))
        }).collect::<serde_json::Map<String,Value>>()),
        "action":anim.action.map(|(a,t)|json!({"name":format!("{a:?}"),"phase":t}))
    })).collect();
    states.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let camera_spec = run
        .script
        .cameras
        .iter()
        .find(|c| c.id == shot.camera)
        .unwrap()
        .clone();
    let report = json!({"name":shot.name,"tick":run.tick,"clock_ns":run.clock.elapsed().as_nanos().to_string(),
        "camera":camera_spec,"actors":states,"renderer":adapter});
    let output = run.output.clone();
    run.phase = Phase::Pending;
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<ScreenshotCaptured>, mut run: ResMut<Run>| {
            let result = (|| -> Result<Value, crate::Error> {
                let image = trigger.event().0.clone().try_into_dynamic()?;
                let temporary = output.join(format!("{}.pending.png", shot.name));
                let path = output.join(format!("{}.png", shot.name));
                image.save(&temporary)?;
                let bytes = std::fs::read(&temporary)?;
                std::fs::rename(&temporary, &path)?;
                let mut entry = report.clone();
                entry["width"] = image.width().into();
                entry["height"] = image.height().into();
                entry["image_fingerprint_fnv1a64"] = fingerprint(&bytes).into();
                Ok(entry)
            })();
            match result {
                Ok(entry) => {
                    run.manifest["captures"].as_array_mut().unwrap().push(entry);
                    run.next_capture += 1;
                    run.phase = Phase::Pose;
                    if let Err(e) = save_manifest(&run) {
                        run.error = Some(e.to_string());
                        run.phase = Phase::Failed;
                    }
                }
                Err(e) => {
                    run.error = Some(e.to_string());
                    run.phase = Phase::Failed;
                }
            }
        },
    );
}

pub(crate) fn cli(arguments: &[String]) -> Result<(), crate::Error> {
    let mut script = None;
    let mut output = None;
    let mut args = arguments.iter();
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--scene-script" => &mut script,
            "--capture" => &mut output,
            _ => return Err(format!("{arg} cannot be combined with --scene-script").into()),
        };
        if slot.is_some() {
            return Err(format!("duplicate {arg}").into());
        }
        *slot = Some(PathBuf::from(
            args.next()
                .ok_or_else(|| format!("missing value for {arg}"))?,
        ));
    }
    run(
        &script.ok_or("--scene-script is required")?,
        &output.ok_or("--scene-script requires --capture with a new output directory")?,
    )
}
