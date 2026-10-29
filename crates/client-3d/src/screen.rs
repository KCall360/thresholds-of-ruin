//! Full screens that hold the keyboard: the owner's advancement pages, in the
//! shared command language; the remembered places, which can be renamed; and
//! the read-only detection charts; the retained history; and a note to
//! annotate the current state.
use crate::{net::Cmd, Game, Net};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use tor_client_3d_model::MAX_PLACE_NAME;
use tor_protocol::MAX_NOTE_BYTES;

/// Advancement commands are short; this bounds what a stuck key can type.
const MAX_COMMAND: usize = 200;

/// Which screen holds the keyboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Advancement,
    /// The known places; with a selection, the line names that place.
    Places {
        renaming: Option<usize>,
    },
    /// The detection charts: a catalog, then the chosen chart's pages.
    Charts,
    /// The latest retained history, read-only.
    History,
    /// A note anchored to the snapshot opened by this editor; Tab switches between
    /// private and visible to the actor's other observers.
    Note {
        snapshot: tor_protocol::SnapshotId,
        private: bool,
    },
}

#[derive(Component)]
pub(crate) struct ScreenRoot;

#[derive(Component)]
pub(crate) struct ScreenText;

pub(crate) fn setup(mut commands: Commands) {
    commands
        .spawn((
            ScreenRoot,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(12.0),
                right: Val::Percent(12.0),
                top: Val::Percent(8.0),
                bottom: Val::Percent(8.0),
                padding: UiRect::all(Val::Px(16.0)),
                border: UiRect::all(Val::Px(1.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.035, 0.05, 0.94)),
            BorderColor(Color::srgba(0.3, 0.75, 0.68, 0.55)),
            BorderRadius::all(Val::Px(6.0)),
            Visibility::Hidden,
        ))
        .with_child((
            ScreenText,
            Text::new(""),
            TextFont {
                font_size: 15.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.88, 0.8)),
        ));
}

/// Open this screen, or close it if it is the one open. Opening the
/// advancement screen asks for the overview.
pub(crate) fn toggle(net: &Net, game: &mut Game, screen: Screen) {
    let same = match (game.screen.clone(), screen.clone()) {
        (Some(Screen::Places { .. }), Screen::Places { .. })
        | (Some(Screen::Note { .. }), Screen::Note { .. }) => true,
        (open, wanted) => open == Some(wanted),
    };
    let closing = game.screen.clone();
    game.screen = (!same).then_some(screen);
    game.typed.clear();
    use tor_client_common::charts::ChartInput;
    match (closing, game.screen.clone()) {
        // Leaving the charts cancels their local page assembly.
        (Some(Screen::Charts), _) => {
            game.send(net, Cmd::Chart(ChartInput::Cancel));
        }
        (_, Some(Screen::Advancement)) => {
            game.send(net, Cmd::Advancement("overview".into()));
        }
        (_, Some(Screen::Charts)) => {
            game.send(net, Cmd::Chart(ChartInput::Overview));
        }
        _ => {}
    }
}

/// Typed characters edit the line; Enter sends it. On the places list a
/// number key picks the place to rename.
pub(crate) fn typing(mut input: EventReader<KeyboardInput>, net: Res<Net>, mut game: ResMut<Game>) {
    let Some(screen) = game.screen.clone() else {
        input.clear();
        return;
    };
    if screen == Screen::History {
        input.clear();
        return;
    }
    if screen == Screen::Charts {
        use tor_client_common::charts::ChartInput;
        for event in input.read() {
            if !event.state.is_pressed() {
                continue;
            }
            let command = match &event.logical_key {
                Key::PageDown => Some(ChartInput::Next),
                Key::PageUp => Some(ChartInput::Previous),
                Key::Tab => Some(ChartInput::Overview),
                Key::Character(text) => text
                    .chars()
                    .next()
                    .and_then(|c| c.to_digit(10))
                    .map(|digit| (digit as usize + 9) % 10)
                    .and_then(|index| game.view.as_ref()?.chart_kinds.get(index).copied())
                    .map(ChartInput::Select),
                _ => None,
            };
            if let Some(command) = command {
                game.send(&net, Cmd::Chart(command));
            }
        }
        return;
    }
    let limit = match &screen {
        Screen::Advancement => MAX_COMMAND,
        Screen::Places { .. } => MAX_PLACE_NAME,
        Screen::Note { .. } => MAX_NOTE_BYTES,
        Screen::Charts | Screen::History => 0,
    };
    for event in input.read() {
        if !event.state.is_pressed() {
            continue;
        }
        if screen == (Screen::Places { renaming: None }) {
            if let Key::Character(text) = &event.logical_key {
                let places = game.view.as_ref().map_or(0, |view| view.places.len());
                let picked = text
                    .chars()
                    .next()
                    .and_then(|c| c.to_digit(10))
                    .map(|digit| (digit as usize + 9) % 10)
                    .filter(|index| *index < places);
                if let Some(index) = picked {
                    game.screen = Some(Screen::Places {
                        renaming: Some(index),
                    });
                    game.typed.clear();
                }
            }
            continue;
        }
        if let (Screen::Note { snapshot, private }, Key::Tab) = (screen.clone(), &event.logical_key)
        {
            game.screen = Some(Screen::Note {
                snapshot,
                private: !private,
            });
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                let line = std::mem::take(&mut game.typed);
                if line.trim().is_empty() {
                    continue;
                }
                let command = match screen.clone() {
                    Screen::Charts | Screen::History => None,
                    Screen::Note { snapshot, private } => {
                        game.screen = None;
                        Some(Cmd::Note {
                            snapshot,
                            text: line,
                            private,
                        })
                    }
                    Screen::Advancement => Some(Cmd::Advancement(line)),
                    Screen::Places { renaming } => {
                        game.screen = Some(Screen::Places { renaming: None });
                        renaming
                            .and_then(|index| game.view.as_ref()?.places.get(index))
                            .map(|place| Cmd::RenamePlace {
                                key: place.key.clone(),
                                name: line,
                            })
                    }
                };
                if let Some(command) = command {
                    game.send(&net, command);
                }
            }
            Key::Backspace => {
                game.typed.pop();
            }
            Key::Space if game.typed.len() < limit => game.typed.push(' '),
            Key::Character(text) => {
                for c in text.chars().filter(|c| !c.is_control()) {
                    if game.typed.len() + c.len_utf8() <= limit {
                        game.typed.push(c);
                    }
                }
            }
            _ => {}
        }
    }
}

fn origin(origin: tor_protocol::PlaceNameOrigin) -> &'static str {
    match origin {
        tor_protocol::PlaceNameOrigin::Invented => "made up when first seen",
        tor_protocol::PlaceNameOrigin::Authored => "its own name",
        tor_protocol::PlaceNameOrigin::Player => "your name for it",
    }
}

pub(crate) fn lines(game: &Game) -> Vec<String> {
    let mut lines = Vec::new();
    match game.screen.clone() {
        None => {}
        Some(Screen::Advancement) => {
            lines.extend(["Advancement".to_string(), String::new()]);
            if game.advancement.is_empty() {
                lines.push("Waiting for the server's pages.".into());
            }
            lines.extend(game.advancement.iter().cloned());
            lines.push(String::new());
            lines.push(format!("> {}_", game.typed));
            lines.push(
                "Type a command (overview, ledger, balances, talents, purchase ..., confirm) and press Enter. Esc or F10 closes."
                    .into(),
            );
        }
        Some(Screen::Charts) => {
            lines.extend(["Detection charts".to_string(), String::new()]);
            if game.charts.is_empty() {
                lines.push("Waiting for the charts.".into());
            }
            lines.extend(game.charts.iter().cloned());
            lines.push(String::new());
            lines.push(
                "A number key opens a chart; Page Up and Page Down page it; Tab returns to the list. Esc or C closes."
                    .into(),
            );
        }
        Some(Screen::History) => {
            lines.extend(["History".to_string(), String::new()]);
            let history = game.view.as_ref().map_or(&[][..], |view| &view.history[..]);
            if history.is_empty() {
                lines.push("Nothing has happened yet.".into());
            }
            lines.extend(history.iter().cloned());
            lines.push(String::new());
            lines.push("Esc or F2 closes.".into());
        }
        Some(Screen::Note { private, .. }) => {
            lines.extend(["Note".to_string(), String::new()]);
            lines.push(format!(
                "Audience: {} (Tab switches)",
                if private {
                    "only you"
                } else {
                    "everyone who observes this actor"
                }
            ));
            lines.push(format!("> {}_", game.typed));
            lines.push(String::new());
            lines.push("Enter saves the note on the current state. Esc or F4 closes.".into());
        }
        Some(Screen::Places { renaming }) => {
            lines.extend(["Places".to_string(), String::new()]);
            let places = game.view.as_ref().map_or(&[][..], |view| &view.places[..]);
            if places.is_empty() {
                lines.push("You know no places yet.".into());
            }
            for (index, place) in places.iter().enumerate() {
                let key = if index < 10 {
                    format!("{} ", (index + 1) % 10)
                } else {
                    "  ".into()
                };
                lines.push(format!("{key}{} ({})", place.name, origin(place.origin)));
            }
            lines.push(String::new());
            match renaming.and_then(|index| places.get(index)) {
                Some(place) => {
                    lines.push(format!("New name for {}: {}_", place.name, game.typed));
                    lines.push("Enter renames it. Esc or F5 closes.".into());
                }
                None => {
                    lines.push("A number key picks a place to rename. Esc or F5 closes.".into())
                }
            }
        }
    }
    lines
}

pub(crate) fn show(
    game: Res<Game>,
    mut root: Query<&mut Visibility, With<ScreenRoot>>,
    mut text: Query<&mut Text, With<ScreenText>>,
) {
    if !game.is_changed() {
        return;
    }
    if let Ok(mut visibility) = root.single_mut() {
        let wanted = if game.screen.is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
    if game.screen.is_none() {
        return;
    }
    let joined = lines(&game).join("\n");
    if let Ok(mut text) = text.single_mut() {
        if text.0 != joined {
            text.0 = joined;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::ButtonState;

    fn key(logical_key: Key) -> KeyboardInput {
        KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }

    fn app(
        view: tor_client_3d_model::View,
    ) -> (App, tokio::sync::mpsc::Receiver<crate::net::Input>) {
        let (net, commands) = crate::net::test_channel();
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                ..Default::default()
            })
            .add_event::<KeyboardInput>()
            .add_systems(Update, typing);
        (app, commands)
    }

    fn press(app: &mut App, key: Key) {
        app.world_mut().send_event(self::key(key));
        app.update();
    }

    #[test]
    fn typed_commands_are_edited_and_sent_only_while_the_screen_is_open() {
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        let (mut app, mut commands) = app(view);
        press(&mut app, Key::Character("x".into()));
        assert!(
            app.world().resource::<Game>().typed.is_empty(),
            "closed: nothing typed"
        );
        app.world_mut().resource_mut::<Game>().screen = Some(Screen::Advancement);
        for c in ["l", "e", "d", "g", "x"] {
            press(&mut app, Key::Character(c.into()));
        }
        press(&mut app, Key::Backspace);
        press(&mut app, Key::Character("e".into()));
        press(&mut app, Key::Character("r".into()));
        assert_eq!(app.world().resource::<Game>().typed, "ledger");
        press(&mut app, Key::Enter);
        let sent = commands.try_recv().expect("Enter sends the command");
        assert!(matches!(sent.command, Cmd::Advancement(text) if text == "ledger"));
        assert!(app.world().resource::<Game>().typed.is_empty());
    }

    #[test]
    fn tab_switches_the_note_audience_and_enter_saves_the_opened_snapshot() {
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        let (mut app, mut commands) = app(view);
        app.world_mut().resource_mut::<Game>().screen = Some(Screen::Note {
            snapshot: tor_protocol::SnapshotId("opened-note-snapshot".into()),
            private: true,
        });
        // A later display must not silently retarget the open note.
        app.world_mut()
            .resource_mut::<Game>()
            .view
            .as_mut()
            .unwrap()
            .shown
            .snapshot = tor_protocol::SnapshotId("newer-display-snapshot".into());
        press(&mut app, Key::Tab);
        for c in ["o", "k"] {
            press(&mut app, Key::Character(c.into()));
        }
        press(&mut app, Key::Enter);
        let sent = commands.try_recv().expect("Enter saves the note");
        assert!(matches!(
            sent.command,
            Cmd::Note { snapshot, text, private: false } if text == "ok" && snapshot.0 == "opened-note-snapshot"
        ));
        assert_eq!(app.world().resource::<Game>().screen, None, "saving closes");
    }

    #[test]
    fn the_history_screen_lists_the_view_and_takes_no_typing() {
        let mut view = crate::test_support::view(0);
        view.history = vec!["T:3  Moved north.".into()];
        let (mut app, mut commands) = app(view);
        app.world_mut().resource_mut::<Game>().screen = Some(Screen::History);
        press(&mut app, Key::Character("x".into()));
        press(&mut app, Key::Enter);
        let game = app.world().resource::<Game>();
        assert!(game.typed.is_empty());
        assert!(commands.try_recv().is_err());
        assert!(lines(game).contains(&"T:3  Moved north.".to_string()));
    }

    #[test]
    fn a_number_picks_a_place_and_enter_renames_it() {
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        view.places = ["Hall", "Crypt"]
            .into_iter()
            .map(|name| tor_protocol::PlaceView {
                key: format!("{name}-key"),
                name: name.into(),
                origin: tor_protocol::PlaceNameOrigin::Invented,
            })
            .collect();
        let (mut app, mut commands) = app(view);
        app.world_mut().resource_mut::<Game>().screen = Some(Screen::Places { renaming: None });
        press(&mut app, Key::Character("7".into()));
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(Screen::Places { renaming: None }),
            "no seventh place"
        );
        press(&mut app, Key::Character("2".into()));
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(Screen::Places { renaming: Some(1) })
        );
        for c in ["B", "o", "n", "e", "s"] {
            press(&mut app, Key::Character(c.into()));
        }
        press(&mut app, Key::Enter);
        let sent = commands.try_recv().expect("Enter renames");
        assert!(matches!(
            sent.command,
            Cmd::RenamePlace { key, name } if key == "Crypt-key" && name == "Bones"
        ));
        assert_eq!(
            app.world().resource::<Game>().screen,
            Some(Screen::Places { renaming: None }),
            "back on the list"
        );
    }
}
