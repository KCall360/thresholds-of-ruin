//! Interpret disclosed cells and interaction targets without a renderer.
use crate::{Actor, Cmd, GroundItem, View};
use tor_protocol::{ActorTarget, Direction, DoorTarget, ItemTarget, Position};

/// What a click on the hovered cell would do.
#[derive(Clone, Debug)]
pub enum Intent {
    Attack(ActorTarget),
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
    pub fn command(&self) -> Option<Cmd> {
        Some(match self.clone() {
            Intent::Attack(target) => Cmd::Attack(target),
            Intent::Climb(direction) => Cmd::Move(direction),
            Intent::Take(item) => Cmd::Take(item),
            Intent::Door(door, open) => Cmd::Door(door, open),
            Intent::Travel(key) => Cmd::Travel(key),
            Intent::Fetch(_) | Intent::Charge(_) => return None,
        })
    }

    pub fn hint(&self) -> &'static str {
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
}

pub fn direction(dx: i32, dy: i32) -> Option<Direction> {
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

pub fn known_cells(view: &View) -> impl Iterator<Item = crate::Terrain<'_>> {
    view.terrain()
}

/// A proximity hint from disclosed body cells; the server decides actual reach.
pub fn near_actor(view: &View, target: ActorTarget) -> bool {
    let near = |a: Position, b: Position| {
        [(a.x, b.x), (a.y, b.y), (a.z, b.z)]
            .into_iter()
            .all(|(a, b)| (i64::from(a) - i64::from(b)).abs() <= 1)
    };
    view.actors
        .iter()
        .filter(|actor| actor.id == target)
        .any(|actor| {
            near(view.feet, actor.position)
                || view
                    .actors
                    .iter()
                    .filter(|own| own.is_self)
                    .any(|own| near(own.position, actor.position))
        })
}

/// Read one exact disclosed cell in the observer frame, including elevation.
pub fn describe(view: &View, at: Position) -> (String, String, Option<Intent>) {
    let feet = view.feet;
    let here = at == feet;
    let creature = view.actors.iter().find(|a| !a.is_self && a.position == at);
    let items: Vec<_> = view.items.iter().filter(|i| i.position == at).collect();
    let door = view
        .visible
        .iter()
        .filter(|c| c.position == at)
        .find_map(|c| c.door.as_ref())
        .map(|door| {
            let reachable = view
                .visible
                .iter()
                .filter_map(|cell| cell.door.as_ref())
                .any(|current| current.id == door.id && current.reachable);
            (
                door.name.as_str(),
                door.description.as_str(),
                door.open,
                reachable.then_some(Intent::Door(door.id, !door.open)),
            )
        })
        .or_else(|| {
            view.memory
                .iter()
                .filter(|c| c.position == at)
                .find_map(|c| c.door.as_ref())
                .map(|door| {
                    (
                        door.name.as_str(),
                        door.description.as_str(),
                        door.open,
                        None,
                    )
                })
        });
    let floor = known_cells(view).find(|c| c.position == at);
    if let Some(creature) = creature {
        return describe_actor(view, creature);
    }
    if !items.is_empty() {
        return describe_items(&items);
    }
    if let Some((name, description, open, intent)) = door {
        let state = if open { "open" } else { "closed" };
        let name = if name.is_empty() {
            "Door".into()
        } else {
            capitalise(name)
        };
        return (format!("{name} ({state})"), description.to_owned(), intent);
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
                Some(Intent::Travel(cell.key.to_owned()))
            };
            (name.into(), String::new(), intent)
        }
        Some(cell) if !cell.wall && !here => (
            "Cave floor".into(),
            String::new(),
            Some(Intent::Travel(cell.key.to_owned())),
        ),
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

/// A selected model keeps its disclosed target even when several share a cell.
#[derive(Clone, Copy, Debug)]
pub enum Focus {
    Cell(Position),
    Actor(ActorTarget),
    Item(ItemTarget),
}

impl Focus {
    pub fn position(self, view: &View) -> Option<Position> {
        match self {
            Self::Cell(at) => known_cells(view).any(|c| c.position == at).then_some(at),
            Self::Actor(id) => view
                .actors
                .iter()
                .find(|a| a.id == id && !a.is_self)
                .map(|a| a.position),
            Self::Item(id) => view.items.iter().find(|i| i.id == id).map(|i| i.position),
        }
    }
}

pub fn describe_focus(view: &View, focus: Focus) -> (String, String, Option<Intent>) {
    match focus {
        Focus::Cell(at) => describe(view, at),
        Focus::Actor(id) => view
            .actors
            .iter()
            .find(|a| a.id == id && !a.is_self)
            .map_or_else(
                || (String::new(), String::new(), None),
                |a| describe_actor(view, a),
            ),
        Focus::Item(id) => view.items.iter().find(|i| i.id == id).map_or_else(
            || (String::new(), String::new(), None),
            |i| describe_items(&[i]),
        ),
    }
}

fn describe_actor(view: &View, creature: &Actor) -> (String, String, Option<Intent>) {
    let intent = if creature.hostile {
        Some(if near_actor(view, creature.id) {
            Intent::Attack(creature.id)
        } else {
            Intent::Charge(creature.id)
        })
    } else {
        None
    };
    let name = if creature.name.is_empty() {
        "A figure".into()
    } else {
        capitalise(&creature.name)
    };
    (name, creature.description.clone(), intent)
}

fn describe_items(items: &[&GroundItem]) -> (String, String, Option<Intent>) {
    let names: Vec<String> = items
        .iter()
        .map(|i| {
            if i.quantity > 1 {
                format!("{} x{}", i.name, i.quantity)
            } else {
                i.name.clone()
            }
        })
        .collect();
    let intent = items
        .iter()
        .find(|i| i.reachable)
        .map_or(Intent::Fetch(items[0].id), |i| Intent::Take(i.id));
    let intent = Some(intent);
    (
        capitalise(&names.join(", ")),
        items[0].description.clone(),
        intent,
    )
}
