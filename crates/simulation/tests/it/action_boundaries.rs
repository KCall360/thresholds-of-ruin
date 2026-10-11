//! Behavior guards for the shared action validation/effect/scheduling boundary.
use std::num::NonZeroU64;
use tor_simulation::{Action, ActorId, Game, GameError, OutcomeKind};
use tor_world::{Direction, Extent, Location, Position, Region, RegionId, World};

fn cell(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}

fn fixture(turn_ticks: u64) -> (Game, ActorId) {
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "Action boundaries".into(),
            bounds: Extent::new(7, 3, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42);
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(turn_ticks).unwrap())
        .unwrap();
    (game, actor)
}

#[test]
fn every_actor_applies_effects_before_recovery_and_uses_the_same_scheduler() {
    for acting_second in [false, true] {
        for action_index in 0..5 {
            let (mut game, first) = fixture(100);
            let second = game
                .spawn_actor(cell(5, 1), NonZeroU64::new(300).unwrap())
                .unwrap();
            let (actor, other, origin, cost) = if acting_second {
                game.act(first, Action::Wait).unwrap();
                (second, first, cell(5, 1), 300)
            } else {
                (first, second, cell(1, 1), 100)
            };
            let item = game.place_item(origin, "token".into()).unwrap();
            let door_cell = cell(origin.position.x + 1, 1);
            let door = game
                .place_door(door_cell, false, game.door_clearance(door_cell))
                .unwrap();
            let action = [
                Action::Wait,
                Action::Take {
                    item,
                    quantity: None,
                },
                Action::SetDoor { door, open: true },
                Action::Move(Direction::North),
                Action::Move(Direction::NorthEast),
            ][action_index];
            let outcome = game.act(actor, action).unwrap();
            assert_eq!(outcome.actor, actor);
            assert_eq!(outcome.at_tick, 0);
            assert_eq!(outcome.next_actor, Some(other));
            assert_eq!(outcome.next_tick, if acting_second { 100 } else { 0 });
            let observed = game.observe(actor).unwrap();
            match action {
                Action::Attack { .. }
                | Action::UseAbility { .. }
                | Action::Equip { .. }
                | Action::Unequip { .. }
                | Action::Drink { .. } => unreachable!("prepared work has separate timing tests"),
                Action::Drop { .. } => unreachable!("tested separately"),
                Action::Wait => assert_eq!(outcome.kind, OutcomeKind::Waited),
                Action::Take { .. } => {
                    assert_eq!(
                        outcome.kind,
                        OutcomeKind::Taken {
                            item,
                            result: item,
                            quantity: 1
                        }
                    );
                    assert!(observed.inventory.iter().any(|entry| entry.id == item));
                    assert!(observed.ground_items.iter().all(|entry| entry.id != item));
                }
                Action::SetDoor { .. } => {
                    assert_eq!(outcome.kind, OutcomeKind::DoorChanged { door, open: true });
                    assert!(observed.visible_cells.iter().any(|cell| {
                        cell.location == door_cell && cell.door.is_some_and(|door| door.open)
                    }));
                }
                Action::Move(direction) => {
                    let to = cell(
                        origin.position.x + i32::from(direction == Direction::NorthEast),
                        0,
                    );
                    assert_eq!(outcome.kind, OutcomeKind::Moved { from: origin, to });
                    assert_eq!(observed.location, to);
                }
            }
            let expected_recovery = match action {
                Action::Take { .. } => cost / 2,
                Action::Move(Direction::NorthEast) => {
                    if acting_second {
                        425
                    } else {
                        142
                    }
                }
                _ => cost,
            };
            for _ in 0..5 {
                if game.next_actor() == Some(actor) {
                    break;
                }
                game.act(other, Action::Wait).unwrap();
            }
            assert_eq!(game.next_actor(), Some(actor));
            assert_eq!(game.tick(), expected_recovery);
        }
    }
}

#[test]
fn recovery_overflow_rejects_each_action_before_applying_its_effect() {
    for action_index in 0..5 {
        let (mut game, actor) = fixture(u64::MAX);
        game.act(actor, Action::Wait).unwrap();
        let item = game.place_item(cell(1, 1), "token".into()).unwrap();
        let door = game
            .place_door(cell(2, 1), false, game.door_clearance(cell(2, 1)))
            .unwrap();
        let action = [
            Action::Wait,
            Action::Take {
                item,
                quantity: None,
            },
            Action::SetDoor { door, open: true },
            Action::Move(Direction::North),
            Action::Move(Direction::NorthEast),
        ][action_index];
        let before = game.clone();
        assert_eq!(game.act(actor, action), Err(GameError::TimeExhausted));
        assert_eq!(game, before);
    }
}
