//! Graphical owner choices request previews; only a separate control commits.
use crate::{chrome, net::Cmd, screen::Screen, Game, Net};
use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};
use tor_client_3d_model::{AdvancementAction, AdvancementBrowser, AdvancementOption};
use tor_protocol::{AdvancementQuery, ReplyContext};

#[derive(Component)]
pub(crate) struct Root;
#[derive(Component)]
pub(crate) struct ConfirmationRoot;
#[derive(Component)]
pub(crate) struct Row(pub(crate) usize, u64);
#[derive(Clone)]
enum Action {
    Choose(AdvancementAction),
    Confirm,
    Details(&'static str),
}
#[derive(Component, Clone)]
pub(crate) struct Control {
    generation: u64,
    action: Action,
}
impl Control {
    pub(crate) fn is_confirmation(&self) -> bool {
        matches!(self.action, Action::Confirm)
    }
}
#[derive(Resource, Default)]
pub(crate) struct State {
    browser: AdvancementBrowser,
    options: Vec<AdvancementOption>,
    selected: Option<usize>,
    stamp: Option<(u64, u64, bool, bool, bool, bool)>,
    generation: u64,
    dirty: bool,
    focus_pending: bool,
}
fn opened(game: &Game) -> bool {
    game.screen == Some(Screen::Advancement)
}
pub(crate) fn graphical_page(game: &Game) -> bool {
    if game.advancement_view.supplies {
        return false;
    }
    game.view
        .as_ref()
        .filter(|v| v.has_control)
        .and_then(|v| game.advancement_view.current_page(&v.shown))
        .is_some_and(|page| {
            matches!(
                page.query,
                AdvancementQuery::Balances { .. } | AdvancementQuery::Catalog { .. }
            )
        })
}
pub(crate) fn structured_preview(game: &Game) -> bool {
    preview::visible(game)
}

pub(crate) fn sync(game: Res<Game>, mut state: ResMut<State>) {
    let stamp = (
        game.view_serial,
        game.advancement_serial,
        game.busy,
        game.fatal,
        opened(&game),
        game.typed.is_empty(),
    );
    if state.stamp == Some(stamp) {
        return;
    }
    let old = state
        .selected
        .and_then(|index| state.options.get(index))
        .map(|o| o.action.clone());
    if opened(&game) {
        if let Some(view) = game.view.as_ref().filter(|v| v.has_control) {
            state.browser.reconcile(&game.advancement_view, &view.shown);
            state.options = state.browser.options(&game.advancement_view, &view.shown);
        } else {
            state.options.clear();
            state.browser = AdvancementBrowser::default();
        }
    } else {
        state.options.clear();
        state.browser = AdvancementBrowser::default();
    }
    state.selected = old
        .and_then(|old| state.options.iter().position(|o| o.action == old))
        .or_else(|| (!state.options.is_empty()).then_some(0));
    state.stamp = Some(stamp);
    state.generation += 1;
    state.dirty = true;
    state.focus_pending = true;
}
fn execute(action: Action, game: &mut Game, net: &Net, state: &mut State) {
    if !opened(game) || game.busy || game.fatal || !game.typed.is_empty() {
        return;
    }
    let Some(view) = game.view.as_ref().filter(|v| v.has_control) else {
        return;
    };
    let snapshot = &game.advancement_view;
    let command = match action {
        Action::Choose(choice) => state.browser.choose(snapshot, &view.shown, &choice),
        Action::Confirm => snapshot
            .confirm_command(&view.shown, view.can_act)
            .map(str::to_owned),
        Action::Details(section) => snapshot
            .current_page(&view.shown)
            .and(snapshot.confirmation.as_ref())
            .map(|_| format!("details {section}")),
    };
    if let Some(command) = command {
        game.send(net, Cmd::Advancement(command));
        state.dirty = true;
    }
}
pub(crate) fn controls(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut state: ResMut<State>,
    mut keys: EventReader<KeyboardInput>,
    mut buttons: Query<
        (&Interaction, &Control, &mut BackgroundColor, Option<&Row>),
        Changed<Interaction>,
    >,
) {
    for event in keys.read() {
        if !event.state.is_pressed() || !opened(&game) || !game.typed.is_empty() {
            continue;
        }
        match event.logical_key {
            Key::ArrowUp | Key::ArrowDown if !state.options.is_empty() => {
                let count = state.options.len();
                let selected = state.selected.unwrap_or(0);
                state.selected = Some(if event.logical_key == Key::ArrowDown {
                    (selected + 1) % count
                } else {
                    (selected + count - 1) % count
                });
                state.dirty = true;
                state.focus_pending = true;
            }
            Key::Enter if !event.repeat => {
                if let Some(action) = state
                    .selected
                    .and_then(|index| state.options.get(index))
                    .map(|option| option.action.clone())
                {
                    execute(Action::Choose(action), &mut game, &net, &mut state);
                }
            }
            Key::F7 if !event.repeat => execute(Action::Confirm, &mut game, &net, &mut state),
            _ => {}
        }
    }
    for (interaction, control, mut color, row) in &mut buttons {
        color.0 = if *interaction != Interaction::None
            || row.is_some_and(|row| Some(row.0) == state.selected && row.1 == state.generation)
        {
            chrome::HOT
        } else {
            chrome::BUTTON
        };
        if *interaction == Interaction::Pressed && control.generation == state.generation {
            execute(control.action.clone(), &mut game, &net, &mut state);
        }
    }
}
fn text(parent: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32) {
    parent.spawn((
        Text::new(value),
        bevy::text::TextLayout {
            linebreak: bevy::text::LineBreak::WordOrCharacter,
            ..default()
        },
        chrome::text(size, chrome::INK),
        Node {
            width: Val::Percent(100.),
            flex_shrink: 0.,
            ..default()
        },
    ));
}
fn button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: Action,
    state: &State,
    selected: bool,
    available: bool,
    index: Option<usize>,
) {
    let mut entity = parent.spawn((
        Button,
        Control {
            generation: state.generation,
            action,
        },
        BackgroundColor(if selected {
            chrome::HOT
        } else {
            chrome::BUTTON
        }),
        BorderColor(if selected { chrome::INK } else { chrome::GOLD }),
        Node {
            width: Val::Percent(100.),
            padding: UiRect::all(Val::Px(10.)),
            border: UiRect::all(Val::Px(1.)),
            flex_shrink: 0.,
            ..default()
        },
    ));
    if let Some(index) = index {
        entity.insert(Row(index, state.generation));
    }
    entity.with_child((
        Text::new(label),
        bevy::text::TextLayout {
            linebreak: bevy::text::LineBreak::WordOrCharacter,
            ..default()
        },
        chrome::text(
            18.,
            if available {
                chrome::INK
            } else {
                Color::srgb(0.50, 0.47, 0.41)
            },
        ),
        Node {
            width: Val::Percent(100.),
            ..default()
        },
    ));
}
pub(crate) fn draw(
    mut commands: Commands,
    game: Res<Game>,
    mut state: ResMut<State>,
    roots: Query<Entity, With<Root>>,
    mut footers: Query<(Entity, &mut Node), With<ConfirmationRoot>>,
) {
    if !state.dirty {
        return;
    }
    let Ok(root) = roots.single() else {
        return;
    };
    state.dirty = false;
    for (entity, mut node) in &mut footers {
        commands.entity(entity).despawn_related::<Children>();
        let show = opened(&game)
            && game.view.as_ref().is_some_and(|view| {
                view.has_control && game.advancement_view.current_page(&view.shown).is_some()
            })
            && game.advancement_view.confirmation.is_some();
        node.display = if show { Display::Flex } else { Display::None };
        if show {
            commands
                .entity(entity)
                .with_children(|footer| confirmation(footer, &game, &state));
        }
    }
    commands.entity(root).despawn_related::<Children>();
    if !opened(&game) {
        return;
    }
    let Some(view) = game.view.as_ref().filter(|v| v.has_control) else {
        return;
    };
    commands.entity(root).with_children(|root| {
        let snapshot = &game.advancement_view;
        let Some(page) = snapshot.current_page(&view.shown) else {
            text(root, "Open Overview to refresh advancement choices.", 18.);
            return;
        };
        if graphical_page(&game) {
            text(
                root,
                match page.query {
                    AdvancementQuery::Balances { .. } => "CHOOSE A POINT POOL",
                    _ => "CHOOSE A PURCHASE TO PREVIEW",
                },
                23.,
            );
            if let Some(pool) = state.browser.funding(snapshot, &view.shown) {
                text(
                    root,
                    format!(
                        "Funding: HD {} · {} · {} unspent",
                        pool.owner + 1,
                        pool.pool.name,
                        pool.unspent
                    ),
                    18.,
                );
            } else if matches!(page.query, AdvancementQuery::Catalog { .. }) {
                text(root, "Choose a compatible pool in Point pools first.", 18.);
            }
            text(
                root,
                format!(
                    "{} entries in this section. Requirements and costs are checked by preview.",
                    page.total
                ),
                15.,
            );
            for (index, option) in state.options.iter().enumerate() {
                let label = format!(
                    "{}{}",
                    if state.selected == Some(index) {
                        "> "
                    } else {
                        ""
                    },
                    option.label
                );
                button(
                    root,
                    &label,
                    Action::Choose(option.action.clone()),
                    &state,
                    state.selected == Some(index),
                    option.available && !game.busy && game.typed.is_empty(),
                    Some(index),
                );
            }
            if state.options.is_empty() {
                text(root, "No choices on this page.", 18.);
            }
            text(root, "Up / Down selects · Enter requests a preview", 15.);
        }
        if let Some(confirmation) = &snapshot.confirmation {
            if preview::visible(&game) {
                preview::draw(root, &game, &confirmation.summary);
            }
            button(
                root,
                "Review point balances / costs",
                Action::Details("balances"),
                &state,
                false,
                !game.busy && game.typed.is_empty(),
                None,
            );
            button(
                root,
                "Review refunds",
                Action::Details("refunds"),
                &state,
                false,
                !game.busy && game.typed.is_empty(),
                None,
            );
        }
    });
}
fn confirmation(root: &mut ChildSpawnerCommands, game: &Game, state: &State) {
    let Some(view) = &game.view else {
        return;
    };
    let snapshot = &game.advancement_view;
    let permitted = snapshot
        .confirm_command(&view.shown, view.can_act)
        .is_some()
        && !game.busy
        && !game.fatal
        && game.typed.is_empty();
    let Some(confirmation) = &snapshot.confirmation else {
        return;
    };
    let prefix = if !game.typed.is_empty() {
        "Finish command to confirm"
    } else if permitted {
        "F7 Confirm"
    } else {
        "Waiting to confirm"
    };
    let label = format!(
        "{prefix}: {}",
        tor_client_common::advancement::proposal_label(&confirmation.proposal)
    );
    button(root, &label, Action::Confirm, state, false, permitted, None);
}
/// Keep a keyboard-selected card within the actual laid-out viewport.
pub(crate) fn focus(
    game: Res<Game>,
    mut state: ResMut<State>,
    rows: Query<(&Row, &ComputedNode, &GlobalTransform)>,
    mut areas: Query<
        (&ComputedNode, &GlobalTransform, &mut ScrollPosition),
        With<crate::character::PageScroll>,
    >,
) {
    if !opened(&game) || !state.focus_pending {
        return;
    }
    let Some((_, row, position)) = rows
        .iter()
        .find(|(row, _, _)| Some(row.0) == state.selected && row.1 == state.generation)
    else {
        return;
    };
    if row.size().min_element() <= 0. {
        return;
    }
    for (area, transform, mut scroll) in &mut areas {
        if area.size().y <= 0. {
            continue;
        }
        let top = transform.translation().y - area.size().y * 0.5;
        let bottom = top + area.size().y;
        let row_top = position.translation().y - row.size().y * 0.5;
        let row_bottom = row_top + row.size().y;
        let delta = if row_top < top {
            row_top - top
        } else {
            (row_bottom - bottom).max(0.)
        };
        let max = (area.content_size().y - area.size().y).max(0.) * area.inverse_scale_factor();
        if delta != 0. {
            scroll.offset_y =
                (scroll.offset_y + delta * area.inverse_scale_factor()).clamp(0., max);
        }
        state.focus_pending = false;
    }
}
pub(crate) fn diagnostics(game: &Game, state: &State) -> serde_json::Value {
    let shown: Option<&ReplyContext> = game.view.as_ref().map(|v| &v.shown);
    serde_json::json!({
        "structured_preview": structured_preview(game),
        "supplies": game.advancement_view.supplies,
        "selected": state.selected,
        "rows": state.options.iter().map(|option| match &option.action {
            AdvancementAction::Pool { row, kind } => serde_json::json!({"label":option.label,"available":option.available,"pool":row.pool.id,"owner":row.owner,"kind":format!("{kind:?}")}),
            AdvancementAction::Preview(selection) => serde_json::json!({"label":option.label,"available":option.available,"selection":selection}),
        }).collect::<Vec<_>>(),
        "funding": shown.and_then(|shown| state.browser.funding(&game.advancement_view, shown)).map(|pool|serde_json::json!({"pool":pool.pool.id,"owner":pool.owner})),
        "can_confirm": shown.is_some_and(|shown| game.view.as_ref().is_some_and(|view| game.advancement_view.confirm_command(shown, view.can_act && view.has_control).is_some() && !game.busy && !game.fatal && game.typed.is_empty())),
        "proposal": game.advancement_view.confirmation.as_ref().map(|c| &c.proposal),
    })
}

mod preview;

#[cfg(test)]
mod tests;
