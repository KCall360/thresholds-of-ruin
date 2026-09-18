use tor_protocol as p;
use tor_simulation as s;
use tor_world as w;

pub fn location(position: crate::journal::Position) -> w::Location {
    w::Location {
        region: w::RegionId(position.region),
        position: w::Position {
            x: position.x,
            y: position.y,
            z: position.z,
        },
    }
}

pub fn position(location: w::Location) -> crate::journal::Position {
    crate::journal::Position {
        region: location.region.0,
        x: location.position.x,
        y: location.position.y,
        z: location.position.z,
    }
}

pub fn direction(direction: p::Direction) -> w::Direction {
    match direction {
        p::Direction::North => w::Direction::North,
        p::Direction::East => w::Direction::East,
        p::Direction::South => w::Direction::South,
        p::Direction::West => w::Direction::West,
        p::Direction::Up => w::Direction::Up,
        p::Direction::Down => w::Direction::Down,
    }
}

pub fn action(action: &p::Action) -> s::Action {
    match action {
        p::Action::SetDoor { door, open } => s::Action::SetDoor {
            door: *door,
            open: *open,
        },
        p::Action::Move { direction: value } => s::Action::Move(direction(*value)),
        p::Action::Take { item } => s::Action::Take(s::ItemId(*item)),
        p::Action::Wait => s::Action::Wait,
    }
}

pub fn event(kind: s::OutcomeKind) -> crate::journal::Event {
    match kind {
        s::OutcomeKind::DoorChanged { door, open } => {
            crate::journal::Event::DoorChanged { door, open }
        }
        s::OutcomeKind::Moved { from, to } => crate::journal::Event::Moved {
            from: position(from),
            to: position(to),
        },
        s::OutcomeKind::Taken { item } => crate::journal::Event::Taken { item: item.0 },
        s::OutcomeKind::Waited => crate::journal::Event::Waited,
    }
}

pub fn observation(
    view: s::Observation,
    scene: Vec<w::SightCell>,
    salt: &str,
    ready: bool,
) -> p::Observation {
    let offset = |position: w::Position| p::Position {
        x: position.x,
        y: position.y,
        z: position.z,
    };
    let mut visible_cells = Vec::new();
    let mut ground_items = Vec::new();
    let mut visible_actors = Vec::new();
    for cell in scene {
        let Some(visible) = view
            .visible_cells
            .iter()
            .find(|visible| visible.location == cell.location)
        else {
            continue;
        };
        visible_cells.push(p::CellView {
            door: visible.door.map(|door| p::DoorView {
                id: door.id,
                name: "wooden door".into(),
                description: "A plain wooden door with an iron handle.".into(),
                open: door.open,
                reachable: visible.door_reachable,
                approaches: visible
                    .door_approaches
                    .iter()
                    .map(|location| cell_key(salt, view.actor.0, *location))
                    .collect(),
            }),
            material: visible.material.into(),
            key: cell_key(salt, view.actor.0, cell.location),
            position: offset(cell.offset),
            wall: cell.wall,
            place_hint: visible.place_hint,
            stairs_up: view
                .exits
                .iter()
                .any(|exit| exit.location == cell.location && exit.direction == w::Direction::Up),
            stairs_down: view
                .exits
                .iter()
                .any(|exit| exit.location == cell.location && exit.direction == w::Direction::Down),
        });
        for item in view
            .ground_items
            .iter()
            .filter(|item| item.location == cell.location)
        {
            ground_items.push(p::GroundItemView {
                item: p::ItemView {
                    description: item.description.clone(),
                    id: item.id.0,
                    name: item.name.clone(),
                },
                position: offset(cell.offset),
                reachable: item.location == view.location,
            });
        }
        for actor in view
            .visible_actors
            .iter()
            .filter(|actor| actor.location == cell.location)
        {
            visible_actors.push(p::ActorView {
                name: actor.name.into(),
                description: actor.description.into(),
                id: p::ActorId(actor.id.0),
                position: offset(cell.offset),
            });
        }
        if cell.location == view.location && cell.offset != (w::Position { x: 0, y: 0, z: 0 }) {
            visible_actors.push(p::ActorView {
                name: "yourself".into(),
                description: "You recognize your own appearance from another angle.".into(),
                id: p::ActorId(view.actor.0),
                position: offset(cell.offset),
            });
        }
    }
    p::Observation {
        actor: p::ActorId(view.actor.0),
        tick: view.tick,
        position: p::Position { x: 0, y: 0, z: 0 },
        ready,
        visible_cells,
        ground_items,
        visible_actors,
        inventory: view
            .inventory
            .into_iter()
            .map(|item| p::ItemView {
                description: item.description,
                id: item.id.0,
                name: item.name,
            })
            .collect(),
    }
}

pub fn cell_key(salt: &str, actor: u64, location: w::Location) -> String {
    use sha1::{Digest, Sha1};
    let mut digest = Sha1::new();
    digest.update(salt.as_bytes());
    digest.update(actor.to_le_bytes());
    digest.update(location.region.0.to_le_bytes());
    for coordinate in [
        location.position.x,
        location.position.y,
        location.position.z,
    ] {
        digest.update(coordinate.to_le_bytes());
    }
    digest.update(salt.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
