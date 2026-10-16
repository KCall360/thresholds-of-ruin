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
use tor_client_common::items::ItemOperation;
use std::collections::{HashMap, HashSet, VecDeque};
use tor_protocol::{ActorTarget, CellView, Direction, DoorTarget, ItemTarget, Position};

/// What a click on the hovered cell would do.
#[derive(Clone, Debug)]
pub enum Intent {
    Attack(Direction),
    Take(ItemTarget),
    /// Walk over to an item out of reach, then pick it up.
    Fetch(ItemTarget),
    /// Close in on a hostile creature, then attack it.
    Charge(ActorTarget),
    Door(DoorTarget, bool),
    Climb(Direction),
    Travel(String),
}

impl Intent {
    /// The command a click sends at once; fetching and charging take several.
    fn command(&self) -> Option<Cmd> {
        Some(match self.clone() {
            Intent::Attack(direction) | Intent::Climb(direction) => Cmd::Move(direction),
            Intent::Take(item) => Cmd::Take(item),
            Intent::Door(door, open) => Cmd::Door(door, open),
            Intent::Travel(key) => Cmd::Travel(key),
            Intent::Fetch(_) | Intent::Charge(_) => return None,
        })
    }

    fn hint(&self) -> &'static str {
        match self {
            Intent::Attack(_) => "Click to attack",
            Intent::Take(_) => "Click to pick up",
            Intent::Fetch(_) => "Click to go and pick up",
            Intent::Charge(_) => "Click to close in and attack",
            Intent::Door(_, true) => "Click to open",
            Intent::Door(_, false) => "Click to close",
            Intent::Climb(Direction::Up) => "Click to climb",
            Intent::Climb(_) => "Click to descend",
            Intent::Travel(_) => "Click to travel here",
        }
    }

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
    cell: Option<IVec3>,
    title: String,
    description: String,
    intent: Option<Intent>,
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
        *self = Self { goal: Some(Goal::Attack(actor)), ..default() };
    }

    pub(crate) fn fetch(&mut self, item: ItemTarget) {
        *self = Self { goal: Some(Goal::Take(item)), ..default() };
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
    built_for: u64,
}

#[derive(Component)]
pub(crate) struct Selection(Accent);

#[derive(Component)]
pub(crate) struct Tooltip;

#[derive(Component)]
pub(crate) struct TooltipTitle;

#[derive(Component)]
pub(crate) struct TooltipHint;

#[derive(Component)]
pub(crate) struct TooltipBody;

#[derive(Component)]
pub(crate) struct PanelRoot;

#[derive(Component, Clone, Copy)]
pub(crate) enum PanelButton {
    Take(ItemTarget),
    Drop(ItemTarget),
    Use(ItemTarget, ItemOperation),
}

const PANEL_WIDTH: f32 = 380.0;
const INK: Color = Color::srgb(0.9, 0.88, 0.8);
const MUTED: Color = Color::srgb(0.55, 0.6, 0.62);
const TEAL: Color = Color::srgb(0.45, 0.95, 0.85);
const SLATE: Color = Color::srgba(0.03, 0.035, 0.05, 0.9);
const EDGE: Color = Color::srgba(0.3, 0.75, 0.68, 0.55);
const BUTTON: Color = Color::srgb(0.1, 0.13, 0.16);
const BUTTON_HOT: Color = Color::srgb(0.16, 0.3, 0.3);

pub fn setup(mut commands: Commands, mut library: ResMut<PropLibrary>, mut meshes: ResMut<Assets<Mesh>>) {
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
            BackgroundColor(SLATE),
            BorderColor(EDGE),
            BorderRadius::all(Val::Px(4.0)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|tip| {
            tip.spawn((TooltipTitle, Text::new(""), TextFont { font_size: 16.0, ..default() }, TextColor(INK)));
            tip.spawn((TooltipHint, Text::new(""), TextFont { font_size: 13.0, ..default() }, TextColor(TEAL)));
            tip.spawn((TooltipBody, Text::new(""), TextFont { font_size: 13.0, ..default() }, TextColor(MUTED)));
        });
    commands.spawn((
        PanelRoot,
        Interaction::default(),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(16.0),
            width: Val::Px(PANEL_WIDTH),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(12.0)),
            border: UiRect::all(Val::Px(1.0)),
            row_gap: Val::Px(6.0),
            ..default()
        },
        BackgroundColor(SLATE),
        BorderColor(EDGE),
        BorderRadius::all(Val::Px(6.0)),
        Visibility::Hidden,
    ));
}

pub(crate) fn direction(dx: i32, dy: i32) -> Option<Direction> {
    Some(match (dx, dy) {
        (0, -1) => Direction::North,
        (1, -1) => Direction::NorthEast,
        (1, 0) => Direction::East,
        (1, 1) => Direction::SouthEast,
        (0, 1) => Direction::South,
        (-1, 1) => Direction::SouthWest,
        (-1, 0) => Direction::West,
        (-1, -1) => Direction::NorthWest,
        _ => return None,
    })
}

fn known_cells(view: &View) -> impl Iterator<Item = &CellView> {
    view.visible.iter().chain(&view.memory)
}

/// Read the cell at a position of the observer frame, at the player's level.
fn describe(view: &View, at: Position) -> (String, String, Option<Intent>) {
    let feet = view.feet;
    let (dx, dy) = (at.x - feet.x, at.y - feet.y);
    let adjacent = direction(dx, dy);
    let here = dx == 0 && dy == 0;
    let column = |z: std::ops::RangeInclusive<i32>| {
        move |p: &Position| p.x == at.x && p.y == at.y && z.contains(&(p.z - feet.z))
    };
    let creature = view
        .actors
        .iter()
        .find(|a| !a.is_self && column(-1..=1)(&a.position));
    let items: Vec<_> = view.items.iter().filter(|i| column(-2..=0)(&i.position)).collect();
    let door = known_cells(view)
        .filter(|c| column(0..=1)(&c.position))
        .find_map(|c| c.door.clone());
    let floor = known_cells(view).find(|c| c.position == at);
    if let Some(creature) = creature {
        let intent = if creature.hostile {
            Some(adjacent.map_or(Intent::Charge(creature.id), Intent::Attack))
        } else {
            None
        };
        let name = if creature.name.is_empty() { "A figure".into() } else { capitalise(&creature.name) };
        return (name, creature.description.clone(), intent);
    }
    if !items.is_empty() {
        let names: Vec<String> = items
            .iter()
            .map(|i| if i.quantity > 1 { format!("{} x{}", i.name, i.quantity) } else { i.name.clone() })
            .collect();
        let intent = items
            .iter()
            .find(|i| i.reachable)
            .map_or(Intent::Fetch(items[0].id), |i| Intent::Take(i.id));
        let intent = Some(intent);
        return (capitalise(&names.join(", ")), items[0].description.clone(), intent);
    }
    if let Some(door) = door {
        let intent = adjacent.map(|_| Intent::Door(door.id, !door.open));
        let state = if door.open { "open" } else { "closed" };
        let name = if door.name.is_empty() { "Door".into() } else { capitalise(&door.name) };
        return (format!("{name} ({state})"), door.description, intent);
    }
    match floor {
        Some(cell) if cell.stairs_up || cell.stairs_down => {
            let (name, direction) = if cell.stairs_up {
                ("Stairs leading up", Direction::Up)
            } else {
                ("Stairs leading down", Direction::Down)
            };
            let intent = if here {
                Some(Intent::Climb(direction))
            } else {
                Some(Intent::Travel(cell.key.clone()))
            };
            (name.into(), String::new(), intent)
        }
        Some(cell) if !cell.wall && !here => ("Cave floor".into(), String::new(), Some(Intent::Travel(cell.key.clone()))),
        Some(cell) if !cell.wall => ("You".into(), String::new(), None),
        Some(_) => ("Rock".into(), String::new(), None),
        None => (String::new(), String::new(), None),
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Find the floor cell under the mouse and read what is there.
pub fn hover(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform)>,
    panel_root: Query<&Interaction, With<PanelRoot>>,
    buttons: Query<&Interaction, With<Button>>,
    game: Res<Game>,
    mut hover: ResMut<Hover>,
) {
    *hover = Hover::default();
    hover.over_ui = panel_root.iter().chain(&buttons).any(|i| *i != Interaction::None);
    let (Ok(window), Ok((camera, eye)), Some(view)) = (window.single(), camera.single(), &game.view) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(eye, cursor) else {
        return;
    };
    let floor = stand(game.feet_world).y;
    let Some(distance) = ray.intersect_plane(Vec3::Y * floor, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };
    let point = ray.get_point(distance);
    let cell = IVec3::new(point.x.round() as i32, point.z.round() as i32, game.feet_world.z);
    let local = cell - game.offset;
    let (title, description, intent) = describe(view, Position { x: local.x, y: local.y, z: local.z });
    if title.is_empty() {
        return;
    }
    hover.cell = Some(cell);
    hover.title = title;
    hover.description = description;
    hover.intent = intent;
}

pub fn show_hover(
    time: Res<Time>,
    hover: Res<Hover>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut marks: Query<(&Selection, &mut Transform, &mut Visibility)>,
    mut tooltip: Query<(&mut Node, &mut Visibility), (With<Tooltip>, Without<Selection>)>,
    mut title: Query<&mut Text, (With<TooltipTitle>, Without<TooltipHint>, Without<TooltipBody>)>,
    mut hint: Query<&mut Text, (With<TooltipHint>, Without<TooltipTitle>, Without<TooltipBody>)>,
    mut body: Query<&mut Text, (With<TooltipBody>, Without<TooltipTitle>, Without<TooltipHint>)>,
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
            let left = if cursor.x + 350.0 > size.x { cursor.x - 340.0 } else { cursor.x + 18.0 };
            let top = if cursor.y + 120.0 > size.y { cursor.y - 100.0 } else { cursor.y + 14.0 };
            node.left = Val::Px(left.max(4.0));
            node.top = Val::Px(top.max(4.0));
            *visibility = Visibility::Visible;
            if let Ok(mut text) = title.single_mut() {
                text.0.clone_from(&hover.title);
            }
            if let Ok(mut text) = hint.single_mut() {
                text.0 = hover.intent.as_ref().map_or("", Intent::hint).into();
            }
            if let Ok(mut text) = body.single_mut() {
                text.0.clone_from(&hover.description);
            }
        }
        _ => *visibility = Visibility::Hidden,
    }
}

fn send(net: &Net, game: &mut Game, command: Cmd) {
    if game.busy || game.fatal {
        return;
    }
    if net.0.lock().unwrap().commands.send(command).is_ok() {
        game.busy = true;
        game.since_send = 0.0;
        game.status.clear();
    }
}

pub fn click(
    mouse: Res<ButtonInput<MouseButton>>,
    hover: Res<Hover>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut pursuit: ResMut<Pursuit>,
) {
    if !mouse.just_pressed(MouseButton::Left) || hover.over_ui {
        return;
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
pub fn pursue(keys: Res<ButtonInput<KeyCode>>, net: Res<Net>, mut game: ResMut<Game>, mut pursuit: ResMut<Pursuit>) {
    let Some(goal) = pursuit.goal else {
        return;
    };
    if keys.get_just_pressed().next().is_some() {
        *pursuit = Pursuit::default();
        return;
    }
    // Let the state settle after each step: a request made from a stale
    // revision is refused.
    let fresh = (pursuit.sent_at != Some(game.view_serial) && game.since_send > 0.2) || game.since_send > 1.5;
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
                let key = known_cells(view).find(|c| c.position == item.position).map(|c| c.key.clone());
                let goals = HashSet::from([(item.position.x, item.position.y)]);
                journey(&mut pursuit, view, key, &goals)
            }
        },
        Goal::Attack(id) => match view.actors.iter().find(|a| a.id == id) {
            None => None,
            Some(foe) => {
                let (dx, dy) = (foe.position.x - feet.x, foe.position.y - feet.y);
                match direction(dx, dy).filter(|_| (foe.position.z - feet.z).abs() <= 1) {
                    Some(direction) => {
                        pursuit.finishing = true;
                        Some(Cmd::Move(direction))
                    }
                    None => {
                        let goals: HashSet<(i32, i32)> = (-1..=1)
                            .flat_map(|x| (-1..=1).map(move |y| (x, y)))
                            .filter(|&o| o != (0, 0))
                            .map(|(x, y)| (foe.position.x + x, foe.position.y + y))
                            .collect();
                        // Travel to the open cell beside it nearest to us.
                        let key = known_cells(view)
                            .filter(|c| {
                                c.position.z == feet.z
                                    && !c.wall
                                    && goals.contains(&(c.position.x, c.position.y))
                                    && !view.actors.iter().any(|a| a.position == c.position)
                            })
                            .min_by_key(|c| (c.position.x - feet.x).abs().max((c.position.y - feet.y).abs()))
                            .map(|c| c.key.clone());
                        journey(&mut pursuit, view, key, &goals)
                    }
                }
            }
        },
    };
    pursuit.steps += 1;
    if std::env::var_os("TOR_3D_DEBUG").is_some() {
        eprintln!("pursue: step {} {}", pursuit.steps, match &command {
            Some(Cmd::Move(d)) => format!("move {d:?}"),
            Some(Cmd::Take(_)) => "take".into(),
            Some(Cmd::Travel(_)) => "travel".into(),
            Some(_) => "other".into(),
            None => "none".into(),
        });
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
fn journey(pursuit: &mut Pursuit, view: &View, key: Option<String>, goals: &HashSet<(i32, i32)>) -> Option<Cmd> {
    match key {
        Some(key) if pursuit.travels < 4 && !pursuit.travel_refused => {
            pursuit.travels += 1;
            Some(Cmd::Travel(key))
        }
        _ => step_toward(view, goals).map(Cmd::Move),
    }
}

/// The first step along the shortest known way to any of `goals`, over open
/// floor on the character's level, around closed doors and other creatures.
/// Diagonal steps never cut a corner.
fn step_toward(view: &View, goals: &HashSet<(i32, i32)>) -> Option<Direction> {
    let feet = view.feet;
    let start = (feet.x, feet.y);
    let creatures: HashSet<(i32, i32)> = view
        .actors
        .iter()
        .filter(|a| !a.is_self && (a.position.z - feet.z).abs() <= 1)
        .map(|a| (a.position.x, a.position.y))
        .collect();
    let open: HashSet<(i32, i32)> = known_cells(view)
        .filter(|c| c.position.z == feet.z && !c.wall && c.door.as_ref().is_none_or(|d| d.open))
        .map(|c| (c.position.x, c.position.y))
        .filter(|cell| !creatures.contains(cell) || goals.contains(cell))
        .collect();
    let mut came_from: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    let mut queue = VecDeque::from([start]);
    came_from.insert(start, start);
    while let Some(at) = queue.pop_front() {
        if goals.contains(&at) && at != start {
            let mut step = at;
            while came_from[&step] != start {
                step = came_from[&step];
            }
            return direction(step.0 - start.0, step.1 - start.1);
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                let next = (at.0 + dx, at.1 + dy);
                if (dx, dy) == (0, 0) || came_from.contains_key(&next) || !open.contains(&next) {
                    continue;
                }
                if dx != 0 && dy != 0 && !(open.contains(&(at.0 + dx, at.1)) && open.contains(&(at.0, at.1 + dy))) {
                    continue;
                }
                came_from.insert(next, at);
                queue.push_back(next);
            }
        }
    }
    None
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
    if keys.just_pressed(KeyCode::Escape) {
        if panel.open {
            panel.open = false;
        } else {
            exit.write(AppExit::Success);
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyI) {
        panel.open = !panel.open;
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
        let hovered = hover.intent.clone().filter(|i| matches!(i, Intent::Door(..)));
        let feet = view.feet;
        let adjacent = || {
            known_cells(view).find_map(|c| {
                let (dx, dy) = (c.position.x - feet.x, c.position.y - feet.y);
                let near = direction(dx, dy).is_some() && (0..=1).contains(&(c.position.z - feet.z));
                c.door.as_ref().filter(|_| near).map(|d| Intent::Door(d.id, !d.open))
            })
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
) {
    let Ok((root, mut visibility)) = root.single_mut() else {
        return;
    };
    *visibility = if panel.open { Visibility::Visible } else { Visibility::Hidden };
    if !panel.open || panel.built_for == game.view_serial {
        return;
    }
    panel.built_for = game.view_serial;
    commands.entity(root).despawn_related::<Children>();
    let Some(view) = &game.view else {
        return;
    };
    commands.entity(root).with_children(|ui| {
        heading(ui, "Pack");
        if view.inventory.is_empty() {
            note(ui, "You carry nothing.");
        }
        for item in &view.inventory {
            let mut label = item.name.clone();
            if item.quantity > 1 {
                label.push_str(&format!(" x{}", item.quantity));
            }
            let mut actions = Vec::new();
            if item.equipped.is_some() {
                actions.push(("Remove", PanelButton::Use(item.id, ItemOperation::Unequip)));
            } else if item.can_equip {
                actions.push(("Equip", PanelButton::Use(item.id, ItemOperation::Equip)));
            }
            if item.edible {
                actions.push(("Eat", PanelButton::Use(item.id, ItemOperation::Eat)));
            }
            if item.drinkable {
                actions.push(("Drink", PanelButton::Use(item.id, ItemOperation::Drink)));
            }
            if item.equipped.is_none() {
                actions.push(("Drop", PanelButton::Drop(item.id)));
            }
            row(ui, &label, item.equipped.as_deref(), &actions);
        }
        let reachable: Vec<_> = view.items.iter().filter(|i| i.reachable).collect();
        if !reachable.is_empty() {
            heading(ui, "Within reach");
            for item in reachable {
                let label = if item.quantity > 1 {
                    format!("{} x{}", item.name, item.quantity)
                } else {
                    item.name.clone()
                };
                row(ui, &label, None, &[("Take", PanelButton::Take(item.id))]);
            }
        }
        note(ui, "I or Esc: close");
    });
}

fn heading(ui: &mut ChildSpawnerCommands, text: &str) {
    ui.spawn((Text::new(text), TextFont { font_size: 17.0, ..default() }, TextColor(TEAL)));
}

fn note(ui: &mut ChildSpawnerCommands, text: &str) {
    ui.spawn((Text::new(text), TextFont { font_size: 12.0, ..default() }, TextColor(MUTED)));
}

fn row(ui: &mut ChildSpawnerCommands, label: &str, detail: Option<&str>, actions: &[(&str, PanelButton)]) {
    ui.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(6.0),
        ..default()
    })
    .with_children(|line| {
        line.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 1.0,
            ..default()
        })
        .with_children(|names| {
            names.spawn((Text::new(label), TextFont { font_size: 15.0, ..default() }, TextColor(INK)));
            if let Some(detail) = detail {
                names.spawn((Text::new(detail), TextFont { font_size: 12.0, ..default() }, TextColor(MUTED)));
            }
        });
        line.spawn(Node {
            flex_direction: FlexDirection::Row,
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
                        BorderColor(EDGE),
                        BorderRadius::all(Val::Px(3.0)),
                    ))
                    .with_child((Text::new(*text), TextFont { font_size: 13.0, ..default() }, TextColor(INK)));
            }
        });
    });
}

pub fn panel_buttons(
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut buttons: Query<(&Interaction, &PanelButton, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, action, mut background) in &mut buttons {
        match interaction {
            Interaction::Pressed => {
                let command = match *action {
                    PanelButton::Take(item) => Cmd::Take(item),
                    PanelButton::Drop(item) => Cmd::Drop(item),
                    PanelButton::Use(item, operation) => Cmd::Use(item, operation),
                };
                send(&net, &mut game, command);
            }
            Interaction::Hovered => background.0 = BUTTON_HOT,
            Interaction::None => background.0 = BUTTON,
        }
    }
}
