//! Backend action facts, independent from transport and persistence schemas.
//! Receipt commands retain the original requested facts; queued work belongs to
//! the simulation and is checked against those facts during recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    UseAbility {
        ability: Ability,
        target: tor_simulation::ActorId,
    },
    Attack {
        target: tor_simulation::ActorId,
    },
    Equip {
        item: u64,
        slot: u16,
    },
    Unequip {
        item: u64,
    },
    Drink {
        item: u64,
    },
    SetDoor {
        door: u64,
        open: bool,
    },
    Move {
        direction: Direction,
    },
    Take {
        item: u64,
        quantity: Option<u64>,
    },
    Drop {
        item: u64,
        quantity: Option<u64>,
    },
    Wait,
}

impl Action {
    pub(crate) fn is_prepared(&self) -> bool {
        matches!(
            self,
            Self::UseAbility { .. }
                | Self::Attack { .. }
                | Self::Equip { .. }
                | Self::Unequip { .. }
                | Self::Drink { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    PowerStrike,
    MagicBolt,
    Fear,
}

/// Directions supported by backend requests and recorded action facts.
/// Native geometry has additional directions; projecting those remains fallible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt;
    use tor_protocol as wire;
    use tor_simulation as sim;
    use tor_world as world;

    #[test]
    fn ability_target_resolution_rejects_foreign_and_undisclosed_tokens_equally() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let observer = wire::ActorId(1);
        for ability in [
            wire::Ability::PowerStrike,
            wire::Ability::MagicBolt,
            wire::Ability::Fear,
        ] {
            let target = engine.target_scope(observer).actor(sim::ActorId(1));
            let decoded = engine
                .decode_action(observer, &wire::Action::UseAbility { ability, target })
                .unwrap();
            assert_eq!(
                decoded,
                Action::UseAbility {
                    ability: adapt::requested_ability(ability),
                    target: sim::ActorId(1)
                }
            );
            let hidden = engine.target_scope(observer).actor(sim::ActorId(u64::MAX));
            let foreign = engine.target_scope(wire::ActorId(2)).actor(sim::ActorId(1));
            let hidden_error = engine
                .decode_action(
                    observer,
                    &wire::Action::UseAbility {
                        ability,
                        target: hidden,
                    },
                )
                .unwrap_err();
            let foreign_error = engine
                .decode_action(
                    observer,
                    &wire::Action::UseAbility {
                        ability,
                        target: foreign,
                    },
                )
                .unwrap_err();
            assert_eq!(hidden_error, foreign_error);
            assert_eq!(hidden_error.code, wire::ErrorCode::InvalidAction);
        }
    }

    #[test]
    fn every_requested_direction_maps_to_its_native_axis_and_wire_name() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let actor = wire::ActorId(1);
        for (backend, wire, native) in [
            (
                Direction::North,
                wire::Direction::North,
                world::Direction::North,
            ),
            (
                Direction::East,
                wire::Direction::East,
                world::Direction::East,
            ),
            (
                Direction::South,
                wire::Direction::South,
                world::Direction::South,
            ),
            (
                Direction::West,
                wire::Direction::West,
                world::Direction::West,
            ),
            (
                Direction::NorthEast,
                wire::Direction::NorthEast,
                world::Direction::NorthEast,
            ),
            (
                Direction::SouthEast,
                wire::Direction::SouthEast,
                world::Direction::SouthEast,
            ),
            (
                Direction::SouthWest,
                wire::Direction::SouthWest,
                world::Direction::SouthWest,
            ),
            (
                Direction::NorthWest,
                wire::Direction::NorthWest,
                world::Direction::NorthWest,
            ),
            (Direction::Up, wire::Direction::Up, world::Direction::Up),
            (
                Direction::Down,
                wire::Direction::Down,
                world::Direction::Down,
            ),
        ] {
            let action = Action::Move { direction: backend };
            assert_eq!(adapt::action(&action), sim::Action::Move(native));
            assert_eq!(
                engine.encode_action(actor, &action),
                wire::Action::Move { direction: wire }
            );
            assert_eq!(
                engine
                    .decode_action(actor, &engine.encode_action(actor, &action))
                    .unwrap(),
                action
            );
            assert_eq!(
                adapt::recorded_action(sim::Action::Move(native)),
                Some(action)
            );
        }
        for direction in [
            world::Direction::EastUp,
            world::Direction::WestUp,
            world::Direction::NorthUp,
            world::Direction::SouthUp,
            world::Direction::EastDown,
            world::Direction::WestDown,
            world::Direction::NorthDown,
            world::Direction::SouthDown,
        ] {
            assert_eq!(adapt::recorded_action(sim::Action::Move(direction)), None);
        }
    }

    #[test]
    fn every_action_preserves_target_and_quantity_without_resolution() {
        let engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        let observer = wire::ActorId(1);
        let mut cases = vec![
            (
                Action::Equip {
                    item: u64::MAX,
                    slot: u16::MAX,
                },
                sim::Action::Equip {
                    item: sim::ItemId(u64::MAX),
                    slot: sim::EquipmentSlotId(u16::MAX),
                },
            ),
            (
                Action::Unequip { item: u64::MAX },
                sim::Action::Unequip {
                    item: sim::ItemId(u64::MAX),
                },
            ),
            (
                Action::Drink { item: u64::MAX },
                sim::Action::Drink {
                    item: sim::ItemId(u64::MAX),
                },
            ),
            (
                Action::Attack {
                    target: sim::ActorId(u64::MAX),
                },
                sim::Action::Attack {
                    target: sim::ActorId(u64::MAX),
                },
            ),
            (
                Action::SetDoor {
                    door: u64::MAX,
                    open: true,
                },
                sim::Action::SetDoor {
                    door: u64::MAX,
                    open: true,
                },
            ),
            (
                Action::SetDoor {
                    door: u64::MAX,
                    open: false,
                },
                sim::Action::SetDoor {
                    door: u64::MAX,
                    open: false,
                },
            ),
            (Action::Wait, sim::Action::Wait),
        ];
        for quantity in [None, Some(0), Some(1), Some(u64::MAX)] {
            cases.push((
                Action::Take {
                    item: u64::MAX,
                    quantity,
                },
                sim::Action::Take {
                    item: sim::ItemId(u64::MAX),
                    quantity,
                },
            ));
            cases.push((
                Action::Drop {
                    item: u64::MAX,
                    quantity,
                },
                sim::Action::Drop {
                    item: sim::ItemId(u64::MAX),
                    quantity,
                },
            ));
        }
        for ability in [Ability::PowerStrike, Ability::MagicBolt, Ability::Fear] {
            cases.push((
                Action::UseAbility {
                    ability,
                    target: sim::ActorId(u64::MAX),
                },
                sim::Action::UseAbility {
                    ability: adapt::simulation_ability(ability),
                    target: sim::ActorId(u64::MAX),
                },
            ));
        }
        for (action, native) in cases {
            assert_eq!(adapt::action(&action), native);
            assert_eq!(adapt::recorded_action(native), Some(action.clone()));
            let command = crate::journal::Command::AdmitIntention {
                expected_revision: u64::MAX,
                action,
            };
            let wire = engine.encode_command(observer, command.clone()).unwrap();
            let decoded = crate::wire_adapter::decode_command(&wire).unwrap();
            assert!(decoded.matches(&command, &engine.target_scope(observer)));
        }
    }
}
