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

pub fn action(action: &a::Action) -> s::Action {
    match action {
        a::Action::Attack { target } => s::Action::Attack { target: *target },
        a::Action::SetDoor { door, open } => s::Action::SetDoor {
            door: *door,
            open: *open,
        },
        a::Action::Move { direction } => s::Action::Move(simulation_direction(*direction)),
        a::Action::Take { item, quantity } => s::Action::Take {
            item: s::ItemId(*item),
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
        a::Action::Wait => s::Action::Wait,
    }
}

pub fn work(action: &a::Action) -> Option<s::Work> {
    Some(match action {
        a::Action::Attack { target } => s::Work::Attack { target: *target },
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
        _ => return None,
    })
}

pub fn recorded_action(action: s::Action) -> Option<a::Action> {
    Some(match action {
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
        s::Action::Attack { target } => a::Action::Attack { target },
        s::Action::SetDoor { door, open } => a::Action::SetDoor { door, open },
        s::Action::Take { item, quantity } => a::Action::Take {
            item: item.0,
            quantity,
        },
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
        s::Action::Wait => a::Action::Wait,
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
        s::OutcomeKind::ItemStarted { work } => crate::journal::Event::ItemStarted {
            action: recorded_action(work.action()).unwrap(),
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
    targets: &crate::wire_adapter::TargetScope,
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
                id: targets.door(door.id),
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
                    class: item_class(item.class),
                    quantity: item.quantity,
                    appearance: item.appearance.clone(),
                    identified: item.identified,
                    description: item.description.clone(),
                    id: targets.item(item.id),
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
                id: targets.actor(actor.id),
                position: offset(cell.offset),
                asset: actor.asset.clone(),
            });
        }
        if cell.location == view.location && cell.offset != (w::Position { x: 0, y: 0, z: 0 }) {
            visible_actors.push(p::ActorView {
                name: String::new(),
                description: String::new(),
                id: targets.actor(view.actor),
                position: offset(cell.offset),
                asset: view.asset.clone(),
            });
        }
    }
    p::Observation {
        interactions: view.interactions.map(|interaction| p::InteractionView {
            slots: interaction.slots.into_iter().map(equipment_slot).collect(),
            preparation: interaction.preparation.map(|progress| p::PreparationView {
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
                    known_equipment: item.known_equipment.map(|equipment| p::EquipmentView {
                        attack: equipment.attack.map(|attack| p::AttackView {
                            bonus: attack.bonus,
                            wind_up: attack.wind_up,
                            recovery: attack.recovery,
                            damage: attack
                                .damage
                                .into_iter()
                                .map(|(kind, amount)| (damage_type(kind), amount))
                                .collect(),
                        }),
                        defense: equipment.defense,
                        reductions: equipment
                            .reductions
                            .into_iter()
                            .map(|(kind, amount)| (damage_type(kind), amount))
                            .collect(),
                    }),
                    drinkable: item.drinkable,
                })
                .collect(),
        }),
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
                    actor: targets.actor(id),
                    hostile,
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
                class: item_class(item.class),
                quantity: item.quantity,
                appearance: item.appearance.clone(),
                identified: item.identified,
                description: item.description,
                id: targets.item(item.id),
                name: item.name,
                asset: item.asset,
            })
            .collect(),
    }
}

fn equipment_slot(slot: s::EquipmentSlot) -> p::EquipmentSlot {
    match slot {
        s::EquipmentSlot::Weapon => p::EquipmentSlot::Weapon,
        s::EquipmentSlot::BodyArmor => p::EquipmentSlot::BodyArmor,
        s::EquipmentSlot::Shield => p::EquipmentSlot::Shield,
        s::EquipmentSlot::HeadArmor => p::EquipmentSlot::HeadArmor,
        s::EquipmentSlot::HandsArmor => p::EquipmentSlot::HandsArmor,
        s::EquipmentSlot::FeetArmor => p::EquipmentSlot::FeetArmor,
        s::EquipmentSlot::Cloak => p::EquipmentSlot::Cloak,
        s::EquipmentSlot::Ring => p::EquipmentSlot::Ring,
        s::EquipmentSlot::Amulet => p::EquipmentSlot::Amulet,
    }
}
fn damage_type(kind: s::combat::DamageType) -> p::DamageType {
    match kind {
        s::combat::DamageType::Energy => p::DamageType::Energy,
        s::combat::DamageType::Impact => p::DamageType::Impact,
        s::combat::DamageType::Keen => p::DamageType::Keen,
        s::combat::DamageType::Spirit => p::DamageType::Spirit,
        s::combat::DamageType::Vital => p::DamageType::Vital,
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
    use s::combat::{AttackOutcome as O, DisclosedCombatEvent as E};
    let id = |actor: s::ActorId| targets.actor(actor);
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

fn item_class(class: s::ItemClass) -> p::ItemClass {
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
