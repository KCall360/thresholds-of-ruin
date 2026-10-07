//! Backend action facts, independent from transport and persistence schemas.
//! Receipt commands retain the original requested facts; queued work belongs to
//! the simulation and is checked against those facts during recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Attack { target: tor_simulation::ActorId },
    SetDoor { door: u64, open: bool },
    Move { direction: Direction },
    Take { item: u64, quantity: Option<u64> },
    Drop { item: u64, quantity: Option<u64> },
    Wait,
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
    fn every_requested_direction_maps_to_its_native_axis_and_wire_name() {
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
            assert_eq!(action.to_wire(), wire::Action::Move { direction: wire });
            assert_eq!(Action::from_wire(&action.to_wire()), action);
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
        let mut cases = vec![
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
        for (action, native) in cases {
            assert_eq!(adapt::action(&action), native);
            assert_eq!(adapt::recorded_action(native), Some(action.clone()));
            assert_eq!(Action::from_wire(&action.to_wire()), action);
        }
    }
}
