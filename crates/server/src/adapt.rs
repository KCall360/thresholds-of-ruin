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
        p::Direction::NorthEast => w::Direction::NorthEast,
        p::Direction::SouthEast => w::Direction::SouthEast,
        p::Direction::SouthWest => w::Direction::SouthWest,
        p::Direction::NorthWest => w::Direction::NorthWest,

        p::Direction::Up => w::Direction::Up,
        p::Direction::Down => w::Direction::Down,
    }
}

pub fn action(action: &p::Action) -> s::Action {
    match action {
        p::Action::Attack { target } => s::Action::Attack {
            target: s::ActorId(target.0),
        },
        p::Action::SetDoor { door, open } => s::Action::SetDoor {
            door: *door,
            open: *open,
        },
        p::Action::Move { direction: value } => s::Action::Move(direction(*value)),
        p::Action::Take { item, quantity } => s::Action::Take {
            item: s::ItemId(*item),
            quantity: *quantity,
        },
        p::Action::Drop { item, quantity } => s::Action::Drop {
            item: s::ItemId(*item),
            quantity: *quantity,
        },
        p::Action::Wait => s::Action::Wait,
    }
}

pub fn disclosed_action(action: s::Action) -> Option<p::Action> {
    Some(match action {
        s::Action::Move(d) => p::Action::Move {
            direction: match d {
                w::Direction::North => p::Direction::North,
                w::Direction::East => p::Direction::East,
                w::Direction::South => p::Direction::South,
                w::Direction::West => p::Direction::West,
                w::Direction::NorthEast => p::Direction::NorthEast,
                w::Direction::SouthEast => p::Direction::SouthEast,
                w::Direction::SouthWest => p::Direction::SouthWest,
                w::Direction::NorthWest => p::Direction::NorthWest,
                w::Direction::Up => p::Direction::Up,
                w::Direction::Down => p::Direction::Down,
                w::Direction::EastUp
                | w::Direction::WestUp
                | w::Direction::NorthUp
                | w::Direction::SouthUp
                | w::Direction::EastDown
                | w::Direction::WestDown
                | w::Direction::NorthDown
                | w::Direction::SouthDown => return None,
            },
        },
        s::Action::Attack { target } => p::Action::Attack {
            target: p::ActorId(target.0),
        },
        s::Action::SetDoor { door, open } => p::Action::SetDoor { door, open },
        s::Action::Take { item, quantity } => p::Action::Take {
            item: item.0,
            quantity,
        },
        s::Action::Drop { item, quantity } => p::Action::Drop {
            item: item.0,
            quantity,
        },
        s::Action::Wait => p::Action::Wait,
    })
}

pub fn event(kind: s::OutcomeKind) -> crate::journal::Event {
    match kind {
        s::OutcomeKind::AttackStarted { target } => crate::journal::Event::AttackStarted {
            target: p::ActorId(target.0),
        },
        s::OutcomeKind::DoorChanged { door, open } => {
            crate::journal::Event::DoorChanged { door, open }
        }
        s::OutcomeKind::Moved { from, to } => crate::journal::Event::Moved {
            from: position(from),
            to: position(to),
        },
        s::OutcomeKind::Taken {
            item,
            result,
            quantity,
        } => crate::journal::Event::Taken {
            item: item.0,
            result: result.0,
            quantity,
        },
        s::OutcomeKind::Dropped {
            item,
            result,
            quantity,
        } => crate::journal::Event::Dropped {
            item: item.0,
            result: result.0,
            quantity,
        },
        s::OutcomeKind::Waited => crate::journal::Event::Waited,
    }
}

/// A region's floor, wall and door assets, if its scenario names them.
pub type Terrain<'a> = &'a dyn Fn(w::RegionId) -> [Option<String>; 3];

pub fn observation(
    view: s::Observation,
    scene: Vec<w::SightCell>,
    salt: &str,
    ready: bool,
    terrain: Terrain,
) -> p::Observation {
    let offset = |position: w::Position| p::Position {
        x: position.x,
        y: position.y,
        z: position.z,
    };
    let mut visible_cells = Vec::new();
    let mut ground_items = Vec::new();
    let mut visible_actors = Vec::new();
    // 3D scenes hold about a thousand cells; index them once instead of
    // scanning every visible cell for each scene cell.
    let disclosed: std::collections::BTreeMap<_, _> = view
        .visible_cells
        .iter()
        .map(|visible| ((visible.location, visible.frame), visible))
        .collect();
    for cell in scene {
        let Some(&visible) = disclosed.get(&(cell.location, cell.rotation)) else {
            continue;
        };
        let [floor, wall, door_asset] = terrain(cell.location.region);
        visible_cells.push(p::CellView {
            asset: if cell.wall { wall } else { floor },
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
                asset: door_asset,
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
                    quantity: item.quantity,
                    appearance: item.appearance.clone(),
                    identified: item.identified,
                    description: item.description.clone(),
                    id: item.id.0,
                    name: item.name.clone(),
                    asset: item.asset.clone(),
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
                name: actor.name.clone(),
                description: String::new(),
                id: p::ActorId(actor.id.0),
                position: offset(cell.offset),
                asset: actor.asset.clone(),
            });
        }
        if cell.location == view.location && cell.offset != (w::Position { x: 0, y: 0, z: 0 }) {
            visible_actors.push(p::ActorView {
                name: String::new(),
                description: String::new(),
                id: p::ActorId(view.actor.0),
                position: offset(cell.offset),
                asset: view.asset.clone(),
            });
        }
    }
    p::Observation {
        combat: view.combat.map(|c| p::CombatView {
            hp: c.hp,
            max_hp: c.max_hp,
            preparation_remaining: c.preparation_remaining,
            preparation_active: c.preparation_active,
            recovery_remaining: c.recovery_remaining,
            actors: c
                .actors
                .into_iter()
                .map(|(id, hostile, injury)| p::CombatActorView {
                    actor: p::ActorId(id.0),
                    hostile,
                    injury: injury_view(injury),
                })
                .collect(),
            events: c.events.into_iter().map(combat_event_view).collect(),
            objective: c.objective.map(|o| match o {
                s::combat::ObjectiveKind::RetrieveAndReturn => p::ObjectiveKind::RetrieveAndReturn,
                s::combat::ObjectiveKind::ReachExit => p::ObjectiveKind::ReachExit,
            }),
            exit: c.exit.map(|at| cell_key(salt, view.actor.0, at)),
            victory: c.victory,
            dead: c.dead,
            terminal: c.terminal,
        }),
        motion: view.motion.map(|m| p::MotionView {
            velocity: m.velocity,
            units_per_cell: 65536,
            displaced: m.displaced,
            impacted: m.impacted,
        }),
        places: vec![],
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
                quantity: item.quantity,
                appearance: item.appearance.clone(),
                identified: item.identified,
                description: item.description,
                id: item.id.0,
                name: item.name,
                asset: item.asset,
            })
            .collect(),
    }
}

fn injury_view(injury: s::combat::Injury) -> p::Injury {
    match injury {
        s::combat::Injury::Healthy => p::Injury::Healthy,
        s::combat::Injury::Wounded => p::Injury::Wounded,
        s::combat::Injury::BadlyWounded => p::Injury::BadlyWounded,
        s::combat::Injury::NearDeath => p::Injury::NearDeath,
    }
}

fn combat_event_view(event: s::combat::DisclosedCombatEvent) -> p::CombatEventView {
    use s::combat::{AttackOutcome as O, DisclosedCombatEvent as E};
    let id = |actor: s::ActorId| p::ActorId(actor.0);
    match event {
        E::Attack {
            attacker,
            target,
            outcome,
        } => p::CombatEventView::Attack {
            attacker: attacker.map(id),
            target: target.map(id),
            outcome: match outcome {
                O::Miss => p::AttackOutcome::Miss,
                O::NoInjury => p::AttackOutcome::NoInjury,
                O::Hit => p::AttackOutcome::Hit,
            },
        },
        E::Interrupted { actor } => p::CombatEventView::Interrupted { actor: id(actor) },
        E::Died { actor } => p::CombatEventView::Died { actor: id(actor) },
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
