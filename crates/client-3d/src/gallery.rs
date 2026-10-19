//! `--gallery`: every prop laid out on a stretch of cave floor, lit as in
//! play, for reviewing the art without a server.
use crate::{
    capture,
    cave::{CaveExtension, CaveMaterial, CaveParams, CaveWorld, Solid},
    props::{PropLibrary, PropMaterials},
    Diagnostics, FOG,
};
use bevy::{
    asset::load_internal_asset,
    core_pipeline::{bloom::Bloom, tonemapping::Tonemapping},
    pbr::{DistanceFog, FogFalloff, MaterialPlugin},
    prelude::*,
    window::WindowResolution,
};
use std::collections::{HashMap, HashSet};
use tor_protocol::ItemClass;

const SAMPLES: [(&str, ItemClass); 21] = [
    ("mace", ItemClass::Weapon),
    ("iron greatsword", ItemClass::Weapon),
    ("dagger", ItemClass::Weapon),
    ("ring mail", ItemClass::Armor),
    ("clean bandage", ItemClass::Misc),
    ("food ration", ItemClass::Food),
    ("emu corpse", ItemClass::Corpse),
    ("dawn seal", ItemClass::Misc),
    ("copper token", ItemClass::Misc),
    ("silver token", ItemClass::Misc),
    ("iron token", ItemClass::Misc),
    ("stone tablet", ItemClass::Misc),
    ("bubbling potion", ItemClass::Potion),
    ("gold ring", ItemClass::Ring),
    ("amulet", ItemClass::Amulet),
    ("scroll", ItemClass::Scroll),
    ("spellbook", ItemClass::Spellbook),
    ("wand", ItemClass::Wand),
    ("ruby", ItemClass::Gem),
    ("pickaxe", ItemClass::Tool),
    ("pouch", ItemClass::Misc),
];

#[derive(Component)]
struct Caption(Vec3, &'static str);

/// Every creature in the scenarios, for the corpse rows.
const CORPSES: [&str; 18] = [
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
    "yeti",
    "emu",
    "snake",
    "quagga",
    "black unicorn",
    "centaur",
    "ember wisp",
    "ice monster",
];

pub fn run(diagnostics: Diagnostics) {
    let mut app = App::new();
    app.add_plugins(crate::graphics::plugins().set(WindowPlugin {
        primary_window: Some(Window {
            title: "Thresholds of Ruin — prop gallery".into(),
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
    .insert_resource(diagnostics)
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            tour,
            crate::creatures::animate,
            crate::creatures::snuff,
            captions,
            capture,
        )
            .chain(),
    )
    .add_systems(Last, crate::props::maintain_cache);
    load_internal_asset!(app, crate::cave::SHADER, "cave.wgsl", Shader::from_wgsl);
    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
    diagnostics: Res<Diagnostics>,
) {
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    // A floor of rock with a wall behind, and nothing cut away.
    let mut solids = HashMap::new();
    let mut open = HashSet::new();
    let mut blocked = HashSet::new();
    for x in -3..=14 {
        for z in -3..=9 {
            let key = (x as u64) << 32 ^ (z as u64);
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
                            key: key ^ 99,
                        },
                    );
                }
            } else {
                open.insert(IVec3::new(x, 0, z));
            }
        }
    }
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
    let spot = |i: usize| IVec3::new((i % 7) as i32 + 2, 0, (i / 7) as i32 + 1);
    for i in 0..SAMPLES.len() {
        blocked.insert(spot(i));
    }
    for cell in [
        IVec3::new(-1, 0, 0),
        IVec3::new(-1, 0, 2),
        IVec3::new(10, 0, 0),
        IVec3::new(10, 0, 2),
    ] {
        blocked.insert(cell);
    }
    let chunks: HashSet<IVec3> = cave.update(solids, open.clone(), open, blocked);
    for chunk in chunks {
        let built = cave.build(chunk);
        if let Some(mesh) = built.rock {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(rock.clone())));
        }
        let glow = [
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.95, 0.85),
                emissive: LinearRgba::rgb(0.5, 2.6, 2.1),
                ..default()
            }),
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.75, 0.5, 1.0),
                emissive: LinearRgba::rgb(1.7, 0.7, 2.8),
                ..default()
            }),
        ];
        for (mesh, material) in built.crystals.into_iter().zip(glow) {
            if let Some(mesh) = mesh {
                commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material)));
            }
        }
        for (at, glow) in built.lights {
            commands.spawn((
                PointLight {
                    color: glow.light(),
                    intensity: 9_000.0,
                    range: 4.5,
                    ..default()
                },
                Transform::from_translation(at),
            ));
        }
    }
    let floor = -0.5;
    for (i, (name, _class)) in SAMPLES.iter().enumerate() {
        let at = spot(i).as_vec3().with_y(floor);
        let model = library.item(crate::showcase::item(name), &mut meshes);
        commands
            .spawn((
                Transform::from_translation(at).with_rotation(Quat::from_rotation_y(0.4)),
                Visibility::default(),
            ))
            .with_children(|p| model.attach(p));
        commands.spawn((
            Caption(at + Vec3::new(0.0, 0.0, 0.42), name),
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
    let frame = library.door_frame(&mut meshes);
    let leaf = library.door_leaf(&mut meshes);
    for (cell, open, label) in [
        (IVec3::new(-1, 0, 0), false, "door"),
        (IVec3::new(-1, 0, 2), true, "open door"),
    ] {
        let at = cell.as_vec3().with_y(floor);
        commands
            .spawn((
                Transform::from_translation(at).with_rotation(Quat::from_rotation_y(0.3)),
                Visibility::default(),
            ))
            .with_children(|p| {
                frame.attach(p);
                p.spawn((
                    Transform::from_xyz(-0.41, 0.0, 0.0)
                        .with_rotation(Quat::from_rotation_y(if open { -1.5 } else { 0.0 })),
                    Visibility::default(),
                ))
                .with_children(|l| leaf.attach(l));
            });
        commands.spawn((
            Caption(at + Vec3::new(0.0, 0.0, 0.5), label),
            Text::new(label),
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
    // Every creature's corpse, built as the game builds a "<creature> corpse" item.
    for (i, name) in CORPSES.iter().enumerate() {
        let at = Vec3::new(
            0.5 + (i % 9) as f32 * 1.5,
            floor,
            5.0 + (i / 9) as f32 * 2.2,
        );
        let item = format!("{name} corpse");
        crate::creatures::spawn_corpse(
            &mut commands,
            &mut library,
            &mut meshes,
            crate::showcase::creature(name),
            at,
            0.0,
            i as u64,
        );
        commands.spawn((
            Caption(at + Vec3::new(0.0, 0.0, 0.7), name),
            Text::new(item),
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
    let up = library.stairs_up(&mut meshes);
    let down = library.stairs_down(&mut meshes);
    for (cell, model, label) in [
        (IVec3::new(10, 0, 0), up, "stairs up"),
        (IVec3::new(10, 0, 2), down, "stairs down"),
    ] {
        let at = cell.as_vec3().with_y(floor);
        commands
            .spawn((Transform::from_translation(at), Visibility::default()))
            .with_children(|p| model.attach(p));
        commands.spawn((
            Caption(at + Vec3::new(0.0, 0.0, 0.55), label),
            Text::new(label),
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
    let distance = diagnostics.distance.unwrap_or(9.0);
    let focus = Vec3::new(4.5, -0.3, 1.2);
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
                start: 14.0,
                end: 40.0,
            },
            ..default()
        },
        Transform::from_translation(focus + Vec3::new(0.0, distance * 0.82, distance * 0.58))
            .looking_at(focus, Vec3::Y),
    ));
    for x in [0.5, 4.5, 8.5] {
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.75, 0.48),
                intensity: 55_000.0,
                range: 14.0,
                shadows_enabled: true,
                ..default()
            },
            Transform::from_xyz(x, 1.8, 2.6),
        ));
    }
    crate::studio_lights(&mut commands);
    commands.insert_resource(library);
}

/// Views the gallery steps through, one per screenshot: the whole room,
/// then each stretch of props close up.
const TOUR: [(Vec3, f32); 13] = [
    (Vec3::new(4.5, -0.3, 1.2), 9.0),
    (Vec3::new(3.0, -0.5, 1.0), 2.6),
    (Vec3::new(6.5, -0.5, 1.0), 2.6),
    (Vec3::new(3.0, -0.5, 2.0), 2.6),
    (Vec3::new(6.5, -0.5, 2.0), 2.6),
    (Vec3::new(3.0, -0.5, 3.0), 2.6),
    (Vec3::new(6.5, -0.5, 3.0), 2.6),
    (Vec3::new(-1.0, 0.3, 1.2), 4.8),
    (Vec3::new(10.0, 0.0, 1.2), 4.8),
    (Vec3::new(2.5, -0.4, 5.0), 3.6),
    (Vec3::new(9.5, -0.4, 5.0), 3.6),
    (Vec3::new(2.5, -0.4, 7.2), 3.6),
    (Vec3::new(9.5, -0.4, 7.2), 3.6),
];

fn tour(
    time: Res<Time>,
    diagnostics: Res<Diagnostics>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
    mut cave_materials: ResMut<Assets<CaveMaterial>>,
) {
    let step = ((time.elapsed_secs() - 1.0).max(0.0) / 4.0) as usize % TOUR.len();
    let (focus, distance) = TOUR[step];
    if let Ok(mut transform) = camera.single_mut() {
        *transform = diagnostics.preview_camera(
            focus,
            Vec3::new(0.0, distance * 0.82, distance * 0.58),
            &mut cave_materials,
        );
    }
}

fn captions(
    mut captions: Query<(&Caption, &mut Node)>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    let Ok((camera, eye)) = camera.single() else {
        return;
    };
    for (caption, mut node) in &mut captions {
        if let Ok(at) = camera.world_to_viewport(eye, caption.0) {
            node.left = Val::Px(at.x - caption.1.len() as f32 * 3.4);
            node.top = Val::Px(at.y);
        }
    }
}
