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
mod diagnostics;
mod gallery;
mod graphics;
mod interact;
mod launch;
mod lighting;
mod minimap;
#[doc(hidden)]
pub mod model_benchmark;
mod net;
mod picking;
mod props;
#[cfg(test)]
mod render_contracts;
mod review_clock;
mod review_script;
mod sdf;
mod showcase;
#[cfg(test)]
mod test_support;
use diagnostics::capture;
mod camera;
mod scene;
use camera::{follow_actors, input, rig_camera, torch_flicker, update_cave_params};
use scene::{animate_props, react, receive, stand, studio_lights, tend_the_dead};

use bevy::{
    asset::load_internal_asset,
    core_pipeline::{bloom::Bloom, tonemapping::Tonemapping},
    input::mouse::{MouseMotion, MouseWheel},
    pbr::{DistanceFog, FogFalloff, MaterialPlugin},
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::WindowResolution,
};
use cave::{CaveExtension, CaveMaterial, CaveParams, CaveWorld, Solid};
use creatures::{Anim, Look, Move, J};
use net::{Cmd, Event as NetEvent, Happening, Input, Network, View};
use props::{PropLibrary, PropMaterials};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tor_client_3d_model::{CreatureModel, ItemModel};
#[cfg(test)]
use tor_protocol::CellView;
use tor_protocol::{ActorId, ActorTarget, Direction, ItemClass, Position};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

const HELP: &str = "WASD/arrows: move   Q/E: turn   wheel: zoom   click: act   G: pick up   \
X: door   I: pack   Space: wait   R/F: up/down   M: map   Tab: control   Esc: close/quit";

pub fn run() -> Result<(), Error> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.iter().any(|arg| arg == "--live-playlist") {
        return review_script::live::cli(&arguments);
    }
    if arguments.iter().any(|arg| arg == "--scene-script") {
        return review_script::cli(&arguments);
    }
    let mut connect: Option<SocketAddr> = None;
    let mut actor = ActorId(1);
    let mut observe = false;
    let mut scenario = launch::repo_root().join("scenarios/rogue-exploration");
    let mut save = None;
    let mut save_root = launch::repo_root().join(".local/client-3d/saves");
    let mut seed = None;
    let exe = if cfg!(windows) {
        "tor-server.exe"
    } else {
        "tor-server"
    };
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
            "--save-root" => save_root = PathBuf::from(value()?),
            "--seed" => seed = Some(value()?.parse()?),
            "--server-bin" => server_bin = PathBuf::from(value()?),
            "--capture" => diagnostics.capture = Some(PathBuf::from(value()?)),
            "--capture-warmup" => {
                let seconds: f32 = value()?.parse()?;
                if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
                    return Err("--capture-warmup must be between 0 and 60 seconds".into());
                }
                diagnostics.capture_warmup = seconds;
            }
            "--demo" => diagnostics.demo = true,
            "--quit-after" => diagnostics.quit_after = Some(value()?.parse()?),
            "--review-step-hz" => {
                let hz = value()?.parse()?;
                review_clock::ReviewStep::new(hz)?;
                diagnostics.review_step_hz = Some(hz);
            }
            "--capture-every" => diagnostics.capture_every = Some(value()?.parse()?),
            "--distance" => diagnostics.distance = Some(value()?.parse()?),
            "--azimuth" => {
                let degrees: f32 = value()?.parse()?;
                if !degrees.is_finite() {
                    return Err("--azimuth must be a finite number of degrees".into());
                }
                diagnostics.azimuth = Some(degrees.rem_euclid(360.0).to_radians());
            }
            "--turn" => {
                let degrees: f32 = value()?.parse()?;
                if !degrees.is_finite() {
                    return Err("--turn must be a finite number of degrees".into());
                }
                diagnostics.turn = degrees.to_radians();
            }
            "--gallery" => gallery = true,
            "--bestiary" => bestiary = true,
            "--travel-study" => {
                bestiary = true;
                diagnostics.travel_study = true;
            }
            "--portraits" => {
                bestiary = true;
                diagnostics.portraits = true;
            }
            "--bored" => diagnostics.bored = true,
            "--appearance" => {
                // Offline bestiary look: "woman" or "man", optionally ":N".
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
            "--outfit" => {
                // Dress every humanoid in the bestiary from the wardrobe.
                let name = value()?;
                diagnostics.outfit = creatures::Wardrobe::parse(&name)
                    .ok_or_else(|| format!("unknown outfit {name}"))?;
            }
            "--wear" => {
                diagnostics.equipment.select(&value()?)?;
            }
            "--wardrobe-cycle" => {
                // Redress the bestiary's humanoids every so many seconds.
                diagnostics.wardrobe_cycle = Some(value()?.parse()?);
            }
            "--family" => {
                diagnostics.families = value()?
                    .split(',')
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            "--jump" => {
                bestiary = true;
                diagnostics.lineups = true;
                diagnostics.jump = true;
            }
            "--bust-study" => {
                bestiary = true;
                diagnostics.lineups = true;
                diagnostics.women = true;
                diagnostics.bust_study = Some(match value()?.as_str() {
                    "contours" => BustStudy::Contours,
                    "volumes" => BustStudy::Volumes,
                    other => return Err(format!("unknown bust study {other:?}").into()),
                });
            }
            "--antic" => {
                bestiary = true;
                diagnostics.lineups = true;
                diagnostics.lively = true;
                diagnostics.bored = true;
                diagnostics.antic = Some(value()?.parse()?);
            }
            "--lively" => {
                bestiary = true;
                diagnostics.lineups = true;
                diagnostics.lively = true;
            }
            "--women" => {
                bestiary = true;
                diagnostics.lineups = true;
                diagnostics.women = true;
            }
            "--fallen-lineups" => {
                bestiary = true;
                diagnostics.lineups = true;
                diagnostics.fallen_lineups = true;
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
                    "tor-client-3d [--scenario DIR] [--save FILE | --save-root DIR] [--seed N] [--server-bin EXE]\n\
                     tor-client-3d --connect 127.0.0.1:PORT [--actor 1] [--observe]  (token from TOR_SERVER_TOKEN)\n\
                     Offline: --bestiary or --gallery [--azimuth DEGREES]\n\
                     Model review: --lineups [--women] [--jump] [--lively] [--antic N] [--turn DEGREES] [--bust-study contours|volumes] or --fallen-lineups [--capture DIR]\n\
                     Wardrobe: --outfit NAME [--wear helm] [--wear boots] [--wear cloak] [--wear breastplate] [--wardrobe-cycle SECS]\n\
                     Motion review: --travel-study [--capture DIR --capture-every SECS]\n\
                     Without --connect it starts a private server with a fresh save.\n{HELP}"
                );
                return Ok(());
            }
        }
    }
    if diagnostics.azimuth.is_some() && !gallery && !bestiary {
        return Err("--azimuth is available only with --bestiary or --gallery".into());
    }
    if diagnostics.fallen_lineups
        && (gallery || diagnostics.travel_study || diagnostics.idles || diagnostics.portraits)
    {
        return Err(
            "--fallen-lineups cannot be combined with --gallery, --travel-study, --idles, --antics, --portraits or --photos"
                .into(),
        );
    }
    if diagnostics.travel_study
        && (gallery || diagnostics.lineups || diagnostics.idles || diagnostics.portraits)
    {
        return Err(
            "--travel-study cannot be combined with --gallery, --lineups, --fallen-lineups, --idles, --antics, --portraits or --photos"
                .into(),
        );
    }
    if !diagnostics.equipment.is_empty() && (connect.is_some() || gallery || !bestiary) {
        return Err(
            "--wear is available only in the offline bestiary; live equipment is server-owned"
                .into(),
        );
    }
    if creatures::PLAYER_LOOK.get().is_some() && (gallery || !bestiary) {
        return Err(
            "--appearance is available only with --bestiary; live appearance is server-owned"
                .into(),
        );
    }
    if diagnostics.review_step_hz.is_some()
        && (connect.is_some() || gallery || !diagnostics.lineups)
    {
        return Err("--review-step-hz requires offline --lineups".into());
    }
    if gallery {
        gallery::run(diagnostics);
        return Ok(());
    }
    if bestiary {
        bestiary::run(diagnostics);
        return Ok(());
    }
    let mut server: Option<launch::OwnedChild> = None;
    let (address, token) = match connect {
        Some(address) => (address, std::env::var("TOR_SERVER_TOKEN")?),
        None => {
            let credentials = launch::Credentials::new();
            let started =
                launch::start(&server_bin, &scenario, save, seed, &save_root, &credentials)?;
            println!(
                "{}",
                serde_json::json!({"type":"local_server", "address":started.address,
                                           "save":started.save, "pid": started.child.id()})
            );
            server = Some(started.child);
            (started.address, credentials.owner)
        }
    };
    let network = Arc::new(Mutex::new(Network::start(address, token, actor, observe)));

    let mut app = App::new();
    app.add_plugins(graphics::plugins().set(WindowPlugin {
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
        color: Color::srgb(0.5, 0.6, 0.85),
        brightness: 190.0,
        ..default()
    })
    .insert_resource(Net(network.clone()))
    .insert_resource(CameraRig {
        distance: diagnostics.distance.unwrap_or(8.5),
        ..default()
    })
    .insert_resource(creatures::Boredom(if diagnostics.bored {
        creatures::BORED_AFTER - 5.0
    } else {
        0.0
    }))
    .insert_resource(diagnostics)
    .insert_resource(Game {
        busy: true,
        ..default()
    })
    .init_resource::<interact::Hover>()
    .init_resource::<interact::Panel>()
    .init_resource::<interact::Pursuit>()
    .insert_resource(autoplay::Autoplay::new(autoplay, hunt))
    .add_systems(
        Startup,
        (setup, interact::setup, minimap::setup, cursor::setup).chain(),
    )
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
            creatures::soften,
            creatures::snuff,
            creatures::effects,
            tend_the_dead,
            (
                animate_props,
                torch_flicker,
                lighting::effects,
                lighting::shade,
                lighting::lamps,
                hud,
            )
                .chain(),
        )
            .chain(),
    )
    .add_systems(
        Update,
        (
            demo,
            capture.after(lighting::lamps),
            autoplay::run,
            autoplay::hunt,
            minimap::toggle,
            minimap::draw,
            boredom,
            cursor::update,
        ),
    )
    .add_systems(Last, (props::maintain_cache, shutdown_on_exit));
    load_internal_asset!(app, cave::SHADER, "cave.wgsl", Shader::from_wgsl);
    let exit = app.run();
    let network_result = network.lock().unwrap().shutdown();

    let server_result = server.as_mut().map_or(Ok(()), launch::OwnedChild::stop);
    network_result?;
    server_result?;
    if exit.is_success() {
        Ok(())
    } else {
        Err("3D client failed".into())
    }
}

const FOG: Color = Color::srgb(0.02, 0.022, 0.035);

/// Which aspect of the chest a controlled line-up varies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BustStudy {
    /// Each contour family in turn, at her kind's typical volume.
    Contours,
    /// Each volume step in turn, in her kind's typical contour.
    Volumes,
}

/// Process-test aids: periodic screenshots and a wandering character.
#[derive(Resource, Default)]
pub(crate) struct Diagnostics {
    capture: Option<PathBuf>,
    demo: bool,
    quit_after: Option<f32>,
    distance: Option<f32>,
    /// Offline camera orbit, in radians relative to the standard showcase view.
    azimuth: Option<f32>,
    /// Line-up individuals turned from their usual facing, in radians:
    /// a quarter turn shows each in profile.
    turn: f32,
    /// Seconds between screenshots with --capture.
    capture_every: Option<f32>,
    review_step_hz: Option<u32>,
    /// In the bestiary, go straight to the close-ups.
    portraits: bool,
    /// Let offline bestiary shaders warm up before starting its capture clock.
    capture_warmup: f32,
    /// In the bestiary, show everyone standing at rest.
    idles: bool,
    /// Start as though the player had been away a while, for the antics.
    bored: bool,
    /// In the bestiary close-ups, show each creature alone, turned three-quarters.
    photos: bool,
    /// In the bestiary, line up several individuals of each creature in turn.
    lineups: bool,
    /// Use the same generated lineup individuals in their settled death poses.
    fallen_lineups: bool,
    /// Line up only women among people, for studying their figures.
    women: bool,
    /// Line-up individuals jump on the spot in turn, for studying motion.
    jump: bool,
    /// Line-up individuals keep their idle stances and fidgets.
    lively: bool,
    /// Line-up individuals all play this antic from the start, held long
    /// enough to see it through.
    antic: Option<u32>,
    /// Line up women alike but for one aspect of their chest.
    bust_study: Option<BustStudy>,
    /// Limit line-ups to these creatures, by their bestiary names.
    families: Vec<String>,
    /// What the bestiary's humanoids wear at first.
    outfit: creatures::Wardrobe,
    /// Independent visual equipment for offline composition review.
    equipment: creatures::Equipment,
    /// Redress them in the next outfit every so many seconds.
    wardrobe_cycle: Option<f32>,
    /// Offline travel, fast gait transitions, and interrupted contact recovery.
    travel_study: bool,
    shots: u32,
    next_shot: f32,
    pending_shots: u32,
    capture_failed: bool,
    next_step: f32,
    heading: usize,
}

impl Diagnostics {
    /// Keep offline orbit views and the cave's camera cutaway in agreement.
    fn preview_camera(
        &self,
        focus: Vec3,
        offset: Vec3,
        cave_materials: &mut Assets<CaveMaterial>,
    ) -> Transform {
        let offset = self
            .azimuth
            .map_or(offset, |angle| Quat::from_rotation_y(angle) * offset);
        let eye = focus + offset;
        if self.azimuth.is_some() {
            // Rear views can cross the stage's back wall. Reuse the normal
            // presentation cutaway, leaving known geometry and shadows intact.
            for (_, material) in cave_materials.iter_mut() {
                material.extension.params.player = (focus + Vec3::Y * 0.1).extend(-0.5);
                material.extension.params.camera = eye.extend(1.55);
            }
        }
        Transform::from_translation(eye).looking_at(focus, Vec3::Y)
    }
}

#[derive(Resource)]
struct Net(Arc<Mutex<Network>>);

/// What the window remembers between views.
#[derive(Resource, Default)]
struct Game {
    /// Added to observer-frame positions to give steady world positions.
    offset: IVec3,
    frame: tor_client_3d_model::SceneFrame,
    snap_camera: bool,
    view: Option<View>,
    feet_world: IVec3,
    cells: HashMap<IVec3, (Entity, CellLook)>,
    cave: CaveWorld,
    chunks: HashMap<IVec3, Entity>,
    door_leaves: HashMap<IVec3, Entity>,
    /// The weapon shown in each figure's hand: hand joint to recipe and model.
    held: HashMap<Entity, (ItemModel, Entity)>,
    /// Counts views, so panels know when to refresh.
    view_serial: u64,
    /// The weapon hand of each figure, for showing what it holds.
    hands: HashMap<Entity, Entity>,
    /// Creatures seen to die, kept where they fell as their own corpses.
    dying: Vec<DeadBody>,
    /// Corpse models by item, with the cell each lies in, kept between views.
    corpses: HashMap<tor_protocol::ItemTarget, CachedCorpse>,
    /// Combat events waiting to be acted out, and the revision they came from.
    pending: Vec<Happening>,
    last_revision: Option<u64>,
    actors: HashMap<ActorTarget, Entity>,
    labels: HashSet<Entity>,
    items: Vec<Entity>,
    log: VecDeque<String>,
    last_narration: Vec<String>,
    status: String,
    /// A device or ability waiting for what it is aimed at.
    aiming: Option<Aim>,
    busy: bool,
    fatal: bool,
    since_send: f32,
    /// Seconds of red torchlight left after taking a wound.
    hurt: f32,
}

impl Game {
    /// Input owns the context used to interpret it. The worker cannot upgrade it
    /// to a newer revision, control grant, stream, or branch.
    fn send(&mut self, net: &Net, command: Cmd) -> bool {
        if self.busy || self.fatal {
            return false;
        }
        let Some(view) = &self.view else {
            return false;
        };
        let input = Input {
            shown: view.shown.clone(),
            command,
        };
        if let Err(reason) = net.0.lock().unwrap().try_command(input) {
            self.status = reason.into();
            return false;
        }
        self.busy = true;
        self.since_send = 0.0;
        self.status.clear();
        true
    }
}

/// What a chosen device or ability still needs before it is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Aim {
    /// A directional device: the next movement input is its direction.
    Device(tor_protocol::ItemTarget),
    /// An ability: a direction for steps and rays, a clicked creature otherwise.
    Ability {
        id: String,
        target: tor_protocol::AbilityTargetKind,
    },
}

impl Aim {
    /// The command a movement input completes, if this aim takes directions.
    fn along(&self, direction: tor_protocol::Direction) -> Option<Cmd> {
        use tor_protocol::AbilityTargetKind as Kind;
        match self {
            Aim::Device(item) => Some(Cmd::Zap(*item, direction)),
            Aim::Ability {
                id,
                target: Kind::SelfStep | Kind::DirectionalRay,
            } => Some(Cmd::Ability(
                id.clone(),
                tor_client_3d_model::AbilityTarget::Direction(direction),
            )),
            Aim::Ability { .. } => None,
        }
    }

    /// The command a clicked creature completes, if this aim takes one.
    fn at(&self, actor: tor_protocol::ActorTarget) -> Option<Cmd> {
        use tor_protocol::AbilityTargetKind as Kind;
        match self {
            Aim::Ability {
                id,
                target: Kind::Actor | Kind::SelfOrAlly | Kind::SelfOnly,
            } => Some(Cmd::Ability(
                id.clone(),
                tor_client_3d_model::AbilityTarget::Actor(actor),
            )),
            _ => None,
        }
    }
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
    Object(ItemModel),
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
    appearance_seed: Option<tor_protocol::AppearanceSeed>,
    appearance_key: String,
    model: CreatureModel,
    /// Disclosed equipment it was built wearing; a change rebuilds it.
    wearing: tor_client_3d_model::Wearing,
    hostile: bool,
    goal: Vec3,
    label: Entity,
    ring: Entity,
    name: String,
    /// Head height, for the label and for aiming effects.
    height: f32,
}

#[derive(Component)]
struct HostileRing;

/// A found corpse retains its rig only while its disclosed source is unchanged.
struct CachedCorpse {
    entity: Entity,
    cell: IVec3,
    model: CreatureModel,
    appearance_seed: Option<tor_protocol::AppearanceSeed>,
}

/// A creature that died in view: its body stays as its corpse.
struct DeadBody {
    id: ActorTarget,
    entity: Entity,
    /// Complete grid cell where it fell, including elevation.
    cell: IVec3,
    appearance_seed: Option<tor_protocol::AppearanceSeed>,
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
        crate::graphics::camera_antialiasing(),
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

    let text = |size: f32, color: Color| {
        (
            TextFont {
                font_size: size,
                ..default()
            },
            TextColor(color),
        )
    };
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
    text.bytes().fold(1469598103934665603u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    })
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
    if moved
        || scrolled
        || keys.get_pressed().next().is_some()
        || mouse.get_pressed().next().is_some()
    {
        bored.0 = 0.0;
    } else {
        bored.0 += time.delta_secs();
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
            parts.push(
                if view.has_control {
                    "in control"
                } else {
                    "observing"
                }
                .to_string(),
            );
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

fn shutdown_on_exit(mut exits: EventReader<AppExit>, net: Res<Net>) {
    if exits.read().next().is_none() {
        return;
    }
    if let Err(error) = net.0.lock().unwrap().shutdown() {
        eprintln!("Could not save and close the connection: {error}");
    }
}

/// With --demo, wander: keep a heading until the server refuses it.
fn demo(
    time: Res<Time>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut diagnostics: ResMut<Diagnostics>,
) {
    if !diagnostics.demo || game.busy || game.fatal || time.elapsed_secs() < diagnostics.next_step {
        return;
    }
    diagnostics.next_step = time.elapsed_secs() + 0.35;
    // Turn when refused, and now and then anyway, to see more of the level.
    let wander = (time.elapsed_secs() * 1000.0) as usize;
    if !game.status.is_empty() || wander.is_multiple_of(11) {
        diagnostics.heading = (diagnostics.heading + 1 + wander % 5) % 8;
    }
    game.send(&net, Cmd::Move(COMPASS[diagnostics.heading]));
}
