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
    render::{
        primitives::Aabb,
        view::screenshot::{save_to_disk, Screenshot},
    },
    window::WindowResolution,
};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

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
const BEASTS: [&str; 8] = [
    "yeti",
    "emu",
    "snake",
    "quagga",
    "black unicorn",
    "centaur",
    "ember wisp",
    "ice monster",
];
/// The Rogue scenario's own people and beasts, from their concept sheets.
const ROGUE_FOLK: [&str; 7] = [
    "orc",
    "troll",
    "leprechaun",
    "nymph",
    "medusa",
    "phantom",
    "wraith",
];
const ROGUE_BEASTS: [&str; 9] = [
    "bat",
    "kestrel",
    "griffin",
    "dragon",
    "jabberwock",
    "aquator",
    "xeroc",
    "venus flytrap",
    "rattlesnake",
];
/// Each row performs as one group, in this order.
const ROWS: [&[&str]; 4] = [&PEOPLE, &BEASTS, &ROGUE_FOLK, &ROGUE_BEASTS];
const GROUP_TAGS: [&str; 4] = ["people", "beasts", "rogue-folk", "rogue-beasts"];
const GROUPS: usize = ROWS.len();
const WALK_TIME: f32 = 6.0;
const MOVES_AT: f32 = 7.0;
const CYCLE: f32 = 40.0;
/// Close-up time for each creature after the group cycles.
const PORTRAIT: f32 = 2.6;
/// Seconds each row stands at rest with `--idles`.
const IDLE: f32 = 12.0;

/// Every move, in the order the groups perform them.
const MOVES: [Move; 19] = [
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
    Move::Breath(Accent::Ember),
    Move::Gaze,
    Move::Bite(Accent::Teal),
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
        .map(|(i, m)| (MOVES_AT + i as f32 * 1.38, Cue::Do(*m)))
        .collect();
    cues.extend([
        (34.0, Cue::Hit),
        (34.8, Cue::Miss),
        (35.6, Cue::Die),
        (39.2, Cue::Revive),
    ]);
    cues
}

fn label(m: Move) -> String {
    format!("{m:?}").to_lowercase().replace(['(', ')'], "-")
}

/// Screenshot moments in each group cycle: each move as it lands.
fn shots() -> Vec<(f32, String)> {
    let mut shots = vec![
        (0.6, "walk-start".to_string()),
        (3.0, "walk".to_string()),
        (5.6, "walk-brake".to_string()),
        (6.6, "walk-settled".to_string()),
    ];
    for (i, m) in MOVES.iter().enumerate() {
        let at = MOVES_AT + i as f32 * 1.38;
        shots.push((at + m.impact() * m.duration() + 0.03, label(*m)));
    }
    shots.extend([
        (34.1, "hurt".to_string()),
        (35.0, "dodge".to_string()),
        (37.8, "dead".to_string()),
    ]);
    shots
}

/// A bounded manual study; root travel still drives the ordinary animation.
fn travel_study_offset(t: f32) -> Vec3 {
    if t < WALK_TIME {
        let progress = (t / WALK_TIME).clamp(0.0, 1.0);
        let a = std::f32::consts::TAU * progress * progress * (3.0 - 2.0 * progress);
        return Vec3::new(1.0 - a.cos(), 0.0, a.sin()) * 0.45;
    }
    if (7.0..13.0).contains(&t) {
        let u = t - 7.0;
        // Accelerate for 1.5s, hold 3.2 cells/s for 3s, then brake for 1.5s.
        // Four complete shuttles end at home; the straight legs expose trot
        // without the constant extreme yaw of a fast circle on this small stage.
        let distance = if u < 1.5 {
            (3.2 / 3.0) * u * u
        } else if u < 4.5 {
            2.4 + 3.2 * (u - 1.5)
        } else {
            let v = u - 4.5;
            12.0 + 3.2 * v - (3.2 / 3.0) * v * v
        };
        let q = (distance + 0.9).rem_euclid(3.6);
        return Vec3::Z * if q <= 1.8 { q - 0.9 } else { 2.7 - q };
    }
    // Linear legs and repeated positions deliberately keep velocity changes
    // abrupt. All points stay within .9 cells; interpolation preserves the bound.
    const WAYPOINTS: &[(f32, Vec2)] = &[
        (13.0, Vec2::ZERO),
        (14.0, Vec2::ZERO),
        (14.5, Vec2::new(0.6, 0.0)),
        (15.0, Vec2::new(0.6, 0.6)),
        (15.5, Vec2::new(0.0, 0.6)),
        (16.0, Vec2::ZERO),
        (16.6, Vec2::new(0.0, 0.72)),
        (16.78, Vec2::new(0.0, 0.72)),
        (16.93, Vec2::new(0.0, 0.9)),
        (17.68, Vec2::ZERO),
        (18.0, Vec2::ZERO),
        (18.6, Vec2::new(0.0, 0.72)),
        (19.0, Vec2::new(0.0, 0.72)),
        (19.6, Vec2::ZERO),
        (22.0, Vec2::ZERO),
        (22.6, Vec2::new(0.0, 0.72)),
        (22.8, Vec2::new(0.0, 0.72)),
        (23.4, Vec2::ZERO),
        (24.0, Vec2::ZERO),
        (24.6, Vec2::new(0.0, 0.72)),
        (24.8, Vec2::new(0.0, 0.72)),
        (25.4, Vec2::ZERO),
        (26.0, Vec2::ZERO),
        (26.6, Vec2::new(0.0, 0.72)),
        (32.4, Vec2::new(0.0, 0.72)),
        (33.0, Vec2::ZERO),
        (CYCLE, Vec2::ZERO),
    ];
    for leg in WAYPOINTS.windows(2) {
        let [(start, from), (end, to)] = leg else {
            continue;
        };
        if t >= *start && t < *end {
            let at = from.lerp(*to, (t - *start) / (*end - *start));
            return Vec3::new(at.x, 0.0, at.y);
        }
    }
    Vec3::ZERO
}

fn travel_study_schedule() -> Vec<(f32, Cue)> {
    vec![
        (18.6, Cue::Do(Move::Heavy)),
        (22.6, Cue::Hit),
        (24.6, Cue::Miss),
        (26.6, Cue::Die),
        (32.0, Cue::Revive),
    ]
}

fn travel_study_shots() -> Vec<(f32, String)> {
    [
        (3.0, "slow-curve"),
        (6.6, "slow-settled"),
        (8.0, "accelerating"),
        (9.5, "fast-shuttle"),
        (11.0, "fast-reversal"),
        (12.5, "braking"),
        (13.6, "fast-settled"),
        (15.1, "sharp-corner"),
        (16.7, "abrupt-stop"),
        (16.85, "restart"),
        (17.15, "reverse"),
        (19.2, "moving-heavy"),
        (20.5, "action-settled"),
        (22.7, "hurt"),
        (23.0, "hurt-restart"),
        (24.7, "dodge"),
        (25.0, "dodge-restart"),
        (27.0, "falling"),
        (31.0, "dead"),
        (32.7, "revived-travel"),
        (34.0, "settled"),
    ]
    .into_iter()
    .map(|(at, name)| (at, format!("travel-{name}")))
    .collect()
}

/// Each creature's signature move, for its portrait.
fn signature(name: &str) -> Move {
    match name {
        "Warrior" => Move::Combo(false),
        "Mage" | "red arcanist" | "ember wisp" => Move::Cast(Accent::Ember),
        "blue sentinel" | "red sentinel" | "stone guardian" | "yeti" | "black unicorn" => {
            Move::Heavy
        }
        "ruin scout" => Move::Thrust,
        "vampire" => Move::Drain,
        "ice monster" => Move::Touch(Accent::Travel),
        "zombie" => Move::Hook,
        "hobgoblin" => Move::Bash,
        "troll" | "jabberwock" => Move::Combo(false),
        "leprechaun" => Move::Touch(Accent::Gold),
        "nymph" => Move::Hex,
        "medusa" => Move::Gaze,
        "wraith" => Move::Drain,
        "dragon" => Move::Breath(Accent::Ember),
        "aquator" => Move::Touch(Accent::Amber),
        "xeroc" => Move::Shove,
        "rattlesnake" => Move::Bite(Accent::Teal),
        _ => Move::Strike,
    }
}

#[derive(Component)]
struct Pacer {
    home: Vec3,
    height: f32,
    name: &'static str,
    index: usize,
    row: usize,
}

#[derive(Component)]
/// A name label: where it hangs, its text, and its row.
struct Caption(Vec3, String, usize);

#[derive(Resource, Default)]
struct Director {
    last: f32,
    shot: usize,
    next_capture: f32,
    study_time: f32,
}

pub fn run(diagnostics: Diagnostics) {
    let mut app = App::new();
    app.add_plugins(crate::graphics::plugins().set(WindowPlugin {
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
        color: Color::srgb(0.5, 0.6, 0.85),
        brightness: 110.0,
        ..default()
    })
    .insert_resource(creatures::Boredom(if diagnostics.bored {
        1000.0
    } else {
        0.0
    }))
    .insert_resource(diagnostics)
    .init_resource::<Director>()
    .add_systems(Startup, setup)
    .add_systems(
        Startup,
        |mut commands: Commands, diagnostics: Res<Diagnostics>| {
            commands.insert_resource(Dress {
                wardrobe: diagnostics.outfit,
                since: 0.0,
            });
        },
    )
    .init_resource::<Lineup>()
    .add_systems(
        Update,
        (
            direct,
            lineup,
            redress,
            creatures::animate,
            creatures::snuff,
            creatures::effects,
            camera,
            captions,
        )
            .chain(),
    )
    .add_systems(Last, crate::props::maintain_cache);
    load_internal_asset!(app, crate::cave::SHADER, "cave.wgsl", Shader::from_wgsl);
    app.run();
}

fn spot(row: usize, i: usize) -> Vec3 {
    match row {
        0 => Vec3::new(i as f32 * 1.6, -0.5, 0.0),
        1 => Vec3::new(1.0 + i as f32 * 1.75, -0.5, ROW_DEPTH),
        2 => Vec3::new(1.5 + i as f32 * 2.0, -0.5, ROW_DEPTH * 2.0),
        _ => Vec3::new(i as f32 * 1.85, -0.5, ROW_DEPTH * 3.0),
    }
}

/// Distance between the rows of the stage.
const ROW_DEPTH: f32 = 4.5;

/// Where a row's first creature falls in the whole roster.
fn row_start(row: usize) -> usize {
    ROWS[..row].iter().map(|names| names.len()).sum()
}

/// A roster index as its row and its place in that row.
fn row_of(index: usize) -> (usize, usize) {
    let mut start = 0;
    for (row, names) in ROWS.iter().enumerate() {
        if index < start + names.len() {
            return (row, index - start);
        }
        start += names.len();
    }
    (GROUPS - 1, ROWS[GROUPS - 1].len() - 1)
}

fn everyone() -> Vec<&'static str> {
    ROWS.iter()
        .flat_map(|names| names.iter().copied())
        .collect()
}

/// The line-up roster: everyone, or only the families asked for by name.
fn roster(diagnostics: &Diagnostics) -> Vec<&'static str> {
    everyone()
        .into_iter()
        .filter(|name| {
            diagnostics.families.is_empty()
                || diagnostics.families.iter().any(|family| family == name)
        })
        .collect()
}

/// What the bestiary's humanoids are wearing, and since when.
#[derive(Resource)]
struct Dress {
    wardrobe: creatures::Wardrobe,
    since: f32,
}

/// A creature as it was made, before it was dressed, and what it holds.
#[derive(Component)]
struct Wearer {
    look: Look,
    sword: bool,
}

impl Wearer {
    /// Build its model dressed from the wardrobe under `entity`, armed as before.
    fn dress(
        &self,
        commands: &mut Commands,
        library: &mut PropLibrary,
        meshes: &mut Assets<Mesh>,
        entity: Entity,
        wardrobe: creatures::Wardrobe,
    ) -> creatures::Rig {
        let look = self.look.clone().dressed(wardrobe);
        let rig = creatures::spawn(commands, library, meshes, entity, &look);
        if self.sword {
            let sword = library.item(tor_client_3d_model::ItemModel::Greatsword, meshes);
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
        rig
    }
}

/// Swap every humanoid into the next outfit on O, or on the diagnostics'
/// cycle. Only the model is replaced: each keeps its motion, pose clock and
/// place, so the change lands mid-stride.
#[allow(clippy::too_many_arguments)]
fn redress(
    mut commands: Commands,
    time: Res<Time<Real>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    diagnostics: Res<Diagnostics>,
    dress: Option<ResMut<Dress>>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    wearers: Query<(Entity, &Wearer, &creatures::Rig)>,
) {
    let Some(mut dress) = dress else {
        return;
    };
    let now = time.elapsed_secs();
    let pressed = keys.is_some_and(|keys| keys.just_pressed(KeyCode::KeyO));
    let due = diagnostics
        .wardrobe_cycle
        .is_some_and(|every| now - dress.since >= every);
    if !pressed && !due {
        return;
    }
    dress.wardrobe = dress.wardrobe.next();
    dress.since = now;
    let mut redressed = 0;
    for (entity, wearer, rig) in &wearers {
        if !wearer.look.dresses() {
            continue;
        }
        if let Some(model) = rig.joint(creatures::J::Lift) {
            commands.entity(model).despawn();
        }
        let rig = wearer.dress(
            &mut commands,
            &mut library,
            &mut meshes,
            entity,
            dress.wardrobe,
        );
        commands.entity(entity).insert(rig);
        redressed += 1;
    }
    eprintln!(
        "Wardrobe redressed {redressed} humanoids as {:?}",
        dress.wardrobe
    );
}

fn setup(
    diagnostics: Res<Diagnostics>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
) {
    let profile_started = crate::props::profile_builds().then(std::time::Instant::now);
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    let mut solids = HashMap::new();
    let mut open = HashSet::new();
    for x in -3..=17 {
        for z in -3..=17 {
            let key = (x as u64) << 32 ^ (z as u64) ^ 0x51;
            solids.insert(
                IVec3::new(x, -1, z),
                Solid {
                    seen: true,
                    lit: true,
                    key,
                },
            );
            if z == -3 {
                for y in 0..2 {
                    solids.insert(
                        IVec3::new(x, y, z),
                        Solid {
                            seen: true,
                            lit: true,
                            key: key ^ 7,
                        },
                    );
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
    for chunk in cave.update(solids, open.clone(), open, blocked) {
        if let Some(mesh) = cave.build(chunk).rock {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(rock.clone())));
        }
    }
    let cave_ms = profile_started.map(|time| time.elapsed().as_secs_f64() * 1000.0);
    let cast_started = profile_started.map(|_| std::time::Instant::now());
    let rows: Vec<(usize, &[&str])> = if diagnostics.lineups {
        Vec::new()
    } else {
        ROWS.iter().copied().enumerate().collect()
    };
    for (row, names) in rows {
        for (i, name) in names.iter().enumerate() {
            let home = spot(row, i);
            let look = Look::preview(
                crate::showcase::creature(name),
                *name == "Warrior",
                creatures::seed(name),
            );
            let mut anim = Anim::new(creatures::seed(name), home);
            // The travel path's initial tangent is +Z. Keep the acceleration
            // preview free of a random initial turn; live spawn headings differ.
            anim.yaw = 0.0;
            anim.target_yaw = Some(0.0);
            let entity = commands
                .spawn((
                    Pacer {
                        home,
                        height: look.height(),
                        name,
                        index: row_start(row) + i,
                        row,
                    },
                    anim,
                    Transform::from_translation(home),
                    Visibility::default(),
                ))
                .id();
            let wearer = Wearer {
                look,
                sword: *name == "Warrior",
            };
            let rig = wearer.dress(
                &mut commands,
                &mut library,
                &mut meshes,
                entity,
                diagnostics.outfit,
            );
            commands.entity(entity).insert((rig, wearer));
            commands.spawn((
                Caption(home + Vec3::new(0.0, 0.0, 0.7), name.to_string(), row),
                Text::new(*name),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.82, 0.74)),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ));
        }
    }
    for x in [1.0, 7.0, 13.0] {
        for z in [1.8, 6.0, 10.5, 15.0] {
            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.75, 0.48),
                    intensity: 45_000.0,
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
                end: 40.0,
            },
            ..default()
        },
        Transform::from_xyz(7.0, 4.0, 9.0).looking_at(Vec3::new(7.0, 0.5, 0.0), Vec3::Y),
    ));
    crate::studio_lights(&mut commands);
    commands.insert_resource(library);
    if let (Some(started), Some(cave), Some(cast)) = (profile_started, cave_ms, cast_started) {
        eprintln!(
            "3d_bestiary_build cave_ms={cave:.3} cast_ms={:.3} total_ms={:.3}",
            cast.elapsed().as_secs_f64() * 1000.0,
            started.elapsed().as_secs_f64() * 1000.0,
        );
    }
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
    let warmed = (time.elapsed_secs() - diagnostics.capture_warmup).max(0.0);
    if diagnostics.travel_study && warmed > 0.0 {
        // Match animate's delta cap: a capture hitch must not become a root
        // placement that clears contacts instead of exercising the fast gait.
        director.study_time += time.delta_secs().min(0.05).min(warmed);
    }
    let elapsed = if diagnostics.travel_study {
        director.study_time
    } else {
        warmed
    };
    let limit = if diagnostics.travel_study {
        Some(
            diagnostics
                .quit_after
                .unwrap_or(CYCLE * GROUPS as f32)
                .min(CYCLE * GROUPS as f32),
        )
    } else {
        diagnostics.quit_after
    };
    if let Some(limit) = limit {
        let finished = if diagnostics.travel_study {
            elapsed >= limit
        } else {
            elapsed > limit
        };
        if finished {
            if pending.is_empty() {
                exit.write(AppExit::Success);
            } else if elapsed > limit + 10.0 {
                exit.write(AppExit::error());
            }
            return;
        }
    }
    let now = clock(&time, &diagnostics, &director);
    let last = director.last;
    let crossed = |at: f32| last < at && now >= at;
    let group = (now / CYCLE) as usize;
    let mut shots_now: Vec<String> = Vec::new();
    if diagnostics.idles {
        // Everyone at rest, one row at a time, with a shot every so often.
        let showing = ((now / IDLE) as usize).min(GROUPS - 1);
        for (pacer, mut anim, _, mut visibility) in &mut actors {
            anim.target_yaw = Some(0.0);
            *visibility = shown(pacer.row == showing);
        }
        let mut at = 1.0;
        while at < IDLE * GROUPS as f32 {
            if crossed(at) {
                let tag = GROUP_TAGS[((at / IDLE) as usize).min(GROUPS - 1)];
                shots_now.push(format!("idle-{tag}-{at:04.1}"));
            }
            at += 1.1;
        }
        if diagnostics.capture.is_some() && now > IDLE * GROUPS as f32 + 0.5 && pending.is_empty() {
            exit.write(AppExit::Success);
        }
    } else if group < GROUPS {
        let base = group as f32 * CYCLE;
        let t = now - base;
        let study_offset = diagnostics.travel_study.then(|| travel_study_offset(t));
        let cues = if diagnostics.travel_study {
            travel_study_schedule()
        } else {
            schedule()
        };
        for (pacer, mut anim, mut transform, mut visibility) in &mut actors {
            // Only the performing row is shown, so no other row stands
            // between it and the camera.
            *visibility = shown(pacer.row == group);
            // Both paths return home and keep neighbours apart. The study
            // retains heading through stops to expose actual contact recovery.
            if let Some(offset) = study_offset {
                transform.translation = pacer.home + offset;
            } else if t < WALK_TIME {
                let progress = (t / WALK_TIME).clamp(0.0, 1.0);
                let a = std::f32::consts::TAU * progress * progress * (3.0 - 2.0 * progress);
                transform.translation = pacer.home + Vec3::new(1.0 - a.cos(), 0.0, a.sin()) * 0.45;
            } else {
                transform.translation = pacer.home;
                anim.target_yaw = Some(0.0);
            }
            for &(at, cue) in &cues {
                if crossed(base + at) {
                    perform(
                        &mut commands,
                        &mut library,
                        &mut meshes,
                        pacer,
                        transform.translation,
                        &mut anim,
                        cue,
                    );
                }
            }
        }
        let tag = GROUP_TAGS[group];
        let moments = if diagnostics.travel_study {
            travel_study_shots()
        } else {
            shots()
        };
        for (at, name) in moments {
            if crossed(base + at) {
                shots_now.push(format!("{tag}-{name}"));
            }
        }
    } else {
        // Portraits: each creature close up, at rest and then in its signature move.
        let start = CYCLE * GROUPS as f32;
        let current = ((now - start) / PORTRAIT).max(0.0) as usize;
        for (pacer, mut anim, mut transform, mut visibility) in &mut actors {
            transform.translation = pacer.home;
            anim.target_yaw = Some(0.0);
            if diagnostics.photos {
                // Alone, and turned to show a flank as well as the face.
                *visibility = if pacer.index == current {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
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
            // Neighbours may stand in front; show only the creature in view.
            *visibility = shown(pacer.index == current);
            if crossed(start) {
                anim.dead = None;
            }
            let at = start + pacer.index as f32 * PORTRAIT;
            let m = signature(pacer.name);
            let tag = format!(
                "portrait-{:02}-{}",
                pacer.index,
                pacer.name.replace(' ', "_")
            );
            if crossed(at + 0.7) {
                shots_now.push(format!("{tag}-rest"));
            }
            if crossed(at + 0.9) {
                perform(
                    &mut commands,
                    &mut library,
                    &mut meshes,
                    pacer,
                    transform.translation,
                    &mut anim,
                    Cue::Do(m),
                );
            }
            if crossed(at + 0.9 + m.impact() * m.duration() + 0.03) {
                shots_now.push(format!("{tag}-{}", label(m)));
            }
        }
        // Leave only once every screenshot has been read back from the GPU.
        if diagnostics.capture.is_some()
            && now > start + everyone().len() as f32 * PORTRAIT + 1.0
            && pending.is_empty()
        {
            exit.write(AppExit::Success);
        }
    }
    if let Some(interval) = diagnostics.capture_every {
        shots_now.clear();
        if interval.is_finite()
            && interval > 0.0
            && time.elapsed_secs() >= diagnostics.capture_warmup
            && now >= director.next_capture
            && pending.is_empty()
        {
            shots_now.push(format!("motion-{now:08.3}"));
            director.next_capture = now + interval.max(1.0 / 60.0);
        }
    }
    if let Some(dir) = diagnostics.capture.clone() {
        for name in shots_now {
            director.shot += 1;
            let path = dir.join(format!("{:03}-{name}.png", director.shot));
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
    }
    director.last = now;
}

fn perform(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    pacer: &Pacer,
    origin: Vec3,
    anim: &mut Anim,
    cue: Cue,
) {
    match cue {
        Cue::Do(m) => {
            anim.act(m);
            let target = (origin + Vec3::Z * 1.4, 1.6);
            creatures::stage(
                commands,
                library,
                meshes,
                m,
                (origin, pacer.height),
                Some(target),
                true,
            );
        }
        Cue::Hit => anim.hit(0.0),
        Cue::Miss => anim.miss(0.0),
        Cue::Die => anim.dead = Some(0.0),
        Cue::Revive => anim.dead = None,
    }
}

/// `--lineups`: each creature in turn, five individuals of it side by side,
/// women and men alternating among people. `--fallen-lineups` reuses those
/// individuals in their settled corpse poses.
#[derive(Resource, Default)]
struct Lineup {
    elapsed: f32,
    shown: Option<usize>,
    group: Vec<Entity>,
    shot: usize,
    height: f32,
    fallen_bounds: Vec<(Vec3, Vec3)>,
}

/// Seconds each line-up stands before the next.
const LINEUP: f32 = 4.0;
const LINEUP_SHOTS: [(f32, &str, usize); 3] =
    [(1.9, "line", 0), (2.8, "close-a", 1), (3.8, "close-b", 2)];

#[allow(clippy::too_many_arguments)]
fn lineup(
    mut commands: Commands,
    time: Res<Time<Real>>,
    diagnostics: Res<Diagnostics>,
    mut state: ResMut<Lineup>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut camera: Query<(&mut Transform, &Projection), With<Camera3d>>,
    children: Query<&Children>,
    parts: Query<(&GlobalTransform, &Aabb)>,
    pending: Query<(), With<Screenshot>>,
    mut exit: EventWriter<AppExit>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
) {
    if !diagnostics.lineups {
        return;
    }
    let names = roster(&diagnostics);
    // Preview deadlines use real time: animation time clamps slow frames to
    // 250 ms, making a four-second capture take minutes on software DX12.
    // Stop at each view and family boundary so a long frame cannot skip a
    // pose, capture a replacement family, or queue several readbacks at once.
    let next_family = state.shot / LINEUP_SHOTS.len();
    let changing_family = state.shown.is_some_and(|shown| next_family > shown);
    let next_moment = if changing_family {
        0.0
    } else {
        LINEUP_SHOTS[state.shot % LINEUP_SHOTS.len()].0
    };
    let deadline = diagnostics.capture_warmup + next_family as f32 * LINEUP + next_moment;
    if pending.is_empty() {
        state.elapsed = (state.elapsed + time.delta_secs()).min(deadline);
    }
    let index = if changing_family && state.elapsed >= deadline {
        next_family
    } else {
        state.shown.unwrap_or(0)
    };
    let now = (state.elapsed - diagnostics.capture_warmup).max(0.0);
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
        state.fallen_bounds.clear();
        // Pick seeds so people alternate woman, man, woman...
        let base = creatures::seed(name);
        let mut seeds = Vec::new();
        let mut j = 0u64;
        while seeds.len() < 5 {
            let seed = base.wrapping_add(j * 7919);
            j += 1;
            let look = Look::preview(crate::showcase::creature(name), false, seed);
            let want = seeds.len() % 2 == 0;
            if look.woman() == want
                || j > 60
                || !matches!(name, n if PEOPLE.contains(&n) || ROGUE_FOLK.contains(&n) || n == "centaur")
            {
                seeds.push(seed);
            }
        }
        let mut height: f32 = 0.0;
        for (k, seed) in seeds.into_iter().enumerate() {
            let look = Look::preview(crate::showcase::creature(name), false, seed);
            height = height.max(look.height());
            let home = Vec3::new(3.4 + k as f32 * 1.6, -0.5, 1.2);
            let mut anim = Anim::new(seed, home);
            anim.yaw = -0.45;
            anim.target_yaw = Some(-0.45);
            anim.quiet = true;
            if diagnostics.fallen_lineups {
                anim.dead = Some(1.0);
                anim.target_yaw = None;
            }
            let entity = commands
                .spawn((
                    anim,
                    Transform::from_translation(home),
                    Visibility::default(),
                ))
                .id();
            let wearer = Wearer { look, sword: false };
            let rig = wearer.dress(
                &mut commands,
                &mut library,
                &mut meshes,
                entity,
                diagnostics.outfit,
            );
            commands.entity(entity).insert((rig, wearer));
            state.group.push(entity);
        }
        state.height = height;
    }
    // The whole line first, then close on the first two individuals.
    let t = now - index as f32 * LINEUP;
    let height = state.height;
    let close = t > 2.1;
    if diagnostics.fallen_lineups && t > 0.2 && state.fallen_bounds.len() != state.group.len() {
        // Global transforms and mesh bounds become available after the first
        // posed frame. Cache the settled geometry once for this lineup.
        let bounds: Vec<_> = state
            .group
            .iter()
            .filter_map(|entity| fallen_bounds(*entity, &children, &parts))
            .collect();
        if bounds.len() == state.group.len() {
            state.fallen_bounds = bounds;
        }
    }
    if let Ok((mut transform, projection)) = camera.single_mut() {
        let (focus, offset) = if diagnostics.fallen_lineups {
            fallen_lineup_camera(&state, t, projection, &diagnostics)
        } else if close {
            // Each of the first two in turn, the upper body filling the frame.
            let x = if t > 3.0 { 5.0 } else { 3.4 };
            (
                Vec3::new(x, -0.5 + height * 0.78, 1.2),
                Vec3::new(0.0, 0.22, 1.0) * (height * 0.75).max(1.1),
            )
        } else {
            (
                Vec3::new(6.6, -0.5 + height * 0.5, 1.2),
                Vec3::new(0.0, 0.22, 1.0) * (height * 1.9).max(6.4),
            )
        };
        *transform = diagnostics.preview_camera(focus, offset, &mut cave_materials);
    }
    for (when, tag, step) in LINEUP_SHOTS {
        let wanted = index * LINEUP_SHOTS.len() + step;
        let due = diagnostics.capture_warmup + index as f32 * LINEUP + when;
        if state.elapsed >= due && state.shot <= wanted {
            state.shot = wanted + 1;
            if let Some(dir) = diagnostics.capture.clone() {
                let mode = if diagnostics.fallen_lineups {
                    "fallen-"
                } else {
                    ""
                };
                let path = dir.join(format!(
                    "{:02}-{mode}{tag}-{}.png",
                    index,
                    name.replace(' ', "_")
                ));
                info!(
                    "Lineup capture family={name} view={tag} real_secs={:.3}",
                    time.elapsed_secs()
                );
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(path));
            }
        }
    }
}

/// World-space bounds include each joint's actual corpse rotation, the
/// Storybook mesh proportions, clothing and held props.
fn fallen_bounds(
    entity: Entity,
    children: &Query<&Children>,
    parts: &Query<(&GlobalTransform, &Aabb)>,
) -> Option<(Vec3, Vec3)> {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for child in children.iter_descendants(entity) {
        let Ok((transform, bounds)) = parts.get(child) else {
            continue;
        };
        let center = Vec3::from(bounds.center);
        let half = Vec3::from(bounds.half_extents);
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let point = transform
                        .affine()
                        .transform_point3(center + half * Vec3::new(x, y, z));
                    low = low.min(point);
                    high = high.max(point);
                }
            }
        }
    }
    low.is_finite().then_some((low, high))
}

fn fallen_lineup_camera(
    state: &Lineup,
    t: f32,
    projection: &Projection,
    diagnostics: &Diagnostics,
) -> (Vec3, Vec3) {
    let close = t > 2.1;
    let bounds = if state.fallen_bounds.len() == state.group.len() {
        if close {
            state.fallen_bounds[usize::from(t > 3.0)]
        } else {
            state.fallen_bounds.iter().copied().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(low, high), (a, b)| (low.min(a), high.max(b)),
            )
        }
    } else {
        // Low framing while transforms are still propagating; captures occur
        // later, after the settled mesh bounds have replaced this fallback.
        let x = if close {
            if t > 3.0 {
                5.0
            } else {
                3.4
            }
        } else {
            6.6
        };
        let half = Vec3::new(if close { state.height } else { 4.2 }, 0.5, state.height);
        let center = Vec3::new(x, -0.25, 1.2);
        (center - half, center + half)
    };
    let focus = (bounds.0 + bounds.1) * 0.5;
    let half = (bounds.1 - bounds.0) * 0.5;
    let base_direction = Vec3::new(0.0, 0.7, 1.0).normalize();
    let direction = Quat::from_rotation_y(diagnostics.azimuth.unwrap_or(0.0)) * base_direction;
    let right = Vec3::Y.cross(direction).normalize();
    let up = direction.cross(right);
    let (tan_y, aspect) = match projection {
        Projection::Perspective(p) => ((p.fov * 0.5).tan(), p.aspect_ratio),
        _ => ((std::f32::consts::FRAC_PI_4 * 0.5).tan(), 1.6),
    };
    let width_fit = right.abs().dot(half) / (tan_y * aspect).max(0.01);
    let height_fit = up.abs().dot(half) / tan_y.max(0.01);
    let distance = direction.abs().dot(half) + width_fit.max(height_fit) * 1.15 + 0.1;
    (focus, base_direction * distance.max(1.1))
}

/// Seconds into the show; `--portraits` starts at the close-ups.
fn clock(time: &Time, diagnostics: &Diagnostics, director: &Director) -> f32 {
    if diagnostics.travel_study {
        return director.study_time;
    }
    (time.elapsed_secs() - diagnostics.capture_warmup).max(0.0)
        + if diagnostics.portraits {
            CYCLE * GROUPS as f32
        } else {
            0.0
        }
}

fn camera(
    time: Res<Time>,
    diagnostics: Res<Diagnostics>,
    director: Res<Director>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
) {
    if diagnostics.lineups {
        return;
    }
    let now = clock(&time, &diagnostics, &director);
    let group = if diagnostics.idles {
        ((now / IDLE) as usize).min(GROUPS - 1)
    } else if diagnostics.travel_study {
        ((now / CYCLE) as usize).min(GROUPS - 1)
    } else {
        (now / CYCLE) as usize
    };
    let (focus, offset) = if group < GROUPS {
        // Frame the whole row from its first to its last creature.
        let names = ROWS[group];
        let (first, last) = (spot(group, 0), spot(group, names.len() - 1));
        let (focus, width) = match group {
            0 => (Vec3::new(7.2, 0.45, 0.0), 14.4),
            1 => (Vec3::new(7.1, 0.35, ROW_DEPTH), 17.0),
            _ => (
                Vec3::new((first.x + last.x) * 0.5, 0.4, first.z),
                (last.x - first.x) * 1.25 + 3.0,
            ),
        };
        let d = diagnostics.distance.unwrap_or(width * 0.62);
        (focus, Vec3::new(0.0, d * 0.42, d * 0.9))
    } else {
        let names = everyone();
        let index = (((now - CYCLE * GROUPS as f32) / PORTRAIT) as usize).min(names.len() - 1);
        let (row, i) = row_of(index);
        let home = spot(row, i);
        let name = names[index];
        let height = Look::preview(
            crate::showcase::creature(name),
            name == "Warrior",
            creatures::seed(name),
        )
        .height();
        let d = (height * 1.45).max(2.4);
        (
            home + Vec3::Y * height * 0.5,
            Vec3::new(d * 0.35, d * 0.3, d * 0.9),
        )
    };
    if let Ok(mut transform) = camera.single_mut() {
        *transform = diagnostics.preview_camera(focus, offset, &mut cave_materials);
    }
}

fn shown(visible: bool) -> Visibility {
    if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn captions(
    time: Res<Time>,
    diagnostics: Res<Diagnostics>,
    director: Res<Director>,
    mut captions: Query<(&Caption, &mut Node, &mut Visibility)>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    let Ok((camera, eye)) = camera.single() else {
        return;
    };
    let now = clock(&time, &diagnostics, &director);
    let portraits = !diagnostics.travel_study && now >= CYCLE * GROUPS as f32;
    let row = if diagnostics.idles {
        ((now / IDLE) as usize).min(GROUPS - 1)
    } else {
        ((now / CYCLE) as usize).min(GROUPS - 1)
    };
    for (caption, mut node, mut visibility) in &mut captions {
        *visibility = shown(!portraits && caption.2 == row);
        if let Ok(at) = camera.world_to_viewport(eye, caption.0) {
            node.left = Val::Px(at.x - caption.1.len() as f32 * 3.4);
            node.top = Val::Px(at.y);
        }
    }
}
