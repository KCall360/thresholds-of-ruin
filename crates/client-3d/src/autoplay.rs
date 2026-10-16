//! `--demo-interact`: a scripted run through every object interaction, for
//! checking them against a real server. Steps that don't apply where the
//! character is are skipped and said so.
use crate::{
    interact::{Panel, Pursuit},
    net::Cmd,
    Game, Net,
};
use bevy::prelude::*;
use tor_client_common::items::ItemOperation;
use tor_protocol::Position;

#[derive(Clone, Copy, Debug)]
enum Step {
    Panel(bool),
    Drop(&'static str),
    TakeAny,
    Use(&'static str, ItemOperation),
    TravelToItem,
    TravelToDoor,
    ToggleDoor,
    TravelFar,
}

const SCRIPT: [Step; 15] = [
    Step::Panel(true),
    Step::Drop("bandage"),
    Step::TakeAny,
    Step::Use("mace", ItemOperation::Unequip),
    Step::Drop("mace"),
    Step::TakeAny,
    Step::Use("mace", ItemOperation::Equip),
    Step::Use("ration", ItemOperation::Eat),
    Step::TravelToItem,
    Step::TakeAny,
    Step::TravelToDoor,
    Step::ToggleDoor,
    Step::ToggleDoor,
    Step::Panel(false),
    Step::TravelFar,
];

#[derive(Resource, Default)]
pub struct Autoplay {
    enabled: bool,
    hunt: bool,
    /// Refusals in a row, so the hunt tries somewhere else.
    stuck: u32,
    /// Places already explored toward, so the hunt keeps pushing outward.
    tried: std::collections::HashSet<String>,
    /// Items already gone after, so a refused pick-up isn't retried forever.
    fetched: std::collections::HashSet<tor_protocol::ItemTarget>,
    next: usize,
    ready_at: f32,
}

impl Autoplay {
    pub fn new(enabled: bool, hunt: bool) -> Self {
        Self {
            enabled,
            hunt,
            ..default()
        }
    }
}

/// `--demo-hunt`: seek out creatures. Attack a hostile beside you, close on
/// one in sight, or else travel to the edge of what you have seen.
pub fn hunt(time: Res<Time>, net: Res<Net>, mut game: ResMut<Game>, mut auto: ResMut<Autoplay>, mut pursuit: ResMut<Pursuit>) {
    let now = time.elapsed_secs();
    if !auto.hunt || game.busy || game.fatal || pursuit.active() || now < auto.ready_at.max(3.0) {
        return;
    }
    let Some(view) = &game.view else {
        return;
    };
    auto.ready_at = now + 0.15;
    // Let a journey or other work under way finish first.
    if !view.can_act && view.events.is_empty() {
        let in_combat = view.actors.iter().any(|a| a.hostile);
        if !in_combat {
            return;
        }
    }
    let feet = view.feet;
    let gap = |p: Position| (p.x - feet.x).abs().max((p.y - feet.y).abs());
    let toward = |p: Position| {
        let (dx, dy) = ((p.x - feet.x).signum(), (p.y - feet.y).signum());
        crate::interact::direction(dx, dy)
    };
    let foe = view
        .actors
        .iter()
        .filter(|a| a.hostile && !a.is_self)
        .min_by_key(|a| gap(a.position));
    if game.status.is_empty() {
        auto.stuck = 0;
    } else {
        auto.stuck += 1;
    }
    let stuck = auto.stuck;
    let stairs = view
        .visible
        .iter()
        .chain(&view.memory)
        .find(|c| c.stairs_down && c.position.z == feet.z)
        .cloned();
    // Close in and fight, or go and pick things up, as a click would.
    if let Some(foe) = foe {
        if gap(foe.position) > 1 {
            pursuit.chase(foe.id);
            return;
        }
    }
    let loot = view
        .items
        .iter()
        .filter(|i| i.class != tor_protocol::ItemClass::Corpse && !auto.fetched.contains(&i.id))
        .min_by_key(|i| gap(i.position))
        .map(|i| i.id);
    if let (None, Some(item)) = (foe, loot) {
        auto.fetched.insert(item);
        pursuit.fetch(item);
        return;
    }
    let command = if let Some(foe) = foe {
        toward(foe.position).map(Cmd::Move)
    } else if stuck > 3 {
        // Waiting lets a pending action finish; then try somewhere else.
        Some(Cmd::Wait)
    } else if let Some(stairs) = stairs {
        // Deeper levels hold more creatures.
        Some(if stairs.position == feet {
            Cmd::Move(tor_protocol::Direction::Down)
        } else {
            Cmd::Travel(stairs.key.clone())
        })
    } else {
        let known: std::collections::HashSet<(i32, i32)> = view
            .visible
            .iter()
            .chain(&view.memory)
            .filter(|c| c.position.z == feet.z)
            .map(|c| (c.position.x, c.position.y))
            .collect();
        let open = || {
            view.visible
                .iter()
                .chain(&view.memory)
                .filter(|c| !c.wall && c.door.is_none() && c.position.z == feet.z && gap(c.position) > 1)
        };
        // The nearest edge of the known map not yet tried; once every edge
        // has been tried, start over from the farthest known floor.
        let frontier = open()
            .filter(|c| {
                (-1..=1).any(|dx| (-1..=1).any(|dy| !known.contains(&(c.position.x + dx, c.position.y + dy))))
            })
            .filter(|c| !auto.tried.contains(&c.key))
            .min_by_key(|c| gap(c.position) * 100 + ((crate::props::hash(&c.key) ^ u64::from(stuck) * 7919) % 97) as i32)
            .map(|c| c.key.clone());
        match frontier {
            Some(key) => Some(Cmd::Travel(key)),
            None => {
                auto.tried.clear();
                open().max_by_key(|c| gap(c.position)).map(|c| Cmd::Travel(c.key.clone()))
            }
        }
    };
    if let Some(command) = command {
        if let Cmd::Travel(key) = &command {
            auto.tried.insert(key.clone());
        }
        if std::env::var_os("TOR_3D_DEBUG").is_some() {
            eprintln!("hunt: {}", match &command {
                Cmd::Move(d) => format!("move {d:?}"),
                Cmd::Travel(_) => "travel".into(),
                Cmd::Wait => "wait".into(),
                _ => "other".into(),
            });
        }
        if net.0.lock().unwrap().commands.send(command).is_ok() {
            game.busy = true;
            game.status.clear();
        }
    }
}

pub fn run(
    time: Res<Time>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut panel: ResMut<Panel>,
    mut auto: ResMut<Autoplay>,
) {
    let now = time.elapsed_secs();
    if !auto.enabled || game.busy || game.fatal || now < auto.ready_at.max(3.0) || auto.next >= SCRIPT.len() {
        return;
    }
    let Some(view) = &game.view else {
        return;
    };
    let step = SCRIPT[auto.next];
    auto.next += 1;
    auto.ready_at = now + 2.0;
    let feet = view.feet;
    let carried = |word: &str| view.inventory.iter().find(|i| i.name.contains(word)).map(|i| i.id);
    let open_cell_near = |target: Position| {
        view.visible
            .iter()
            .filter(|c| !c.wall && c.door.is_none() && c.position.z == feet.z)
            .filter(|c| (c.position.x - target.x).abs().max((c.position.y - target.y).abs()) == 1)
            .min_by_key(|c| (c.position.x - feet.x).abs() + (c.position.y - feet.y).abs())
            .map(|c| c.key.clone())
    };
    let command = match step {
        Step::Panel(open) => {
            panel.open = open;
            None
        }
        Step::Drop(word) => carried(word).map(Cmd::Drop),
        Step::TakeAny => view.items.iter().find(|i| i.reachable).map(|i| Cmd::Take(i.id)),
        Step::Use(word, operation) => carried(word).map(|item| Cmd::Use(item, operation)),
        Step::TravelToItem => view
            .items
            .iter()
            .find(|i| !i.reachable)
            .and_then(|i| view.visible.iter().find(|c| c.position == i.position))
            .map(|c| Cmd::Travel(c.key.clone())),
        Step::TravelToDoor => view
            .visible
            .iter()
            .find(|c| c.door.is_some() && c.position.z == feet.z)
            .and_then(|c| open_cell_near(c.position))
            .map(Cmd::Travel),
        Step::ToggleDoor => view.visible.iter().find_map(|c| {
            let near = (c.position.x - feet.x).abs().max((c.position.y - feet.y).abs()) == 1;
            c.door.as_ref().filter(|_| near).map(|d| Cmd::Door(d.id, !d.open))
        }),
        Step::TravelFar => view
            .visible
            .iter()
            .filter(|c| !c.wall && c.position.z == feet.z)
            .max_by_key(|c| (c.position.x - feet.x).abs() + (c.position.y - feet.y).abs())
            .map(|c| Cmd::Travel(c.key.clone())),
    };
    match command {
        Some(command) => {
            eprintln!("autoplay: {step:?}");
            if net.0.lock().unwrap().commands.send(command).is_ok() {
                game.busy = true;
                game.status.clear();
            }
        }
        None if matches!(step, Step::Panel(_)) => eprintln!("autoplay: {step:?}"),
        None => eprintln!("autoplay: {step:?} skipped: nothing applies here"),
    }
}
