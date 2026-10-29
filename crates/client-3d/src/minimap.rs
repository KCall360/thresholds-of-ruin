//! A north-up mini-map of the character's level: what is seen and
//! remembered, doors and stairs, items, creatures, and the way the camera
//! faces. `M` opens the larger map and journal.

use crate::{CameraRig, Game};
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::collections::HashMap;

/// Cells shown each way from the character.
const REACH: i32 = 22;
/// Pixels per cell.
const CELL: i32 = 5;
const SIZE: i32 = (REACH * 2 + 1) * CELL;

#[derive(Resource)]
pub(crate) struct MiniMap {
    image: Handle<Image>,
    /// The view and camera heading last drawn.
    drawn: Option<(u64, f32)>,
}

#[derive(Component)]
pub(crate) struct MiniMapFrame;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Ground {
    Wall,
    Floor,
    Object,
    Door,
    Up,
    Down,
    Trap,
}

impl Ground {
    pub(crate) fn color(self) -> [f32; 3] {
        match self {
            Self::Floor => [0.62, 0.52, 0.38],
            Self::Object => [0.95, 0.78, 0.3],
            Self::Wall => [0.3, 0.32, 0.4],
            Self::Door => [0.86, 0.6, 0.24],
            Self::Up => [0.36, 0.84, 0.86],
            Self::Down => [0.4, 0.55, 0.95],
            // Orange, as the ASCII map marks a discovered trap.
            Self::Trap => [0.98, 0.5, 0.16],
        }
    }
}

pub(crate) fn terrain_kind(cell: &tor_client_3d_model::Terrain<'_>, dz: i32) -> Option<Ground> {
    Some(if cell.stairs_up || cell.stairs_down {
        match cell.object_model {
            Some(tor_client_3d_model::ItemModel::Stairs) => Ground::Down,
            Some(_) => Ground::Object,
            None if cell.stairs_up => Ground::Up,
            None => Ground::Down,
        }
    } else if cell.door.is_some() {
        Ground::Door
    } else if cell.trap.is_some() && dz == 0 {
        Ground::Trap
    } else if cell.wall && dz == 0 {
        Ground::Wall
    } else if !cell.wall && dz == 0 {
        Ground::Floor
    } else {
        return None;
    })
}

fn item_color(model: tor_client_3d_model::ItemModel) -> [f32; 3] {
    if model == tor_client_3d_model::ItemModel::Stairs {
        [0.4, 0.55, 0.95]
    } else {
        [0.95, 0.78, 0.3]
    }
}

pub(crate) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut image = Image::new_fill(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let image = images.add(image);
    commands
        .spawn((
            MiniMapFrame,
            crate::chrome::BlockWorld,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.0),
                top: Val::Px(16.0),
                width: Val::Px(SIZE as f32 + 8.0),
                height: Val::Px(SIZE as f32 + 8.0),
                padding: UiRect::all(Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            crate::chrome::frame(),
        ))
        .with_children(|frame| {
            frame.spawn((
                ImageNode::new(image.clone()),
                Node {
                    width: Val::Px(SIZE as f32),
                    height: Val::Px(SIZE as f32),
                    ..default()
                },
            ));
            frame.spawn((
                Text::new("N"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.82, 0.6)),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(SIZE as f32 * 0.5),
                    top: Val::Px(-1.0),
                    ..default()
                },
            ));
        });
    commands.insert_resource(MiniMap { image, drawn: None });
}

pub(crate) fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    net: Res<crate::Net>,
    mut game: ResMut<Game>,
    panel: Res<crate::interact::Panel>,
) {
    if panel.keyboard_captured || (panel.open && panel.search_focused) {
        return;
    }
    if keys.just_pressed(KeyCode::KeyM)
        && game
            .screen
            .is_none_or(|open| open == crate::screen::Screen::Places { renaming: None })
    {
        crate::screen::toggle(
            &net,
            &mut game,
            crate::screen::Screen::Places { renaming: None },
        );
    }
}

pub(crate) fn draw(
    game: Res<Game>,
    rig: Res<CameraRig>,
    mut map: ResMut<MiniMap>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(view) = &game.view else {
        return;
    };
    let now = (game.view_serial, rig.yaw);
    if map
        .drawn
        .is_some_and(|(serial, yaw)| serial == now.0 && (yaw - now.1).abs() < 0.01)
    {
        return;
    }
    map.drawn = Some(now);
    let Some(image) = images.get_mut(&map.image) else {
        return;
    };
    let Some(data) = image.data.as_mut() else {
        return;
    };
    let feet = view.feet;

    // What lies in each column of the character's level; seen beats remembered.
    let mut ground: HashMap<(i32, i32), (Ground, bool)> = HashMap::new();
    let cells = view.terrain();
    for cell in cells {
        let seen = cell.seen;
        let (dx, dy, dz) = (
            cell.position.x - feet.x,
            cell.position.y - feet.y,
            cell.position.z - feet.z,
        );
        if dx.abs() > REACH || dy.abs() > REACH || !(-1..=1).contains(&dz) {
            continue;
        }
        let Some(kind) = terrain_kind(&cell, dz) else {
            continue;
        };
        let entry = ground.entry((dx, dy)).or_insert((kind, seen));
        if kind > entry.0 {
            entry.0 = kind;
        }
        entry.1 |= seen;
    }

    let mut canvas = Canvas(data);
    canvas.clear();
    for (&(dx, dy), &(kind, seen)) in &ground {
        let lit = if seen { 1.0 } else { 0.5 };
        let color = kind.color();
        canvas.cell(dx, dy, color.map(|c| c * lit), 0.92);
    }

    // The camera's view, as a soft wedge from the character.
    let forward = Vec2::new(rig.yaw.sin(), -rig.yaw.cos());
    let centre = Vec2::splat(SIZE as f32 * 0.5);
    let reach = CELL as f32 * 6.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - centre;
            let length = d.length();
            if length < 2.0 || length > reach {
                continue;
            }
            let along = d.normalize().dot(forward);
            if along > 0.8 {
                let fade = (1.0 - length / reach) * ((along - 0.8) / 0.2).min(1.0);
                canvas.blend(x, y, [1.0, 0.95, 0.8], 0.4 * fade);
            }
        }
    }

    for item in &view.items {
        let (dx, dy) = (item.position.x - feet.x, item.position.y - feet.y);
        if (item.position.z - feet.z).abs() <= 1 {
            canvas.dot(dx, dy, 1, item_color(item.model));
        }
    }
    for actor in view.actors.iter().filter(|a| !a.is_self) {
        let (dx, dy) = (actor.position.x - feet.x, actor.position.y - feet.y);
        if (actor.position.z - feet.z).abs() <= 2 {
            let color = if actor.hostile {
                [0.95, 0.28, 0.22]
            } else {
                [0.85, 0.85, 0.78]
            };
            canvas.dot(dx, dy, 2, color);
        }
    }
    canvas.dot(0, 0, 2, [0.35, 0.95, 0.85]);
    canvas.mask();
}

/// The mini-map's pixels.
struct Canvas<'a>(&'a mut Vec<u8>);

impl Canvas<'_> {
    fn clear(&mut self) {
        for pixel in self.0.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[8, 10, 14, 150]);
        }
    }

    fn put(&mut self, x: i32, y: i32, color: [f32; 3], alpha: f32) {
        if !(0..SIZE).contains(&x) || !(0..SIZE).contains(&y) {
            return;
        }
        let i = ((y * SIZE + x) * 4) as usize;
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0) as u8;
        self.0[i..i + 4].copy_from_slice(&[
            byte(color[0]),
            byte(color[1]),
            byte(color[2]),
            byte(alpha),
        ]);
    }

    fn blend(&mut self, x: i32, y: i32, color: [f32; 3], amount: f32) {
        let i = ((y * SIZE + x) * 4) as usize;
        for (c, channel) in color.into_iter().enumerate() {
            let old = f32::from(self.0[i + c]) / 255.0;
            self.0[i + c] = ((old + (channel - old) * amount) * 255.0) as u8;
        }
        self.0[i + 3] = self.0[i + 3].max((amount * 255.0) as u8);
    }

    /// Fill a cell, leaving a hairline gap so the grid reads.
    fn cell(&mut self, dx: i32, dy: i32, color: [f32; 3], alpha: f32) {
        let (x0, y0) = ((dx + REACH) * CELL, (dy + REACH) * CELL);
        for y in 0..CELL {
            for x in 0..CELL {
                let edge = x == CELL - 1 || y == CELL - 1;
                let shade = if edge { 0.82 } else { 1.0 };
                self.put(x0 + x, y0 + y, color.map(|c| c * shade), alpha);
            }
        }
    }

    /// A round marker centred on a cell, `radius` pixels beyond its middle.
    fn dot(&mut self, dx: i32, dy: i32, radius: i32, color: [f32; 3]) {
        let (cx, cy) = (
            (dx + REACH) * CELL + CELL / 2,
            (dy + REACH) * CELL + CELL / 2,
        );
        let r = radius as f32 + 0.5;
        for y in -radius - 1..=radius + 1 {
            for x in -radius - 1..=radius + 1 {
                let d = ((x * x + y * y) as f32).sqrt();
                if d <= r {
                    self.put(cx + x, cy + y, color, 1.0);
                } else if d <= r + 1.0 {
                    self.put(cx + x, cy + y, [0.05, 0.05, 0.07], 1.0);
                }
            }
        }
    }

    /// Round the map off, fading at the rim.
    fn mask(&mut self) {
        let half = SIZE as f32 * 0.5;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let d = Vec2::new(x as f32 + 0.5 - half, y as f32 + 0.5 - half).length() / half;
                let i = ((y * SIZE + x) * 4 + 3) as usize;
                if d > 1.0 {
                    self.0[i] = 0;
                } else if d > 0.9 {
                    self.0[i] = (f32::from(self.0[i]) * (1.0 - d) / 0.1) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod cosmetic_tests {
    use super::*;
    use tor_client_3d_model::{ItemModel, Terrain};
    use tor_protocol::{ItemClass, ObjectPresentation, ObjectPresentationKind, Position};

    #[test]
    fn map_shortcut_opens_the_combined_places_screen_without_a_world_action() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut app = App::new();
        app.insert_resource(net)
            .init_resource::<Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, toggle);
        app.world_mut().spawn((MiniMapFrame, Visibility::Inherited));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyM);
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(crate::screen::Screen::Places { renaming: None })
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn typing_fields_do_not_toggle_the_map_and_the_shortcut_returns_afterwards() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut app = App::new();
        app.insert_resource(net)
            .init_resource::<Game>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, toggle);
        for (screen, search, captured) in [
            (None, true, false),
            (None, false, true),
            (Some(crate::screen::Screen::History), false, false),
            (
                Some(crate::screen::Screen::Places { renaming: Some(0) }),
                false,
                false,
            ),
            (
                Some(crate::screen::Screen::Note {
                    revision: 1,
                    private: true,
                }),
                false,
                false,
            ),
        ] {
            app.world_mut().resource_mut::<Game>().screen = screen;
            {
                let mut panel = app.world_mut().resource_mut::<crate::interact::Panel>();
                panel.open = search;
                panel.search_focused = search;
                panel.keyboard_captured = captured;
            }
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyM);
            app.update();
            assert_eq!(
                app.world().resource::<Game>().screen,
                screen,
                "typing fields retain M"
            );
        }
        app.world_mut().resource_mut::<Game>().screen = None;
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(crate::screen::Screen::Places { renaming: None })
        );
        app.update();
        assert_eq!(app.world().resource::<Game>().screen, None);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_discovered_trap_is_marked_on_its_level_only() {
        let trap = Terrain {
            object_presentation: None,
            object_model: None,
            key: "trap",
            position: Position { x: 1, y: 0, z: 0 },
            wall: false,
            stairs_up: false,
            stairs_down: false,
            door: None,
            trap: Some(None),
            seen: true,
            lit: true,
        };
        assert_eq!(terrain_kind(&trap, 0), Some(Ground::Trap));
        assert_eq!(terrain_kind(&trap, 1), None);
    }

    #[test]
    fn sampled_stair_colors_leave_factual_geometry_and_navigation_unchanged() {
        let mut cell = Terrain {
            object_presentation: None,
            object_model: None,
            key: "stairs",
            position: Position { x: 1, y: 0, z: 0 },
            wall: false,
            stairs_up: true,
            stairs_down: false,
            door: None,
            trap: None,
            seen: true,
            lit: true,
        };
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Up));
        let potion = ObjectPresentation {
            name: "flask".into(),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        };
        cell.object_presentation = Some(&potion);
        cell.object_model = Some(ItemModel::Potion);
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Object));
        assert!(cell.stairs_up && !cell.stairs_down);
        let stairs = ObjectPresentation {
            name: "stairs".into(),
            asset: None,
            kind: ObjectPresentationKind::Stairs,
        };
        cell.object_presentation = Some(&stairs);
        cell.object_model = Some(ItemModel::Stairs);
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Down));
        assert_eq!(item_color(ItemModel::Stairs), [0.4, 0.55, 0.95]);
        assert_eq!(item_color(ItemModel::Potion), [0.95, 0.78, 0.3]);
        cell.object_model = None;
        cell.object_presentation = None;
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Up));
        // Even a malformed cosmetic-only cell cannot manufacture stair topology.
        cell.stairs_up = false;
        cell.object_model = Some(ItemModel::Stairs);
        assert_eq!(terrain_kind(&cell, 0), Some(Ground::Floor));
    }
}
