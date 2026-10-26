//! The owner's advancement screen: the server's pages as the shared client
//! presents them, and a typed command line using the shared command language.
use crate::{net::Cmd, Game, Net};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

/// Commands are short; this bounds what a stuck key can type.
const MAX_COMMAND: usize = 200;

#[derive(Component)]
pub(crate) struct AdvancementRoot;

#[derive(Component)]
pub(crate) struct AdvancementText;

pub(crate) fn setup(mut commands: Commands) {
    commands
        .spawn((
            AdvancementRoot,
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
            AdvancementText,
            Text::new(""),
            TextFont {
                font_size: 15.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.88, 0.8)),
        ));
}

/// Open or close the screen; opening asks for the overview.
pub(crate) fn toggle(net: &Net, game: &mut Game) {
    game.advancement_open = !game.advancement_open;
    game.typed.clear();
    if game.advancement_open {
        game.send(net, Cmd::Advancement("overview".into()));
    }
}

/// Typed characters edit the command line; Enter sends it.
pub(crate) fn typing(mut input: EventReader<KeyboardInput>, net: Res<Net>, mut game: ResMut<Game>) {
    if !game.advancement_open {
        input.clear();
        return;
    }
    for event in input.read() {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                let command = std::mem::take(&mut game.typed);
                if !command.trim().is_empty() {
                    game.send(&net, Cmd::Advancement(command));
                }
            }
            Key::Backspace => {
                game.typed.pop();
            }
            Key::Space if game.typed.len() < MAX_COMMAND => game.typed.push(' '),
            Key::Character(text) => {
                for c in text.chars().filter(|c| !c.is_control()) {
                    if game.typed.len() < MAX_COMMAND {
                        game.typed.push(c);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn show(
    game: Res<Game>,
    mut root: Query<&mut Visibility, With<AdvancementRoot>>,
    mut text: Query<&mut Text, With<AdvancementText>>,
) {
    if !game.is_changed() {
        return;
    }
    if let Ok(mut visibility) = root.single_mut() {
        let wanted = if game.advancement_open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
    if !game.advancement_open {
        return;
    }
    let mut lines = vec!["Advancement".to_string(), String::new()];
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
    let joined = lines.join("\n");
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

    #[test]
    fn typed_commands_are_edited_and_sent_only_while_the_screen_is_open() {
        let (net, mut commands) = crate::net::test_channel();
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                ..Default::default()
            })
            .add_event::<KeyboardInput>()
            .add_systems(Update, typing);
        let press = |app: &mut App, key: Key| {
            app.world_mut().send_event(self::key(key));
            app.update();
        };
        press(&mut app, Key::Character("x".into()));
        assert!(
            app.world().resource::<Game>().typed.is_empty(),
            "closed: nothing typed"
        );
        app.world_mut().resource_mut::<Game>().advancement_open = true;
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
}
