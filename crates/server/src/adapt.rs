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

pub fn action(action: &a::Action) -> s::Action {
    match action {
        a::Action::ChooseGrowthSource { basis, source } => s::Action::ChooseGrowthSource {
            basis: (**basis).clone(),
            source: source.clone(),
        },
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
            invocation: match invocation {
                a::AbilityInvocation::ActorTarget { target } => {
                    s::abilities::AbilityInvocation::ActorTarget { target: *target }
                }
                a::AbilityInvocation::SelfStep { direction } => {
                    s::abilities::AbilityInvocation::SelfStep {
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
            supplies: simulation_supplies(supplies),
        },
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
        a::Action::Respec {
            basis,
            replacement,
            supplies,
        } => s::Work::Respec {
            basis: basis.as_ref().clone(),
            replacement: replacement.clone(),
            supplies: simulation_supplies(supplies),
        },
        a::Action::ChooseGrowthSource { .. } | a::Action::Purchase { .. } => return None,
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
        a::Action::SetDoor { .. }
        | a::Action::Move { .. }
        | a::Action::Take { .. }
        | a::Action::Drop { .. }
        | a::Action::Wait => return None,
    })
}

pub fn recorded_action(action: s::Action) -> Option<a::Action> {
    Some(match action {
        s::Action::ChooseGrowthSource { basis, source } => a::Action::ChooseGrowthSource {
            basis: Box::new(basis),
            source,
        },
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
            invocation: match invocation {
                s::abilities::AbilityInvocation::ActorTarget { target } => {
                    a::AbilityInvocation::ActorTarget { target }
                }
                s::abilities::AbilityInvocation::SelfStep { direction } => {
                    a::AbilityInvocation::SelfStep {
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
            supplies: recorded_supplies(supplies),
        },
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
        s::OutcomeKind::GrowthSourceChosen { .. } => crate::journal::Event::GrowthSourceChosen,
        s::OutcomeKind::Purchased { .. } => crate::journal::Event::Purchased,
        s::OutcomeKind::RespecStarted { .. } => crate::journal::Event::RespecStarted,
        s::OutcomeKind::AbilityStarted { id, name, target } => {
            crate::journal::Event::AbilityStarted {
                id,
                name,
                target: p::ActorId(target.0),
            }
        }
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
        E::Ability {
            caster,
            target,
            id: ability_id,
            name,
            outcome,
            strike,
        } => p::CombatEventView::Ability {
            caster: caster.map(id),
            target: target.map(id),
            ability: p::ContentRef {
                id: ability_id.to_string(),
                name,
            },
            outcome: crate::creature_view::outcome_status(outcome),
            strike,
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

#[cfg(test)]
mod ability_journal_tests {
    #[test]
    fn arbitrary_content_ids_preserve_action_and_preparation_identity() {
        let id = tor_simulation::content_ids::AbilityId::new("fixture_ability").unwrap();
        let action = crate::actions::Action::UseAbility {
            id: id.clone(),
            invocation: crate::actions::AbilityInvocation::ActorTarget {
                target: tor_simulation::ActorId(2),
            },
            supplies: Default::default(),
        };
        assert_eq!(
            super::work(&action),
            Some(tor_simulation::Work::UseAbility {
                id: id.clone(),
                invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                    target: tor_simulation::ActorId(2)
                },
                supplies: Default::default(),
            })
        );
        assert_eq!(super::recorded_action(super::action(&action)), Some(action));
        let event = super::event(tor_simulation::OutcomeKind::AbilityStarted {
            id: id.clone(),
            name: "Fixture Ability".into(),
            target: tor_simulation::ActorId(2),
        });
        assert_eq!(
            event,
            crate::journal::Event::AbilityStarted {
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
                    target: Some(s::ActorId(1)),
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
