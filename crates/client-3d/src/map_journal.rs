//! Combined map, known places and retained journal; backend facts stay authoritative.
use crate::{
    chrome,
    screen::{self, Screen},
    Game, Net,
};
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
mod raster;
pub(crate) mod route;
#[derive(Component)]
pub(crate) struct Root;
#[derive(Component)]
pub(crate) struct Body;
#[derive(Component)]
pub(crate) struct Picture;
#[derive(Component)]
pub(crate) struct Scroll;
#[derive(Component)]
pub(crate) struct MapPane;
#[derive(Component)]
pub(crate) struct JournalPane;
#[derive(Component, Clone)]
pub(crate) enum Action {
    Places,
    History,
    Note,
    Rename(String),
    Close,
}
#[derive(Resource)]
pub(crate) struct Map {
    image: Handle<Image>,
    serial: Option<u64>,
    fit: Option<raster::Fit>,
    route_generation: u64,
}
fn active(screen: Option<Screen>) -> bool {
    matches!(
        screen,
        Some(Screen::Places { .. } | Screen::History | Screen::Note { .. })
    )
}
fn text(parent: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32) {
    parent.spawn((
        Text::new(value),
        chrome::text(size, chrome::INK),
        TextLayout::new_with_linebreak(bevy::text::LineBreak::WordOrCharacter),
        Node {
            width: Val::Percent(100.),
            flex_shrink: 0.,
            ..default()
        },
    ));
}
fn button(parent: &mut ChildSpawnerCommands, label: impl Into<String>, action: Action) {
    parent
        .spawn((
            Button,
            action,
            BackgroundColor(chrome::BUTTON),
            BorderColor(chrome::GOLD),
            Node {
                padding: UiRect::axes(Val::Px(10.), Val::Px(7.)),
                border: UiRect::all(Val::Px(1.)),
                ..default()
            },
        ))
        .with_children(|b| {
            text(b, label, 18.);
        });
}
pub(crate) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.init_resource::<route::State>();
    let mut image = Image::new_fill(
        Extent3d {
            width: raster::SIZE as u32,
            height: raster::SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[8, 10, 14, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let image = images.add(image);
    commands.insert_resource(Map {
        image: image.clone(),
        serial: None,
        fit: None,
        route_generation: 0,
    });
    commands.spawn((Root,chrome::BlockWorld,Interaction::default(),chrome::frame(),GlobalZIndex(6),Visibility::Hidden,Node {position_type:PositionType::Absolute,left:Val::Px(16.),right:Val::Px(16.),top:Val::Px(16.),bottom:Val::Px(184.),padding:UiRect::all(Val::Px(16.)),border:UiRect::all(Val::Px(1.)),column_gap:Val::Px(18.),row_gap:Val::Px(12.),overflow:Overflow::clip(),..default()})).with_children(|root|{
  root.spawn((MapPane,Node {width:Val::Percent(60.),height:Val::Percent(100.),min_width:Val::Px(0.),flex_direction:FlexDirection::Column,row_gap:Val::Px(8.),..default()})).with_children(|map|{
   text(map,"MAP · NORTH UP",25.);
   map.spawn(Node {flex_grow:1.,min_height:Val::Px(0.),width:Val::Percent(100.),align_items:AlignItems::Center,justify_content:JustifyContent::Center,overflow:Overflow::clip(),..default()}).with_child((Picture,ImageNode::new(image),Node {height:Val::Percent(100.),max_width:Val::Percent(100.),aspect_ratio:Some(1.),..default()}));
   text(map,"Known current level · bright: visible · dim: remembered\nTeal: you / stairs up · blue: stairs down · gold: doors / items\nDark areas are unexplored. The map fits all known terrain.",16.);
  });
  root.spawn((JournalPane,Node {flex_grow:1.,width:Val::Percent(38.),height:Val::Percent(100.),min_width:Val::Px(0.),flex_direction:FlexDirection::Column,row_gap:Val::Px(12.),..default()})).with_children(|journal|{
   text(journal,"MAP & JOURNAL",25.);
   journal.spawn(Node {flex_wrap:FlexWrap::Wrap,column_gap:Val::Px(6.),row_gap:Val::Px(6.),flex_shrink:0.,..default()}).with_children(|tabs|{button(tabs,"F5 · Places",Action::Places);button(tabs,"F2 · History",Action::History);button(tabs,"F4 · Write note",Action::Note);button(tabs,"Close",Action::Close);});
   journal.spawn((Scroll,chrome::ScrollArea,ScrollPosition::default(),Node {flex_grow:1.,min_height:Val::Px(0.),width:Val::Percent(100.),overflow:Overflow::scroll_y(),..default()})).with_child((Body,Node {width:Val::Percent(100.),flex_direction:FlexDirection::Column,row_gap:Val::Px(10.),flex_shrink:0.,..default()}));
   journal.spawn((route::Root,Node{width:Val::Percent(100.),flex_direction:FlexDirection::Column,row_gap:Val::Px(6.),flex_shrink:0.,..default()}));
   text(journal,"Page Up / Down: scroll · Esc closes\nBrowsing takes no game time",16.);
  });
 });
}
pub(crate) fn buttons(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut buttons: Query<(&Interaction, &Action, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, action, mut color) in &mut buttons {
        color.0 = if *interaction == Interaction::None {
            chrome::BUTTON
        } else {
            chrome::HOT
        };
        if *interaction != Interaction::Pressed || !active(game.screen) {
            continue;
        }
        match action {
            Action::Close => {
                if let Some(open) = game.screen {
                    screen::toggle(&net, &mut game, open);
                }
            }
            Action::Places => {
                game.screen = Some(Screen::Places { renaming: None });
                game.typed.clear();
            }
            Action::History => {
                game.screen = Some(Screen::History);
                game.typed.clear();
            }
            Action::Note => {
                if let Some(revision) = game.view.as_ref().map(|v| v.revision) {
                    game.screen = Some(Screen::Note {
                        revision,
                        private: true,
                    });
                    game.typed.clear();
                }
            }
            Action::Rename(key) => {
                if let Some(index) = game
                    .view
                    .as_ref()
                    .and_then(|v| v.places.iter().position(|p| &p.key == key))
                {
                    game.screen = Some(Screen::Places {
                        renaming: Some(index),
                    });
                    game.typed.clear();
                }
            }
        }
    }
}
type PaneFilter = (Or<(With<MapPane>, With<JournalPane>)>, Without<Root>);
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Layout<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    roots: Query<'w, 's, (&'static mut Visibility, &'static mut Node), With<Root>>,
    panes: Query<'w, 's, (&'static mut Node, Option<&'static MapPane>), PaneFilter>,
    bodies: Query<'w, 's, Entity, With<Body>>,
    scrolls: Query<'w, 's, &'static mut ScrollPosition, With<Scroll>>,
}
type Stamp = Option<(Option<u64>, Option<Screen>, String)>;
pub(crate) fn show(
    mut commands: Commands,
    game: Res<Game>,
    mut map: ResMut<Map>,
    mut images: ResMut<Assets<Image>>,
    layout: Layout,
    mut stamp: Local<Stamp>,
    route: Option<Res<route::State>>,
) {
    let Layout {
        windows,
        mut roots,
        mut panes,
        bodies,
        mut scrolls,
    } = layout;
    let wanted = active(game.screen);
    let narrow = windows.single().is_ok_and(|w| w.width() < 900.);
    for (mut visibility, mut node) in &mut roots {
        visibility.set_if_neq(if wanted {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        node.flex_direction = if narrow {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        };
    }
    if !wanted {
        return;
    }
    for (mut node, is_map) in &mut panes {
        node.width = Val::Percent(if narrow {
            100.
        } else if is_map.is_some() {
            60.
        } else {
            38.
        });
        node.height = Val::Percent(if narrow && is_map.is_some() {
            40.
        } else if narrow {
            58.
        } else {
            100.
        });
    }
    let serial = game.view.as_ref().map(|_| game.view_serial);
    let generation = route.as_ref().map_or(0, |state| state.generation);
    if map.serial != serial || map.route_generation != generation {
        let selected = route
            .as_ref()
            .and_then(|state| state.selected.as_ref())
            .map(|selection| selection.destination.position);
        if let Some(image) = images.get_mut(&map.image) {
            let (data, fit) = game.view.as_ref().map_or_else(
                || (raster::empty(), None),
                |view| {
                    let (data, fit) = raster::render(view, selected);
                    (data, Some(fit))
                },
            );
            image.data = Some(data);
            map.fit = fit;
        }
        map.serial = serial;
        map.route_generation = generation;
    }
    let now = (serial, game.screen, game.typed.clone());
    if stamp.as_ref().is_none_or(|old| old.1 != game.screen) {
        for mut scroll in &mut scrolls {
            scroll.offset_y = 0.;
        }
    }
    if stamp.as_ref() == Some(&now) {
        return;
    }
    *stamp = Some(now);
    for body in &bodies {
        commands.entity(body).despawn_related::<Children>();
        commands.entity(body).with_children(|body| {
            if let Some(Screen::Places { renaming: None }) = game.screen {
                text(body, "KNOWN PLACES", 22.);
                let places = game.view.as_ref().map_or(&[][..], |v| v.places.as_slice());
                if places.is_empty() {
                    text(body, "You know no places yet.", 18.);
                }
                for (index, place) in places.iter().enumerate() {
                    button(
                        body,
                        format!(
                            "{} · {}\n{} · Rename",
                            index + 1,
                            place.name,
                            screen::origin(place.origin)
                        ),
                        Action::Rename(place.key.clone()),
                    );
                }
                text(
                    body,
                    "Select a place to rename it. Number keys select the first ten places.",
                    16.,
                );
            } else {
                for line in screen::lines(&game) {
                    text(body, line, 18.);
                }
            }
        });
    }
}
pub(crate) fn keyboard(
    mut events: EventReader<bevy::input::keyboard::KeyboardInput>,
    game: Res<Game>,
    mut scrolls: Query<(&ComputedNode, &mut ScrollPosition), With<Scroll>>,
) {
    if !active(game.screen) || !game.typed.is_empty() {
        events.clear();
        return;
    }
    for event in events.read().filter(|e| e.state.is_pressed()) {
        let sign = match event.logical_key {
            bevy::input::keyboard::Key::PageDown => 1.,
            bevy::input::keyboard::Key::PageUp => -1.,
            _ => continue,
        };
        for (node, mut scroll) in &mut scrolls {
            let max = (node.content_size().y - node.size().y).max(0.) * node.inverse_scale_factor();
            let step = node.size().y * node.inverse_scale_factor() * 0.8;
            scroll.offset_y = (scroll.offset_y + sign * step).clamp(0., max);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_tabs_and_stale_place_buttons_are_local_and_draft_safe() {
        let (net, mut rx) = crate::net::test_channel();
        let mut view = crate::test_support::view(7);
        view.places = vec![tor_protocol::PlaceView {
            key: "opaque:1".into(),
            name: "A hall".into(),
            origin: tor_protocol::PlaceNameOrigin::Invented,
        }];
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                screen: Some(Screen::History),
                view: Some(view),
                ..default()
            })
            .add_systems(Update, buttons);
        let button = app
            .world_mut()
            .spawn((
                Action::Rename("missing".into()),
                Interaction::Pressed,
                BackgroundColor(chrome::BUTTON),
            ))
            .id();
        app.update();
        assert_eq!(app.world().resource::<Game>().screen, Some(Screen::History));
        app.world_mut()
            .entity_mut(button)
            .insert((Action::Rename("opaque:1".into()), Interaction::Pressed));
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(Screen::Places { renaming: Some(0) })
        );
        app.world_mut()
            .entity_mut(button)
            .insert((Action::Note, Interaction::Pressed));
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(Screen::Note {
                revision: 7,
                private: true
            })
        );
        assert!(rx.try_recv().is_err());
        app.world_mut().resource_mut::<Game>().screen = None;
        app.world_mut()
            .entity_mut(button)
            .insert((Action::History, Interaction::Pressed));
        app.update();
        assert_eq!(app.world().resource::<Game>().screen, None);
    }
    #[test]
    fn map_is_allocated_once_and_redrawn_only_for_a_new_view() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .insert_resource(Game {
                screen: Some(Screen::History),
                view: Some(crate::test_support::view(1)),
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(Update, show);
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        let image = app.world().resource::<Map>().image.clone();
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&image)
            .unwrap()
            .data
            .as_mut()
            .unwrap()[0] = 99;
        app.update();
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&image)
                .unwrap()
                .data
                .as_ref()
                .unwrap()[0],
            99
        );
        app.world_mut().resource_mut::<Game>().view_serial += 1;
        app.update();
        assert_ne!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&image)
                .unwrap()
                .data
                .as_ref()
                .unwrap()[0],
            99
        );
    }
    #[test]
    fn layout_reflows_and_lost_views_clear_the_previous_map() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .insert_resource(Game {
                screen: Some(Screen::History),
                view: Some(crate::test_support::view(1)),
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(Update, show);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (1440_f32, 900_f32).into(),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        app.update();
        let root = app
            .world_mut()
            .query_filtered::<Entity, With<Root>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(root).unwrap().flex_direction,
            FlexDirection::Row
        );
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set(700., 800.);
        app.update();
        assert_eq!(
            app.world().get::<Node>(root).unwrap().flex_direction,
            FlexDirection::Column
        );
        let image = app.world().resource::<Map>().image.clone();
        app.world_mut().resource_mut::<Game>().view = None;
        app.update();
        assert!(app
            .world()
            .resource::<Assets<Image>>()
            .get(&image)
            .unwrap()
            .data
            .as_ref()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel == &[8, 10, 14, 255]));
    }
    #[test]
    fn keyboard_scroll_is_bounded_and_preserves_note_drafts() {
        use bevy::input::{
            keyboard::{Key, KeyboardInput},
            ButtonState,
        };
        let mut app = App::new();
        app.insert_resource(Game {
            screen: Some(Screen::History),
            ..default()
        })
        .add_event::<KeyboardInput>()
        .add_systems(Update, keyboard);
        let scroll = app
            .world_mut()
            .spawn((
                Scroll,
                ScrollPosition::default(),
                ComputedNode {
                    size: Vec2::new(100., 100.),
                    content_size: Vec2::new(100., 300.),
                    inverse_scale_factor: 1.,
                    ..default()
                },
            ))
            .id();
        let event = KeyboardInput {
            key_code: KeyCode::PageDown,
            logical_key: Key::PageDown,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        for _ in 0..4 {
            app.world_mut().send_event(event.clone());
            app.update();
        }
        assert_eq!(
            app.world().get::<ScrollPosition>(scroll).unwrap().offset_y,
            200.
        );
        app.world_mut().resource_mut::<Game>().typed = "draft".into();
        let mut up = event.clone();
        up.logical_key = Key::PageUp;
        app.world_mut().send_event(up.clone());
        app.update();
        assert_eq!(
            app.world().get::<ScrollPosition>(scroll).unwrap().offset_y,
            200.
        );
        app.world_mut().resource_mut::<Game>().typed.clear();
        app.world_mut().send_event(up);
        app.update();
        assert_eq!(
            app.world().get::<ScrollPosition>(scroll).unwrap().offset_y,
            120.
        );
    }
    #[test]
    fn static_journal_labels_are_supported_by_the_bundled_font() {
        let font = fontdue::Font::from_bytes(
            include_bytes!("../assets/ui/CrimsonPro.ttf") as &[u8],
            fontdue::FontSettings::default(),
        )
        .unwrap();
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .add_systems(Startup, setup);
        app.update();
        let missing: Vec<_> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .flat_map(|text| text.0.chars())
            .filter(|c| !c.is_control() && !c.is_whitespace() && font.lookup_glyph_index(*c) == 0)
            .collect();
        assert!(
            missing.is_empty(),
            "static UI labels require supported glyphs: {missing:?}"
        );
    }
}
