use tor_protocol as p;
use tor_simulation as s;
use tor_world as w;

pub fn location(position: p::Position) -> w::Location {
    w::Location {
        region: w::RegionId(position.region),
        position: w::Position {
            x: position.x,
            y: position.y,
            z: position.z,
        },
    }
}

pub fn position(location: w::Location) -> p::Position {
    p::Position {
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

fn wire_direction(direction: w::Direction) -> p::Direction {
    match direction {
        w::Direction::North => p::Direction::North,
        w::Direction::East => p::Direction::East,
        w::Direction::South => p::Direction::South,
        w::Direction::West => p::Direction::West,
        w::Direction::Up => p::Direction::Up,
        w::Direction::Down => p::Direction::Down,
    }
}

pub fn action(action: &p::Action) -> s::Action {
    match action {
        p::Action::Move { direction: value } => s::Action::Move(direction(*value)),
        p::Action::Take { item } => s::Action::Take(s::ItemId(*item)),
        p::Action::Wait => s::Action::Wait,
    }
}

pub fn event(kind: s::OutcomeKind) -> p::Event {
    match kind {
        s::OutcomeKind::Moved { from, to } => p::Event::Moved {
            from: position(from),
            to: position(to),
        },
        s::OutcomeKind::Taken { item } => p::Event::Taken { item: item.0 },
        s::OutcomeKind::Waited => p::Event::Waited,
    }
}

pub fn observation(view: s::Observation, ready: bool) -> p::Observation {
    let region_id = view.location.region;
    let cell = |cell: w::Position| {
        position(w::Location {
            region: region_id,
            position: cell,
        })
    };
    let (width, depth, height) = view.region.bounds.dimensions();
    p::Observation {
        actor: p::ActorId(view.actor.0),
        tick: view.tick,
        position: position(view.location),
        ready,
        region: p::RegionView {
            id: view.region.id.0,
            name: view.region.name,
            width,
            depth,
            height,
        },
        ground_items: view
            .ground_items
            .into_iter()
            .map(|item| p::GroundItemView {
                item: p::ItemView {
                    id: item.id.0,
                    name: item.name,
                },
                position: cell(item.position),
            })
            .collect(),
        inventory: view
            .inventory
            .into_iter()
            .map(|item| p::ItemView {
                id: item.id.0,
                name: item.name,
            })
            .collect(),
        visible_actors: view
            .visible_actors
            .into_iter()
            .map(|actor| p::ActorView {
                id: p::ActorId(actor.id.0),
                position: cell(actor.position),
            })
            .collect(),
        exits: view
            .exits
            .into_iter()
            .map(|exit| p::ExitView {
                position: cell(exit.position),
                direction: wire_direction(exit.direction),
            })
            .collect(),
        known_places: view
            .known_places
            .into_iter()
            .map(|place| p::Place {
                id: place.id.0,
                name: place.name,
            })
            .collect(),
    }
}
