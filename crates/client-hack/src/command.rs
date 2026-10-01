//! What a key does to the current observation. Bump, fight, open, close,
//! pickup, and drop read that observation only. They do not read the chart.

use crate::config::BumpAttacks;
use crate::queue::Intent;
use std::collections::BTreeSet;
use tor_client_common::RememberedCell;
use tor_protocol::{
    Action, ActorId, CellView, Direction, GroundItemView, Observation, Position, TravelPhase,
};

pub const REPEAT_CAP: u16 = 100;
pub const RUN_CAP: u16 = 32;

pub const NOTHING_TO_FIGHT: &str = "There is nothing there to fight.";
pub const NO_DOOR: &str = "There is no door in that direction.";
pub const ILLEGAL_TARGET: &str = "That isn't a legal target.";
pub const NOTHING_HERE: &str = "There is nothing here.";
pub const BUFFER_FULL: &str = "Input buffer full.";
pub const UNAVAILABLE: &str = "That isn't available yet.";
pub const CANCEL_TRAVEL_FIRST: &str =
    "Press Esc to cancel travel before selecting another destination.";
pub const BAD_QUANTITY: &str = "That quantity isn't available.";
pub const HELP_LINE: &str = "Move (hjkl yubn and the numpad), . wait, F fight, o/c door, , pickup, d drop, i inventory, :/; look, _ travel, @ autopickup, #adjust. S does not save.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    Attack(ActorId),
    Open(u64),
    Close(u64),
    Move(Direction),
    /// Local message, nothing is sent.
    Miss(&'static str),
    /// A run stops before a closed door and does not send the open.
    Stop,
}

pub fn bounded_repeat(count: u32, run: bool) -> u16 {
    let cap = u32::from(if run { RUN_CAP } else { REPEAT_CAP });
    u16::try_from(count.min(cap)).unwrap_or(REPEAT_CAP)
}

pub fn repeat_intent(
    direction: Direction,
    count: u32,
    run: bool,
    pickup: bool,
    suppress_attack: bool,
) -> Intent {
    Intent::Repeat {
        direction,
        remaining: bounded_repeat(count.max(1), run),
        pickup,
        suppress_attack,
    }
}

pub fn to_action(resolved: &Resolved) -> Option<Action> {
    match resolved {
        Resolved::Attack(target) => Some(Action::Attack { target: *target }),
        Resolved::Open(door) => Some(Action::SetDoor {
            door: *door,
            open: true,
        }),
        Resolved::Close(door) => Some(Action::SetDoor {
            door: *door,
            open: false,
        }),
        Resolved::Move(direction) => Some(Action::Move {
            direction: *direction,
        }),
        Resolved::Miss(_) | Resolved::Stop => None,
    }
}

pub fn bump(
    observation: &Observation,
    direction: Direction,
    attacks: BumpAttacks,
    suppress_attack: bool,
    running: bool,
) -> Resolved {
    if !suppress_attack {
        if let Some(target) = drawn_creature(observation, direction) {
            if attacks_target(observation, target, attacks) {
                return Resolved::Attack(target);
            }
        }
    }
    if let Some(door) = reachable_door(observation, direction, false) {
        if running {
            return Resolved::Stop;
        }
        return Resolved::Open(door);
    }
    Resolved::Move(direction)
}

pub fn fight(observation: &Observation, direction: Direction) -> Resolved {
    match drawn_creature(observation, direction) {
        Some(target) => Resolved::Attack(target),
        None => Resolved::Miss(NOTHING_TO_FIGHT),
    }
}

/// The creature a click would fight: lowest other id in that column, unless the standing plane drew a wall or a closed door.
pub fn creature_at(observation: &Observation, x: i32, y: i32) -> Option<ActorId> {
    let at = Position {
        x,
        y,
        z: observation.position.z,
    };
    if standing_blocks(observation, at) {
        return None;
    }
    observation
        .visible_actors
        .iter()
        .filter(|actor| {
            actor.id != observation.actor && actor.position.x == x && actor.position.y == y
        })
        .map(|actor| actor.id)
        .min()
}

pub fn door_at(observation: &Observation, at: Position, open: bool) -> Option<u64> {
    observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == at)
        .and_then(|cell| cell.door.as_ref())
        .filter(|door| door.reachable && door.open != open)
        .map(|door| door.id)
}

pub fn door_command(observation: &Observation, direction: Direction, open: bool) -> Resolved {
    match reachable_door(observation, direction, !open) {
        Some(door) if open => Resolved::Open(door),
        Some(door) => Resolved::Close(door),
        None => Resolved::Miss(NO_DOOR),
    }
}

/// Lowest other actor id at the destination column, any z.
/// A standing wall or closed door on that step draws first, so the creature is not a target.
pub fn drawn_creature(observation: &Observation, direction: Direction) -> Option<ActorId> {
    let at = stepped(observation, direction);
    if standing_blocks(observation, at) {
        return None;
    }
    observation
        .visible_actors
        .iter()
        .filter(|actor| {
            actor.id != observation.actor && actor.position.x == at.x && actor.position.y == at.y
        })
        .map(|actor| actor.id)
        .min()
}

pub fn other_actors(observation: &Observation) -> BTreeSet<ActorId> {
    observation
        .visible_actors
        .iter()
        .map(|actor| actor.id)
        .filter(|id| *id != observation.actor)
        .collect()
}

pub fn feet_items(observation: &Observation) -> Vec<&GroundItemView> {
    let mut items: Vec<_> = observation
        .ground_items
        .iter()
        .filter(|item| item.reachable && item.position == observation.position)
        .collect();
    items.sort_by_key(|item| item.item.id);
    items
}

pub fn take_quantity(disclosed: u64, count: Option<u64>) -> Result<Option<u64>, &'static str> {
    match count {
        None => Ok(None),
        Some(0) => Err(BAD_QUANTITY),
        Some(quantity) if quantity > disclosed => Err(BAD_QUANTITY),
        Some(quantity) => Ok(Some(quantity)),
    }
}

/// After an accepted step, `remaining` is the count still unsent.
pub fn repeat_interrupted(
    observation: &Observation,
    direction: Direction,
    actors_at_send: &BTreeSet<ActorId>,
) -> bool {
    let arrived = other_actors(observation);
    if !arrived.is_subset(actors_at_send) {
        return true;
    }
    if standing_closed_door(observation, stepped(observation, direction)) {
        return true;
    }
    on_stair(observation)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPickup {
    Skip,
    Take(u64),
    Menu,
}

pub struct AutoQuery<'a> {
    pub session_on: bool,
    pub movement_allows: bool,
    pub has_control: bool,
    pub same_branch: bool,
    pub ready: bool,
    pub travel: Option<TravelPhase>,
    pub actors_at_send: &'a BTreeSet<ActorId>,
    pub observation: &'a Observation,
}

pub fn decide_autopickup(query: AutoQuery<'_>) -> AutoPickup {
    if query.travel == Some(TravelPhase::Active) {
        return AutoPickup::Skip;
    }
    if !query.session_on
        || !query.movement_allows
        || !query.has_control
        || !query.same_branch
        || !query.ready
    {
        return AutoPickup::Skip;
    }
    if !other_actors(query.observation).is_subset(query.actors_at_send) {
        return AutoPickup::Skip;
    }
    match feet_items(query.observation).as_slice() {
        [] => AutoPickup::Skip,
        [only] => AutoPickup::Take(only.item.id),
        _ => AutoPickup::Menu,
    }
}

pub fn pickup_sentence(via_travel: bool, name: &str) -> String {
    if via_travel {
        format!("You walk over to {name} and pick it up.")
    } else {
        format!("You pick up the {name}.")
    }
}

pub fn gone_sentence(name: &str) -> String {
    format!("You walk over to {name}, but it is no longer within reach.")
}

pub fn look_at(observation: &Observation, chart: &[&RememberedCell], x: i32, y: i32) -> String {
    let current = column_current(observation, x, y);
    if current {
        return format!("You see {}.", column_phrase(observation, x, y, false));
    }
    let remembered = chart
        .iter()
        .any(|cell| cell.position.x == x && cell.position.y == y);
    if remembered {
        let phrase = remembered_phrase(chart, x, y);
        return format!("You remember {phrase}.");
    }
    "You have never seen that place.".into()
}

pub fn offset(direction: Direction) -> (i32, i32, i32) {
    match direction {
        Direction::North => (0, -1, 0),
        Direction::South => (0, 1, 0),
        Direction::East => (1, 0, 0),
        Direction::West => (-1, 0, 0),
        Direction::NorthEast => (1, -1, 0),
        Direction::SouthEast => (1, 1, 0),
        Direction::SouthWest => (-1, 1, 0),
        Direction::NorthWest => (-1, -1, 0),
        Direction::Up => (0, 0, 1),
        Direction::Down => (0, 0, -1),
    }
}

fn stepped(observation: &Observation, direction: Direction) -> Position {
    let (dx, dy, dz) = offset(direction);
    Position {
        x: observation.position.x + dx,
        y: observation.position.y + dy,
        z: observation.position.z + dz,
    }
}

fn standing_blocks(observation: &Observation, at: Position) -> bool {
    observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == at)
        .is_some_and(|cell| cell.wall || cell.door.as_ref().is_some_and(|door| !door.open))
}

fn standing_closed_door(observation: &Observation, at: Position) -> bool {
    observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == at)
        .is_some_and(|cell| cell.door.as_ref().is_some_and(|door| !door.open))
}

fn reachable_door(observation: &Observation, direction: Direction, want_open: bool) -> Option<u64> {
    let at = stepped(observation, direction);
    observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == at)
        .and_then(|cell| cell.door.as_ref())
        .filter(|door| door.reachable && door.open == want_open)
        .map(|door| door.id)
}

fn attacks_target(observation: &Observation, target: ActorId, attacks: BumpAttacks) -> bool {
    match attacks {
        BumpAttacks::Off => false,
        BumpAttacks::Any => true,
        BumpAttacks::Hostile => observation.combat.as_ref().is_some_and(|combat| {
            combat
                .actors
                .iter()
                .any(|actor| actor.actor == target && actor.hostile)
        }),
    }
}

fn on_stair(observation: &Observation) -> bool {
    observation
        .visible_cells
        .iter()
        .find(|cell| cell.position == observation.position)
        .is_some_and(|cell| cell.stairs_up || cell.stairs_down)
}

fn column_current(observation: &Observation, x: i32, y: i32) -> bool {
    observation
        .visible_cells
        .iter()
        .any(|cell| cell.position.x == x && cell.position.y == y)
        || observation
            .visible_actors
            .iter()
            .any(|actor| actor.position.x == x && actor.position.y == y)
        || observation
            .ground_items
            .iter()
            .any(|item| item.position.x == x && item.position.y == y)
}

fn column_phrase(observation: &Observation, x: i32, y: i32, remembered_items: bool) -> String {
    let mut parts = Vec::new();
    if (x, y) == (observation.position.x, observation.position.y) && !remembered_items {
        parts.push("yourself".into());
    }
    for actor in observation.visible_actors.iter().filter(|actor| {
        actor.position.x == x && actor.position.y == y && actor.id != observation.actor
    }) {
        let name = if actor.name.is_empty() {
            "a creature"
        } else {
            actor.name.as_str()
        };
        parts.push(name.into());
    }
    for item in observation
        .ground_items
        .iter()
        .filter(|item| item.position.x == x && item.position.y == y)
    {
        let name = item.item.name.as_str();
        if remembered_items {
            parts.push(format!("{name}, which may be gone"));
        } else {
            parts.push(name.into());
        }
    }
    for cell in observation
        .visible_cells
        .iter()
        .filter(|cell| cell.position.x == x && cell.position.y == y)
    {
        push_cell(&mut parts, cell);
    }
    if parts.is_empty() {
        "nothing of note".into()
    } else {
        parts.join(", ")
    }
}

fn remembered_phrase(chart: &[&RememberedCell], x: i32, y: i32) -> String {
    let mut parts = Vec::new();
    for cell in chart
        .iter()
        .filter(|cell| cell.position.x == x && cell.position.y == y)
    {
        for item in &cell.ground_items {
            parts.push(format!("{}, which may be gone", item.item.name));
        }
        if cell.wall {
            parts.push("a wall".into());
        }
        if let Some(door) = &cell.door {
            parts.push(if door.open {
                "an open door".into()
            } else {
                "a closed door".into()
            });
        }
        if cell.stairs_up {
            parts.push("stairs up".into());
        }
        if cell.stairs_down {
            parts.push("stairs down".into());
        }
        if !cell.wall && cell.door.is_none() && !cell.stairs_up && !cell.stairs_down {
            parts.push("floor".into());
        }
    }
    if parts.is_empty() {
        "that place".into()
    } else {
        parts.join(", ")
    }
}

fn push_cell(parts: &mut Vec<String>, cell: &CellView) {
    if cell.wall {
        parts.push("a wall".into());
    }
    if let Some(door) = &cell.door {
        let name = if door.name.is_empty() {
            "door"
        } else {
            door.name.as_str()
        };
        parts.push(if door.open {
            format!("an open {name}")
        } else {
            format!("a closed {name}")
        });
    }
    if cell.stairs_up {
        parts.push("stairs up".into());
    }
    if cell.stairs_down {
        parts.push("stairs down".into());
    }
    if !cell.wall && cell.door.is_none() && !cell.stairs_up && !cell.stairs_down && parts.is_empty()
    {
        parts.push("the floor".into());
    }
}

pub fn adjacent(from: Position, to: Position) -> bool {
    (from.x - to.x).abs() <= 1 && (from.y - to.y).abs() <= 1 && (from.z - to.z).abs() <= 1
}
