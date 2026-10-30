//! Interacting with the world: the cell under the mouse, what a click there
//! would do, the inventory panel and object keys.
//!
//! Every action is decided from the view the server sent, as the ASCII
//! client decides it; the server stays the authority and explains refusals.
use crate::{
    net::{Cmd, View},
    props::{Accent, Model, PropLibrary},
    stand, Game, Net,
};
use bevy::{prelude::*, window::PrimaryWindow};
use std::collections::HashSet;
use tor_client_common::items::ItemOperation;
use tor_protocol::{ActorTarget, ItemTarget, Position};

use tor_client_3d_model::{
    approach_cells, attack_at_hand, describe_focus, known_cells, step_toward, Focus,
};
pub(crate) use tor_client_3d_model::{direction, Intent};

trait IntentAccent {
    fn accent(&self) -> Accent;
}
impl IntentAccent for Intent {
    fn accent(&self) -> Accent {
        match self {
            Intent::Attack(_) | Intent::Charge(_) => Accent::Danger,
            Intent::Travel(_) => Accent::Travel,
            _ => Accent::Gold,
        }
    }
}

/// The hovered cell and what is known of it.
#[derive(Resource, Default)]
pub struct Hover {
    /// In steady world grid coordinates.
    pub(super) cell: Option<IVec3>,
    pub(super) title: String,
    description: String,
    intent: Option<Intent>,
    /// What is under the pointer, for aiming abilities at creatures.
    focus: Option<Focus>,
    over_ui: bool,
}

impl Hover {
    /// The cursor for what a click here would do.
    pub(crate) fn pointer(&self) -> crate::cursor::Pointer {
        if self.over_ui || self.cell.is_none() {
            return crate::cursor::Pointer::Default;
        }
        crate::cursor::pointer(self.intent.as_ref())
    }
}

/// A click's errand that takes several turns: walking up to an item or a
/// creature, then picking it up or attacking.
#[derive(Resource, Default)]
pub struct Pursuit {
    goal: Option<Goal>,
    steps: u32,
    /// The view the last step was sent from; the next waits for a newer one.
    sent_at: Option<u64>,
    /// Journeys asked of the server's travel so far; after a few, or once
    /// one is refused, the errand walks a step at a time instead.
    travels: u32,
    /// A journey was just asked for; a status after it means it was refused.
    travelling: bool,
    travel_refused: bool,
    /// The final pick-up or blow has been sent; done unless it was refused.
    finishing: bool,
    refused: u32,
}

impl Pursuit {
    pub(crate) fn active(&self) -> bool {
        self.goal.is_some()
    }

    pub(crate) fn chase(&mut self, actor: ActorTarget) {
        *self = Self {
            goal: Some(Goal::Attack(actor)),
            ..default()
        };
    }

    pub(crate) fn fetch(&mut self, item: ItemTarget) {
        *self = Self {
            goal: Some(Goal::Take(item)),
            ..default()
        };
    }
}

#[derive(Clone, Copy)]
enum Goal {
    Take(ItemTarget),
    Attack(ActorTarget),
}

#[derive(Resource, Default)]
pub struct Panel {
    pub open: bool,
    pub(crate) page: crate::equipment::Page,
    pub(crate) equipment_slot: Option<u16>,
    pub(crate) browser: tor_client_3d_model::InventoryBrowser,
    pub(crate) scope: Option<crate::inventory::Scope>,
    pub(crate) search_focused: bool,
    pub(crate) keyboard_captured: bool,
    pub(crate) dirty: bool,
    pub(crate) reset_scroll: bool,
    pub(crate) scroll: f32,
    built_for: u64,
    reconciled_for: u64,
    /// Whether the panel was opened for the identification choice now waiting.
    prompted: bool,
    /// The projectile prompt it was last built with.
    shown_projectile: Option<crate::net::ProjectilePrompt>,
    /// Its actions in plan order, for the number keys.
    pub(crate) actions: Vec<(String, PanelButton)>,
}

#[derive(Component)]
pub(crate) struct Selection(Accent);

#[derive(Component)]
pub(crate) struct Tooltip;

#[derive(Component)]
pub(crate) enum TooltipLine {
    Title,
    Hint,
    Body,
}

type TooltipFilter = (With<Tooltip>, Without<Selection>);

#[derive(Component)]
pub(crate) struct PanelRoot;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelButton {
    Take(ItemTarget),
    Drop(ItemTarget),
    Use(ItemTarget, ItemOperation),
    /// Hold a directional device until a direction is chosen.
    Aim(ItemTarget),
    /// Zap a device that offers yourself as its target.
    ZapSelf(ItemTarget),
    Identify(Option<ItemTarget>),
    /// The view's granted ability at this index.
    Ability(usize),
    Escape(tor_protocol::HoldTarget),
    ProjectileChoose(usize),
    ProjectileNext,
    ProjectilePrevious,
    ProjectileCancel,
}

use crate::chrome::{BUTTON, GOLD, HOT, INK, MUTED, STONE};

pub fn setup(
    mut commands: Commands,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for accent in [Accent::Gold, Accent::Danger, Accent::Travel, Accent::Teal] {
        let model: Model = library.selection(accent, &mut meshes);
        commands
            .spawn((Selection(accent), Transform::default(), Visibility::Hidden))
            .with_children(|parent| model.attach(parent));
    }
    commands
        .spawn((
            Tooltip,
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                max_width: Val::Px(320.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(STONE),
            BorderColor(GOLD),
            BorderRadius::all(Val::Px(4.0)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|tip| {
            tip.spawn((
                TooltipLine::Title,
                Text::new(""),
                TextFont {
                    font: crate::chrome::FONT,
                    font_size: 16.0,
                    ..default()
                },
                TextColor(INK),
            ));
            tip.spawn((
                TooltipLine::Hint,
                Text::new(""),
                TextFont {
                    font: crate::chrome::FONT,
                    font_size: 13.0,
                    ..default()
                },
                TextColor(GOLD),
            ));
            tip.spawn((
                TooltipLine::Body,
                Text::new(""),
                TextFont {
                    font: crate::chrome::FONT,
                    font_size: 13.0,
                    ..default()
                },
                TextColor(MUTED),
            ));
        });
    commands.spawn((
        PanelRoot,
        crate::chrome::BlockWorld,
        crate::chrome::FrameSkin,
        GlobalZIndex(5),
        Interaction::default(),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(16.0),
            bottom: Val::Px(184.0),
            width: Val::Percent(54.0),
            min_width: Val::Px(320.0),
            max_width: Val::Percent(94.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(12.0)),
            border: UiRect::all(Val::Px(1.0)),
            row_gap: Val::Px(6.0),
            ..default()
        },
        BackgroundColor(STONE),
        BorderColor(GOLD),
        BorderRadius::all(Val::Px(6.0)),
        Visibility::Hidden,
    ));
}

/// Find the floor cell under the mouse and read what is there.
pub fn hover(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), crate::portrait::ExplorationCamera>,
    ui: crate::chrome::UiPointer,
    game: Res<Game>,
    mut hover: ResMut<Hover>,
    models: crate::picking::Models,
) {
    *hover = Hover::default();
    hover.over_ui = ui.over();
    let (Ok(window), Ok((camera, eye)), Some(view)) =
        (window.single(), camera.single(), &game.view)
    else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(eye, cursor) else {
        return;
    };
    // Match the scene's permanent omission of overhead terrain.
    let Some(focus) = models.select(ray, view, game.offset, false) else {
        return;
    };
    let Some(local) = focus.position(view) else {
        return;
    };
    let cell = crate::ivec(local) + game.offset;
    let (title, description, intent) = describe_focus(view, focus);
    if title.is_empty() {
        return;
    }
    hover.cell = Some(cell);
    hover.focus = Some(focus);
    hover.title = title;
    hover.description = description;
    hover.intent = intent;
}

pub fn show_hover(
    time: Res<Time>,
    hover: Res<Hover>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut marks: Query<(&Selection, &mut Transform, &mut Visibility)>,
    mut tooltip: Query<(&mut Node, &mut Visibility), TooltipFilter>,
    mut labels: Query<(&TooltipLine, &mut Text)>,
) {
    let shown = hover.cell.filter(|_| !hover.over_ui);
    let accent = hover.intent.as_ref().map_or(Accent::Teal, Intent::accent);
    let pulse = 1.0 + (time.elapsed_secs() * 4.0).sin() * 0.03;
    for (selection, mut transform, mut visibility) in &mut marks {
        match shown {
            Some(cell) if selection.0 == accent => {
                transform.translation = stand(cell) + Vec3::Y * 0.02;
                transform.scale = Vec3::new(pulse, 1.0, pulse);
                *visibility = Visibility::Visible;
            }
            _ => *visibility = Visibility::Hidden,
        }
    }
    let Ok((mut node, mut visibility)) = tooltip.single_mut() else {
        return;
    };
    let window = window.single().ok();
    let cursor = window.and_then(Window::cursor_position);
    let size = window.map_or(Vec2::ZERO, |w| w.size());
    match (shown, cursor) {
        (Some(_), Some(cursor)) => {
            // Keep the tooltip on screen, flipping it to the cursor's other side.
            let left = if cursor.x + 350.0 > size.x {
                cursor.x - 340.0
            } else {
                cursor.x + 18.0
            };
            let top = if cursor.y + 120.0 > size.y {
                cursor.y - 100.0
            } else {
                cursor.y + 14.0
            };
            node.left = Val::Px(left.max(4.0));
            node.top = Val::Px(top.max(4.0));
            *visibility = Visibility::Visible;
            for (line, mut text) in &mut labels {
                match line {
                    TooltipLine::Title => text.0.clone_from(&hover.title),
                    TooltipLine::Hint => {
                        text.0 = hover.intent.as_ref().map_or("", Intent::hint).into()
                    }
                    TooltipLine::Body => text.0.clone_from(&hover.description),
                }
            }
        }
        _ => *visibility = Visibility::Hidden,
    }
}

fn send(net: &Net, game: &mut Game, command: Cmd) {
    game.send(net, command);
}

pub fn click(
    mouse: Res<ButtonInput<MouseButton>>,
    hover: Res<Hover>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut pursuit: ResMut<Pursuit>,
) {
    if !mouse.just_pressed(MouseButton::Left) || hover.over_ui || game.screen.is_some() {
        return;
    }
    // An ability waiting for a creature takes the clicked one.
    if let (Some(aim), Some(Focus::Actor(actor))) = (&game.aiming, hover.focus) {
        if let Some(command) = aim.at(actor) {
            game.aiming = None;
            send(&net, &mut game, command);
            return;
        }
    }
    *pursuit = Pursuit::default();
    match &hover.intent {
        Some(Intent::Fetch(item)) => pursuit.goal = Some(Goal::Take(*item)),
        Some(Intent::Charge(actor)) => pursuit.goal = Some(Goal::Attack(*actor)),
        Some(intent) => {
            if let Some(command) = intent.command() {
                send(&net, &mut game, command);
            }
        }
        None => {}
    }
}

/// Carry out a clicked errand a step at a time: walk the known floor toward
/// the item or creature, then pick it up or strike. Any key calls it off.
pub fn pursue(
    keys: Res<ButtonInput<KeyCode>>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut pursuit: ResMut<Pursuit>,
) {
    let Some(goal) = pursuit.goal else {
        return;
    };
    if keys.get_just_pressed().next().is_some() {
        *pursuit = Pursuit::default();
        return;
    }
    // Let the state settle after each step: a request made from a stale
    // revision is refused.
    let fresh = (pursuit.sent_at != Some(game.view_serial) && game.since_send > 0.2)
        || game.since_send > 1.5;
    if game.busy || game.fatal || !fresh {
        return;
    }
    if pursuit.travelling {
        pursuit.travelling = false;
        // A stale revision only means the state moved on: ask again.
        if game.status.starts_with("Refresh") {
            pursuit.travels = pursuit.travels.saturating_sub(1);
        } else if !game.status.is_empty() {
            pursuit.travel_refused = true;
        }
    }
    if pursuit.finishing {
        if game.status.is_empty() {
            *pursuit = Pursuit::default();
            return;
        }
        pursuit.finishing = false;
        pursuit.refused += 1;
        if pursuit.refused > 5 {
            *pursuit = Pursuit::default();
            return;
        }
    }
    let Some(view) = &game.view else {
        return;
    };
    if !view.can_act {
        // Mid-fight, time moves on only when asked: continue until our turn.
        if view.actors.iter().any(|a| a.hostile) {
            pursuit.sent_at = Some(game.view_serial);
            send(&net, &mut game, Cmd::Wait);
        }
        return;
    }
    let feet = view.feet;
    let command = match goal {
        Goal::Take(id) => match view.items.iter().find(|i| i.id == id) {
            None => None,
            Some(item) if item.reachable => {
                pursuit.finishing = true;
                Some(Cmd::Take(id))
            }
            Some(item) => {
                let key = known_cells(view)
                    .find(|c| c.position == item.position)
                    .map(|c| c.key.to_owned());
                let goals = HashSet::from([item.position]);
                journey(&mut pursuit, view, key, &goals)
            }
        },
        Goal::Attack(id) => match view.actors.iter().find(|a| a.id == id && a.hostile) {
            None => None,
            Some(_) => {
                match attack_at_hand(view, id) {
                    Some(command) => {
                        pursuit.finishing = true;
                        Some(command)
                    }
                    None => {
                        let goals = approach_cells(view, id);
                        // Travel to the open cell beside it nearest to us.
                        let key = known_cells(view)
                            .filter(|c| goals.contains(&c.position))
                            .min_by_key(|c| {
                                [
                                    (c.position.x, feet.x),
                                    (c.position.y, feet.y),
                                    (c.position.z, feet.z),
                                ]
                                .into_iter()
                                .map(|(a, b)| (i64::from(a) - i64::from(b)).abs())
                                .max()
                                .unwrap_or(0)
                            })
                            .map(|c| c.key.to_owned());
                        journey(&mut pursuit, view, key, &goals)
                    }
                }
            }
        },
    };
    pursuit.steps += 1;
    if std::env::var_os("TOR_3D_DEBUG").is_some() {
        eprintln!(
            "pursue: step {} {}",
            pursuit.steps,
            match &command {
                Some(Cmd::Move(d)) => format!("move {d:?}"),
                Some(Cmd::Take(_)) => "take".into(),
                Some(Cmd::Travel(_)) => "travel".into(),
                Some(_) => "other".into(),
                None => "none".into(),
            }
        );
    }
    match command {
        Some(command) if pursuit.steps <= 80 => {
            pursuit.travelling = matches!(command, Cmd::Travel(_));
            pursuit.sent_at = Some(game.view_serial);
            send(&net, &mut game, command);
        }
        _ => {
            if command.is_none() && pursuit.steps > 1 {
                // The item or creature is gone: nothing more to do.
            } else {
                game.status = "You can't find a way there.".into();
            }
            *pursuit = Pursuit::default();
        }
    }
}

/// The next move of an errand toward a cell: the server's own travel while
/// it serves (a journey can stop short when something comes into view, so
/// it is asked again), then single steps over the known floor.
fn journey(
    pursuit: &mut Pursuit,
    view: &View,
    key: Option<String>,
    goals: &HashSet<Position>,
) -> Option<Cmd> {
    match key {
        Some(key) if pursuit.travels < 4 && !pursuit.travel_refused => {
            pursuit.travels += 1;
            Some(Cmd::Travel(key))
        }
        _ => step_toward(view, goals).map(Cmd::Move),
    }
}

/// G picks up, X opens or closes a door beside you, I shows your pack.
#[allow(clippy::too_many_arguments)]
pub fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    hover: Res<Hover>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut panel: ResMut<Panel>,
    mut exit: EventWriter<AppExit>,
) {
    panel.keyboard_captured = false;
    if panel.open
        && crate::inventory::grid(&game)
        && keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.just_pressed(KeyCode::KeyF)
    {
        panel.search_focused = true;
        panel.keyboard_captured = true;
        panel.dirty = true;
        return;
    }
    if panel.open && panel.search_focused {
        panel.keyboard_captured = true;
        if keys.just_pressed(KeyCode::Escape) {
            panel.search_focused = false;
            panel.dirty = true;
        }
        return;
    }
    // A full screen takes the keyboard for its typed line.
    use crate::screen::{toggle, Screen};
    if keys.just_pressed(KeyCode::F10) {
        toggle(&net, &mut game, Screen::Advancement);
        return;
    }
    if keys.just_pressed(KeyCode::F2) {
        toggle(&net, &mut game, Screen::History);
        return;
    }
    if keys.just_pressed(KeyCode::F4) {
        let snapshot = game.view.as_ref().map(|view| view.shown.snapshot.clone());
        if let Some(snapshot) = snapshot.filter(|_| {
            game.screen.as_ref().is_none_or(|open| {
                matches!(
                    open,
                    Screen::Note { .. } | Screen::Places { .. } | Screen::History
                )
            })
        }) {
            toggle(
                &net,
                &mut game,
                Screen::Note {
                    snapshot,
                    private: true,
                },
            );
        }
        return;
    }
    if keys.just_pressed(KeyCode::F5) {
        toggle(&net, &mut game, Screen::Places { renaming: None });
        return;
    }
    if keys.just_pressed(KeyCode::KeyC)
        && game
            .screen
            .as_ref()
            .is_none_or(|open| *open == Screen::Charts)
    {
        toggle(&net, &mut game, Screen::Charts);
        return;
    }
    if let Some(open) = game.screen.clone() {
        if keys.just_pressed(KeyCode::Escape) {
            toggle(&net, &mut game, open);
        }
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if game.projectile.is_some() {
            game.aiming = None;
            send(
                &net,
                &mut game,
                Cmd::Projectile(tor_client_3d_model::ProjectileStep::Cancel),
            );
        } else if game.aiming.take().is_some() {
            game.status = "You stop aiming.".into();
        } else if panel.open {
            panel.open = false;
        } else {
            exit.write(AppExit::Success);
        }
        return;
    }
    if keys.any_just_pressed([KeyCode::KeyI, KeyCode::KeyZ]) {
        panel.toggle_page(crate::equipment::Page::Inventory);
    }
    if keys.just_pressed(KeyCode::F6) {
        panel.toggle_page(crate::equipment::Page::Equipment);
    }
    if panel.open && crate::inventory::grid(&game) && keys.just_pressed(KeyCode::Tab) {
        let next = if panel.page == crate::equipment::Page::Inventory {
            crate::equipment::Page::Equipment
        } else {
            crate::equipment::Page::Inventory
        };
        panel.set_page(next);
        panel.keyboard_captured = true;
        return;
    }
    // Paused or ongoing work, as the ASCII client's F8 and F9.
    if keys.just_pressed(KeyCode::F8) {
        send(&net, &mut game, Cmd::Resume);
        return;
    }
    if keys.just_pressed(KeyCode::F9) {
        send(&net, &mut game, Cmd::Cancel);
        return;
    }
    if panel.open {
        const DIGITS: [KeyCode; crate::pack::HOTKEYS] = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
            KeyCode::Digit0,
        ];
        if let Some(position) = DIGITS.iter().position(|key| keys.just_pressed(*key)) {
            if let Some((_, action)) = panel.actions.get(position).cloned() {
                activate(action, &net, &mut panel, &mut game);
            }
            return;
        }
    }
    let Some(view) = &game.view else {
        return;
    };
    if keys.any_just_pressed([KeyCode::KeyG, KeyCode::Comma]) {
        // Things underfoot first, then anything else within reach.
        let feet = view.feet;
        let item = view
            .items
            .iter()
            .filter(|i| i.reachable)
            .min_by_key(|i| (i.position.x - feet.x).abs() + (i.position.y - feet.y).abs());
        match item.map(|i| i.id) {
            Some(item) => send(&net, &mut game, Cmd::Take(item)),
            None => game.status = "There is nothing here to pick up.".into(),
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        let hovered = hover
            .intent
            .clone()
            .filter(|i| matches!(i, Intent::Door(..)));
        let adjacent = || {
            view.visible
                .iter()
                .filter_map(|cell| cell.door.as_ref())
                .find(|door| door.reachable)
                .map(|door| Intent::Door(door.id, !door.open))
        };
        match hovered.or_else(adjacent) {
            Some(intent) => {
                if let Some(command) = intent.command() {
                    send(&net, &mut game, command);
                }
            }
            None => game.status = "There is no door beside you.".into(),
        }
    }
}

/// Rebuild the inventory panel when it opens or what it shows changes.
pub fn panel(
    mut commands: Commands,
    mut panel: ResMut<Panel>,
    game: Res<Game>,
    mut root: Query<(Entity, &mut Visibility), With<PanelRoot>>,
    scroll: Query<&ScrollPosition, With<crate::inventory::Scroll>>,
) {
    let Ok((root, mut visibility)) = root.single_mut() else {
        return;
    };
    // A waiting identification or projectile choice opens the pack once.
    let waiting = game.view.as_ref().is_some_and(|v| v.identify.is_some())
        || game.projectile.as_ref().is_some_and(|p| p.choosing);
    if waiting && !panel.prompted {
        panel.open = true;
        panel.built_for = u64::MAX;
    }
    panel.prompted = waiting;
    if let Some(view) = &game.view {
        if panel
            .scope
            .as_ref()
            .is_none_or(|scope| !scope.matches(view))
        {
            panel.browser = default();
            if panel.page == crate::equipment::Page::Equipment {
                panel
                    .browser
                    .set_filter(tor_client_3d_model::InventoryFilter::Gear);
            }
            panel.equipment_slot = None;
            panel.scope = Some(crate::inventory::Scope::of(view));
            panel.search_focused = false;
            panel.reset_scroll = true;
            panel.dirty = true;
        }
        if panel.reconciled_for != game.view_serial || panel.dirty {
            panel.browser.reconcile(view);
            panel.reconciled_for = game.view_serial;
        }
    }
    if !panel.open || !crate::inventory::grid(&game) {
        panel.search_focused = false;
    }
    let wanted = if panel.open && game.screen.is_none() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    visibility.set_if_neq(wanted);
    if !panel.open
        || (!panel.dirty
            && panel.built_for == game.view_serial
            && panel.shown_projectile == game.projectile)
    {
        return;
    }
    if panel.reset_scroll || panel.shown_projectile != game.projectile {
        panel.scroll = 0.0;
    } else if let Ok(scroll) = scroll.single() {
        panel.scroll = scroll.offset_y;
    }
    panel.dirty = false;
    panel.reset_scroll = false;
    panel.shown_projectile.clone_from(&game.projectile);
    panel.built_for = game.view_serial;
    commands.entity(root).despawn_related::<Children>();
    let Some(view) = &game.view else {
        return;
    };
    let lines = crate::pack::plan(view, game.projectile.as_ref());
    panel.actions = crate::pack::actions(&lines);
    commands.entity(root).with_children(|ui| {
        if crate::inventory::grid(&game) {
            if panel.page == crate::equipment::Page::Equipment {
                crate::equipment::draw(ui, view, &panel);
            } else {
                crate::inventory::draw(ui, view, &panel, &lines);
            }
        } else {
            ui.spawn((
                crate::chrome::ScrollArea,
                crate::inventory::Scroll,
                ScrollPosition {
                    offset_y: panel.scroll,
                    ..default()
                },
                Node {
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    row_gap: Val::Px(6.0),
                    ..default()
                },
            ))
            .with_children(|body| draw_lines(body, &lines));
        }
    });
}

pub(crate) fn draw_lines(ui: &mut ChildSpawnerCommands, lines: &[crate::pack::Line]) {
    let mut position = 0;
    for line in lines {
        match line {
            crate::pack::Line::Heading(text) => heading(ui, text),
            crate::pack::Line::Note(text) => note(ui, text),
            crate::pack::Line::Row {
                label,
                detail,
                actions,
            } => {
                let labelled: Vec<(String, PanelButton)> = actions
                    .iter()
                    .map(|(text, button)| {
                        let key = crate::pack::hotkey(position);
                        position += 1;
                        (
                            key.map_or_else(|| (*text).to_string(), |key| format!("{key} {text}")),
                            *button,
                        )
                    })
                    .collect();
                row(ui, label, detail.as_deref(), &labelled);
            }
        }
    }
}

fn heading(ui: &mut ChildSpawnerCommands, text: &str) {
    ui.spawn((
        Text::new(text),
        TextFont {
            font: crate::chrome::FONT,
            font_size: 17.0,
            ..default()
        },
        TextColor(GOLD),
    ));
}

fn note(ui: &mut ChildSpawnerCommands, text: &str) {
    ui.spawn((
        Text::new(text),
        TextFont {
            font: crate::chrome::FONT,
            font_size: 12.0,
            ..default()
        },
        TextColor(MUTED),
    ));
}

pub(crate) fn row(
    ui: &mut ChildSpawnerCommands,
    label: &str,
    detail: Option<&str>,
    actions: &[(String, PanelButton)],
) {
    ui.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(6.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|line| {
        line.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 1.0,
            ..default()
        })
        .with_children(|names| {
            names.spawn((
                Text::new(label),
                TextFont {
                    font: crate::chrome::FONT,
                    font_size: 15.0,
                    ..default()
                },
                TextColor(INK),
            ));
            if let Some(detail) = detail {
                names.spawn((
                    Text::new(detail),
                    TextFont {
                        font: crate::chrome::FONT,
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(MUTED),
                ));
            }
        });
        line.spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            flex_shrink: 1.0,
            max_width: Val::Percent(100.0),
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|buttons| {
            for (text, action) in actions {
                buttons
                    .spawn((
                        Button,
                        *action,
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                        BorderColor(GOLD),
                        BorderRadius::all(Val::Px(3.0)),
                    ))
                    .with_child((
                        Text::new(text.as_str()),
                        TextFont {
                            font: crate::chrome::FONT,
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(INK),
                    ));
            }
        });
    });
}

pub fn panel_buttons(
    net: Res<Net>,
    mut panel: ResMut<Panel>,
    mut game: ResMut<Game>,
    mut buttons: Query<(&Interaction, &PanelButton, &mut BackgroundColor), Changed<Interaction>>,
) {
    if !panel.open {
        return;
    }
    for (interaction, action, mut background) in &mut buttons {
        match interaction {
            Interaction::Pressed => activate(*action, &net, &mut panel, &mut game),
            Interaction::Hovered => background.0 = HOT,
            Interaction::None => background.0 = BUTTON,
        }
    }
}

/// Carry out a pack action, whether clicked or chosen by its number key.
pub(crate) fn activate(action: PanelButton, net: &Net, panel: &mut Panel, game: &mut Game) {
    let command = match action {
        PanelButton::Take(item) => Cmd::Take(item),
        PanelButton::Drop(item) => Cmd::Drop(item),
        PanelButton::Use(item, operation) => Cmd::Use(item, operation),
        PanelButton::ZapSelf(item) => Cmd::ZapSelf(item),
        PanelButton::Identify(item) => Cmd::Identify(item),
        PanelButton::Escape(hold) => Cmd::Escape(hold),
        PanelButton::ProjectileChoose(index) => {
            Cmd::Projectile(tor_client_3d_model::ProjectileStep::Choose(index))
        }
        PanelButton::ProjectileNext => Cmd::Projectile(tor_client_3d_model::ProjectileStep::Next),
        PanelButton::ProjectilePrevious => {
            Cmd::Projectile(tor_client_3d_model::ProjectileStep::Previous)
        }
        PanelButton::ProjectileCancel => {
            Cmd::Projectile(tor_client_3d_model::ProjectileStep::Cancel)
        }
        PanelButton::Aim(item) => {
            // Close the pack so the map takes the direction.
            game.aiming = Some(crate::Aim::Device(item));
            game.status = "Choose a direction to zap. Esc cancels.".into();
            panel.open = false;
            return;
        }
        PanelButton::Ability(index) => {
            let Some(view) = &game.view else { return };
            let Some(ability) = view.abilities.get(index).cloned() else {
                return;
            };
            let own = view.actors.iter().find(|a| a.is_self).map(|a| a.id);
            match (ability.target, own) {
                (tor_protocol::AbilityTargetKind::DirectionalProjectile, _) => {
                    Cmd::Projectile(tor_client_3d_model::ProjectileStep::Start(ability.id))
                }
                (tor_protocol::AbilityTargetKind::SelfOnly, Some(own)) => {
                    Cmd::Ability(ability.id, tor_client_3d_model::AbilityTarget::Actor(own))
                }
                (target, _) => {
                    game.status = match target {
                        tor_protocol::AbilityTargetKind::SelfStep
                        | tor_protocol::AbilityTargetKind::DirectionalRay => {
                            format!("Choose a direction for {}. Esc cancels.", ability.label)
                        }
                        _ => format!("Click a creature for {}. Esc cancels.", ability.label),
                    };
                    game.aiming = Some(crate::Aim::Ability {
                        id: ability.id,
                        target,
                    });
                    panel.open = false;
                    return;
                }
            }
        }
    };
    send(net, game, command);
}
