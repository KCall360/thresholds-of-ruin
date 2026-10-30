//! Local map selection and explicit, snapshot-bound travel confirmation.
use super::{Map, Picture};
use crate::{chrome, screen::Screen, Game, Net};
use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
    window::PrimaryWindow,
};
use tor_client_3d_model::{map_destinations, MapDestination, MapSelection};
#[derive(Resource, Default)]
pub(crate) struct State {
    serial: Option<u64>,
    pub(super) rows: Vec<MapDestination>,
    pub(super) selected: Option<MapSelection>,
    pub(super) generation: u64,
    open: bool,
}
#[derive(Component)]
pub(crate) struct Root;
#[derive(Component, Clone)]
pub(crate) struct Travel(pub(super) MapSelection);
fn active(game: &Game) -> bool {
    game.typed.is_empty()
        && matches!(
            game.screen,
            Some(Screen::Places { renaming: None } | Screen::History)
        )
}
fn ready(game: &Game) -> bool {
    !game.busy && !game.fatal && game.projectile.is_none() && game.aiming.is_none()
}
pub(crate) fn sync(game: Res<Game>, mut state: ResMut<State>) {
    let open = active(&game);
    let serial = game
        .view
        .as_ref()
        .filter(|_| open)
        .map(|_| game.view_serial);
    if serial != state.serial || open != state.open {
        state.serial = serial;
        state.open = open;
        state.rows = game
            .view
            .as_ref()
            .filter(|_| open)
            .map_or_else(Vec::new, map_destinations);
        state.selected = None;
        state.generation = state.generation.wrapping_add(1);
    }
}
fn select(game: &Game, state: &mut State, key: &str) {
    let selected = game
        .view
        .as_ref()
        .and_then(|view| MapSelection::select(view, key));
    if state.selected != selected {
        state.selected = selected;
        state.generation = state.generation.wrapping_add(1);
    }
}
fn travel(net: &Net, game: &mut Game, state: &mut State, shown: &MapSelection) {
    if !active(game) || state.selected.as_ref() != Some(shown) {
        return;
    }
    let Some(command) = game
        .view
        .as_ref()
        .and_then(|view| shown.command(view, ready(game)))
    else {
        return;
    };
    if game.send(net, command) {
        game.screen = None;
        state.selected = None;
        state.generation = state.generation.wrapping_add(1);
    }
}
pub(crate) fn keyboard(
    mut events: EventReader<KeyboardInput>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut state: ResMut<State>,
) {
    if !active(&game) {
        events.clear();
        return;
    }
    for event in events.read().filter(|event| event.state.is_pressed()) {
        if !active(&game) {
            break;
        }
        if matches!(event.logical_key, Key::Enter) {
            if !event.repeat {
                if let Some(shown) = state.selected.clone() {
                    travel(&net, &mut game, &mut state, &shown);
                }
            }
            continue;
        }
        let len = state.rows.len();
        if len == 0 {
            continue;
        }
        let current = state
            .selected
            .as_ref()
            .and_then(|s| state.rows.iter().position(|r| r == &s.destination));
        let index = match event.logical_key {
            Key::ArrowDown => current.map_or(0, |i| (i + 1) % len),
            Key::ArrowUp => current.map_or(len - 1, |i| (i + len - 1) % len),
            Key::Home => 0,
            Key::End => len - 1,
            _ => continue,
        };
        let key = state.rows[index].key.clone();
        select(&game, &mut state, &key);
    }
}
pub(crate) fn buttons(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut state: ResMut<State>,
    buttons: Query<(&Interaction, &Travel), Changed<Interaction>>,
) {
    for (interaction, target) in &buttons {
        if *interaction == Interaction::Pressed {
            travel(&net, &mut game, &mut state, &target.0);
        }
    }
}
pub(super) fn cursor_uv(
    size: Vec2,
    center: Vec2,
    clip: Option<Rect>,
    cursor: Vec2,
) -> Option<Vec2> {
    if size.min_element() <= 0.
        || !cursor.is_finite()
        || clip.is_some_and(|rect| !rect.contains(cursor))
    {
        return None;
    }
    let rect = Rect::from_center_size(center, size);
    rect.contains(cursor).then(|| (cursor - rect.min) / size)
}
pub(crate) fn pointer(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    game: Res<Game>,
    map: Res<Map>,
    mut state: ResMut<State>,
    pictures: Query<
        (
            &ComputedNode,
            &GlobalTransform,
            &InheritedVisibility,
            Option<&CalculatedClip>,
        ),
        With<Picture>,
    >,
) {
    if !mouse.just_pressed(MouseButton::Left)
        || !active(&game)
        || map.serial != Some(game.view_serial)
    {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window
        .cursor_position()
        .map(|point| point * window.scale_factor())
    else {
        return;
    };
    let Ok((node, transform, visible, clip)) = pictures.single() else {
        return;
    };
    if !visible.get() {
        return;
    }
    let Some(uv) = cursor_uv(
        node.size(),
        transform.translation().truncate(),
        clip.map(|clip| clip.clip),
        cursor,
    ) else {
        return;
    };
    let Some((x, y)) = map.fit.and_then(|fit| fit.pick(uv.to_array())) else {
        return;
    };
    if let Some(key) = state
        .rows
        .iter()
        .find(|row| row.position.x == x && row.position.y == y)
        .map(|row| row.key.clone())
    {
        select(&game, &mut state, &key);
    }
}
type Stamp = Option<(u64, bool, bool)>;
pub(crate) fn draw(
    mut commands: Commands,
    game: Res<Game>,
    state: Res<State>,
    roots: Query<Entity, With<Root>>,
    mut stamp: Local<Stamp>,
) {
    let now = (
        state.generation,
        ready(&game),
        game.view
            .as_ref()
            .is_some_and(|view| view.can_act && view.has_control),
    );
    if stamp.as_ref() == Some(&now) {
        return;
    }
    *stamp = Some(now);
    for root in &roots {
        commands.entity(root).despawn_related::<Children>();
        commands.entity(root).with_children(|root| {
            super::text(root, "ROUTE DESTINATION", 20.);
            let Some(selection) = state.selected.as_ref() else {
                super::text(
                    root,
                    "Select a known tile on the map, or use Up / Down.",
                    16.,
                );
                return;
            };
            let index = state
                .rows
                .iter()
                .position(|row| row == &selection.destination)
                .map_or(0, |i| i + 1);
            super::text(
                root,
                format!(
                    "{} · destination {} of {}",
                    selection.destination.label,
                    index,
                    state.rows.len()
                ),
                18.,
            );
            let can = game
                .view
                .as_ref()
                .is_some_and(|view| selection.command(view, ready(&game)).is_some());
            root.spawn((
                Button,
                Travel(selection.clone()),
                BackgroundColor(if can { chrome::HOT } else { chrome::BUTTON }),
                BorderColor(chrome::GOLD),
                Node {
                    width: Val::Percent(100.),
                    padding: UiRect::all(Val::Px(9.)),
                    border: UiRect::all(Val::Px(1.)),
                    ..default()
                },
            ))
            .with_children(|button| {
                super::text(
                    button,
                    if can {
                        "Enter · Travel to known tile"
                    } else {
                        "Travel unavailable in the current state"
                    },
                    18.,
                );
            });
            super::text(
                root,
                "Travel follows the server's route and may stop when interrupted.",
                15.,
            );
        });
    }
}
pub(crate) fn diagnostics(
    state: Option<&State>,
    game: Option<&Game>,
    map: Option<&Map>,
) -> serde_json::Value {
    let fit = map.and_then(|map| map.fit);
    serde_json::json!({"selected":state.and_then(|s|s.selected.as_ref()).map(|s|&s.destination.key),"can_travel":state.zip(game).is_some_and(|(s,g)|s.selected.as_ref().zip(g.view.as_ref()).is_some_and(|(s,v)|active(g)&&s.command(v,ready(g)).is_some())),"feet":game.and_then(|g|g.view.as_ref()).map(|v|[v.feet.x,v.feet.y,v.feet.z]),"options":state.map(|state|state.rows.iter().map(|row|serde_json::json!({"key":row.key,"position":[row.position.x,row.position.y,row.position.z],"label":row.label,"point":fit.map(|fit|fit.normalized(row.position.x,row.position.y))})).collect::<Vec<_>>()).unwrap_or_default()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::ButtonState;
    use tor_protocol::{CellView, Position};
    fn fixture() -> (App, tokio::sync::mpsc::Receiver<crate::net::Input>) {
        let (net, rx) = crate::net::test_channel();
        let mut view = crate::test_support::view(7);
        view.has_control = true;
        view.can_act = true;
        view.visible = (0..=2)
            .map(|x| CellView {
                object_presentation: None,
                lit: true,
                key: format!("cell:{x}"),
                position: Position { x, y: 0, z: 0 },
                wall: false,
                material: String::new(),
                trap: None,
                door: None,
                asset: None,
                stairs_up: false,
                stairs_down: false,
                place_hint: false,
            })
            .collect();
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                screen: Some(Screen::Places { renaming: None }),
                ..default()
            })
            .init_resource::<State>()
            .add_event::<KeyboardInput>()
            .add_systems(Update, (sync, keyboard, buttons).chain());
        (app, rx)
    }
    fn press(app: &mut App, key: Key, repeat: bool) {
        app.world_mut().send_event(KeyboardInput {
            key_code: KeyCode::Enter,
            logical_key: key,
            state: ButtonState::Pressed,
            text: None,
            repeat,
            window: Entity::PLACEHOLDER,
        });
        app.update();
    }
    #[test]
    fn selecting_is_local_and_enter_sends_one_exact_journey() {
        let (mut app, mut rx) = fixture();
        press(&mut app, Key::ArrowDown, false);
        let selected = app.world().resource::<State>().selected.clone().unwrap();
        assert_eq!(selected.destination.key, "cell:1");
        assert!(rx.try_recv().is_err());
        press(&mut app, Key::Enter, true);
        assert!(rx.try_recv().is_err(), "a repeated Enter never commits");
        press(&mut app, Key::Enter, false);
        let sent = rx.try_recv().unwrap();
        assert_eq!(sent.shown, selected.shown);
        assert!(matches!(sent.command,crate::net::Cmd::Travel(key)if key=="cell:1"));
        assert_eq!(app.world().resource::<Game>().screen, None);
        press(&mut app, Key::Enter, false);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn stale_rendered_buttons_cannot_replace_the_current_destination() {
        let (mut app, mut rx) = fixture();
        press(&mut app, Key::ArrowDown, false);
        let old = app.world().resource::<State>().selected.clone().unwrap();
        press(&mut app, Key::ArrowDown, false);
        let current = app.world().resource::<State>().selected.clone().unwrap();
        let button = app
            .world_mut()
            .spawn((Travel(old), Interaction::Pressed))
            .id();
        app.update();
        assert!(rx.try_recv().is_err());
        app.world_mut()
            .entity_mut(button)
            .insert((Travel(current.clone()), Interaction::Pressed));
        app.update();
        assert!(
            matches!(rx.try_recv().unwrap().command,crate::net::Cmd::Travel(key)if key==current.destination.key)
        );
    }
    #[test]
    fn unavailable_or_changed_states_reject_confirmation() {
        for mode in 0..7 {
            let (mut app, mut rx) = fixture();
            press(&mut app, Key::ArrowDown, false);
            {
                let mut game = app.world_mut().resource_mut::<Game>();
                match mode {
                    0 => game.busy = true,
                    1 => game.fatal = true,
                    2 => {
                        game.aiming = Some(crate::Aim::Device(
                            tor_protocol::ItemTarget::from_digest([9; 32]),
                        ))
                    }
                    3 => game.view.as_mut().unwrap().identify = Some(vec![]),
                    4 => game.view.as_mut().unwrap().can_act = false,
                    5 => game.view.as_mut().unwrap().has_control = false,
                    _ => game.view.as_mut().unwrap().shown.revision += 1,
                };
            }
            press(&mut app, Key::Enter, false);
            assert!(rx.try_recv().is_err(), "rejection{mode}");
            assert!(app.world().resource::<Game>().screen.is_some());
        }
    }
    #[test]
    fn drafts_and_new_views_clear_selection_without_sending() {
        let (mut app, mut rx) = fixture();
        press(&mut app, Key::ArrowDown, false);
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.screen = Some(Screen::Places { renaming: Some(0) });
            game.typed = "my place".into();
        }
        press(&mut app, Key::ArrowDown, false);
        assert!(app.world().resource::<State>().selected.is_none());
        assert_eq!(app.world().resource::<Game>().typed, "my place");
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.screen = Some(Screen::History);
            game.typed.clear();
        }
        press(&mut app, Key::ArrowDown, false);
        app.world_mut().resource_mut::<Game>().view_serial += 1;
        press(&mut app, Key::Enter, false);
        assert!(app.world().resource::<State>().selected.is_none());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn pointer_coordinates_respect_the_actual_picture_and_clip() {
        let size = Vec2::new(200., 100.);
        let center = Vec2::new(300., 200.);
        assert_eq!(
            cursor_uv(size, center, None, Vec2::new(250., 175.)),
            Some(Vec2::splat(0.25))
        );
        assert_eq!(
            cursor_uv(
                size,
                center,
                Some(Rect::new(200., 150., 275., 200.)),
                Vec2::new(280., 175.)
            ),
            None
        );
        assert_eq!(cursor_uv(size, center, None, Vec2::new(199., 175.)), None);
        assert_eq!(cursor_uv(Vec2::ZERO, center, None, center), None);
        assert_eq!(cursor_uv(size, center, None, Vec2::splat(f32::NAN)), None);
    }
    #[test]
    fn picture_press_selects_the_exact_cell_without_dispatching() {
        let (mut app, mut rx) = fixture();
        app.init_resource::<ButtonInput<MouseButton>>()
            .add_systems(Update, pointer.after(sync));
        let view = app
            .world()
            .resource::<Game>()
            .view
            .as_ref()
            .unwrap()
            .clone();
        let (_, fit) = super::super::raster::render(&view, None);
        app.insert_resource(Map {
            image: Handle::default(),
            serial: Some(0),
            fit: Some(fit),
            route_generation: 0,
        });
        let point = Vec2::from(fit.normalized(2, 0)) * 200.;
        let mut window = Window {
            resolution: (200_f32, 200_f32).into(),
            ..default()
        };
        window.set_cursor_position(Some(point));
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().spawn((
            Picture,
            ComputedNode {
                size: Vec2::splat(200.),
                ..default()
            },
            GlobalTransform::from_translation(Vec3::new(100., 100., 0.)),
            InheritedVisibility::VISIBLE,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(
            app.world()
                .resource::<State>()
                .selected
                .as_ref()
                .unwrap()
                .destination
                .key,
            "cell:2"
        );
        assert!(rx.try_recv().is_err());
    }
}
