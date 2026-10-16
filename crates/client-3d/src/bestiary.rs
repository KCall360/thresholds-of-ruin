//! `--bestiary`: every creature on a stretch of cave floor. Each group walks,
//! then performs every move together on a fixed schedule; then the camera
//! visits each creature close up, at rest and in its signature move. With
//! `--capture DIR`, screenshots are taken at each moment and named for it.
//! For reviewing models and animation without a server.
use crate::{
    cave::{CaveExtension, CaveMaterial, CaveParams, CaveWorld, Solid},
    creatures::{self, Anim, Look, Move},
    props::{Accent, PropLibrary, PropMaterials},
    Diagnostics, FOG,
};
use bevy::{
    asset::load_internal_asset,
    core_pipeline::{bloom::Bloom, tonemapping::Tonemapping},
    pbr::{DistanceFog, FogFalloff, MaterialPlugin},
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::WindowResolution,
};
use std::collections::{HashMap, HashSet};

const PEOPLE: [&str; 10] = [
    "Warrior",
    "Mage",
    "blue sentinel",
    "red sentinel",
    "red arcanist",
    "ruin scout",
    "stone guardian",
    "hobgoblin",
    "zombie",
    "vampire",
];
const BEASTS: [&str; 8] = ["yeti", "emu", "snake", "quagga", "black unicorn", "centaur", "ember wisp", "ice monster"];
const CYCLE: f32 = 31.0;
/// Close-up time for each creature after the group cycles.
const PORTRAIT: f32 = 2.6;
/// Seconds each row stands at rest with `--idles`.
const IDLE: f32 = 12.0;

/// Every move, in the order the groups perform them.
const MOVES: [Move; 16] = [
    Move::Strike,
    Move::Thrust,
    Move::Combo(false),
    Move::Hook,
    Move::Bash,
    Move::Shove,
    Move::Heavy,
    Move::Focus,
    Move::Cast(Accent::Ember),
    Move::Hex,
    Move::Fear,
    Move::Drain,
    Move::Touch(Accent::Travel),
    Move::Guard(Accent::Gold),
    Move::Step,
    Move::Mend,
];

#[derive(Clone, Copy)]
enum Cue {
    Do(Move),
    Hit,
    Miss,
    Die,
    Revive,
}

/// What everyone does, and when, in each group cycle.
fn schedule() -> Vec<(f32, Cue)> {
    let mut cues: Vec<(f32, Cue)> = MOVES
        .iter()
        .enumerate()
        .map(|(i, m)| (3.0 + i as f32 * 1.38, Cue::Do(*m)))
        .collect();
    cues.extend([(25.2, Cue::Hit), (26.0, Cue::Miss), (26.8, Cue::Die), (30.4, Cue::Revive)]);
    cues
}

fn label(m: Move) -> String {
    format!("{m:?}").to_lowercase().replace(['(', ')'], "-")
}

/// Screenshot moments in each group cycle: each move as it lands.
fn shots() -> Vec<(f32, String)> {
    let mut shots = vec![(1.6, "walk".to_string())];
    for (i, m) in MOVES.iter().enumerate() {
        let at = 3.0 + i as f32 * 1.38;
        shots.push((at + m.impact() * m.duration() + 0.03, label(*m)));
    }
    shots.extend([(25.3, "hurt".to_string()), (26.2, "dodge".to_string()), (29.0, "dead".to_string())]);
    shots
}

/// Each creature's signature move, for its portrait.
fn signature(name: &str) -> Move {
    match name {
        "Warrior" => Move::Combo(false),
        "Mage" | "red arcanist" | "ember wisp" => Move::Cast(Accent::Ember),
        "blue sentinel" | "red sentinel" | "stone guardian" | "yeti" | "black unicorn" => Move::Heavy,
        "ruin scout" => Move::Thrust,
        "vampire" => Move::Drain,
        "ice monster" => Move::Touch(Accent::Travel),
        "zombie" => Move::Hook,
        "hobgoblin" => Move::Bash,
        _ => Move::Strike,
    }
}

#[derive(Component)]
struct Pacer {
    home: Vec3,
    height: f32,
    name: &'static str,
    index: usize,
}

#[derive(Component)]
struct Caption(Vec3, String);

#[derive(Resource, Default)]
struct Director {
    last: f32,
    shot: usize,
}

pub fn run(diagnostics: Diagnostics) {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Thresholds of Ruin — bestiary".into(),
            resolution: WindowResolution::new(1600., 1000.),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(MaterialPlugin::<CaveMaterial>::default())
    .insert_resource(ClearColor(FOG))
    .insert_resource(AmbientLight {
        color: Color::srgb(0.55, 0.6, 0.8),
        brightness: 240.0,
        ..default()
    })
    .insert_resource(creatures::Boredom(if diagnostics.bored { 1000.0 } else { 0.0 }))
    .insert_resource(diagnostics)
    .init_resource::<Director>()
    .add_systems(Startup, setup)
    .init_resource::<Lineup>()
    .add_systems(Update, (direct, lineup, creatures::animate, creatures::snuff, creatures::effects, camera, captions).chain());
    load_internal_asset!(app, crate::cave::SHADER, "cave.wgsl", Shader::from_wgsl);
    app.run();
}

fn spot(row: usize, i: usize) -> Vec3 {
    if row == 0 {
        Vec3::new(i as f32 * 1.6, -0.5, 0.0)
    } else {
        Vec3::new(1.0 + i as f32 * 1.75, -0.5, 4.5)
    }
}

fn everyone() -> Vec<&'static str> {
    PEOPLE.iter().chain(BEASTS.iter()).copied().collect()
}

fn setup(
    diagnostics: Res<Diagnostics>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
) {
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    let mut solids = HashMap::new();
    let mut open = HashSet::new();
    for x in -3..=17 {
        for z in -3..=8 {
            let key = (x as u64) << 32 ^ (z as u64) ^ 0x51;
            solids.insert(IVec3::new(x, -1, z), Solid { seen: true, key });
            if z == -3 {
                for y in 0..2 {
                    solids.insert(IVec3::new(x, y, z), Solid { seen: true, key: key ^ 7 });
                }
            } else {
                open.insert(IVec3::new(x, 0, z));
            }
        }
    }
    // Keep the stage clear of crystals and rubble.
    let blocked: HashSet<IVec3> = open.iter().copied().filter(|c| c.z > -2).collect();
    let mut cave = CaveWorld::default();
    let rock = cave_materials.add(CaveMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.88,
            ..default()
        },
        extension: CaveExtension {
            params: CaveParams {
                player: Vec4::new(0.0, 0.0, 0.0, 100.0),
                ..default()
            },
        },
    });
    for chunk in cave.update(solids, open, blocked) {
        if let Some(mesh) = cave.build(chunk).rock {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(rock.clone())));
        }
    }
    let rows: &[(usize, &[&str])] = if diagnostics.lineups { &[] } else { &[(0, &PEOPLE[..]), (1, &BEASTS[..])] };
    for &(row, names) in rows {
        for (i, name) in names.iter().enumerate() {
            let home = spot(row, i);
            let look = Look::of(name, *name == "Warrior", creatures::seed(name));
            let entity = commands
                .spawn((
                    Pacer {
                        home,
                        height: look.height(),
                        name,
                        index: row * PEOPLE.len() + i,
                    },
                    Anim::new(creatures::seed(name), home),
                    Transform::from_translation(home),
                    Visibility::default(),
                ))
                .id();
            let rig = creatures::spawn(&mut commands, &mut library, &mut meshes, entity, &look);
            if *name == "Warrior" {
                let sword = library.item("iron greatsword", tor_protocol::ItemClass::Weapon, &mut meshes);
                if let Some(hand) = rig.joint(creatures::J::HandR) {
                    let held = commands
                        .spawn((
                            Transform::from_rotation(
                                Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)
                                    * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                            )
                            .with_translation(Vec3::new(0.0, -0.02, 0.29)),
                            Visibility::default(),
                        ))
                        .with_children(|p| sword.attach(p))
                        .id();
                    commands.entity(hand).add_child(held);
                }
            }
            commands.entity(entity).insert(rig);
            commands.spawn((
                Caption(home + Vec3::new(0.0, 0.0, 0.7), name.to_string()),
                Text::new(*name),
                TextFont { font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.85, 0.82, 0.74)),
                Node { position_type: PositionType::Absolute, ..default() },
            ));
        }
    }
    for x in [1.0, 7.0, 13.0] {
        for z in [1.8, 6.0] {
            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.75, 0.48),
                    intensity: 160_000.0,
                    range: 12.0,
                    shadows_enabled: z < 2.0,
                    ..default()
                },
                Transform::from_xyz(x, 2.2, z),
            ));
        }
    }
    commands.spawn((
        Camera3d::default(),
        Camera { hdr: true, ..default() },
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: FOG,
            falloff: FogFalloff::Linear { start: 16.0, end: 40.0 },
            ..default()
        },
        Transform::from_xyz(7.0, 4.0, 9.0).looking_at(Vec3::new(7.0, 0.5, 0.0), Vec3::Y),
    ));
    crate::studio_lights(&mut commands);
    commands.insert_resource(library);
}

#[allow(clippy::too_many_arguments)]
fn direct(
    mut commands: Commands,
    time: Res<Time>,
    mut director: ResMut<Director>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    diagnostics: Res<Diagnostics>,
    mut actors: Query<(&Pacer, &mut Anim, &mut Transform, &mut Visibility)>,
    pending: Query<(), With<Screenshot>>,
    mut exit: EventWriter<AppExit>,
) {
    if diagnostics.lineups {
        return;
    }
    let now = clock(&time, &diagnostics);
    let last = director.last;
    let crossed = |at: f32| last < at && now >= at;
    let group = (now / CYCLE) as usize;
    let mut shots_now: Vec<String> = Vec::new();
    if diagnostics.idles {
        // Everyone at rest, people then beasts, with a shot every so often.
        for (_, mut anim, _, _) in &mut actors {
            anim.target_yaw = Some(0.0);
        }
        let mut at = 1.0;
        while at < IDLE * 2.0 {
            if crossed(at) {
                let tag = if at < IDLE { "people" } else { "beasts" };
                shots_now.push(format!("idle-{tag}-{at:04.1}"));
            }
            at += 1.1;
        }
        if diagnostics.capture.is_some() && now > IDLE * 2.0 + 0.5 && pending.is_empty() {
            exit.write(AppExit::Success);
        }
    } else if group < 2 {
        let base = group as f32 * CYCLE;
        let t = now - base;
        for (pacer, mut anim, mut transform, _) in &mut actors {
            // Walk a small circle, then stand facing the camera for the moves.
            if t < 2.8 {
                let a = t * 1.6 + pacer.home.x;
                transform.translation = pacer.home + Vec3::new(a.cos(), 0.0, a.sin()) * 0.45;
            } else {
                transform.translation = pacer.home;
                anim.target_yaw = Some(0.0);
            }
            for (at, cue) in schedule() {
                if crossed(base + at) {
                    perform(&mut commands, &mut library, &mut meshes, pacer, &mut anim, cue);
                }
            }
        }
        let tag = if group == 0 { "people" } else { "beasts" };
        for (at, name) in shots() {
            if crossed(base + at) {
                shots_now.push(format!("{tag}-{name}"));
            }
        }
    } else {
        // Portraits: each creature close up, at rest and then in its signature move.
        let start = CYCLE * 2.0;
        let current = ((now - start) / PORTRAIT).max(0.0) as usize;
        for (pacer, mut anim, mut transform, mut visibility) in &mut actors {
            transform.translation = pacer.home;
            anim.target_yaw = Some(0.0);
            if diagnostics.photos {
                // Alone, and turned to show a flank as well as the face.
                *visibility = if pacer.index == current { Visibility::Inherited } else { Visibility::Hidden };
                anim.target_yaw = Some(-0.55);
                anim.quiet = true;
                if pacer.index == current {
                    anim.yaw = -0.55;
                }
                let tag = format!("photo-{:02}-{}", pacer.index, pacer.name.replace(' ', "_"));
                if crossed(start + pacer.index as f32 * PORTRAIT + 1.2) {
                    shots_now.push(tag);
                }
                continue;
            }
            if crossed(start) {
                anim.dead = None;
            }
            let at = start + pacer.index as f32 * PORTRAIT;
            let m = signature(pacer.name);
            let tag = format!("portrait-{:02}-{}", pacer.index, pacer.name.replace(' ', "_"));
            if crossed(at + 0.7) {
                shots_now.push(format!("{tag}-rest"));
            }
            if crossed(at + 0.9) {
                perform(&mut commands, &mut library, &mut meshes, pacer, &mut anim, Cue::Do(m));
            }
            if crossed(at + 0.9 + m.impact() * m.duration() + 0.03) {
                shots_now.push(format!("{tag}-{}", label(m)));
            }
        }
        // Leave only once every screenshot has been read back from the GPU.
        if diagnostics.capture.is_some() && now > start + everyone().len() as f32 * PORTRAIT + 1.0 && pending.is_empty() {
            exit.write(AppExit::Success);
        }
    }
    if let Some(dir) = diagnostics.capture.clone() {
        for name in shots_now {
            director.shot += 1;
            let path = dir.join(format!("{:03}-{name}.png", director.shot));
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        }
    }
    director.last = now;
}

fn perform(commands: &mut Commands, library: &mut PropLibrary, meshes: &mut Assets<Mesh>, pacer: &Pacer, anim: &mut Anim, cue: Cue) {
    match cue {
        Cue::Do(m) => {
            anim.act(m);
            let target = (pacer.home + Vec3::Z * 1.4, 1.6);
            creatures::stage(commands, library, meshes, m, (pacer.home, pacer.height), Some(target), false);
        }
        Cue::Hit => anim.hit(0.0),
        Cue::Miss => anim.miss(0.0),
        Cue::Die => anim.dead = Some(0.0),
        Cue::Revive => anim.dead = None,
    }
}

/// `--lineups`: each creature in turn, five individuals of it side by side,
/// women and men alternating among people.
#[derive(Resource, Default)]
struct Lineup {
    shown: Option<usize>,
    group: Vec<Entity>,
    shot: usize,
    height: f32,
}

/// Seconds each line-up stands before the next.
const LINEUP: f32 = 4.0;

#[allow(clippy::too_many_arguments)]
fn lineup(
    mut commands: Commands,
    time: Res<Time>,
    diagnostics: Res<Diagnostics>,
    mut state: ResMut<Lineup>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
    pending: Query<(), With<Screenshot>>,
    mut exit: EventWriter<AppExit>,
) {
    if !diagnostics.lineups {
        return;
    }
    let names = everyone();
    let now = time.elapsed_secs();
    let index = (now / LINEUP) as usize;
    if index >= names.len() {
        if pending.is_empty() {
            exit.write(AppExit::Success);
        }
        return;
    }
    let name = names[index];
    if state.shown != Some(index) {
        for e in state.group.drain(..) {
            commands.entity(e).despawn();
        }
        state.shown = Some(index);
        // Pick seeds so people alternate woman, man, woman...
        let base = creatures::seed(name);
        let mut seeds = Vec::new();
        let mut j = 0u64;
        while seeds.len() < 5 {
            let seed = base.wrapping_add(j * 7919);
            j += 1;
            let look = Look::of(name, false, seed);
            let want = seeds.len() % 2 == 0;
            if look.woman() == want || j > 60 || !matches!(name, n if PEOPLE.contains(&n) || n == "centaur") {
                seeds.push(seed);
            }
        }
        let mut height: f32 = 0.0;
        for (k, seed) in seeds.into_iter().enumerate() {
            let look = Look::of(name, false, seed);
            height = height.max(look.height());
            let home = Vec3::new(3.4 + k as f32 * 1.6, -0.5, 1.2);
            let mut anim = Anim::new(seed, home);
            anim.yaw = -0.45;
            anim.target_yaw = Some(-0.45);
            anim.quiet = true;
            let entity = commands.spawn((anim, Transform::from_translation(home), Visibility::default())).id();
            let rig = creatures::spawn(&mut commands, &mut library, &mut meshes, entity, &look);
            commands.entity(entity).insert(rig);
            state.group.push(entity);
        }
        state.height = height;
    }
    // The whole line first, then close on the middle pair.
    let t = now - index as f32 * LINEUP;
    let height = state.height;
    let close = t > 2.1;
    if let Ok(mut transform) = camera.single_mut() {
        let (focus, d) = if close {
            // Each of the first two in turn, the upper body filling the frame.
            let x = if t > 3.0 { 5.0 } else { 3.4 };
            (Vec3::new(x, -0.5 + height * 0.78, 1.2), (height * 0.75).max(1.1))
        } else {
            (Vec3::new(6.6, -0.5 + height * 0.5, 1.2), (height * 1.9).max(6.4))
        };
        *transform = Transform::from_translation(focus + Vec3::new(0.0, d * 0.22, d)).looking_at(focus, Vec3::Y);
    }
    for (when, tag, step) in [(1.9, "line", 0), (2.8, "close-a", 1), (3.8, "close-b", 2)] {
        let wanted = index * 3 + step;
        if t >= when && state.shot <= wanted {
            state.shot = wanted + 1;
            if let Some(dir) = diagnostics.capture.clone() {
                let path = dir.join(format!("{:02}-{tag}-{}.png", index, name.replace(' ', "_")));
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            }
        }
    }
}

/// Seconds into the show; `--portraits` starts at the close-ups.
fn clock(time: &Time, diagnostics: &Diagnostics) -> f32 {
    time.elapsed_secs() + if diagnostics.portraits { CYCLE * 2.0 } else { 0.0 }
}

fn camera(time: Res<Time>, diagnostics: Res<Diagnostics>, mut camera: Query<&mut Transform, With<Camera3d>>) {
    if diagnostics.lineups {
        return;
    }
    let now = clock(&time, &diagnostics);
    let group = if diagnostics.idles { usize::from(now >= IDLE) } else { (now / CYCLE) as usize };
    let (focus, offset) = if group < 2 {
        let (focus, width) = if group == 0 {
            (Vec3::new(7.2, 0.45, 0.0), 14.4)
        } else {
            (Vec3::new(7.1, 0.35, 4.5), 17.0)
        };
        let d = diagnostics.distance.unwrap_or(width * 0.62);
        (focus, Vec3::new(0.0, d * 0.42, d * 0.9))
    } else {
        let names = everyone();
        let index = (((now - CYCLE * 2.0) / PORTRAIT) as usize).min(names.len() - 1);
        let (row, i) = if index < PEOPLE.len() { (0, index) } else { (1, index - PEOPLE.len()) };
        let home = spot(row, i);
        let name = names[index];
        let height = Look::of(name, name == "Warrior", creatures::seed(name)).height();
        let d = (height * 1.45).max(2.4);
        (home + Vec3::Y * height * 0.5, Vec3::new(d * 0.35, d * 0.3, d * 0.9))
    };
    if let Ok(mut transform) = camera.single_mut() {
        *transform = Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y);
    }
}

fn captions(
    time: Res<Time>,
    diagnostics: Res<Diagnostics>,
    mut captions: Query<(&Caption, &mut Node, &mut Visibility)>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    let Ok((camera, eye)) = camera.single() else {
        return;
    };
    let portraits = clock(&time, &diagnostics) >= CYCLE * 2.0;
    for (caption, mut node, mut visibility) in &mut captions {
        *visibility = if portraits { Visibility::Hidden } else { Visibility::Inherited };
        if let Ok(at) = camera.world_to_viewport(eye, caption.0) {
            node.left = Val::Px(at.x - caption.1.len() as f32 * 3.4);
            node.top = Val::Px(at.y);
        }
    }
}
