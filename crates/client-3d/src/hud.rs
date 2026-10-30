//! Persistent live HUD. It presents disclosed facts and reuses existing actions.
use crate::{chrome, interact, net::Cmd, screen, Game, Net};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

const ORB_SIZE: usize = 128;

#[derive(Component, Clone, Copy, Debug)]
pub(crate) enum Action {
    Inventory,
    Equipment,
    Character,
    Map,
    Places,
    Journal,
    Wait,
}

const ACTIONS: [(Action, &str); 7] = [
    (Action::Inventory, "I\nInventory"),
    (Action::Equipment, "F6\nEquipment"),
    (Action::Character, "F10\nCharacter"),
    (Action::Map, "M\nMap"),
    (Action::Places, "F5\nPlaces"),
    (Action::Journal, "F2\nJournal"),
    (Action::Wait, "Space\nWait"),
];

#[derive(Component, Debug)]
pub(crate) enum Field {
    Health,
    Readiness,
    Status,
    Message,
    Log,
}

#[derive(Component)]
pub(crate) struct LogFrame;

#[derive(Resource)]
pub(crate) struct HealthOrb {
    image: Handle<Image>,
    shown: Option<(u64, u64)>,
}

pub(crate) fn readiness(game: &Game) -> &'static str {
    if game.fatal {
        return "DISCONNECTED";
    }
    let Some(view) = &game.view else {
        return "CONNECTING";
    };
    if !view.has_control {
        return "OBSERVING";
    }
    if view.hp.is_some_and(|(hp, _)| hp == 0) {
        return "FALLEN";
    }
    if game.busy {
        return "WORKING";
    }
    if view.can_act {
        "READY"
    } else {
        "WAITING"
    }
}

/// Original software-painted globe, updated only when disclosed health changes.
/// Unknown health is an empty dark globe, never a fabricated full resource.
fn orb_pixels(health: Option<(u64, u64)>) -> Vec<u8> {
    let fraction = health
        .filter(|(_, max)| *max > 0)
        .map_or(0.0, |(hp, max)| hp.min(max) as f64 / max as f64);
    let mut bytes = Vec::with_capacity(ORB_SIZE * ORB_SIZE * 4);
    for y in 0..ORB_SIZE {
        for x in 0..ORB_SIZE {
            let nx = (x as f64 + 0.5) / ORB_SIZE as f64 * 2.0 - 1.0;
            let ny = (y as f64 + 0.5) / ORB_SIZE as f64 * 2.0 - 1.0;
            let r2 = nx * nx + ny * ny;
            if r2 > 1.0 {
                bytes.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let rim = r2 > 0.87;
            let filled = (y as f64 + 0.5) / ORB_SIZE as f64 >= 1.0 - fraction;
            let shine = (1.0 - ((nx + 0.32).powi(2) + (ny + 0.43).powi(2)) * 7.0).max(0.0);
            let depth = (1.0 - r2).sqrt();
            let rgb = if rim {
                [
                    100.0 + depth * 110.0,
                    73.0 + depth * 82.0,
                    37.0 + depth * 50.0,
                ]
            } else if filled {
                [
                    45.0 + depth * 115.0 + shine * 80.0,
                    5.0 + shine * 60.0,
                    12.0 + shine * 45.0,
                ]
            } else {
                [12.0 + shine * 20.0, 9.0 + shine * 17.0, 11.0 + shine * 18.0]
            };
            bytes.extend(rgb.map(|v| v.min(255.0) as u8));
            bytes.push(255);
        }
    }
    bytes
}

pub(crate) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(Image::new(
        Extent3d {
            width: ORB_SIZE as u32,
            height: ORB_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        orb_pixels(None),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(HealthOrb {
        image: image.clone(),
        shown: None,
    });
    commands
        .spawn((Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },))
        .with_children(|root| {
            root.spawn((
                chrome::BlockWorld,
                Interaction::default(),
                chrome::frame(),
                Node {
                    padding: UiRect::axes(Val::Px(28.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
            ))
            .with_child((
                Field::Readiness,
                Text::new("CONNECTING"),
                chrome::text(16.0, chrome::INK),
            ));
        });
    commands
        .spawn((
            LogFrame,
            chrome::BlockWorld,
            Interaction::default(),
            chrome::frame(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(190.0),
                width: Val::Percent(40.0),
                max_width: Val::Px(650.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
        ))
        .with_child((
            Field::Log,
            Text::new("Connecting..."),
            chrome::text(17.0, chrome::INK),
        ));
    commands
        .spawn((
            chrome::BlockWorld,
            Interaction::default(),
            chrome::frame(),
            GlobalZIndex(1),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                width: Val::Percent(100.0),
                min_height: Val::Px(166.0),
                border: UiRect::top(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(14.0)),
                column_gap: Val::Px(18.0),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|dock| {
            dock.spawn(Node {
                width: Val::Px(112.0),
                height: Val::Px(132.0),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|orb| {
                orb.spawn((
                    ImageNode::new(image),
                    Node {
                        width: Val::Px(112.0),
                        height: Val::Px(112.0),
                        ..default()
                    },
                ))
                .with_child((
                    Field::Health,
                    Text::new("— / —"),
                    chrome::text(18.0, Color::WHITE),
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        top: Val::Px(49.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    TextLayout::new_with_justify(JustifyText::Center),
                ));
                orb.spawn((Text::new("HEALTH"), chrome::text(15.0, chrome::INK)));
            });
            dock.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_width: Val::Px(0.0),
                row_gap: Val::Px(9.0),
                ..default()
            })
            .with_children(|center| {
                center
                    .spawn(Node {
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|buttons| {
                        for (action, label) in ACTIONS {
                            buttons
                                .spawn((
                                    Button,
                                    action,
                                    BackgroundColor(chrome::BUTTON),
                                    BorderColor(chrome::GOLD),
                                    Node {
                                        min_width: Val::Px(76.0),
                                        padding: UiRect::axes(Val::Px(10.0), Val::Px(7.0)),
                                        border: UiRect::all(Val::Px(1.0)),
                                        justify_content: JustifyContent::Center,
                                        ..default()
                                    },
                                ))
                                .with_child((
                                    Text::new(label),
                                    chrome::text(16.0, chrome::INK),
                                    TextLayout::new_with_justify(JustifyText::Center),
                                ));
                        }
                    });
                center.spawn((
                    Field::Status,
                    Text::new(""),
                    chrome::text(16.0, chrome::INK),
                ));
                center.spawn((
                    Field::Message,
                    Text::new(""),
                    chrome::text(15.0, chrome::MUTED),
                ));
            });
        });
}

pub(crate) fn update(
    game: Res<Game>,
    mut fields: Query<(&Field, &mut Text)>,
    mut orb: ResMut<HealthOrb>,
    mut images: ResMut<Assets<Image>>,
    mut log_frames: Query<&mut Visibility, With<LogFrame>>,
) {
    if !game.is_changed() {
        return;
    }
    if let Ok(mut visibility) = log_frames.single_mut() {
        visibility.set_if_neq(if game.log.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    let hp = game.view.as_ref().and_then(|v| v.hp);
    if orb.shown != hp {
        if let Some(image) = images.get_mut(&orb.image) {
            image.data = Some(orb_pixels(hp));
        }
        orb.shown = hp;
    }
    for (field, mut text) in &mut fields {
        let value = match field {
            Field::Readiness => readiness(&game).to_owned(),
            Field::Health => hp.map_or_else(|| "— / —".into(), |(hp, max)| format!("{hp} / {max}")),
            Field::Status => game.view.as_ref().map_or_else(String::new, |v| {
                v.status
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
            Field::Message => {
                let mut parts = Vec::new();
                if !game.status.is_empty() {
                    parts.push(game.status.clone());
                }
                if let Some(view) = &game.view {
                    if view.can_resume {
                        parts.push("F8: resume".into());
                    }
                    if view.can_cancel {
                        parts.push("F9: cancel".into());
                    }
                    let here: Vec<_> = view
                        .items
                        .iter()
                        .filter(|i| i.position == view.feet)
                        .map(|i| i.name.as_str())
                        .collect();
                    if !here.is_empty() {
                        parts.push(format!("G: take · {}", here.join(", ")));
                    }
                }
                parts.join("   |   ")
            }
            Field::Log => game
                .log
                .iter()
                .rev()
                .take(4)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

pub(crate) fn buttons(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut panel: ResMut<interact::Panel>,
    mut buttons: Query<(&Interaction, &Action, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, action, mut color) in &mut buttons {
        color.0 = if *interaction == Interaction::None {
            chrome::BUTTON
        } else {
            chrome::HOT
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::Inventory | Action::Equipment => {
                if let Some(open) = game.screen.clone() {
                    screen::toggle(&net, &mut game, open);
                }
                panel.toggle_page(if matches!(action, Action::Equipment) {
                    crate::equipment::Page::Equipment
                } else {
                    crate::equipment::Page::Inventory
                });
            }
            Action::Character => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::Advancement);
            }
            Action::Places => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::Places { renaming: None });
            }
            Action::Journal => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::History);
            }
            Action::Map => {
                panel.open = false;
                screen::toggle(&net, &mut game, screen::Screen::Places { renaming: None });
            }
            Action::Wait
                if !panel.open
                    && game.screen.is_none()
                    && !game.busy
                    && !game.fatal
                    && game
                        .view
                        .as_ref()
                        .is_some_and(|v| v.has_control && v.can_act) =>
            {
                game.send(&net, Cmd::Wait);
            }
            Action::Wait => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_message_log_does_not_leave_a_blank_panel_over_the_world() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Game>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        app.update();
        let root = app
            .world_mut()
            .query_filtered::<Entity, With<LogFrame>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        app.world_mut()
            .resource_mut::<Game>()
            .log
            .push_back("You move north.".into());
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );
    }

    fn dock(action: Action) -> (App, tokio::sync::mpsc::Receiver<crate::net::Input>) {
        let (net, receiver) = crate::net::test_channel();
        let mut view = crate::test_support::view(7);
        view.has_control = true;
        view.can_act = true;
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                ..default()
            })
            .init_resource::<interact::Panel>()
            .add_systems(Update, buttons);
        app.world_mut().spawn((
            action,
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        (app, receiver)
    }

    #[test]
    fn dock_equipment_opens_locally_and_switches_back_to_inventory() {
        let (mut app, mut receiver) = dock(Action::Equipment);
        app.update();
        assert!(app.world().resource::<interact::Panel>().open);
        assert_eq!(
            app.world().resource::<interact::Panel>().page,
            crate::equipment::Page::Equipment
        );
        assert!(receiver.try_recv().is_err());
        let button = app
            .world_mut()
            .query_filtered::<Entity, With<Action>>()
            .single(app.world())
            .unwrap();
        *app.world_mut().get_mut::<Action>(button).unwrap() = Action::Inventory;
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
        app.update();
        assert!(app.world().resource::<interact::Panel>().open);
        assert_eq!(
            app.world().resource::<interact::Panel>().page,
            crate::equipment::Page::Inventory
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_map_toggles_once_per_press_without_a_server_action() {
        let (mut app, mut receiver) = dock(Action::Map);
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(screen::Screen::Places { renaming: None })
        );
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(screen::Screen::Places { renaming: None }),
            "holding the dock button opens only once"
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_inventory_leaves_charts_through_their_cancellation_boundary() {
        let (mut app, mut receiver) = dock(Action::Inventory);
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            game.screen = Some(screen::Screen::Charts);
            game.typed = "abandoned input".into();
        }
        app.update();
        assert!(app.world().resource::<interact::Panel>().open);
        let game = app.world().resource::<Game>();
        assert_eq!(game.screen, None);
        assert!(game.typed.is_empty());
        assert!(matches!(
            receiver.try_recv().unwrap().command,
            Cmd::Chart(tor_client_common::charts::ChartInput::Cancel)
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_character_fetches_overview_and_closes_the_inventory() {
        let (mut app, mut receiver) = dock(Action::Character);
        app.world_mut().resource_mut::<interact::Panel>().open = true;
        app.update();
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(screen::Screen::Advancement)
        );
        assert!(!app.world().resource::<interact::Panel>().open);
        assert!(
            matches!(receiver.try_recv().unwrap().command, Cmd::Advancement(line) if line == "overview")
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn dock_read_only_pages_remain_available_to_observers_and_close_inventory() {
        for (action, screen) in [
            (Action::Places, screen::Screen::Places { renaming: None }),
            (Action::Journal, screen::Screen::History),
        ] {
            let (mut app, mut receiver) = dock(action);
            app.world_mut().resource_mut::<interact::Panel>().open = true;
            app.world_mut()
                .resource_mut::<Game>()
                .view
                .as_mut()
                .unwrap()
                .has_control = false;
            app.update();
            assert_eq!(app.world().resource::<Game>().screen, Some(screen));
            assert!(!app.world().resource::<interact::Panel>().open);
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn dock_wait_requires_ready_control_without_a_modal_or_outstanding_work() {
        for blocked in 0..8 {
            let (mut app, mut receiver) = dock(Action::Wait);
            if blocked == 6 {
                app.world_mut().resource_mut::<interact::Panel>().open = true;
            }
            {
                let mut game = app.world_mut().resource_mut::<Game>();
                match blocked {
                    0 => game.view = None,
                    1 => game.view.as_mut().unwrap().has_control = false,
                    2 => game.view.as_mut().unwrap().can_act = false,
                    3 => game.busy = true,
                    4 => game.fatal = true,
                    5 => game.screen = Some(screen::Screen::History),
                    _ => {}
                }
            }
            app.update();
            if blocked == 7 {
                let input = receiver.try_recv().expect("a ready owner can wait");
                assert!(matches!(input.command, Cmd::Wait));
                assert_eq!(input.shown.revision, 7);
                assert!(app.world().resource::<Game>().busy);
            }
            assert!(
                receiver.try_recv().is_err(),
                "blocked state {blocked} must not send a command"
            );
        }
    }

    #[test]
    fn live_fields_and_globe_follow_disclosure_without_growing_assets() {
        let mut app = App::new();
        let mut view = crate::test_support::view(0);
        view.hp = Some((9, 18));
        view.has_control = true;
        view.can_act = true;
        view.status = ["Known place".into(), "Stamina 3/4".into()];
        app.init_resource::<Assets<Image>>()
            .insert_resource(Game {
                view: Some(view),
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(Update, update);
        app.update();
        let values: Vec<_> = app
            .world_mut()
            .query::<(&Field, &Text)>()
            .iter(app.world())
            .map(|(field, text)| (format!("{field:?}"), text.0.clone()))
            .collect();
        assert!(values.contains(&("Health".into(), "9 / 18".into())));
        assert!(values.contains(&("Status".into(), "Known place\nStamina 3/4".into())));
        let handle = app.world().resource::<HealthOrb>().image.clone();
        let before = app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data
            .clone();
        app.world_mut()
            .resource_mut::<Game>()
            .view
            .as_mut()
            .unwrap()
            .hp = None;
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        let after = &app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data;
        assert_ne!(&before, after);
        assert_eq!(after.as_ref().unwrap(), &orb_pixels(None));
        assert!(app
            .world_mut()
            .query::<(&Field, &Text)>()
            .iter(app.world())
            .any(|(field, text)| matches!(field, Field::Health) && text.0 == "— / —"));
    }
    #[test]
    fn readiness_respects_control_death_and_outstanding_work() {
        let mut game = Game::default();
        assert_eq!(readiness(&game), "CONNECTING");
        let mut view = crate::test_support::view(0);
        view.can_act = true;
        view.hp = Some((18, 18));
        game.view = Some(view);
        assert_eq!(readiness(&game), "OBSERVING");
        game.view.as_mut().unwrap().has_control = true;
        assert_eq!(readiness(&game), "READY");
        game.busy = true;
        assert_eq!(readiness(&game), "WORKING");
        game.view.as_mut().unwrap().hp = Some((0, 18));
        assert_eq!(readiness(&game), "FALLEN");
        game.fatal = true;
        assert_eq!(readiness(&game), "DISCONNECTED");
        game.fatal = false;
        game.busy = false;
        game.view.as_mut().unwrap().hp = None;
        game.view.as_mut().unwrap().can_act = false;
        assert_eq!(readiness(&game), "WAITING");
    }
    #[test]
    fn health_globe_has_transparent_corners_and_truthful_fill() {
        let empty = orb_pixels(None);
        assert_eq!(empty.len(), 128 * 128 * 4);
        assert_eq!(&empty[..4], &[0, 0, 0, 0]);
        assert_eq!(empty, orb_pixels(Some((0, 0))));
        assert_eq!(empty, orb_pixels(Some((0, 18))));
        let full = orb_pixels(Some((18, 18)));
        assert_eq!(full, orb_pixels(Some((u64::MAX, 18))));
        let half = orb_pixels(Some((9, 18)));
        let top = (32 * 128 + 64) * 4;
        let bottom = (96 * 128 + 64) * 4;
        assert_eq!(&half[top..top + 4], &empty[top..top + 4]);
        assert_eq!(&half[bottom..bottom + 4], &full[bottom..bottom + 4]);
        assert_ne!(&full[top..top + 4], &empty[top..top + 4]);
    }
}
