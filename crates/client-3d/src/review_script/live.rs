//! Wall-clock presentation in one window; deliberately independent of capture barriers.
use super::*;
use bevy::window::PresentMode;
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Write;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Playlist {
    version: u32,
    scenes: Vec<Chapter>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Chapter {
    script: PathBuf,
    shots: Vec<Shot>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Shot {
    camera: String,
    hold_seconds: f64,
    transition_seconds: f64,
}
struct Scene {
    script: Script,
    shots: Vec<Shot>,
    duration: f64,
}
impl Scene {
    fn camera(&self, at: f64, next: &Scene) -> (Transform, f32) {
        let mut cursor = 0.0;
        for (i, shot) in self.shots.iter().enumerate() {
            let end = cursor + shot.hold_seconds + shot.transition_seconds;
            if at <= end || i + 1 == self.shots.len() {
                let from = self
                    .script
                    .cameras
                    .iter()
                    .find(|c| c.id == shot.camera)
                    .unwrap();
                let to = if let Some(s) = self.shots.get(i + 1) {
                    self.script
                        .cameras
                        .iter()
                        .find(|c| c.id == s.camera)
                        .unwrap()
                } else {
                    next.script
                        .cameras
                        .iter()
                        .find(|c| c.id == next.shots[0].camera)
                        .unwrap()
                };
                let t = ((at - cursor - shot.hold_seconds) / shot.transition_seconds)
                    .clamp(0.0, 1.0) as f32;
                let t = t * t * (3.0 - 2.0 * t);
                let pose = |c: &CameraSpec| {
                    Transform::from_translation(Vec3::from_array(c.position))
                        .looking_at(Vec3::from_array(c.target), Vec3::Y)
                };
                let a = pose(from);
                let b = pose(to);
                return (
                    Transform {
                        translation: a.translation.lerp(b.translation, t),
                        rotation: a.rotation.slerp(b.rotation, t),
                        ..default()
                    },
                    from.fov_degrees + (to.fov_degrees - from.fov_degrees) * t,
                );
            }
            cursor = end;
        }
        unreachable!()
    }
}
// Keep camera interpolation plain; no keyframe can request a zero-length transition.
fn read_playlist(path: &Path) -> Result<Vec<Scene>, crate::Error> {
    fn read(path: &Path) -> Result<Vec<u8>, crate::Error> {
        if std::fs::metadata(path)?.len() > 1_048_576 {
            return Err("live input exceeds 1 MiB".into());
        }
        Ok(std::fs::read(path)?)
    }
    let playlist: Playlist = serde_json::from_slice(&read(path)?)?;
    if playlist.version != 1 || playlist.scenes.is_empty() || playlist.scenes.len() > 16 {
        return Err("live playlist requires version 1 and 1..16 scenes".into());
    }
    let mut result = Vec::new();
    for chapter in playlist.scenes {
        let script = Script::parse_live(&read(
            &path.parent().unwrap_or(Path::new(".")).join(chapter.script),
        )?)?;
        if chapter.shots.is_empty() || chapter.shots.len() > 64 {
            return Err("live scene needs 1..64 shots".into());
        }
        let mut duration = 0.0;
        for shot in &chapter.shots {
            if !shot.hold_seconds.is_finite()
                || !shot.transition_seconds.is_finite()
                || shot.hold_seconds < 0.01
                || shot.transition_seconds < 0.01
                || !script.cameras.iter().any(|c| c.id == shot.camera)
            {
                return Err("invalid live camera hold, transition or reference".into());
            }
            duration += shot.hold_seconds + shot.transition_seconds;
        }
        if duration > 600.0
            || script
                .events
                .iter()
                .any(|e| f64::from(e.tick) / f64::from(script.hz) >= duration)
        {
            return Err("live scene exceeds 600 seconds or ends before its events".into());
        }
        result.push(Scene {
            script,
            shots: chapter.shots,
            duration,
        });
    }
    if result
        .iter()
        .any(|s| s.script.viewport != result[0].script.viewport)
    {
        return Err("live scenes must share one viewport".into());
    }
    Ok(result)
}

/// Bounded histogram, with exact count/sum/max; percentiles are upper 1-ms bins.
#[derive(Clone)]
struct Timing {
    bins: [u64; 1001],
    count: u64,
    sum: f64,
    max: f64,
    over_33: u64,
    over_50: u64,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            bins: [0; 1001],
            count: 0,
            sum: 0.0,
            max: 0.0,
            over_33: 0,
            over_50: 0,
        }
    }
}
impl Timing {
    fn add(&mut self, seconds: f64) {
        let ms = seconds * 1000.0;
        self.bins[(ms.ceil() as usize).min(1000)] += 1;
        self.count += 1;
        self.sum += ms;
        self.max = self.max.max(ms);
        self.over_33 += u64::from(ms > 33.3334);
        self.over_50 += u64::from(ms > 50.0);
    }
    fn json(&self) -> Value {
        let percentile = |fraction: f64| {
            let target = (self.count as f64 * fraction).ceil() as u64;
            let mut sum = 0;
            for (i, n) in self.bins.iter().enumerate() {
                sum += n;
                if sum >= target {
                    return if i == 1000 {
                        self.max.ceil() as usize
                    } else {
                        i
                    };
                }
            }
            1000
        };
        json!({"count":self.count,"mean_ms":if self.count>0 {self.sum/self.count as f64}else{0.0},
            "p50_upper_ms":percentile(0.5),"p95_upper_ms":percentile(0.95),"max_ms":self.max,
            "over_33ms":self.over_33,"over_50ms":self.over_50,"overflow_bin_1000ms":self.bins[1000]})
    }
}
#[derive(Default)]
struct RenderTiming {
    last: Option<Instant>,
    intervals: Timing,
    active: bool,
}
#[derive(Resource, Clone, Default)]
struct RenderMeter(Arc<Mutex<RenderTiming>>);
fn render_tick(meter: Res<RenderMeter>) {
    let mut meter = meter.0.lock().unwrap();
    if !meter.active {
        meter.last = None;
        return;
    }
    let now = Instant::now();
    if let Some(last) = meter.last {
        meter.intervals.add(now.duration_since(last).as_secs_f64());
    }
    meter.last = Some(now);
}
struct VariantInstance {
    entity: Entity,
    spec: Actor,
    rig: Option<Rig>,
    rest: Vec<(Entity, Transform, Option<crate::props::SoftBone>)>,
}
struct Slot {
    scene: usize,
    id: String,
    variants: Vec<VariantInstance>,
    current: usize,
    velocity: Vec3,
}
#[derive(Resource)]
struct Live {
    scenes: Vec<Scene>,
    slots: Vec<Slot>,
    scene: usize,
    local: f64,
    cycle: u32,
    cycles: u32,
    event: usize,
    started: bool,
    status: String,
    started_at: Instant,
    clock: Time<Virtual>,
    elapsed: f64,
    discarded: f64,
    steps: u64,
    builds: usize,
    frames: Timing,
    animation: Timing,
    entries: Vec<Value>,
    windows: HashSet<Entity>,
    report: Arc<Mutex<Value>>,
    last_report: f64,
    proof: Option<PathBuf>,
    proof_requested: bool,
    proof_deadline: Option<Instant>,
    proof_frames: u32,
    error: Option<String>,
}

fn compositions(scene: &Scene, actor: &Actor) -> Result<Vec<Actor>, crate::Error> {
    let mut current = actor.clone();
    let mut result = vec![current.clone()];
    for event in scene.script.events.iter().filter(|e| e.actor == actor.id) {
        match &event.operation {
            Operation::Dress { outfit } => current.outfit = outfit.clone(),
            Operation::Equipment { items } => current.equipment = items.clone(),
            _ => continue,
        }
        let key = current.look()?.key;
        if !result.iter().any(|a| a.look().unwrap().key == key) {
            result.push(current.clone());
        }
    }
    Ok(result)
}
fn setup_live(world: &mut World) {
    world.resource_scope(|world, mut live: Mut<Live>| {
        let mut materials = world.remove_resource::<Assets<StandardMaterial>>().unwrap();
        let mut meshes = world.remove_resource::<Assets<Mesh>>().unwrap();
        let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
        let mut slots = Vec::new();
        for (scene_index, scene) in live.scenes.iter().enumerate() {
            for actor in &scene.script.actors {
                let mut variants = Vec::new();
                for spec in compositions(scene, actor).expect("validated live compositions") {
                    let position = Vec3::from_array(spec.position);
                    let entity = world
                        .spawn((
                            Anim::new(spec.seed, position),
                            Transform::from_translation(position),
                            Visibility::Hidden,
                        ))
                        .id();
                    let mut queue = bevy::ecs::world::CommandQueue::default();
                    let rig = creatures::spawn(
                        &mut Commands::new(&mut queue, world),
                        &mut library,
                        &mut meshes,
                        entity,
                        &spec.look().unwrap(),
                    );
                    queue.apply(world);
                    let mut pending = vec![entity];
                    let mut rest = Vec::new();
                    while let Some(e) = pending.pop() {
                        if let Some(children) = world.get::<Children>(e) {
                            pending.extend(children.iter());
                        }
                        if let Some(t) = world.get::<Transform>(e) {
                            rest.push((e, *t, world.get::<crate::props::SoftBone>(e).copied()));
                        }
                    }
                    variants.push(VariantInstance {
                        entity,
                        spec,
                        rig: Some(rig),
                        rest,
                    });
                }
                slots.push(Slot {
                    scene: scene_index,
                    id: actor.id.clone(),
                    variants,
                    current: 0,
                    velocity: Vec3::ZERO,
                });
            }
        }
        live.slots = slots;
        live.builds = live.slots.iter().map(|s| s.variants.len()).sum();
        world.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(100.0, 100.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.21, 0.23),
                perceptual_roughness: 0.9,
                ..default()
            })),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ));
        world.spawn((
            DirectionalLight {
                illuminance: 8000.0,
                shadows_enabled: true,
                ..default()
            },
            Transform::from_xyz(3.0, 5.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        world.spawn((
            ReviewCamera,
            Camera3d::default(),
            crate::graphics::camera_antialiasing(),
            Transform::default(),
        ));
        world.resource::<Readiness>().0.lock().unwrap().meshes = meshes.ids().collect();
        world.insert_resource(meshes);
        world.insert_resource(materials);
        world.insert_resource(library);
        enter_scene(world, &mut live);
        set_camera(world, &live);
    });
}
fn show(world: &mut World, variant: &mut VariantInstance) {
    for (entity, transform, bone) in &variant.rest {
        world.entity_mut(*entity).insert(*transform);
        if let Some(bone) = bone {
            world.entity_mut(*entity).insert(*bone);
        }
    }
    let mut rig = variant.rig.take().expect("inactive variant owns rig");
    rig.reset_presentation();
    let mut anim = Anim::new(variant.spec.seed, Vec3::from_array(variant.spec.position));
    anim.yaw = variant.spec.yaw.to_radians();
    anim.target_yaw = Some(anim.yaw);
    world
        .entity_mut(variant.entity)
        .insert((rig, anim, Visibility::Visible));
}
fn enter_scene(world: &mut World, live: &mut Live) {
    for slot in &mut live.slots {
        for variant in &mut slot.variants {
            if let Some(rig) = world.entity_mut(variant.entity).take::<Rig>() {
                variant.rig = Some(rig);
            }
            world.entity_mut(variant.entity).insert(Visibility::Hidden);
        }
        slot.current = 0;
        slot.velocity = Vec3::ZERO;
        if slot.scene == live.scene {
            show(world, &mut slot.variants[0]);
        }
    }
    live.local = 0.0;
    live.event = 0;
    if live.entries.len() < 128 {
        let positions: Vec<_> = live
            .slots
            .iter()
            .filter(|s| s.scene == live.scene)
            .map(|s| {
                world
                    .get::<Transform>(s.variants[0].entity)
                    .unwrap()
                    .translation
                    .to_array()
            })
            .collect();
        live.entries
            .push(json!({"scene":live.scene,"cycle":live.cycle,"positions":positions}));
    }
}
fn apply_event(world: &mut World, live: &mut Live, event: &schema::Event) {
    let slot = live
        .slots
        .iter_mut()
        .find(|s| s.scene == live.scene && s.id == event.actor)
        .unwrap();
    let entity = slot.variants[slot.current].entity;
    let mut spec = slot.variants[slot.current].spec.clone();
    let changed = {
        let mut query = world.query::<(&mut Anim, &mut Transform)>();
        let (mut anim, mut transform) = query.get_mut(world, entity).unwrap();
        apply_operation(
            &mut spec,
            &mut slot.velocity,
            &mut anim,
            &mut transform,
            &event.operation,
        )
    };
    if changed {
        let key = spec.look().unwrap().key;
        let next = slot
            .variants
            .iter()
            .position(|v| v.spec.look().unwrap().key == key)
            .unwrap();
        if next == slot.current {
            return;
        }
        let transform = *world.get::<Transform>(entity).unwrap();
        let anim = world.entity_mut(entity).take::<Anim>().unwrap();
        slot.variants[slot.current].rig = world.entity_mut(entity).take::<Rig>();
        world.entity_mut(entity).insert(Visibility::Hidden);
        slot.current = next;
        show(world, &mut slot.variants[next]);
        world
            .entity_mut(slot.variants[next].entity)
            .insert((anim, transform));
    }
    if changed
        || matches!(
            event.operation,
            Operation::Action { .. } | Operation::Teleport { .. }
        )
    {
        world
            .entity_mut(slot.variants[slot.current].entity)
            .insert(creatures::PoseRefresh);
        refresh_animation(world);
    }
}
fn set_camera(world: &mut World, live: &Live) {
    let (pose, fov) = live.scenes[live.scene].camera(
        live.local,
        &live.scenes[(live.scene + 1) % live.scenes.len()],
    );
    let mut query = world.query_filtered::<(&mut Transform, &mut Projection), With<ReviewCamera>>();
    for (mut transform, mut projection) in query.iter_mut(world) {
        *transform = pose;
        if let Projection::Perspective(p) = &mut *projection {
            p.fov = fov.to_radians();
            p.near = 0.01;
        }
    }
}
fn report(world: &World, live: &Live) -> Value {
    let render = world.resource::<RenderMeter>().0.lock().unwrap();
    let actors: Vec<_> = live
        .slots
        .iter()
        .filter(|slot| slot.scene == live.scene)
        .map(|slot| {
            let variant = &slot.variants[slot.current];
            json!({"id":slot.id,"outfit":variant.spec.outfit,
            "position":world.get::<Transform>(variant.entity).unwrap().translation.to_array()})
        })
        .collect();
    json!({"actors":actors,"version":1,"status":live.status,"error":live.error,"completed_cycles":live.cycle,
        "scene":live.scene,"scene_seconds":live.local,"simulated_seconds":live.elapsed,
        "discarded_wall_seconds":live.discarded,"animation_steps":live.steps,"model_builds":live.builds,
        "windows_seen":live.windows.len(),"scene_entries":live.entries,"scene_entries_truncated":live.cycle as usize*live.scenes.len()>128,
        "frame_intervals":live.frames.json(),"animation_work":live.animation.json(),"render_intervals":render.intervals.json(),
        "capture_holds":0,"proof_frames":live.proof_frames,
        "timing_scope":"CPU main-update and render-cleanup intervals, not GPU durations or confirmed display drops; warmup excluded; percentiles use 1ms bins; overflow bin above 999ms uses observed maximum as upper bound",
        "renderer":world.resource::<Readiness>().0.lock().unwrap().adapter})
}
fn step_budget(seconds: f64) -> (f64, f64) {
    (seconds.min(0.25), (seconds - 0.25).max(0.0))
}
fn live_frame(world: &mut World) {
    world.resource_scope(|world, mut live: Mut<Live>| {
        let windows: Vec<Entity> = world
            .query_filtered::<Entity, With<Window>>()
            .iter(world)
            .collect();
        live.windows.extend(windows);
        let readiness = world.resource::<Readiness>().0.lock().unwrap();
        let ready = readiness.ready;
        let error = readiness.error.clone();
        drop(readiness);
        if let Some(error) = error {
            live.error = Some(error);
            live.status = "failed".into();
        }
        if !live.started && live.started_at.elapsed() > Duration::from_secs(180) {
            live.error = Some("live warmup timed out".into());
            live.status = "failed".into();
        }
        if world
            .resource::<ButtonInput<KeyCode>>()
            .just_pressed(KeyCode::Escape)
        {
            live.status = "stopped".into();
        }
        if live.status == "complete" && live.proof.is_some() && live.proof_frames == 0 {
            if live
                .proof_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                live.status = "failed".into();
                live.error = Some("proof screenshot timed out".into());
            } else {
                return;
            }
        }
        if live.status != "running" {
            *live.report.lock().unwrap() = report(world, &live);
            world.send_event(if live.status == "failed" {
                AppExit::error()
            } else {
                AppExit::Success
            });
            return;
        }
        if !live.started {
            if ready {
                live.started = true;
                world.resource::<RenderMeter>().0.lock().unwrap().active = true;
            }
            return;
        }
        let delta = world.resource::<Time<Real>>().delta_secs_f64();
        live.frames.add(delta);
        let (mut remaining, lost) = step_budget(delta);
        live.discarded += lost;
        let begin = Instant::now();
        loop {
            let scene = &live.scenes[live.scene];
            if live.local >= scene.duration - 1e-8 {
                live.scene += 1;
                if live.scene == live.scenes.len() {
                    live.scene = 0;
                    live.cycle += 1;
                }
                if live.cycles > 0 && live.cycle >= live.cycles {
                    live.scene = live.scenes.len() - 1;
                    live.status = "complete".into();
                    break;
                }
                enter_scene(world, &mut live);
                continue;
            }
            if let Some(event) = scene.script.events.get(live.event) {
                if f64::from(event.tick) / f64::from(scene.script.hz) <= live.local + 1e-8 {
                    let event = event.clone();
                    apply_event(world, &mut live, &event);
                    live.event += 1;
                    continue;
                }
            }
            if remaining <= 1e-8 {
                break;
            }
            let next = scene
                .script
                .events
                .get(live.event)
                .map_or(scene.duration, |e| {
                    f64::from(e.tick) / f64::from(scene.script.hz)
                });
            let dt = remaining
                .min(0.05)
                .min(scene.duration - live.local)
                .min(next - live.local);
            for slot in live.slots.iter().filter(|s| s.scene == live.scene) {
                world
                    .get_mut::<Transform>(slot.variants[slot.current].entity)
                    .unwrap()
                    .translation += slot.velocity * dt as f32;
            }
            live.clock.advance_by(Duration::from_secs_f64(dt));
            *world.resource_mut::<Time<Virtual>>() = live.clock;
            *world.resource_mut::<Time>() = live.clock.as_generic();
            advance_animation(world);
            live.steps += 1;
            live.local += dt;
            live.elapsed += dt;
            remaining -= dt;
        }
        live.animation.add(begin.elapsed().as_secs_f64());
        set_camera(world, &live);
        if !live.proof_requested && (live.elapsed >= 0.1 || live.status == "complete") {
            if let Some(path) = live.proof.clone() {
                live.proof_requested = true;
                live.proof_deadline = Some(Instant::now() + Duration::from_secs(30));
                world.spawn(Screenshot::primary_window()).observe(
                    move |trigger: Trigger<ScreenshotCaptured>, mut live: ResMut<Live>| {
                        let result = trigger
                            .event()
                            .0
                            .clone()
                            .try_into_dynamic()
                            .map_err(|e| e.to_string())
                            .and_then(|image| image.save(&path).map_err(|e| e.to_string()));
                        match result {
                            Ok(()) => live.proof_frames += 1,
                            Err(e) => {
                                live.error = Some(e);
                                live.status = "failed".into();
                            }
                        }
                    },
                );
            }
        }
        if live.elapsed - live.last_report >= 1.0 || live.status != "running" {
            *live.report.lock().unwrap() = report(world, &live);
            live.last_report = live.elapsed;
            if let Ok(mut window) = world.query::<&mut Window>().single_mut(world) {
                let mean = if live.frames.count > 0 {
                    live.frames.sum / live.frames.count as f64
                } else {
                    0.0
                };
                window.title = format!(
                    "Live model review | scene {}/{} | loop {} | {:.1} ms avg | {} frames >50ms",
                    live.scene + 1,
                    live.scenes.len(),
                    live.cycle + 1,
                    mean,
                    live.frames.over_50
                );
            }
        }
    });
}
fn on_exit(world: &mut World) {
    if !world.resource::<Events<AppExit>>().is_empty() {
        let live = world.resource::<Live>();
        let mut value = report(world, live);
        if value["status"] == "running" {
            value["status"] = json!("stopped");
        }
        *live.report.lock().unwrap() = value;
    }
}

pub(crate) fn cli(args: &[String]) -> Result<(), crate::Error> {
    let mut input = None;
    let mut output = None;
    let mut proof = None;
    let mut cycles = 0u32;
    let mut seen = HashSet::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if !seen.insert(arg.clone()) {
            return Err(format!("duplicate {arg}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--live-playlist" => input = Some(PathBuf::from(value)),
            "--diagnostics" => output = Some(PathBuf::from(value)),
            "--proof-frame" => proof = Some(PathBuf::from(value)),
            "--cycles" => cycles = value.parse()?,
            _ => return Err(format!("unsupported live option {arg}").into()),
        }
    }
    let scenes = read_playlist(&input.ok_or("--live-playlist is required")?)?;
    let count: usize = scenes
        .iter()
        .map(|s| {
            s.script
                .actors
                .iter()
                .map(|a| compositions(s, a).map(|c| c.len()))
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.iter().sum::<usize>())
        })
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    if count > 128 {
        return Err("live playlist exceeds 128 preloaded compositions".into());
    }
    let output = output.ok_or("--diagnostics requires a new report file")?;
    if proof.as_ref().is_some_and(|p| p.exists() || p == &output) {
        return Err("proof image already exists".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    file.write_all(b"{\"status\":\"starting\"}\n")?;
    drop(file);
    let viewport = scenes[0].script.viewport;
    let readiness = Readiness::default();
    let meter = RenderMeter::default();
    let saved = Arc::new(Mutex::new(json!({"status":"starting"})));
    let mut app = App::new();
    app.add_plugins(
        crate::graphics::plugins().set(WindowPlugin {
            primary_window: Some(Window {
                title: "Live model review — preparing scenes".into(),
                resolution: WindowResolution::new(viewport[0] as f32, viewport[1] as f32)
                    .with_scale_factor_override(1.0),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }),
    )
    .insert_resource(bevy::winit::WinitSettings {
        focused_mode: bevy::winit::UpdateMode::Continuous,
        unfocused_mode: bevy::winit::UpdateMode::Continuous,
    })
    .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
    .insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 180.0,
        ..default()
    })
    .insert_resource(readiness.clone())
    .insert_resource(meter.clone())
    .insert_resource(Live {
        scenes,
        slots: Vec::new(),
        scene: 0,
        local: 0.0,
        cycle: 0,
        cycles,
        event: 0,
        started: false,
        status: "running".into(),
        started_at: Instant::now(),
        clock: Time::default(),
        elapsed: 0.0,
        discarded: 0.0,
        steps: 0,
        builds: 0,
        frames: Timing::default(),
        animation: Timing::default(),
        entries: Vec::new(),
        windows: HashSet::new(),
        report: saved.clone(),
        last_report: 0.0,
        proof,
        proof_requested: false,
        proof_deadline: None,
        proof_frames: 0,
        error: None,
    })
    .add_systems(Startup, setup_live)
    .add_systems(Update, live_frame)
    .add_systems(Last, on_exit);
    configure_animation(&mut app);
    app.sub_app_mut(RenderApp)
        .insert_resource(readiness)
        .insert_resource(meter)
        .add_systems(
            Render,
            (renderer_ready, render_tick)
                .chain()
                .in_set(RenderSet::Cleanup),
        );
    let exit = app.run();
    let mut report = saved.lock().unwrap().clone();
    if report["status"] == "running" || report["status"] == "starting" {
        report["status"] = json!("stopped");
    }
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    if exit.is_success() && (report["status"] == "complete" || report["status"] == "stopped") {
        Ok(())
    } else {
        Err("live playback failed; see diagnostics".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Scene {
        let script = Script::parse(include_bytes!(
            "../../review-scripts/showcase/01-court.json"
        ))
        .unwrap();
        let shots = script
            .cameras
            .iter()
            .take(2)
            .map(|c| Shot {
                camera: c.id.clone(),
                hold_seconds: 2.0,
                transition_seconds: 1.0,
            })
            .collect();
        Scene {
            script,
            shots,
            duration: 6.0,
        }
    }
    #[test]
    fn camera_holds_and_transitions_are_continuous_through_loop() {
        let s = scene();
        let (start, _) = s.camera(0.0, &s);
        assert_eq!(s.camera(1.99, &s).0, start);
        let boundary = s.camera(3.0, &s).0;
        assert!(
            boundary
                .translation
                .distance(s.camera(3.00001, &s).0.translation)
                < 0.0001
        );
        assert!(s.camera(6.0, &s).0.translation.distance(start.translation) < 0.0001);
        assert!(s.camera(6.0, &s).0.rotation.angle_between(start.rotation) < 0.001);
        let mid = s.camera(2.5, &s).0;
        assert!(mid.rotation.is_normalized());
        assert!(mid.translation.distance(start.translation) > 0.01);
    }
    #[test]
    fn timing_reports_stalls_without_unbounded_samples() {
        let mut t = Timing::default();
        for _ in 0..19 {
            t.add(0.016);
        }
        t.add(0.2);
        let v = t.json();
        assert_eq!(v["count"], 20);
        assert_eq!(v["p95_upper_ms"], 16);
        assert_eq!(v["over_50ms"], 1);
        assert_eq!(v["max_ms"], 200.0);
        assert_eq!(step_budget(0.016), (0.016, 0.0));
        assert_eq!(step_budget(1.0), (0.25, 0.75));
    }
    #[test]
    fn repeated_compositions_share_preloaded_variants() {
        let mut s = scene();
        let actor = s.script.actors[0].clone();
        s.script.events = ["knight", "princess", "knight"]
            .into_iter()
            .enumerate()
            .map(|(i, outfit)| schema::Event {
                tick: i as u32 + 1,
                actor: actor.id.clone(),
                operation: Operation::Dress {
                    outfit: outfit.into(),
                },
            })
            .collect();
        let variants = compositions(&s, &actor).unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].outfit, "princess");
        assert_eq!(variants[1].outfit, "knight");
    }
    #[test]
    fn timing_overflow_keeps_a_truthful_upper_bound() {
        let mut timing = Timing::default();
        timing.add(2.5);
        assert_eq!(timing.json()["p95_upper_ms"], 2500);
        assert_eq!(timing.json()["overflow_bin_1000ms"], 1);
    }
    #[test]
    fn live_showcase_validates_and_has_long_holds() {
        let scenes = read_playlist(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("review-scripts/live-showcase.json"),
        )
        .unwrap();
        assert_eq!(scenes.len(), 4);
        for scene in scenes {
            assert_eq!(scene.duration, 40.0);
            assert!(scene.shots.iter().all(|s| s.hold_seconds >= 8.0));
        }
    }
}
