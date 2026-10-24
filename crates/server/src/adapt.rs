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

use crate::actions as a;

pub fn requested_direction(direction: p::Direction) -> a::Direction {
    match direction {
        p::Direction::North => a::Direction::North,
        p::Direction::East => a::Direction::East,
        p::Direction::South => a::Direction::South,
        p::Direction::West => a::Direction::West,
        p::Direction::NorthEast => a::Direction::NorthEast,
        p::Direction::SouthEast => a::Direction::SouthEast,
        p::Direction::SouthWest => a::Direction::SouthWest,
        p::Direction::NorthWest => a::Direction::NorthWest,
        p::Direction::Up => a::Direction::Up,
        p::Direction::Down => a::Direction::Down,
    }
}

pub fn wire_direction(direction: a::Direction) -> p::Direction {
    match direction {
        a::Direction::North => p::Direction::North,
        a::Direction::East => p::Direction::East,
        a::Direction::South => p::Direction::South,
        a::Direction::West => p::Direction::West,
        a::Direction::NorthEast => p::Direction::NorthEast,
        a::Direction::SouthEast => p::Direction::SouthEast,
        a::Direction::SouthWest => p::Direction::SouthWest,
        a::Direction::NorthWest => p::Direction::NorthWest,
        a::Direction::Up => p::Direction::Up,
        a::Direction::Down => p::Direction::Down,
    }
}

pub fn simulation_direction(direction: a::Direction) -> w::Direction {
    match direction {
        a::Direction::North => w::Direction::North,
        a::Direction::East => w::Direction::East,
        a::Direction::South => w::Direction::South,
        a::Direction::West => w::Direction::West,
        a::Direction::NorthEast => w::Direction::NorthEast,
        a::Direction::SouthEast => w::Direction::SouthEast,
        a::Direction::SouthWest => w::Direction::SouthWest,
        a::Direction::NorthWest => w::Direction::NorthWest,
        a::Direction::Up => w::Direction::Up,
        a::Direction::Down => w::Direction::Down,
    }
}

pub fn direction(direction: p::Direction) -> w::Direction {
    simulation_direction(requested_direction(direction))
}

fn simulation_supplies(value: &a::SupplySelection) -> s::supplies::SupplySelection {
    match value {
        a::SupplySelection::Carried => s::supplies::SupplySelection::Carried,
        a::SupplySelection::Explicit { items } => s::supplies::SupplySelection::Explicit {
            items: items
                .iter()
                .map(|row| s::supplies::SelectedSupply {
                    cost_ordinal: row.cost_ordinal,
                    item: s::ItemId(row.item),
                })
                .collect(),
        },
    }
}
fn recorded_supplies(value: s::supplies::SupplySelection) -> a::SupplySelection {
    match value {
        s::supplies::SupplySelection::Carried => a::SupplySelection::Carried,
        s::supplies::SupplySelection::Explicit { items } => a::SupplySelection::Explicit {
            items: items
                .into_iter()
                .map(|row| a::SelectedSupply {
                    cost_ordinal: row.cost_ordinal,
                    item: row.item.0,
                })
                .collect(),
        },
    }
}

fn simulation_projectile_delivery(
    value: a::ProjectileDeliverySelection,
) -> s::abilities::ProjectileDeliverySelection {
    match value {
        a::ProjectileDeliverySelection::Throw { source } => {
            s::abilities::ProjectileDeliverySelection::Throw {
                source: match source {
                    a::ThrownSourceSelection::Equipped { item, socket } => {
                        s::abilities::ThrownSourceSelection::Equipped {
                            item: s::ItemId(item),
                            socket: s::EquipmentSlotId(socket),
                        }
                    }
                    a::ThrownSourceSelection::Carried { item, socket } => {
                        s::abilities::ThrownSourceSelection::Carried {
                            item: s::ItemId(item),
                            socket: s::EquipmentSlotId(socket),
                        }
                    }
                },
            }
        }
        a::ProjectileDeliverySelection::Launch {
            launcher,
            ammunition,
        } => s::abilities::ProjectileDeliverySelection::Launch {
            launcher: s::abilities::EquippedProjectileWeapon {
                item: s::ItemId(launcher.item),
                socket: s::EquipmentSlotId(launcher.socket),
            },
            ammunition: s::ItemId(ammunition),
        },
    }
}
fn recorded_projectile_delivery(
    value: s::abilities::ProjectileDeliverySelection,
) -> a::ProjectileDeliverySelection {
    match value {
        s::abilities::ProjectileDeliverySelection::Throw { source } => {
            a::ProjectileDeliverySelection::Throw {
                source: match source {
                    s::abilities::ThrownSourceSelection::Equipped { item, socket } => {
                        a::ThrownSourceSelection::Equipped {
                            item: item.0,
                            socket: socket.0,
                        }
                    }
                    s::abilities::ThrownSourceSelection::Carried { item, socket } => {
                        a::ThrownSourceSelection::Carried {
                            item: item.0,
                            socket: socket.0,
                        }
                    }
                },
            }
        }
        s::abilities::ProjectileDeliverySelection::Launch {
            launcher,
            ammunition,
        } => a::ProjectileDeliverySelection::Launch {
            launcher: a::EquippedProjectileWeapon {
                item: launcher.item.0,
                socket: launcher.socket.0,
            },
            ammunition: ammunition.0,
        },
    }
}

pub fn action(action: &a::Action) -> s::Action {
    match action {
        a::Action::Escape { hold } => s::Action::Escape { hold: *hold },
        a::Action::ChooseGrowthSource { basis, source } => s::Action::ChooseGrowthSource {
            basis: (**basis).clone(),
            source: source.clone(),
        },
        a::Action::ChooseIntellectExpertise { basis, choice } => {
            s::Action::ChooseIntellectExpertise {
                basis: basis.as_ref().clone(),
                choice: choice.clone(),
            }
        }
        a::Action::Purchase { basis, purchase } => s::Action::Purchase {
            basis: basis.as_ref().clone(),
            purchase: purchase.clone(),
        },
        a::Action::Respec {
            basis,
            replacement,
            supplies,
        } => s::Action::Respec {
            basis: basis.as_ref().clone(),
            replacement: replacement.clone(),
            supplies: simulation_supplies(supplies),
        },
        a::Action::UseAbility {
            id,
            invocation,
            supplies,
        } => s::Action::UseAbility {
            id: id.clone(),
            invocation: s::abilities::AbilityInvocation {
                aim: match &invocation.aim {
                    a::AbilityAim::ActorTarget { target } => {
                        s::abilities::AbilityAim::ActorTarget { target: *target }
                    }
                    a::AbilityAim::ContactTarget { target } => {
                        s::abilities::AbilityAim::ContactTarget { target: *target }
                    }
                    a::AbilityAim::DirectionalProjectile {
                        direction,
                        delivery,
                    } => s::abilities::AbilityAim::DirectionalProjectile {
                        delivery: simulation_projectile_delivery(*delivery),
                        direction: match direction {
                            a::Direction::North => s::abilities::StepDirection::North,
                            a::Direction::East => s::abilities::StepDirection::East,
                            a::Direction::South => s::abilities::StepDirection::South,
                            a::Direction::West => s::abilities::StepDirection::West,
                            a::Direction::NorthEast => s::abilities::StepDirection::NorthEast,
                            a::Direction::SouthEast => s::abilities::StepDirection::SouthEast,
                            a::Direction::SouthWest => s::abilities::StepDirection::SouthWest,
                            a::Direction::NorthWest => s::abilities::StepDirection::NorthWest,
                            a::Direction::Up => s::abilities::StepDirection::Up,
                            a::Direction::Down => s::abilities::StepDirection::Down,
                        },
                    },
                    a::AbilityAim::SelfStep { direction } => s::abilities::AbilityAim::SelfStep {
                        direction: match direction {
                            a::Direction::North => s::abilities::StepDirection::North,
                            a::Direction::East => s::abilities::StepDirection::East,
                            a::Direction::South => s::abilities::StepDirection::South,
                            a::Direction::West => s::abilities::StepDirection::West,
                            a::Direction::NorthEast => s::abilities::StepDirection::NorthEast,
                            a::Direction::SouthEast => s::abilities::StepDirection::SouthEast,
                            a::Direction::SouthWest => s::abilities::StepDirection::SouthWest,
                            a::Direction::NorthWest => s::abilities::StepDirection::NorthWest,
                            a::Direction::Up => s::abilities::StepDirection::Up,
                            a::Direction::Down => s::abilities::StepDirection::Down,
                        },
                    },
                    a::AbilityAim::DirectionalRay { direction } => {
                        s::abilities::AbilityAim::DirectionalRay {
                            direction: match direction {
                                a::Direction::North => s::abilities::StepDirection::North,
                                a::Direction::East => s::abilities::StepDirection::East,
                                a::Direction::South => s::abilities::StepDirection::South,
                                a::Direction::West => s::abilities::StepDirection::West,
                                a::Direction::NorthEast => s::abilities::StepDirection::NorthEast,
                                a::Direction::SouthEast => s::abilities::StepDirection::SouthEast,
                                a::Direction::SouthWest => s::abilities::StepDirection::SouthWest,
                                a::Direction::NorthWest => s::abilities::StepDirection::NorthWest,
                                a::Direction::Up => s::abilities::StepDirection::Up,
                                a::Direction::Down => s::abilities::StepDirection::Down,
                            },
                        }
                    }
                },
                weapon: match invocation.weapon {
                    a::WeaponSelection::Default => s::abilities::WeaponSelection::Default,
                    a::WeaponSelection::Equipped { item, socket } => {
                        s::abilities::WeaponSelection::Equipped {
                            item: s::ItemId(item),
                            socket: s::EquipmentSlotId(socket),
                        }
                    }
                },
            },
            supplies: simulation_supplies(supplies),
        },
        a::Action::ExamineTrap { trap } => s::Action::ExamineTrap { trap: *trap },
        a::Action::SetDoor { door, open } => s::Action::SetDoor {
            door: *door,
            open: *open,
        },
        a::Action::Move { direction } => s::Action::Move(simulation_direction(*direction)),
        a::Action::Take { item, quantity } => s::Action::Take {
            item: s::ItemId(*item),
            quantity: *quantity,
        },
        a::Action::TouchApparentObject { subject, quantity } => s::Action::TouchApparentObject {
            subject: *subject,
            quantity: *quantity,
        },
        a::Action::Drop { item, quantity } => s::Action::Drop {
            item: s::ItemId(*item),
            quantity: *quantity,
        },
        a::Action::Equip { item, slot } => s::Action::Equip {
            item: s::ItemId(*item),
            slot: s::EquipmentSlotId(*slot),
        },
        a::Action::Unequip { item } => s::Action::Unequip {
            item: s::ItemId(*item),
        },
        a::Action::Drink { item } => s::Action::Drink {
            item: s::ItemId(*item),
        },
        a::Action::Eat { item } => s::Action::Eat {
            item: s::ItemId(*item),
        },
        a::Action::Read { item } => s::Action::Read {
            item: s::ItemId(*item),
        },
        a::Action::Zap { item, aim } => s::Action::Zap {
            item: s::ItemId(*item),
            aim: device_aim(*aim),
        },
        a::Action::Identify { choice, item } => s::Action::Identify {
            choice: *choice,
            item: item.map(s::ItemId),
        },
        a::Action::Wait => s::Action::Wait,
    }
}

pub fn work(action: &a::Action) -> Option<s::Work> {
    Some(match action {
        a::Action::Escape { hold } => s::Work::Escape { hold: *hold },
        a::Action::Respec {
            basis,
            replacement,
            supplies,
        } => s::Work::Respec {
            basis: basis.as_ref().clone(),
            replacement: replacement.clone(),
            supplies: simulation_supplies(supplies),
        },
        a::Action::ChooseGrowthSource { .. }
        | a::Action::ChooseIntellectExpertise { .. }
        | a::Action::Purchase { .. } => return None,
        a::Action::UseAbility { .. } => {
            let s::Action::UseAbility {
                id,
                invocation,
                supplies,
            } = self::action(action)
            else {
                unreachable!()
            };
            s::Work::UseAbility {
                id,
                invocation,
                supplies,
            }
        }
        a::Action::Equip { item, slot } => s::Work::Equip {
            item: s::ItemId(*item),
            slot: s::EquipmentSlotId(*slot),
        },
        a::Action::Unequip { item } => s::Work::Unequip {
            item: s::ItemId(*item),
        },
        a::Action::Drink { item } => s::Work::Drink {
            item: s::ItemId(*item),
        },
        a::Action::Eat { item } => s::Work::Eat {
            item: s::ItemId(*item),
        },
        a::Action::Read { item } => s::Work::Read {
            item: s::ItemId(*item),
        },
        a::Action::Zap { item, aim } => s::Work::Zap {
            item: s::ItemId(*item),
            aim: device_aim(*aim),
        },
        a::Action::Identify { .. }
        | a::Action::ExamineTrap { .. }
        | a::Action::SetDoor { .. }
        | a::Action::Move { .. }
        | a::Action::Take { .. }
        | a::Action::TouchApparentObject { .. }
        | a::Action::Drop { .. }
        | a::Action::Wait => return None,
    })
}

pub fn recorded_action(action: s::Action) -> Option<a::Action> {
    Some(match action {
        s::Action::Escape { hold } => a::Action::Escape { hold },
        s::Action::ChooseGrowthSource { basis, source } => a::Action::ChooseGrowthSource {
            basis: Box::new(basis),
            source,
        },
        s::Action::ChooseIntellectExpertise { basis, choice } => {
            a::Action::ChooseIntellectExpertise {
                basis: Box::new(basis),
                choice,
            }
        }
        s::Action::Purchase { basis, purchase } => a::Action::Purchase {
            basis: Box::new(basis),
            purchase,
        },
        s::Action::Respec {
            basis,
            replacement,
            supplies,
        } => a::Action::Respec {
            basis: Box::new(basis),
            replacement,
            supplies: recorded_supplies(supplies),
        },
        s::Action::Move(direction) => a::Action::Move {
            direction: match direction {
                w::Direction::North => a::Direction::North,
                w::Direction::East => a::Direction::East,
                w::Direction::South => a::Direction::South,
                w::Direction::West => a::Direction::West,
                w::Direction::NorthEast => a::Direction::NorthEast,
                w::Direction::SouthEast => a::Direction::SouthEast,
                w::Direction::SouthWest => a::Direction::SouthWest,
                w::Direction::NorthWest => a::Direction::NorthWest,
                w::Direction::Up => a::Direction::Up,
                w::Direction::Down => a::Direction::Down,
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
        s::Action::UseAbility {
            id,
            invocation,
            supplies,
        } => a::Action::UseAbility {
            id,
            invocation: a::AbilityInvocation {
                aim: match invocation.aim {
                    s::abilities::AbilityAim::ActorTarget { target } => {
                        a::AbilityAim::ActorTarget { target }
                    }
                    s::abilities::AbilityAim::ContactTarget { target } => {
                        a::AbilityAim::ContactTarget { target }
                    }
                    s::abilities::AbilityAim::DirectionalProjectile {
                        direction,
                        delivery,
                    } => a::AbilityAim::DirectionalProjectile {
                        delivery: recorded_projectile_delivery(delivery),
                        direction: match direction {
                            s::abilities::StepDirection::North => a::Direction::North,
                            s::abilities::StepDirection::East => a::Direction::East,
                            s::abilities::StepDirection::South => a::Direction::South,
                            s::abilities::StepDirection::West => a::Direction::West,
                            s::abilities::StepDirection::NorthEast => a::Direction::NorthEast,
                            s::abilities::StepDirection::SouthEast => a::Direction::SouthEast,
                            s::abilities::StepDirection::SouthWest => a::Direction::SouthWest,
                            s::abilities::StepDirection::NorthWest => a::Direction::NorthWest,
                            s::abilities::StepDirection::Up => a::Direction::Up,
                            s::abilities::StepDirection::Down => a::Direction::Down,
                        },
                    },
                    s::abilities::AbilityAim::SelfStep { direction } => a::AbilityAim::SelfStep {
                        direction: match direction {
                            s::abilities::StepDirection::North => a::Direction::North,
                            s::abilities::StepDirection::East => a::Direction::East,
                            s::abilities::StepDirection::South => a::Direction::South,
                            s::abilities::StepDirection::West => a::Direction::West,
                            s::abilities::StepDirection::NorthEast => a::Direction::NorthEast,
                            s::abilities::StepDirection::SouthEast => a::Direction::SouthEast,
                            s::abilities::StepDirection::SouthWest => a::Direction::SouthWest,
                            s::abilities::StepDirection::NorthWest => a::Direction::NorthWest,
                            s::abilities::StepDirection::Up => a::Direction::Up,
                            s::abilities::StepDirection::Down => a::Direction::Down,
                        },
                    },
                    s::abilities::AbilityAim::DirectionalRay { direction } => {
                        a::AbilityAim::DirectionalRay {
                            direction: match direction {
                                s::abilities::StepDirection::North => a::Direction::North,
                                s::abilities::StepDirection::East => a::Direction::East,
                                s::abilities::StepDirection::South => a::Direction::South,
                                s::abilities::StepDirection::West => a::Direction::West,
                                s::abilities::StepDirection::NorthEast => a::Direction::NorthEast,
                                s::abilities::StepDirection::SouthEast => a::Direction::SouthEast,
                                s::abilities::StepDirection::SouthWest => a::Direction::SouthWest,
                                s::abilities::StepDirection::NorthWest => a::Direction::NorthWest,
                                s::abilities::StepDirection::Up => a::Direction::Up,
                                s::abilities::StepDirection::Down => a::Direction::Down,
                            },
                        }
                    }
                },
                weapon: match invocation.weapon {
                    s::abilities::WeaponSelection::Default => a::WeaponSelection::Default,
                    s::abilities::WeaponSelection::Equipped { item, socket } => {
                        a::WeaponSelection::Equipped {
                            item: item.0,
                            socket: socket.0,
                        }
                    }
                },
            },
            supplies: recorded_supplies(supplies),
        },
        s::Action::ExamineTrap { trap } => a::Action::ExamineTrap { trap },
        s::Action::SetDoor { door, open } => a::Action::SetDoor { door, open },
        s::Action::Take { item, quantity } => a::Action::Take {
            item: item.0,
            quantity,
        },
        s::Action::TouchApparentObject { subject, quantity } => {
            a::Action::TouchApparentObject { subject, quantity }
        }
        s::Action::Drop { item, quantity } => a::Action::Drop {
            item: item.0,
            quantity,
        },
        s::Action::Equip { item, slot } => a::Action::Equip {
            item: item.0,
            slot: slot.0,
        },
        s::Action::Unequip { item } => a::Action::Unequip { item: item.0 },
        s::Action::Drink { item } => a::Action::Drink { item: item.0 },
        s::Action::Eat { item } => a::Action::Eat { item: item.0 },
        s::Action::Read { item } => a::Action::Read { item: item.0 },
        s::Action::Zap { item, aim } => a::Action::Zap {
            item: item.0,
            aim: recorded_device_aim(aim),
        },
        s::Action::Identify { choice, item } => a::Action::Identify {
            choice,
            item: item.map(|item| item.0),
        },
        s::Action::Wait => a::Action::Wait,
    })
}

pub fn event(kind: s::OutcomeKind) -> crate::journal::Event {
    match kind {
        s::OutcomeKind::TrapExamined { trap } => crate::journal::Event::TrapExamined { trap },
        s::OutcomeKind::IdentificationResolved { choice, item } => {
            crate::journal::Event::IdentificationResolved {
                choice,
                item: item.map(|item| item.0),
            }
        }
        s::OutcomeKind::EscapeStarted { hold } => crate::journal::Event::EscapeStarted { hold },
        s::OutcomeKind::GrowthSourceChosen { .. } => crate::journal::Event::GrowthSourceChosen,
        s::OutcomeKind::IntellectExpertiseChosen { .. } => {
            crate::journal::Event::IntellectExpertiseChosen
        }
        s::OutcomeKind::Purchased { .. } => crate::journal::Event::Purchased,
        s::OutcomeKind::RespecStarted { .. } => crate::journal::Event::RespecStarted,
        s::OutcomeKind::AbilityStarted {
            id,
            name,
            target,
            reference,
        } => crate::journal::Event::AbilityStarted {
            reference,
            id,
            name,
            target: p::ActorId(target.0),
        },
        s::OutcomeKind::DoorChanged { door, open } => {
            crate::journal::Event::DoorChanged { door, open }
        }
        s::OutcomeKind::Moved { from, to } => crate::journal::Event::Moved {
            from: position(from),
            to: position(to),
        },
        s::OutcomeKind::Crumbled { item, quantity } => crate::journal::Event::Crumbled {
            item: item.0,
            quantity,
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
        s::OutcomeKind::ItemStarted { work } => crate::journal::Event::ItemStarted {
            action: recorded_action(work.action()).unwrap(),
        },
        s::OutcomeKind::Escaped { item } => crate::journal::Event::Escaped { item: item.0 },
        // The paid touch carries no creature identity or pickup result publicly.
        s::OutcomeKind::ApparentObjectTouched { .. } | s::OutcomeKind::Waited => {
            crate::journal::Event::Waited
        }
    }
}

/// A region's floor, wall and door assets, if its scenario names them.
pub type Terrain<'a> = &'a dyn Fn(w::RegionId) -> [Option<String>; 3];

pub(crate) fn trap_family(family: w::TrapFamily) -> p::TrapFamily {
    match family {
        w::TrapFamily::Rust => p::TrapFamily::Rust,
        w::TrapFamily::SleepingGas => p::TrapFamily::SleepingGas,
        w::TrapFamily::TrapDoor => p::TrapFamily::TrapDoor,
        w::TrapFamily::Arrow => p::TrapFamily::Arrow,
        w::TrapFamily::BearTrap => p::TrapFamily::BearTrap,
        w::TrapFamily::Teleport => p::TrapFamily::Teleport,
        w::TrapFamily::PoisonDart => p::TrapFamily::PoisonDart,
        w::TrapFamily::Mysterious => p::TrapFamily::Mysterious,
    }
}

pub(crate) const WOODEN_DOOR_NAME: &str = "wooden door";
pub(crate) const WOODEN_DOOR_DESCRIPTION: &str = "A plain wooden door with an iron handle.";
pub(crate) fn door_appearance(secret: bool) -> (&'static str, &'static str) {
    if secret {
        (
            "secret entrance",
            "A concealed entrance in the surrounding stone.",
        )
    } else {
        (WOODEN_DOOR_NAME, WOODEN_DOOR_DESCRIPTION)
    }
}

pub fn observation(
    view: s::Observation,
    scene: Vec<w::SightCell>,
    salt: &str,
    ready: bool,
    terrain: Terrain,
    targets: &crate::wire_adapter::TargetScope,
) -> p::Observation {
    let offset = |position: w::Position| p::Position {
        x: position.x,
        y: position.y,
        z: position.z,
    };
    let rules = view
        .combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
        .map(|stats| stats.rules.clone());
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
            lit: visible.lit,
            asset: if visible.wall { wall } else { floor },
            door: visible.door.map(|door| p::DoorView {
                id: targets.door(door.id),
                name: door_appearance(visible.door_secret).0.into(),
                description: door_appearance(visible.door_secret).1.into(),
                open: door.open,
                reachable: visible.door_reachable,
                approaches: visible
                    .door_approaches
                    .iter()
                    .map(|location| cell_key(salt, view.actor.0, *location))
                    .collect(),
                asset: door_asset,
            }),
            trap: visible.trap.as_ref().map(|trap| p::TrapView {
                id: targets.trap(trap.id),
                family: trap.family.map(trap_family),
                reachable: trap.reachable,
            }),
            material: visible.material.into(),
            key: cell_key(salt, view.actor.0, cell.location),
            position: offset(cell.offset),
            wall: visible.wall,
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
                    corpse: item.corpse.as_ref().map(|source| p::CreatureAppearance {
                        seed: targets.appearance_seed(source.actor),
                        asset: source.asset.clone(),
                    }),
                    class: item_class(item.class),
                    quantity: item.quantity,
                    appearance: item.appearance.clone(),
                    identified: item.identified,
                    known_currency: item.known_currency.map(currency_kind),
                    known_supply: item.known_supply.as_ref().map(|value| p::SupplyView {
                        id: value.id.to_string(),
                        name: value.name.clone(),
                    }),
                    description: item.description.clone(),
                    id: targets.item(item.id),
                    name: item.name.clone(),
                    asset: item.asset.clone(),
                },
                position: offset(cell.offset),
                reachable: item.location == view.location,
                supply_reachable: item.supply_reachable,
            });
        }
        for object in view
            .apparent_objects
            .iter()
            .filter(|object| object.location == cell.location)
        {
            ground_items.push(p::GroundItemView {
                item: p::ItemView {
                    corpse: None,
                    class: item_class(object.appearance.class),
                    quantity: 1,
                    appearance: object.appearance.name.clone(),
                    identified: true,
                    known_currency: None,
                    known_supply: None,
                    description: object.appearance.description.clone(),
                    id: targets.apparent_object(object.subject),
                    name: object.appearance.name.clone(),
                    asset: object.appearance.asset.clone(),
                },
                position: offset(cell.offset),
                reachable: object.location == view.location,
                supply_reachable: false,
            });
        }
        for actor in view
            .visible_actors
            .iter()
            .filter(|actor| actor.location == cell.location)
        {
            visible_actors.push(p::ActorView {
                name: actor.name.clone(),
                appearance_seed: (actor.disclosure == s::perception::ActorDisclosure::Creature)
                    .then(|| targets.appearance_seed(actor.id)),
                description: String::new(),
                id: targets.actor_reference(
                    actor.id,
                    actor
                        .disclosure
                        .reference()
                        .expect("selectable actor disclosure"),
                ),
                position: offset(cell.offset),
                asset: actor.asset.clone(),
            });
        }
        if cell.location == view.location && cell.offset != (w::Position { x: 0, y: 0, z: 0 }) {
            visible_actors.push(p::ActorView {
                name: String::new(),
                appearance_seed: Some(targets.appearance_seed(view.actor)),
                description: String::new(),
                id: targets.actor(view.actor),
                position: offset(cell.offset),
                asset: view.asset.clone(),
            });
        }
    }
    let linked_holds = view
        .linked_holds
        .iter()
        .map(|hold| p::LinkedHold {
            target: targets.hold(hold.id),
            holder: visible_actors
                .iter()
                .any(|actor| actor.id == targets.actor(hold.holder))
                .then(|| targets.actor(hold.holder)),
        })
        .collect();
    p::Observation {
        self_appearance: Some(p::CreatureAppearance {
            seed: targets.appearance_seed(view.actor),
            asset: view.asset.clone(),
        }),
        linked_holds,
        interactions: view.interactions.map(|interaction| p::InteractionView {
            identification: interaction
                .identification
                .map(|choice| p::IdentificationChoiceView {
                    choice: targets.identification_choice(choice.choice),
                    classes: choice.classes.into_iter().map(item_class).collect(),
                    items: choice
                        .items
                        .into_iter()
                        .map(|item| targets.item(item))
                        .collect(),
                }),
            curse_blocked: interaction
                .curse_blocked
                .into_iter()
                .map(|item| targets.item(item))
                .collect(),
            completed: interaction
                .completed
                .into_iter()
                .map(|work| {
                    crate::wire_adapter::encode_action(
                        &recorded_action(work.action()).expect("completed item action"),
                        targets,
                    )
                })
                .collect(),
            slots: interaction.slots.into_iter().map(equipment_slot).collect(),
            // Attached spectators share this ordinary projection. Private advancement
            // operands are available only through the authorized owner query.
            preparation: interaction
                .preparation
                .filter(|progress| !matches!(progress.work, s::Work::Respec { .. }))
                .map(|progress| p::PreparationView {
                    action: crate::wire_adapter::encode_action(
                        &recorded_action(progress.work.action()).unwrap(),
                        targets,
                    ),
                    remaining: progress.remaining,
                    active: progress.active,
                }),
            inventory: interaction
                .inventory
                .into_iter()
                .map(|item| p::ItemInteractionView {
                    item: targets.item(item.item),
                    slot: item.slot.map(equipment_slot),
                    equipped_slot: item.equipped_slot.map(|slot| slot.0),
                    known_projectile_bonus: item.known_projectile_bonus.map(|bonus| {
                        p::ProjectileBonusView {
                            hit: bonus.hit,
                            damage: bonus.damage,
                        }
                    }),
                    known_cursed: item.known_cursed,
                    known_protected: item.known_protected,
                    known_equipment: item.known_equipment.and_then(|equipment| {
                        rules.as_deref().map(|rules| p::EquipmentView {
                            attack: equipment.attack.as_ref().map(|attack| {
                                crate::creature_view::attack_view(attack, Some(rules))
                            }),
                            modifiers: equipment
                                .modifiers
                                .iter()
                                .map(|value| crate::creature_view::modifier(value, rules))
                                .collect(),
                        })
                    }),
                    drinkable: item.drinkable,
                    edible: item.edible,
                    readable: item.readable,
                    zappable: item.zappable,
                    device_activation: item.device_activation.map(|activation| match activation {
                        s::DeviceActivation::Direction => p::DeviceActivation::Direction,
                        s::DeviceActivation::SelfTarget => p::DeviceActivation::SelfTarget,
                    }),
                    charges: item.charges,
                })
                .collect(),
        }),
        combat: view.combat.map(|c| p::CombatView {
            public_plan: c
                .public_plan
                .as_deref()
                .map(|record| crate::public_plan_view::view(record, salt, view.actor)),
            own_stats: c.own_stats.map(crate::creature_view::own_stats),
            hp: c.hp,
            max_hp: c.max_hp,
            preparation_remaining: c.preparation_remaining,
            preparation_active: c.preparation_active,
            sequence: c.sequence.map(|sequence| p::SequenceView {
                completed: sequence.completed,
                total: sequence.total,
                next_strike_remaining: sequence.next_strike_remaining,
                cancel_recovery_remaining: sequence.cancel_recovery_remaining,
            }),
            recovery_remaining: c.recovery_remaining,
            actors: c
                .actors
                .into_iter()
                .map(|(id, hostile, allied, injury)| p::CombatActorView {
                    actor: targets.actor(id),
                    hostile,
                    allied,
                    injury: injury_view(injury),
                })
                .collect(),
            events: c
                .events
                .into_iter()
                .map(|event| combat_event_view(event, targets))
                .collect(),
            objective: c.objective.map(|o| match o {
                s::combat::ObjectiveKind::RetrieveAndReturn => p::ObjectiveKind::RetrieveAndReturn,
                s::combat::ObjectiveKind::ReachExit => p::ObjectiveKind::ReachExit,
                s::combat::ObjectiveKind::RetrieveAndExit => p::ObjectiveKind::RetrieveAndExit,
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
        self_target: targets.actor(view.actor),
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
                corpse: item.corpse.as_ref().map(|source| p::CreatureAppearance {
                    seed: targets.appearance_seed(source.actor),
                    asset: source.asset.clone(),
                }),
                class: item_class(item.class),
                quantity: item.quantity,
                appearance: item.appearance.clone(),
                identified: item.identified,
                known_currency: item.known_currency.map(currency_kind),
                known_supply: item.known_supply.as_ref().map(|value| p::SupplyView {
                    id: value.id.to_string(),
                    name: value.name.clone(),
                }),
                description: item.description,
                id: targets.item(item.id),
                name: item.name,
                asset: item.asset,
            })
            .collect(),
    }
}

fn equipment_slot(slot: s::EquipmentSlotView) -> p::EquipmentSlot {
    p::EquipmentSlot {
        id: slot.id.to_string(),
        name: slot.name,
        equipped_phrase: slot.equipped_phrase,
        requires_attack: slot.requires_attack,
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

fn combat_event_view(
    event: s::combat::DisclosedCombatEvent,
    targets: &crate::wire_adapter::TargetScope,
) -> p::CombatEventView {
    use s::combat::DisclosedCombatEvent as E;
    let id = |actor: s::ActorId| targets.actor(actor);
    match event {
        E::UnseenAbility {
            caster,
            target,
            outcome,
        } => p::CombatEventView::UnseenAbility {
            caster: caster.map(|actor| targets.contact(actor)),
            target: targets.actor(target),
            outcome: crate::creature_view::outcome_status(outcome),
        },
        E::AnonymousAbility {
            caster,
            caster_reference,
            target,
            target_reference,
            outcome,
        } => p::CombatEventView::AnonymousAbility {
            caster: caster
                .zip(caster_reference)
                .map(|(actor, kind)| targets.actor_reference(actor, kind)),
            target: target
                .zip(target_reference)
                .map(|(actor, kind)| targets.actor_reference(actor, kind)),
            outcome: crate::creature_view::outcome_status(outcome),
        },
        E::EscapeResolved { hold, success } => p::CombatEventView::EscapeResolved {
            hold: targets.hold(hold),
            success,
        },
        E::Ability {
            caster,
            caster_reference,
            target,
            target_reference,
            id: ability_id,
            name,
            outcome,
            strike,
        } => p::CombatEventView::Ability {
            caster: caster
                .zip(caster_reference)
                .map(|(actor, kind)| targets.actor_reference(actor, kind)),
            target: target
                .zip(target_reference)
                .map(|(actor, kind)| targets.actor_reference(actor, kind)),
            ability: p::ContentRef {
                id: ability_id.to_string(),
                name,
            },
            outcome: crate::creature_view::outcome_status(outcome),
            strike,
        },
        E::Condition { id, name } => p::CombatEventView::Condition {
            condition: p::ContentRef {
                id: id.to_string(),
                name,
            },
        },
        E::Interrupted { actor } => p::CombatEventView::Interrupted { actor: id(actor) },
        E::Died { actor, reference } => p::CombatEventView::Died {
            actor: targets.actor_reference(actor, reference),
        },
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

fn currency_kind(currency: s::CurrencyKind) -> p::CurrencyKind {
    match currency {
        s::CurrencyKind::Gold => p::CurrencyKind::Gold,
    }
}

pub(crate) fn item_class(class: s::ItemClass) -> p::ItemClass {
    match class {
        s::ItemClass::Misc => p::ItemClass::Misc,
        s::ItemClass::Weapon => p::ItemClass::Weapon,
        s::ItemClass::Armor => p::ItemClass::Armor,
        s::ItemClass::Potion => p::ItemClass::Potion,
        s::ItemClass::Food => p::ItemClass::Food,
        s::ItemClass::Corpse => p::ItemClass::Corpse,
        s::ItemClass::Tool => p::ItemClass::Tool,
        s::ItemClass::Amulet => p::ItemClass::Amulet,
        s::ItemClass::Ring => p::ItemClass::Ring,
        s::ItemClass::Scroll => p::ItemClass::Scroll,
        s::ItemClass::Spellbook => p::ItemClass::Spellbook,
        s::ItemClass::Wand => p::ItemClass::Wand,
        s::ItemClass::Coin => p::ItemClass::Coin,
        s::ItemClass::Gem => p::ItemClass::Gem,
    }
}

#[cfg(test)]
mod ability_journal_tests {
    #[test]
    fn arbitrary_content_ids_preserve_action_and_preparation_identity() {
        let id = tor_simulation::content_ids::AbilityId::new("fixture_ability").unwrap();
        let action = crate::actions::Action::UseAbility {
            id: id.clone(),
            invocation: crate::actions::AbilityInvocation {
                aim: crate::actions::AbilityAim::ActorTarget {
                    target: tor_simulation::ActorId(2),
                },
                weapon: crate::actions::WeaponSelection::Default,
            },
            supplies: Default::default(),
        };
        assert_eq!(
            super::work(&action),
            Some(tor_simulation::Work::UseAbility {
                id: id.clone(),
                invocation: tor_simulation::abilities::AbilityInvocation {
                    aim: tor_simulation::abilities::AbilityAim::ActorTarget {
                        target: tor_simulation::ActorId(2)
                    },
                    weapon: tor_simulation::abilities::WeaponSelection::Default
                },
                supplies: Default::default(),
            })
        );
        assert_eq!(super::recorded_action(super::action(&action)), Some(action));
        let reference = tor_simulation::perception::ActorReferenceKind::Creature;
        let event = super::event(tor_simulation::OutcomeKind::AbilityStarted {
            reference,
            id: id.clone(),
            name: "Fixture Ability".into(),
            target: tor_simulation::ActorId(2),
        });
        assert_eq!(
            event,
            crate::journal::Event::AbilityStarted {
                reference,
                id,
                name: "Fixture Ability".into(),
                target: tor_protocol::ActorId(2)
            }
        );
    }
}

#[cfg(test)]
mod ordered_strike_privacy_tests {
    use super::*;

    #[test]
    fn ordered_strikes_disclosure_maps_actual_ordinal_without_hidden_source_facts() {
        let scope = crate::wire_adapter::TargetScope::new(uuid::Uuid::nil(), s::ActorId(1));
        for caster in [None, Some(s::ActorId(2))] {
            let view = combat_event_view(
                s::combat::DisclosedCombatEvent::Ability {
                    caster,
                    caster_reference: caster.map(|_| s::perception::ActorReferenceKind::Creature),
                    target: Some(s::ActorId(1)),
                    target_reference: Some(s::perception::ActorReferenceKind::Creature),
                    id: s::content_ids::AbilityId::new("triple_strike").unwrap(),
                    name: "Triple Strike".into(),
                    outcome: s::abilities::OutcomeStatus::Miss,
                    strike: Some(1),
                },
                &scope,
            );
            let value = serde_json::to_value(view).unwrap();
            assert_eq!(value["strike"], 1);
            assert_eq!(
                value["caster"],
                serde_json::to_value(caster.map(|id| scope.actor(id))).unwrap()
            );
            for key in [
                "total",
                "check",
                "tokens",
                "plan",
                "intention",
                "actor",
                "origin",
            ] {
                assert!(value.get(key).is_none(), "private {key}");
            }
        }
    }
}

fn step_direction(direction: a::Direction) -> s::abilities::StepDirection {
    match direction {
        a::Direction::North => s::abilities::StepDirection::North,
        a::Direction::East => s::abilities::StepDirection::East,
        a::Direction::South => s::abilities::StepDirection::South,
        a::Direction::West => s::abilities::StepDirection::West,
        a::Direction::NorthEast => s::abilities::StepDirection::NorthEast,
        a::Direction::SouthEast => s::abilities::StepDirection::SouthEast,
        a::Direction::SouthWest => s::abilities::StepDirection::SouthWest,
        a::Direction::NorthWest => s::abilities::StepDirection::NorthWest,
        a::Direction::Up => s::abilities::StepDirection::Up,
        a::Direction::Down => s::abilities::StepDirection::Down,
    }
}

fn recorded_step_direction(direction: s::abilities::StepDirection) -> a::Direction {
    match direction {
        s::abilities::StepDirection::North => a::Direction::North,
        s::abilities::StepDirection::East => a::Direction::East,
        s::abilities::StepDirection::South => a::Direction::South,
        s::abilities::StepDirection::West => a::Direction::West,
        s::abilities::StepDirection::NorthEast => a::Direction::NorthEast,
        s::abilities::StepDirection::SouthEast => a::Direction::SouthEast,
        s::abilities::StepDirection::SouthWest => a::Direction::SouthWest,
        s::abilities::StepDirection::NorthWest => a::Direction::NorthWest,
        s::abilities::StepDirection::Up => a::Direction::Up,
        s::abilities::StepDirection::Down => a::Direction::Down,
    }
}

pub fn device_aim(aim: a::DeviceAim) -> s::DeviceAim {
    match aim {
        a::DeviceAim::Direction { direction } => s::DeviceAim::Direction {
            direction: step_direction(direction),
        },
        a::DeviceAim::SelfTarget {} => s::DeviceAim::SelfTarget {},
    }
}
pub fn recorded_device_aim(aim: s::DeviceAim) -> a::DeviceAim {
    match aim {
        s::DeviceAim::Direction { direction } => a::DeviceAim::Direction {
            direction: recorded_step_direction(direction),
        },
        s::DeviceAim::SelfTarget {} => a::DeviceAim::SelfTarget {},
    }
}
pub fn requested_device_aim(aim: p::DeviceAim) -> a::DeviceAim {
    match aim {
        p::DeviceAim::Direction { direction } => a::DeviceAim::Direction {
            direction: requested_direction(direction),
        },
        p::DeviceAim::SelfTarget {} => a::DeviceAim::SelfTarget {},
    }
}
pub fn wire_device_aim(aim: a::DeviceAim) -> p::DeviceAim {
    match aim {
        a::DeviceAim::Direction { direction } => p::DeviceAim::Direction {
            direction: wire_direction(direction),
        },
        a::DeviceAim::SelfTarget {} => p::DeviceAim::SelfTarget {},
    }
}
