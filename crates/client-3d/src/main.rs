//! Prototype 3D client: the character's knowledge of the dungeon as lit
//! blocks, with the creatures it can see standing in it.
//!
//! The server describes cells in an observer frame that moves with the
//! character. Matching cell keys between views recovers that motion, so the
//! window keeps one steady world and the camera glides after the player.
mod autoplay;
mod bestiary;
mod cave;
mod creatures;
mod cursor;
mod gallery;
mod interact;
mod launch;
mod minimap;
mod net;
mod props;
mod sdf;

use bevy::{
    core_pipeline::{bloom::Bloom, tonemapping::Tonemapping},
    input::mouse::{MouseMotion, MouseWheel},
    asset::load_internal_asset,
    pbr::{DistanceFog, FogFalloff, MaterialPlugin},
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::WindowResolution,
};
use cave::{CaveExtension, CaveMaterial, CaveParams, CaveWorld, Solid};
use creatures::{Anim, Look, Move, J};
use net::{Cmd, Event as NetEvent, Happening, Network, View};
use props::{PropLibrary, PropMaterials};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    path::PathBuf,
    process::Child,
    sync::{Arc, Mutex},
};
use tor_protocol::{ActorId, ActorTarget, CellView, Direction, ItemClass, Position};

type Error = Box<dyn std::error::Error + Send + Sync>;

const HELP: &str = "WASD/arrows: move   Q/E: turn   wheel: zoom   click: act   G: pick up   \
X: door   I: pack   Space: wait   R/F: up/down   C: ceilings   M: map   Tab: control   Esc: close/quit";

fn main() -> Result<(), Error> {
    let mut connect: Option<SocketAddr> = None;
    let mut actor = ActorId(1);
    let mut observe = false;
    let mut scenario = launch::repo_root().join("scenarios/rogue-exploration");
    let mut save = None;
    let mut seed = None;
    let exe = if cfg!(windows) { "tor-server.exe" } else { "tor-server" };
    let mut server_bin = launch::repo_root().join("target/release").join(exe);
    let mut diagnostics = Diagnostics::default();
    let mut gallery = false;
    let mut bestiary = false;
    let mut autoplay = false;
    let mut hunt = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("Missing value for {arg}"));
        match arg.as_str() {
            "--connect" => connect = Some(value()?.parse()?),
            "--actor" => actor = ActorId(value()?.parse()?),
            "--observe" => observe = true,
            "--scenario" => scenario = PathBuf::from(value()?),
            "--save" => save = Some(PathBuf::from(value()?)),
            "--seed" => seed = Some(value()?.parse()?),
            "--server-bin" => server_bin = PathBuf::from(value()?),
            "--capture" => diagnostics.capture = Some(PathBuf::from(value()?)),
            "--demo" => diagnostics.demo = true,
            "--quit-after" => diagnostics.quit_after = Some(value()?.parse()?),
            "--capture-every" => diagnostics.capture_every = Some(value()?.parse()?),
            "--distance" => diagnostics.distance = Some(value()?.parse()?),
            "--gallery" => gallery = true,
            "--bestiary" => bestiary = true,
            "--portraits" => {
                bestiary = true;
                diagnostics.portraits = true;
            }
            "--bored" => diagnostics.bored = true,
            "--appearance" => {
                // The player's own look: "woman" or "man", optionally ":N" to
                // pick among that sex's hair, skin and build.
                let choice = value()?;
                let (sex, pick) = choice.split_once(':').unwrap_or((choice.as_str(), "0"));
                let woman = sex.eq_ignore_ascii_case("woman");
                let pick: u64 = pick.parse()?;
                let look = (0..400u64)
                    .map(|k| creatures::Variant::roll(k * 104_729 + pick, true))
                    .filter(|v| v.woman == woman)
                    .nth(pick as usize % 50)
                    .unwrap_or_default();
                let _ = creatures::PLAYER_LOOK.set(look);
            }
            "--lineups" => {
                bestiary = true;
                diagnostics.lineups = true;
            }
            "--photos" => {
                bestiary = true;
                diagnostics.portraits = true;
                diagnostics.photos = true;
            }
            "--antics" => {
                bestiary = true;
                diagnostics.idles = true;
                diagnostics.bored = true;
            }
            "--idles" => {
                bestiary = true;
                diagnostics.idles = true;
            }
            "--demo-interact" => autoplay = true,
            "--demo-hunt" => hunt = true,
            _ => {
                println!(
                    "tor-client-3d [--scenario DIR] [--save FILE] [--seed N] [--server-bin EXE]\n\
                     tor-client-3d --connect 127.0.0.1:PORT [--actor 1] [--observe]  (token from TOR_SERVER_TOKEN)\n\
                     Without --connect it starts a private server with a fresh save.\n{HELP}"
                );
                return Ok(());
            }
        }
    }
    if gallery {
        gallery::run(diagnostics);
        return Ok(());
    }
    if bestiary {
        bestiary::run(diagnostics);
        return Ok(());
    }
    let server: Arc<Mutex<Option<Child>>> = Arc::default();
    let (address, token) = match connect {
        Some(address) => (address, std::env::var("TOR_SERVER_TOKEN")?),
        None => {
            let token = launch::token();
            let started = launch::start(&server_bin, &scenario, save, seed, &token)?;
            println!("Server at {} saving to {}", started.address, started.save.display());
            *server.lock().unwrap() = Some(started.child);
            (started.address, token)
        }
    };
    let network = Network::start(address, token, actor, observe);

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Thresholds of Ruin — 3D prototype".into(),
                resolution: WindowResolution::new(1440., 900.),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(MaterialPlugin::<CaveMaterial>::default())
        .insert_resource(ClearColor(FOG))
        .insert_resource(AmbientLight {
            color: Color::srgb(0.62, 0.64, 0.78),
            brightness: 480.0,
            ..default()
        })
        .insert_resource(Net(Mutex::new(network)))
        .insert_resource(ServerProcess(server.clone()))
        .insert_resource(CameraRig {
            distance: diagnostics.distance.unwrap_or(8.5),
            ..default()
        })
        .insert_resource(creatures::Boredom(if diagnostics.bored { creatures::BORED_AFTER - 5.0 } else { 0.0 }))
        .insert_resource(diagnostics)
        .init_resource::<Game>()
        .init_resource::<interact::Hover>()
        .init_resource::<interact::Panel>()
        .init_resource::<interact::Pursuit>()
        .insert_resource(autoplay::Autoplay::new(autoplay, hunt))
        .add_systems(Startup, (setup, interact::setup, minimap::setup, cursor::setup).chain())
        .add_systems(
            Update,
            (
                receive,
                react,
                interact::keys,
                interact::panel,
                interact::panel_buttons,
                input,
                interact::hover,
                interact::click,
                interact::pursue,
                interact::show_hover,
                rig_camera,
                update_cave_params,
                follow_actors,
                creatures::animate,
                creatures::snuff,
                creatures::effects,
                tend_the_dead,
                animate_props,
                torch_flicker,
                hud,
            )
                .chain(),
        )
        .add_systems(Update, (demo, capture, autoplay::run, autoplay::hunt, minimap::toggle, minimap::draw, boredom, cursor::update))
        .add_systems(Last, shutdown_on_exit);
    load_internal_asset!(app, cave::SHADER, "cave.wgsl", Shader::from_wgsl);
    app.run();

    if let Some(mut child) = server.lock().unwrap().take() {
        let _ = child.kill();
    }
    Ok(())
}

const FOG: Color = Color::srgb(0.02, 0.022, 0.035);

/// Process-test aids: periodic screenshots and a wandering character.
#[derive(Resource, Default)]
pub(crate) struct Diagnostics {
    capture: Option<PathBuf>,
    demo: bool,
    quit_after: Option<f32>,
    distance: Option<f32>,
    /// Seconds between screenshots with --capture.
    capture_every: Option<f32>,
    /// In the bestiary, go straight to the close-ups.
    portraits: bool,
    /// In the bestiary, show everyone standing at rest.
    idles: bool,
    /// Start as though the player had been away a while, for the antics.
    bored: bool,
    /// In the bestiary close-ups, show each creature alone, turned three-quarters.
    photos: bool,
    /// In the bestiary, line up several individuals of each creature in turn.
    lineups: bool,
    shots: u32,
    next_shot: f32,
    next_step: f32,
    heading: usize,
}

#[derive(Resource)]
struct Net(Mutex<Network>);

#[derive(Resource)]
struct ServerProcess(Arc<Mutex<Option<Child>>>);

/// What the window remembers between views.
#[derive(Resource, Default)]
struct Game {
    /// Added to observer-frame positions to give steady world positions.
    offset: IVec3,
    previous_keys: HashMap<String, Position>,
    view: Option<View>,
    feet_world: IVec3,
    cells: HashMap<IVec3, (Entity, CellLook)>,
    cave: CaveWorld,
    chunks: HashMap<IVec3, Entity>,
    door_leaves: HashMap<IVec3, Entity>,
    /// The equipped weapon shown on the player: its name, the body, the model.
    held: Option<(String, Entity, Entity)>,
    /// Counts views, so panels know when to refresh.
    view_serial: u64,
    /// The weapon hand of each figure, for showing what it holds.
    hands: HashMap<Entity, Entity>,
    /// Creatures seen to die, kept where they fell as their own corpses.
    dying: Vec<DeadBody>,
    /// Corpse models by item, with the cell each lies in, kept between views.
    corpses: HashMap<tor_protocol::ItemTarget, (Entity, IVec3)>,
    /// Combat events waiting to be acted out, and the revision they came from.
    pending: Vec<Happening>,
    last_revision: Option<u64>,
    actors: HashMap<ActorTarget, Entity>,
    items: Vec<Entity>,
    log: VecDeque<String>,
    last_narration: Vec<String>,
    status: String,
    busy: bool,
    fatal: bool,
    cutaway: bool,
    since_send: f32,
    /// Seconds of red torchlight left after taking a wound.
    hurt: f32,
}

#[derive(Resource)]
struct CameraRig {
    /// Compass eighths the camera faces; unbounded so turning stays smooth.
    facing: i32,
    yaw: f32,
    distance: f32,
    focus: Vec3,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            facing: 0,
            yaw: 0.0,
            distance: 14.0,
            focus: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CellLook {
    Door,
    StairsUp,
    StairsDown,
}

#[derive(Resource)]
struct Palette {
    ring: Handle<Mesh>,
    cave_mat: Handle<CaveMaterial>,
    crystal_mat: [Handle<StandardMaterial>; 2],
    hostile_ring: Handle<StandardMaterial>,
}

#[derive(Component)]
struct ActorBody {
    goal: Vec3,
    label: Entity,
    ring: Entity,
    name: String,
    /// Head height, for the label and for aiming effects.
    height: f32,
}

#[derive(Component)]
struct HostileRing;

/// A creature that died in view: its body stays as its corpse.
struct DeadBody {
    id: ActorTarget,
    entity: Entity,
    /// Grid column where it fell.
    cell: IVec2,
    name: String,
    age: f32,
    /// Whether a corpse lies where it fell, so the body stands in for it.
    holds: bool,
}

/// The twinkle above an item on the floor.
#[derive(Component)]
struct Glint {
    phase: f32,
}

/// A door's swinging leaf.
#[derive(Component)]
struct DoorLeaf {
    open: bool,
}

#[derive(Component)]
struct Torch;

#[derive(Component)]
struct LogText;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct Label;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
) {
    let lit = |materials: &mut Assets<StandardMaterial>, color: Color, glow: LinearRgba| {
        materials.add(StandardMaterial {
            base_color: color,
            emissive: glow,
            perceptual_roughness: 0.85,
            ..default()
        })
    };
    let palette = Palette {
        ring: meshes.add(Torus::new(0.36, 0.44)),
        cave_mat: cave_materials.add(CaveMaterial {
            base: StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.88,
                ..default()
            },
            extension: CaveExtension {
                params: CaveParams::default(),
            },
        }),
        crystal_mat: [
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.95, 0.85),
                emissive: LinearRgba::rgb(0.5, 2.6, 2.1),
                perceptual_roughness: 0.12,
                reflectance: 0.8,
                ..default()
            }),
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.75, 0.5, 1.0),
                emissive: LinearRgba::rgb(1.7, 0.7, 2.8),
                perceptual_roughness: 0.12,
                reflectance: 0.8,
                ..default()
            }),
        ],
        hostile_ring: lit(&mut materials, Color::BLACK, LinearRgba::rgb(2.5, 0.2, 0.1)),
    };
    commands.insert_resource(palette);
    commands.insert_resource(PropLibrary::new(PropMaterials::new(&mut materials)));

    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: FOG,
            falloff: FogFalloff::Linear {
                start: 16.0,
                end: 48.0,
            },
            ..default()
        },
        Transform::from_xyz(0.0, 10.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Torch,
        PointLight {
            color: Color::srgb(1.0, 0.72, 0.42),
            intensity: 260_000.0,
            range: 16.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 1.5, 0.0),
    ));
    studio_lights(&mut commands);

    let text = |size: f32, color: Color| (TextFont { font_size: size, ..default() }, TextColor(color));
    commands.spawn((
        LogText,
        Text::new("Connecting..."),
        text(17.0, Color::srgb(0.9, 0.88, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            top: Val::Px(12.0),
            max_width: Val::Px(900.0),
            ..default()
        },
    ));
    commands.spawn((
        StatusText,
        Text::new(""),
        text(16.0, Color::srgb(0.7, 0.85, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            bottom: Val::Px(34.0),
            ..default()
        },
    ));
    commands.spawn((
        Text::new(HELP),
        text(13.0, Color::srgb(0.45, 0.5, 0.55)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            bottom: Val::Px(12.0),
            ..default()
        },
    ));
}

/// Observer-frame (x south-positive y, z up) to Bevy (y up, z south).
fn world(p: IVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.z as f32, p.y as f32)
}

fn ivec(p: Position) -> IVec3 {
    IVec3::new(p.x, p.y, p.z)
}

fn hash(text: &str) -> u64 {
    text.bytes()
        .fold(1469598103934665603u64, |h, b| (h ^ u64::from(b)).wrapping_mul(1099511628211))
}

/// A creature's colour, the same for every creature with that name.
fn creature_tint(name: &str) -> Vec3 {
    let colors = [
        (0.94, 0.58, 0.55),
        (0.95, 0.76, 0.31),
        (0.85, 0.55, 0.94),
        (0.5, 0.83, 0.94),
        (0.65, 0.88, 0.48),
        (0.94, 0.55, 0.31),
    ];
    let (r, g, b) = colors[(hash(name) % colors.len() as u64) as usize];
    Vec3::new(r, g, b)
}


fn receive(
    mut commands: Commands,
    net: Res<Net>,
    mut game: ResMut<Game>,
    palette: Res<Palette>,
    mut bodies: Query<&mut ActorBody>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut library: ResMut<PropLibrary>,
) {
    let events: Vec<NetEvent> = net.0.lock().unwrap().events.try_iter().collect();
    for event in events {
        match event {
            NetEvent::Ready => game.busy = false,
            NetEvent::Status(text) => {
                if std::env::var_os("TOR_3D_DEBUG").is_some() {
                    eprintln!("status: {text}");
                }
                game.status = text;
            }
            NetEvent::Fatal(text) => {
                game.status = text;
                game.fatal = true;
            }
            NetEvent::View(view) => {
                align(&mut game, &view);
                if view.narration != game.last_narration {
                    // Turn-timing lines repeat every step; the status shows readiness.
                    let noise = ["You must wait.", "You can act again."];
                    for line in view.narration.iter().filter(|l| !noise.contains(&l.as_str())) {
                        if std::env::var_os("TOR_3D_DEBUG").is_some() {
                            eprintln!("log: {line}");
                        }
                        // Bevy's default font has no middle dot.
                        game.log.push_back(line.replace('\u{b7}', "|"));
                    }
                    while game.log.len() > 200 {
                        game.log.pop_front();
                    }
                    game.last_narration = view.narration.clone();
                }
                game.feet_world = ivec(view.feet) + game.offset;
                let before = game.view.as_ref().and_then(|v| v.hp).map(|(hp, _)| hp);
                if let (Some(before), Some((now, _))) = (before, view.hp) {
                    if now < before {
                        game.hurt = 0.35;
                    }
                }
                if std::env::var_os("TOR_3D_DEBUG").is_some() {
                    eprintln!(
                        "feet {:?} offset {:?} actors {:?}",
                        view.feet,
                        game.offset,
                        view.actors.iter().map(|a| (a.position, a.is_self, a.name.as_str())).collect::<Vec<_>>()
                    );
                }
                build_cells(&mut commands, &mut game, &palette, &mut library, &mut meshes, &view);
                let fresh = game.last_revision != Some(view.revision);
                game.last_revision = Some(view.revision);
                if fresh {
                    game.pending.extend(view.events.iter().cloned());
                }
                let died: HashSet<ActorTarget> = view
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        Happening::Died(id) if fresh => Some(*id),
                        _ => None,
                    })
                    .collect();
                build_actors(&mut commands, &mut game, &palette, &mut library, &mut meshes, &view, &mut bodies, &died);
                build_items(&mut commands, &mut game, &mut library, &mut meshes, &view);
                update_held(&mut commands, &mut game, &mut library, &mut meshes, &view);
                game.view = Some(*view);
                game.view_serial += 1;
            }
        }
    }
}

/// Recover how the observer frame moved: every cell seen once in both views
/// must agree on the shift, as the shared map memory requires.
fn align(game: &mut Game, view: &View) {
    let mut counts: HashMap<&str, (Position, u32)> = HashMap::new();
    for cell in &view.visible {
        let entry = counts.entry(&cell.key).or_insert((cell.position, 0));
        entry.1 += 1;
    }
    let mut shift = None;
    let mut conflict = false;
    for (key, (now, count)) in &counts {
        if *count != 1 {
            continue;
        }
        if let Some(before) = game.previous_keys.get(*key) {
            let delta = ivec(*now) - ivec(*before);
            if shift.is_some_and(|s| s != delta) {
                conflict = true;
            }
            shift = Some(delta);
        }
    }
    if let (Some(delta), false) = (shift, conflict) {
        game.offset -= delta;
    }
    game.previous_keys = counts
        .into_iter()
        .filter(|(_, (_, n))| *n == 1)
        .map(|(k, (p, _))| (k.to_string(), p))
        .collect();
}

fn look(cell: &CellView) -> CellLook {
    if cell.door.is_some() {
        CellLook::Door
    } else if cell.stairs_up {
        CellLook::StairsUp
    } else {
        CellLook::StairsDown
    }
}

/// Grid (x, south, up) to Bevy cell coordinates (x, up, south).
fn bevy_cell(at: IVec3) -> IVec3 {
    IVec3::new(at.x, at.z, at.y)
}

fn build_cells(
    commands: &mut Commands,
    game: &mut Game,
    palette: &Palette,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
) {
    let mut solids = HashMap::new();
    let mut open = HashSet::new();
    let mut blocked: HashSet<IVec3> = view
        .items
        .iter()
        .map(|i| bevy_cell(ivec(i.position) + game.offset))
        .collect();
    let feet_z = view.feet.z;
    let mut wanted: HashMap<IVec3, (CellLook, &CellView)> = HashMap::new();
    let cells = view
        .visible
        .iter()
        .map(|c| (c, true))
        .chain(view.memory.iter().map(|c| (c, false)));
    let debug = std::env::var_os("TOR_3D_DEBUG").is_some();
    for (cell, seen) in cells {
        let dz = cell.position.z - feet_z;
        if debug && (cell.stairs_up || cell.stairs_down) {
            eprintln!(
                "stairs: up={} down={} wall={} seen={seen} dz={dz} at={:?}",
                cell.stairs_up, cell.stairs_down, cell.wall, cell.position
            );
        }
        // Ceilings over the character's head would hide it from above.
        if !game.cutaway && dz >= 2 {
            continue;
        }
        let at = ivec(cell.position) + game.offset;
        if cell.wall && cell.door.is_none() {
            // Visible cells come first, so a fresh sighting wins.
            solids.entry(bevy_cell(at)).or_insert(Solid {
                seen,
                key: hash(&cell.key),
            });
            continue;
        }
        open.insert(bevy_cell(at));
        if !(cell.door.is_some() || cell.stairs_up || cell.stairs_down) {
            continue;
        }
        blocked.insert(bevy_cell(at));
        let _ = seen;
        wanted.entry(at).or_insert((look(cell), cell));
    }
    let started = std::time::Instant::now();
    let dirty = game.cave.update(solids, open, blocked);
    let count = dirty.len();
    rebuild_chunks(commands, game, palette, meshes, dirty);
    if std::env::var_os("TOR_3D_DEBUG").is_some() {
        eprintln!("cave: {count} chunks in {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
    }
    let door_leaves = &mut game.door_leaves;
    game.cells.retain(|at, (entity, current)| {
        let keep = wanted.get(at).is_some_and(|(look, _)| look == current);
        if !keep {
            commands.entity(*entity).despawn();
            door_leaves.remove(at);
        }
        keep
    });
    let doors: HashSet<IVec3> = wanted
        .iter()
        .filter(|(_, (look, _))| *look == CellLook::Door)
        .map(|(at, _)| *at)
        .collect();
    for (at, (look, cell)) in &wanted {
        let at = *at;
        if game.cells.contains_key(&at) {
            continue;
        }
        let base = stand(at);
        let entity = match look {
            CellLook::Door => {
                // A door two cells high is drawn once, from its lower cell.
                if doors.contains(&(at - IVec3::Z)) {
                    continue;
                }
                let b = bevy_cell(at);
                let walls_x = [IVec3::X, IVec3::NEG_X]
                    .iter()
                    .any(|d| game.cave.solids.contains_key(&(b + *d)));
                let turn = if walls_x { 0.0 } else { std::f32::consts::FRAC_PI_2 };
                let open = cell.door.as_ref().is_some_and(|d| d.open);
                let (frame, leaf_model) = (library.door_frame(meshes), library.door_leaf(meshes));
                let leaf = commands
                    .spawn((
                        DoorLeaf { open },
                        Transform::from_xyz(-0.41, 0.0, 0.0).with_rotation(leaf_rotation(open)),
                        Visibility::default(),
                    ))
                    .with_children(|parent| leaf_model.attach(parent))
                    .id();
                game.door_leaves.insert(at, leaf);
                commands
                    .spawn((Transform::from_translation(base).with_rotation(Quat::from_rotation_y(turn)), Visibility::default()))
                    .with_children(|parent| {
                        frame.attach(parent);
                        parent.spawn((
                            PointLight {
                                color: props::Accent::Amber.light(),
                                intensity: 6_000.0,
                                range: 4.0,
                                shadows_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 1.7, 0.4),
                        ));
                    })
                    .add_child(leaf)
                    .id()
            }
            CellLook::StairsUp => {
                let model = library.stairs_up(meshes);
                commands
                    .spawn((Transform::from_translation(base), Visibility::default()))
                    .with_children(|parent| model.attach(parent))
                    .id()
            }
            CellLook::StairsDown => {
                let model = library.stairs_down(meshes);
                commands
                    .spawn((Transform::from_translation(base), Visibility::default()))
                    .with_children(|parent| {
                        model.attach(parent);
                        parent.spawn((
                            PointLight {
                                color: Color::srgb(0.3, 0.5, 1.0),
                                intensity: 5_000.0,
                                range: 3.5,
                                shadows_enabled: false,
                                ..default()
                            },
                            Transform::from_xyz(0.0, -0.6, 0.0),
                        ));
                    })
                    .id()
            }
        };
        game.cells.insert(at, (entity, *look));
    }
    // Doors already standing swing to their new state.
    for (at, (_, cell)) in &wanted {
        if let (Some(leaf), Some(door)) = (game.door_leaves.get(at), &cell.door) {
            commands.entity(*leaf).insert(DoorLeaf { open: door.open });
        }
    }
    let _ = palette;
}

fn leaf_rotation(open: bool) -> Quat {
    Quat::from_rotation_y(if open { -1.5 } else { 0.0 })
}

/// Rebuild the cave meshes of changed chunks: rock, crystals and their light.
fn rebuild_chunks(
    commands: &mut Commands,
    game: &mut Game,
    palette: &Palette,
    meshes: &mut Assets<Mesh>,
    dirty: HashSet<IVec3>,
) {
    for chunk in dirty {
        if let Some(entity) = game.chunks.remove(&chunk) {
            commands.entity(entity).despawn();
        }
        let built = game.cave.build(chunk);
        if built.rock.is_none() && built.lights.is_empty() {
            continue;
        }
        let entity = commands
            .spawn((Transform::default(), Visibility::default()))
            .with_children(|parent| {
                if let Some(rock) = built.rock {
                    parent.spawn((Mesh3d(meshes.add(rock)), MeshMaterial3d(palette.cave_mat.clone())));
                }
                for (mesh, material) in built.crystals.into_iter().zip(&palette.crystal_mat) {
                    if let Some(mesh) = mesh {
                        parent.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone())));
                    }
                }
                for (at, glow) in built.lights {
                    parent.spawn((
                        PointLight {
                            color: glow.light(),
                            intensity: 9_000.0,
                            range: 4.5,
                            radius: 0.15,
                            shadows_enabled: false,
                            ..default()
                        },
                        Transform::from_translation(at),
                    ));
                }
            })
            .id();
        game.chunks.insert(chunk, entity);
    }
}

/// Feet of a creature standing in a cell: on top of the solid cell below.
fn stand(at: IVec3) -> Vec3 {
    world(at) - Vec3::Y * 0.5
}

#[allow(clippy::too_many_arguments)]
fn build_actors(
    commands: &mut Commands,
    game: &mut Game,
    palette: &Palette,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
    bodies: &mut Query<&mut ActorBody>,
    died: &HashSet<ActorTarget>,
) {
    let present: HashSet<ActorTarget> = view.actors.iter().map(|a| a.id).collect();
    let mut fallen = Vec::new();
    game.actors.retain(|id, entity| {
        let keep = present.contains(id);
        if !keep {
            if let Ok(body) = bodies.get(*entity) {
                commands.entity(body.label).despawn();
                commands.entity(body.ring).insert(Visibility::Hidden);
                if died.contains(id) {
                    let cell = IVec2::new(body.goal.x.round() as i32, body.goal.z.round() as i32);
                    fallen.push(DeadBody {
                        id: *id,
                        entity: *entity,
                        cell,
                        name: body.name.clone(),
                        age: 0.0,
                        holds: false,
                    });
                    return false;
                }
            }
            commands.entity(*entity).despawn();
        }
        keep
    });
    game.dying.extend(fallen);
    // Creatures are reported by a body cell, not necessarily their feet:
    // stand each on the first solid cell beneath it.
    let solid: HashSet<Position> = view
        .visible
        .iter()
        .chain(&view.memory)
        .filter(|c| c.wall)
        .map(|c| c.position)
        .collect();
    let ground = |mut p: Position| {
        for _ in 0..4 {
            let below = Position { z: p.z - 1, ..p };
            if solid.contains(&below) {
                break;
            }
            p = below;
        }
        p
    };
    for actor in &view.actors {
        let goal = stand(ivec(ground(actor.position)) + game.offset);
        if let Some(entity) = game.actors.get(&actor.id) {
            if let Ok(mut body) = bodies.get_mut(*entity) {
                body.goal = goal;
                commands.entity(body.ring).insert(ring_visibility(actor.hostile));
            }
            continue;
        }
        let look = Look::of(&actor.name, actor.is_self, creatures::individual(&actor.id));
        let name = if actor.is_self { String::new() } else { actor.name.clone() };
        let label = commands
            .spawn((
                Label,
                Text::new(name.clone()),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(if actor.hostile {
                    Color::srgb(1.0, 0.55, 0.5)
                } else {
                    Color::srgb(0.85, 0.85, 0.8)
                }),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .id();
        let ring = commands
            .spawn((
                HostileRing,
                Mesh3d(palette.ring.clone()),
                MeshMaterial3d(palette.hostile_ring.clone()),
                Transform::from_xyz(0.0, 0.03, 0.0).with_scale(Vec3::splat((look.scale * 1.5).max(0.8))),
                ring_visibility(actor.hostile),
            ))
            .id();
        let entity = commands
            .spawn((
                ActorBody {
                    goal,
                    label,
                    ring,
                    name: actor.name.clone(),
                    height: look.height(),
                },
                Anim::new(creatures::seed(&actor.name), goal),
                Transform::from_translation(goal),
                Visibility::default(),
            ))
            .add_child(ring)
            .id();
        let rig = creatures::spawn(commands, library, meshes, entity, &look);
        if let Some(hand) = rig.joint(J::HandR) {
            game.hands.insert(entity, hand);
        }
        commands.entity(entity).insert(rig);
        game.actors.insert(actor.id, entity);
    }
}

/// Act out what the server said happened: the caster's move, its effect, and
/// the target's flinch or sidestep when the blow lands.
fn react(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bodies: Query<(&ActorBody, &mut Anim)>,
) {
    let pending = std::mem::take(&mut game.pending);
    let find = |game: &Game, id: Option<ActorTarget>| {
        id.and_then(|id| {
            game.actors
                .get(&id)
                .copied()
                .or_else(|| game.dying.iter().find(|d| d.id == id).map(|d| d.entity))
        })
    };
    for happening in pending {
        match happening {
            Happening::Died(id) => {
                if let Some(entity) = find(&game, Some(id)) {
                    if let Ok((body, mut anim)) = bodies.get_mut(entity) {
                        anim.dead = Some(-0.35);
                        anim.target_yaw = Some(lying_yaw(&game, body.goal));
                    }
                }
            }
            Happening::Ability { caster, target, name, missed, strike } => {
                let (Some(caster_entity), target_entity) = (find(&game, caster), find(&game, target)) else {
                    continue;
                };
                let self_targeted = caster == target || target.is_none();
                let action = Move::of(&name, self_targeted, strike);
                let Ok((caster_body, _)) = bodies.get(caster_entity) else {
                    continue;
                };
                let from = (caster_body.goal, caster_body.height);
                let to = target_entity
                    .filter(|_| !self_targeted)
                    .and_then(|e| bodies.get(e).ok())
                    .map(|(b, _)| (b.goal, b.height));
                if let Ok((_, mut anim)) = bodies.get_mut(caster_entity) {
                    anim.act(action);
                    if let Some((at, _)) = to {
                        let d = at - from.0;
                        if d.length_squared() > 0.01 {
                            anim.target_yaw = Some(d.x.atan2(d.z));
                        }
                    }
                }
                let reaction = creatures::stage(&mut commands, &mut library, &mut meshes, action, from, to, missed);
                if let (Some(delay), Some(target_entity)) = (reaction, target_entity.filter(|_| !self_targeted)) {
                    if let Ok((_, mut anim)) = bodies.get_mut(target_entity) {
                        if missed {
                            anim.miss(delay);
                        } else {
                            anim.hit(delay);
                        }
                    }
                }
            }
        }
    }
}

/// Which way a body lying in a cell should point so it stays out of the
/// rock: along whichever axis has open cells on both sides. Its length runs
/// along the rig's x axis once it has fallen.
fn lying_yaw(game: &Game, at: Vec3) -> f32 {
    let cell = IVec3::new(at.x.round() as i32, at.z.round() as i32, (at.y + 0.5).round() as i32);
    let rock = |d: IVec3| {
        [d, -d]
            .iter()
            .filter(|d| game.cave.solids.contains_key(&bevy_cell(cell + **d)))
            .count()
    };
    if rock(IVec3::X) <= rock(IVec3::Y) {
        0.0
    } else {
        std::f32::consts::FRAC_PI_2
    }
}

/// Bodies of the fallen stay while their corpse lies where they fell.
fn tend_the_dead(mut commands: Commands, time: Res<Time>, mut game: ResMut<Game>) {
    let dt = time.delta_secs();
    game.dying.retain_mut(|body| {
        body.age += dt;
        let keep = body.holds || body.age < 2.0;
        if !keep {
            commands.entity(body.entity).despawn();
        }
        keep
    });
}

fn ring_visibility(hostile: bool) -> Visibility {
    if hostile {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

/// Items are drawn larger than life so they read at play distance.
const ITEM_SCALE: f32 = 0.72;

/// Lay each item on the floor as its model, with a glint so it reads in the
/// dark. Several in one cell gather around its middle.
fn build_items(
    commands: &mut Commands,
    game: &mut Game,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
) {
    for entity in game.items.drain(..) {
        commands.entity(entity).despawn();
    }
    for body in &mut game.dying {
        body.holds = false;
    }
    // Corpses stay where they lie: keep the ones still seen in place.
    let mut seen_corpses = HashSet::new();
    let mut per_cell: HashMap<IVec3, u32> = HashMap::new();
    let glint = library.glint(meshes);
    for item in &view.items {
        let at = ivec(item.position) + game.offset;
        let count = per_cell.entry(at).or_insert(0);
        let index = *count;
        *count += 1;
        let key = hash(&item.name) ^ u64::from(index);
        let angle = (key % 6283) as f32 / 1000.0;
        let spread = if index == 0 {
            Vec3::ZERO
        } else {
            let a = index as f32 * 2.4;
            Vec3::new(a.cos(), 0.0, a.sin()) * 0.27
        };
        if item.class == ItemClass::Corpse || item.name.contains("corpse") {
            let creature = creatures::corpse_of(&item.name).to_string();
            let column = IVec2::new(at.x, at.y);
            if let Some(body) = game
                .dying
                .iter_mut()
                .find(|d| !d.holds && d.cell == column && (d.name == creature || d.name.is_empty()))
            {
                body.holds = true;
                continue;
            }
            seen_corpses.insert(item.id);
            if game.corpses.get(&item.id).is_some_and(|(_, cell)| *cell == at) {
                continue;
            }
            if let Some((old, _)) = game.corpses.remove(&item.id) {
                commands.entity(old).despawn();
            }
            let entity = creatures::spawn_corpse(
                commands,
                library,
                meshes,
                &item.name,
                stand(at),
                lying_yaw(game, stand(at)),
                creatures::individual(&item.id),
            );
            commands.entity(entity).with_children(|parent| {
                parent
                    .spawn((Glint { phase: angle }, Transform::from_xyz(0.0, 0.45, 0.0), Visibility::default()))
                    .with_children(|g| glint.attach(g));
            });
            game.corpses.insert(item.id, (entity, at));
            continue;
        }
        let model = library.item(&item.name, item.class, meshes);
        let entity = commands
            .spawn((
                Transform::from_translation(stand(at) + spread)
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(Vec3::splat(ITEM_SCALE)),
                Visibility::default(),
            ))
            .with_children(|parent| {
                model.attach(parent);
                parent
                    .spawn((Glint { phase: angle }, Transform::from_xyz(0.0, 0.32, 0.0), Visibility::default()))
                    .with_children(|g| glint.attach(g));
            })
            .id();
        game.items.push(entity);
    }
    prune_corpses(commands, game, &seen_corpses);
}

/// The cast's lighting, after the art direction: a broad warm key from the
/// upper left, a cool blue-violet fill from the right, and a soft back light
/// that rims silhouettes against the dark.
pub(crate) fn studio_lights(commands: &mut Commands) {
    for (color, lux, from) in [
        (Color::srgb(1.0, 0.86, 0.68), 1300.0, Vec3::new(-6.0, 10.0, 6.0)),
        (Color::srgb(0.55, 0.6, 1.0), 650.0, Vec3::new(9.0, 4.0, 2.0)),
        (Color::srgb(0.85, 0.78, 1.0), 550.0, Vec3::new(1.0, 5.0, -9.0)),
    ] {
        commands.spawn((
            DirectionalLight { color, illuminance: lux, ..default() },
            Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
}

/// Count the seconds since the player last touched keyboard or mouse.
fn boredom(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    mut bored: ResMut<creatures::Boredom>,
) {
    let moved = motion.read().any(|m| m.delta.length_squared() > 0.5);
    let scrolled = wheel.read().count() > 0;
    if moved || scrolled || keys.get_pressed().next().is_some() || mouse.get_pressed().next().is_some() {
        bored.0 = 0.0;
    } else {
        bored.0 += time.delta_secs();
    }
}

/// Clear corpses no longer seen, after a view's items are laid out.
fn prune_corpses(commands: &mut Commands, game: &mut Game, seen: &HashSet<tor_protocol::ItemTarget>) {
    game.corpses.retain(|id, (entity, _)| {
        let keep = seen.contains(id);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
}

/// Show the equipped weapon at the player's side.
fn update_held(
    commands: &mut Commands,
    game: &mut Game,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    view: &View,
) {
    let weapon = view
        .inventory
        .iter()
        .find(|c| c.equipped.is_some() && c.class == ItemClass::Weapon);
    let body = view
        .actors
        .iter()
        .find(|a| a.is_self)
        .and_then(|a| game.actors.get(&a.id))
        .and_then(|body| game.hands.get(body))
        .copied();
    let wanted = weapon.zip(body).map(|(w, b)| (w.name.clone(), b));
    if wanted == game.held.as_ref().map(|(name, body, _)| (name.clone(), *body)) {
        return;
    }
    if let Some((_, _, model)) = game.held.take() {
        commands.entity(model).try_despawn();
    }
    if let Some((name, body)) = wanted {
        let model = library.item(&name, ItemClass::Weapon, meshes);
        let held = commands
            .spawn((
                // Point forward from the fist, edge up, the grip in the hand.
                Transform::from_rotation(
                    Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                )
                .with_translation(Vec3::new(0.0, -0.02, if name.contains("dagger") { 0.17 } else { 0.29 })),
                Visibility::default(),
            ))
            .with_children(|parent| model.attach(parent))
            .id();
        commands.entity(body).add_child(held);
        game.held = Some((name, body, held));
    }
}

/// The compass, clockwise from north, as the server names directions.
const COMPASS: [Direction; 8] = [
    Direction::North,
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::South,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
];

fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: EventReader<MouseWheel>,
    time: Res<Time>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut rig: ResMut<CameraRig>,
) {
    if keys.just_pressed(KeyCode::KeyQ) {
        rig.facing -= 1;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        rig.facing += 1;
    }
    for event in wheel.read() {
        rig.distance = (rig.distance - event.y * 0.8).clamp(4.0, 32.0);
    }
    if keys.just_pressed(KeyCode::KeyC) {
        game.cutaway = !game.cutaway;
        game.status = if game.cutaway { "Showing ceilings." } else { "Ceilings cut away." }.into();
    }
    game.since_send += time.delta_secs();
    if game.busy || game.fatal {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let pressed = |codes: &[KeyCode]| keys.any_pressed(codes.iter().copied());
    let fresh = |codes: &[KeyCode]| keys.any_just_pressed(codes.iter().copied());

    let command = if fresh(&[KeyCode::Tab]) {
        Some(Cmd::Control)
    } else if fresh(&[KeyCode::KeyR, KeyCode::PageUp]) || (shift && fresh(&[KeyCode::Comma])) {
        Some(Cmd::Move(Direction::Up))
    } else if fresh(&[KeyCode::KeyF, KeyCode::PageDown]) || (shift && fresh(&[KeyCode::Period])) {
        Some(Cmd::Move(Direction::Down))
    } else {
        // A screen-relative heading from held keys: up is away from the camera.
        let numpad = [
            (KeyCode::Numpad8, (0, 1)),
            (KeyCode::Numpad9, (1, 1)),
            (KeyCode::Numpad6, (1, 0)),
            (KeyCode::Numpad3, (1, -1)),
            (KeyCode::Numpad2, (0, -1)),
            (KeyCode::Numpad1, (-1, -1)),
            (KeyCode::Numpad4, (-1, 0)),
            (KeyCode::Numpad7, (-1, 1)),
        ];
        let mut heading = numpad.iter().find(|(k, _)| keys.pressed(*k)).map(|(_, d)| *d);
        if heading.is_none() {
            let x = i32::from(pressed(&[KeyCode::KeyD, KeyCode::ArrowRight]))
                - i32::from(pressed(&[KeyCode::KeyA, KeyCode::ArrowLeft]));
            let y = i32::from(pressed(&[KeyCode::KeyW, KeyCode::ArrowUp]))
                - i32::from(pressed(&[KeyCode::KeyS, KeyCode::ArrowDown]));
            heading = (x, y).ne(&(0, 0)).then_some((x, y));
        }
        let movement_keys = [
            KeyCode::KeyW,
            KeyCode::KeyA,
            KeyCode::KeyS,
            KeyCode::KeyD,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Numpad1,
            KeyCode::Numpad2,
            KeyCode::Numpad3,
            KeyCode::Numpad4,
            KeyCode::Numpad6,
            KeyCode::Numpad7,
            KeyCode::Numpad8,
            KeyCode::Numpad9,
        ];
        // A tap acts at once; a held key repeats at a walking pace. A short
        // grace lets two keys pressed together make one diagonal step.
        let tapped = fresh(&movement_keys);
        let repeat = game.since_send > 0.16;
        match heading {
            Some((x, y)) if tapped || repeat => {
                let local = [(0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1)]
                    .iter()
                    .position(|d| *d == (x, y))
                    .unwrap_or(0) as i32;
                Some(Cmd::Move(COMPASS[(local + rig.facing).rem_euclid(8) as usize]))
            }
            _ if fresh(&[KeyCode::Space, KeyCode::Numpad5]) || (!shift && fresh(&[KeyCode::Period])) => {
                Some(Cmd::Wait)
            }
            _ => None,
        }
    };
    if let Some(command) = command {
        if net.0.lock().unwrap().commands.send(command).is_ok() {
            game.busy = true;
            game.since_send = 0.0;
            game.status.clear();
        }
    }
}

fn rig_camera(
    time: Res<Time>,
    game: Res<Game>,
    mut rig: ResMut<CameraRig>,
    mut camera: Query<&mut Transform, (With<Camera3d>, Without<Torch>)>,
    mut torch: Query<&mut Transform, (With<Torch>, Without<Camera3d>)>,
) {
    let dt = time.delta_secs();
    let player = stand(game.feet_world) + Vec3::Y * 0.65;
    let smooth = 1.0 - (-dt * 7.0).exp();
    rig.focus = rig.focus.lerp(player, smooth);
    let goal_yaw = rig.facing as f32 * std::f32::consts::FRAC_PI_4;
    rig.yaw += (goal_yaw - rig.yaw) * (1.0 - (-dt * 9.0).exp());
    // Facing north at yaw 0: the camera sits south of the player (+z).
    let forward = Vec3::new(rig.yaw.sin(), 0.0, -rig.yaw.cos());
    let pitch = 0.95f32;
    let eye = rig.focus - forward * rig.distance * pitch.cos() + Vec3::Y * rig.distance * pitch.sin();
    if let Ok(mut transform) = camera.single_mut() {
        *transform = Transform::from_translation(eye).looking_at(rig.focus, Vec3::Y);
    }
    if let Ok(mut transform) = torch.single_mut() {
        transform.translation = rig.focus + Vec3::Y * 1.4;
    }
}

/// Tell the cave shader where the player and camera are, for the cutaway.
fn update_cave_params(
    rig: Res<CameraRig>,
    palette: Res<Palette>,
    mut materials: ResMut<Assets<CaveMaterial>>,
    camera: Query<&Transform, With<Camera3d>>,
) {
    let (Ok(eye), Some(material)) = (camera.single(), materials.get_mut(&palette.cave_mat)) else {
        return;
    };
    let params = &mut material.extension.params;
    let floor = rig.focus.y - 0.65;
    params.player = (rig.focus + Vec3::Y * 0.1).extend(floor);
    params.camera = eye.translation.extend(1.55);
}

fn follow_actors(
    time: Res<Time>,
    mut bodies: Query<(&ActorBody, &mut Transform)>,
    mut labels: Query<(&mut Node, &mut Visibility), With<Label>>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    let smooth = 1.0 - (-time.delta_secs() * 12.0).exp();
    let Ok((camera, eye)) = camera.single() else {
        return;
    };
    for (body, mut transform) in &mut bodies {
        let step = body.goal - transform.translation;
        transform.translation += step * smooth;
        if let Ok((mut node, mut visibility)) = labels.get_mut(body.label) {
            let head = transform.translation + Vec3::Y * (body.height + 0.15);
            match camera.world_to_viewport(eye, head) {
                Ok(at) if !body.name.is_empty() => {
                    node.left = Val::Px(at.x - body.name.len() as f32 * 3.6);
                    node.top = Val::Px(at.y - 10.0);
                    *visibility = Visibility::Visible;
                }
                _ => *visibility = Visibility::Hidden,
            }
        }
    }
}

fn animate_props(
    time: Res<Time>,
    mut glints: Query<(&Glint, &mut Transform), Without<DoorLeaf>>,
    mut leaves: Query<(&DoorLeaf, &mut Transform), Without<Glint>>,
) {
    let t = time.elapsed_secs();
    for (glint, mut transform) in &mut glints {
        let twinkle = ((t * 1.7 + glint.phase).sin() * 0.5 + 0.5).powi(6);
        transform.scale = Vec3::splat(0.25 + 0.9 * twinkle);
        transform.rotation = Quat::from_rotation_y(t * 0.9 + glint.phase) * Quat::from_rotation_z(0.6);
        transform.translation.y = 0.3 + (t * 1.3 + glint.phase).sin() * 0.03;
    }
    let smooth = 1.0 - (-time.delta_secs() * 6.0).exp();
    for (leaf, mut transform) in &mut leaves {
        transform.rotation = transform.rotation.slerp(leaf_rotation(leaf.open), smooth);
    }
}

fn torch_flicker(time: Res<Time>, mut game: ResMut<Game>, mut torch: Query<&mut PointLight, With<Torch>>) {
    let t = time.elapsed_secs();
    if game.hurt > 0.0 {
        game.hurt = (game.hurt - time.delta_secs()).max(0.0);
    }
    let red = game.hurt / 0.35;
    let flicker = 1.0 + 0.06 * (t * 11.0).sin() + 0.04 * (t * 23.7).sin() + 0.03 * (t * 5.3).cos();
    if let Ok(mut light) = torch.single_mut() {
        light.intensity = 260_000.0 * flicker * (1.0 + red);
        light.color = Color::srgb(1.0, 0.72 * (1.0 - red * 0.8), 0.42 * (1.0 - red * 0.8));
    }
}

fn hud(
    game: Res<Game>,
    mut log: Query<&mut Text, (With<LogText>, Without<StatusText>)>,
    mut status: Query<&mut Text, (With<StatusText>, Without<LogText>)>,
) {
    if !game.is_changed() {
        return;
    }
    if let Ok(mut text) = log.single_mut() {
        let lines: Vec<&str> = game.log.iter().rev().take(7).map(String::as_str).collect();
        text.0 = lines.into_iter().rev().collect::<Vec<_>>().join("\n");
    }
    if let Ok(mut text) = status.single_mut() {
        let mut parts = Vec::new();
        if let Some(view) = &game.view {
            match view.hp {
                Some((0, _)) => parts.push("You have fallen.".into()),
                Some((hp, max)) => parts.push(format!("HP {hp}/{max}")),
                None => {}
            }
            parts.push(format!("tick {}", view.tick));
            parts.push(if view.has_control { "in control" } else { "observing" }.to_string());
            let here: Vec<&str> = view
                .items
                .iter()
                .filter(|i| i.position == view.feet)
                .map(|i| i.name.as_str())
                .collect();
            if !here.is_empty() {
                parts.push(format!("here: {}", here.join(", ")));
            }
        }
        if game.busy {
            parts.push("...".into());
        }
        if !game.status.is_empty() {
            parts.push(game.status.clone());
        }
        text.0 = parts.join("   |   ");
    }
}

fn shutdown_on_exit(mut exits: EventReader<AppExit>, net: Res<Net>, server: Res<ServerProcess>) {
    if exits.read().next().is_none() {
        return;
    }
    net.0.lock().unwrap().shutdown();
    if let Some(mut child) = server.0.lock().unwrap().take() {
        let _ = child.kill();
    }
}

/// With --demo, wander: keep a heading until the server refuses it.
fn demo(time: Res<Time>, net: Res<Net>, mut game: ResMut<Game>, mut diagnostics: ResMut<Diagnostics>) {
    if !diagnostics.demo || game.busy || game.fatal || time.elapsed_secs() < diagnostics.next_step {
        return;
    }
    diagnostics.next_step = time.elapsed_secs() + 0.35;
    // Turn when refused, and now and then anyway, to see more of the level.
    let wander = (time.elapsed_secs() * 1000.0) as usize;
    if !game.status.is_empty() || wander % 11 == 0 {
        diagnostics.heading = (diagnostics.heading + 1 + wander % 5) % 8;
    }
    if net.0.lock().unwrap().commands.send(Cmd::Move(COMPASS[diagnostics.heading])).is_ok() {
        game.busy = true;
        game.status.clear();
    }
}

/// With --capture DIR, save a numbered screenshot every few seconds.
fn capture(
    mut commands: Commands,
    time: Res<Time>,
    mut diagnostics: ResMut<Diagnostics>,
    mut exit: EventWriter<AppExit>,
) {
    if diagnostics.quit_after.is_some_and(|t| time.elapsed_secs() > t) {
        exit.write(AppExit::Success);
    }
    let Some(dir) = diagnostics.capture.clone() else {
        return;
    };
    if time.elapsed_secs() < diagnostics.next_shot.max(3.0) {
        return;
    }
    diagnostics.next_shot = time.elapsed_secs() + diagnostics.capture_every.unwrap_or(4.0);
    diagnostics.shots += 1;
    let path = dir.join(format!("shot-{:02}.png", diagnostics.shots));
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}
