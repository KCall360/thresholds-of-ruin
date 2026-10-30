//! Character sheet and free navigation of the shared owner's advancement pages.
//! Display prose comes from the common reducer; it is never parsed into rules.
use crate::{chrome, net::Cmd, screen::Screen, Game, Net};
use bevy::prelude::*;

pub(crate) const SECTIONS: [(&str, &str); 7] = [
    ("Overview", "overview"),
    ("Point pools", "balances"),
    ("Skills", "skills"),
    ("Talents", "talents"),
    ("Expertises", "expertises"),
    ("Intellect", "intellect-expertises"),
    ("Ledger", "ledger"),
];

#[derive(Component)]
pub(crate) struct Root;
#[derive(Component)]
pub(crate) struct PageScroll;
#[derive(Component)]
pub(crate) enum Field {
    Attributes,
    Page,
    Prompt,
}
#[derive(Component, Clone, Copy)]
pub(crate) struct Browse(&'static str);

pub(crate) fn setup(mut commands: Commands) {
    commands.init_resource::<crate::progression::State>();
    commands
        .spawn((
            Root,
            chrome::BlockWorld,
            Interaction::default(),
            chrome::frame(),
            GlobalZIndex(5),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(16.),
                width: Val::Percent(54.),
                top: Val::Px(16.),
                bottom: Val::Px(184.),
                padding: UiRect::all(Val::Px(18.)),
                border: UiRect::all(Val::Px(1.)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .with_children(|sheet| {
            sheet.spawn((
                Text::new("CHARACTER & ADVANCEMENT"),
                chrome::text(24., chrome::INK),
                Node {
                    flex_shrink: 0.,
                    ..default()
                },
            ));
            sheet
                .spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(6.),
                    row_gap: Val::Px(6.),
                    flex_shrink: 0.,
                    ..default()
                })
                .with_children(|row| {
                    for (label, command) in SECTIONS {
                        button(row, label, command);
                    }
                });
            sheet.spawn((
                Field::Attributes,
                Text::new(""),
                chrome::text(17., chrome::INK),
                BorderColor(chrome::GOLD),
                Node {
                    width: Val::Percent(100.),
                    padding: UiRect::vertical(Val::Px(6.)),
                    border: UiRect::bottom(Val::Px(1.)),
                    flex_shrink: 0.,
                    ..default()
                },
            ));
            sheet
                .spawn((
                    PageScroll,
                    chrome::ScrollArea,
                    ScrollPosition::default(),
                    Node {
                        flex_grow: 1.,
                        flex_basis: Val::Px(0.),
                        min_height: Val::Px(0.),
                        width: Val::Percent(100.),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .with_children(|page| {
                    page.spawn((
                        Field::Page,
                        Text::new(""),
                        chrome::text(18., chrome::INK),
                        Node {
                            width: Val::Percent(100.),
                            flex_shrink: 0.,
                            ..default()
                        },
                    ));
                    page.spawn((
                        crate::progression::Root,
                        Node {
                            width: Val::Percent(100.),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.),
                            flex_shrink: 0.,
                            ..default()
                        },
                    ));
                });
            sheet
                .spawn(Node {
                    column_gap: Val::Px(8.),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(6.),
                    flex_shrink: 0.,
                    ..default()
                })
                .with_children(|row| {
                    button(row, "Page Up / Previous", "previous");
                    button(row, "Page Down / Next", "next");
                });
            sheet.spawn((
                crate::progression::ConfirmationRoot,
                Node {
                    width: Val::Percent(100.),
                    flex_shrink: 0.,
                    display: Display::None,
                    ..default()
                },
            ));
            sheet.spawn((
                Field::Prompt,
                Text::new(""),
                chrome::text(18., chrome::INK),
                Node {
                    flex_shrink: 0.,
                    ..default()
                },
            ));
            sheet.spawn((
                Text::new(
                    "Tab: sections · Up/Down: select · Enter: preview · F7: confirm
Type other commands below. Esc or F10 closes.",
                ),
                chrome::text(15., chrome::INK),
                Node {
                    flex_shrink: 0.,
                    ..default()
                },
            ));
        });
}

fn button(row: &mut ChildSpawnerCommands, label: &str, command: &'static str) {
    row.spawn((
        Button,
        Browse(command),
        BackgroundColor(chrome::BUTTON),
        BorderColor(chrome::GOLD),
        Node {
            padding: UiRect::axes(Val::Px(12.), Val::Px(7.)),
            border: UiRect::all(Val::Px(1.)),
            ..default()
        },
    ))
    .with_child((Text::new(label), chrome::text(18., chrome::INK)));
}

fn can_browse(game: &Game) -> bool {
    game.screen == Some(Screen::Advancement) && game.typed.is_empty() && !game.busy && !game.fatal
}

pub(crate) fn buttons(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut buttons: Query<(&Interaction, &Browse, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, action, mut color) in &mut buttons {
        color.0 = if *interaction == Interaction::None {
            chrome::BUTTON
        } else {
            chrome::HOT
        };
        if *interaction == Interaction::Pressed && can_browse(&game) {
            game.send(&net, Cmd::Advancement(action.0.into()));
        }
    }
}

fn attributes(game: &Game) -> String {
    let Some(stats) = game.view.as_ref().and_then(|view| view.attributes.as_ref()) else {
        return "ATTRIBUTES\n\nNot disclosed.".into();
    };
    format!("Strength     {}   ·   Speed     {}   ·   Intellect     {}\nWillpower     {}   ·   Awareness     {}   ·   Presence     {}",
        stats.strength, stats.speed, stats.intellect, stats.willpower, stats.awareness, stats.presence)
}

#[derive(Default)]
pub(crate) struct Shown {
    open: bool,
    serial: u64,
    lines: Vec<String>,
    typed: String,
}

type SheetRoot = (With<Root>, Without<Field>);

pub(crate) fn show(
    game: Res<Game>,
    mut shown: Local<Shown>,
    mut roots: Query<(&mut Visibility, &mut Node), SheetRoot>,
    mut fields: Query<(&Field, &mut Text, &mut Node), Without<Root>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut scroll: Query<&mut ScrollPosition, With<PageScroll>>,
) {
    let open = game.screen == Some(Screen::Advancement);
    let width = if window.single().is_ok_and(|window| window.width() < 1100.) {
        94.
    } else {
        54.
    };
    for (mut visibility, mut node) in &mut roots {
        visibility.set_if_neq(if open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        if node.width != Val::Percent(width) {
            node.width = Val::Percent(width);
        }
    }
    if !open {
        shown.open = false;
        return;
    }
    let newly_open = !shown.open;
    let page_changed = newly_open || shown.lines != game.advancement;
    if page_changed {
        for mut position in &mut scroll {
            position.offset_y = 0.;
        }
    }
    for (field, mut text, mut node) in &mut fields {
        if matches!(field, Field::Page) {
            let display = if crate::progression::graphical_page(&game)
                || crate::progression::structured_preview(&game)
            {
                Display::None
            } else {
                Display::Flex
            };
            if node.display != display {
                node.display = display;
            }
        }
        let value = match field {
            Field::Attributes if newly_open || shown.serial != game.view_serial => {
                Some(attributes(&game))
            }
            Field::Page if page_changed => Some(if game.advancement.is_empty() {
                "Waiting for the server's pages.".into()
            } else {
                game.advancement.join("\n")
            }),
            Field::Prompt if newly_open || shown.typed != game.typed => {
                Some(format!("> {}_", game.typed))
            }
            _ => None,
        };
        if let Some(value) = value {
            text.0 = value;
        }
    }
    if page_changed {
        shown.lines.clone_from(&game.advancement);
    }
    if shown.typed != game.typed {
        shown.typed.clone_from(&game.typed);
    }
    shown.serial = game.view_serial;
    shown.open = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smaller_windows_expand_the_character_sheet_instead_of_squeezing_it_beside_a_portrait() {
        let mut app = App::new();
        app.insert_resource(Game {
            screen: Some(Screen::Advancement),
            ..default()
        })
        .add_systems(Startup, setup)
        .add_systems(Update, show);
        let window = app
            .world_mut()
            .spawn((
                bevy::window::PrimaryWindow,
                Window {
                    resolution: bevy::window::WindowResolution::new(1440., 900.)
                        .with_scale_factor_override(1.),
                    ..default()
                },
            ))
            .id();
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<&Node, With<Root>>()
                .single(app.world())
                .unwrap()
                .width,
            Val::Percent(54.)
        );
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set(900., 700.);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<&Node, With<Root>>()
                .single(app.world())
                .unwrap()
                .width,
            Val::Percent(94.)
        );
    }

    #[test]
    fn browsing_uses_shared_commands_and_preserves_drafts_and_hidden_screen() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                ..default()
            })
            .add_systems(Update, buttons);
        let button = app
            .world_mut()
            .spawn((
                Interaction::Pressed,
                Browse("talents"),
                BackgroundColor::default(),
            ))
            .id();
        app.update();
        assert!(receiver.try_recv().is_err());
        app.world_mut().entity_mut(button).insert(Interaction::None);
        app.update();
        app.world_mut().resource_mut::<Game>().screen = Some(Screen::Advancement);
        app.world_mut().resource_mut::<Game>().typed = "purchase 1 pool talent example".into();
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            app.world().resource::<Game>().typed,
            "purchase 1 pool talent example"
        );
        for (_, command) in SECTIONS
            .into_iter()
            .chain([("Previous", "previous"), ("Next", "next")])
        {
            let (net, mut receiver) = crate::net::test_channel();
            app.insert_resource(net);
            app.world_mut().resource_mut::<Game>().typed.clear();
            app.world_mut().resource_mut::<Game>().busy = false;
            app.world_mut().entity_mut(button).insert(Interaction::None);
            app.update();
            app.world_mut()
                .entity_mut(button)
                .insert((Interaction::Pressed, Browse(command)));
            app.update();
            assert!(
                matches!(receiver.try_recv().unwrap().command, Cmd::Advancement(text) if text == command)
            );
            assert!(
                receiver.try_recv().is_err(),
                "one browse never implicitly confirms"
            );
        }
    }

    #[test]
    fn sheet_renders_disclosed_attributes_and_keeps_prompt_outside_the_scrolling_page() {
        let mut app = App::new();
        let mut view = crate::test_support::view(0);
        let stats = tor_protocol::AttributeView {
            strength: 7,
            speed: 4,
            intellect: 3,
            willpower: 2,
            awareness: 1,
            presence: 0,
        };
        view.attributes = Some(stats);
        app.insert_resource(Game {
            view: Some(view),
            screen: Some(Screen::Advancement),
            advancement: vec!["Server page".into()],
            ..default()
        })
        .add_systems(Startup, setup)
        .add_systems(Update, show);
        app.update();
        let world = app.world_mut();
        let rows: Vec<_> = world.query::<(&Field, &Text)>().iter(world).collect();
        assert!(rows
            .iter()
            .any(|(field, text)| matches!(field, Field::Attributes)
                && text.0.contains("Strength     7")));
        assert!(rows
            .iter()
            .any(|(field, text)| matches!(field, Field::Page) && text.0 == "Server page"));
        assert!(rows
            .iter()
            .any(|(field, text)| matches!(field, Field::Prompt) && text.0 == "> _"));
        let prompt = world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| matches!(field, Field::Prompt))
            .unwrap()
            .0;
        let parent = world.get::<ChildOf>(prompt).unwrap().parent();
        assert!(
            world.get::<Root>(parent).is_some(),
            "command line must stay outside scroll areas"
        );
        let mut commands: Vec<_> = world.query::<&Browse>().iter(world).map(|b| b.0).collect();
        commands.sort_unstable();
        assert_eq!(
            commands,
            [
                "balances",
                "expertises",
                "intellect-expertises",
                "ledger",
                "next",
                "overview",
                "previous",
                "skills",
                "talents"
            ],
            "the rendered catalog contains only these free browse operations"
        );
        assert_eq!(
            *world.get::<Visibility>(parent).unwrap(),
            Visibility::Visible
        );
        for mut scroll in world
            .query_filtered::<&mut ScrollPosition, With<PageScroll>>()
            .iter_mut(world)
        {
            scroll.offset_y = 100.;
        }
        {
            let mut game = world.resource_mut::<Game>();
            game.view.as_mut().unwrap().attributes = None;
            game.view_serial += 1;
            game.advancement = vec!["Replacement page".into()];
        }
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world
                .query_filtered::<&ScrollPosition, With<PageScroll>>()
                .single(world)
                .unwrap()
                .offset_y,
            0.,
            "a new server page starts at its beginning"
        );
        let rows: Vec<_> = world.query::<(&Field, &Text)>().iter(world).collect();
        assert!(
            rows.iter()
                .any(|(field, text)| matches!(field, Field::Attributes)
                    && text.0 == "ATTRIBUTES\n\nNot disclosed."),
            "redaction replaces previously visible attributes"
        );
        assert!(rows
            .iter()
            .any(|(field, text)| matches!(field, Field::Page) && text.0 == "Replacement page"));
        world.resource_mut::<Game>().screen = None;
        app.update();
        let world = app.world_mut();
        assert_eq!(
            *world
                .query_filtered::<&Visibility, With<Root>>()
                .single(world)
                .unwrap(),
            Visibility::Hidden
        );
        assert_eq!(attributes(&Game::default()), "ATTRIBUTES\n\nNot disclosed.");
    }
}
