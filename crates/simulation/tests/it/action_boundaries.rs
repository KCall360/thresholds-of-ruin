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
    let mut game = Game::new(world, 42, super::creature_fixture::content());
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
            ][action_index]
                .clone();
            let outcome = game.act(actor, action.clone()).unwrap();
            assert_eq!(outcome.actor, actor);
            assert_eq!(outcome.at_tick, 0);
            assert_eq!(outcome.next_actor, Some(other));
            assert_eq!(outcome.next_tick, if acting_second { 100 } else { 0 });
            let observed = game.observe(actor).unwrap();
            match action {
                Action::UseAbility { .. }
                | Action::Equip { .. }
                | Action::Unequip { .. }
                | Action::Drink { .. } => unreachable!("prepared work has separate timing tests"),
                Action::Drop { .. }
                | Action::Purchase { .. }
                | Action::ChooseGrowthSource { .. }
                | Action::Respec { .. } => {
                    unreachable!("tested separately")
                }
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
        ][action_index]
            .clone();
        let before = game.clone();
        assert_eq!(
            game.act(actor, action.clone()),
            Err(GameError::Schedule(
                tor_simulation::ScheduleError::TimeExhausted
            ))
        );
        assert_eq!(game, before);
    }
}

// Restore a valid current checkpoint at a chosen absolute clock. This reaches
// overflow edges directly without advancing creature timers for 2^64 ticks.
fn at_tick(game: &Game, tick: u64) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    assert_eq!(game.tick(), 0);
    snapshot["tick"] = serde_json::json!(tick);
    for table in pool["actors"].as_array_mut().unwrap() {
        for actor in table.as_object_mut().unwrap().values_mut() {
            assert_eq!(actor["ready_at"], serde_json::json!(0));
            actor["ready_at"] = serde_json::json!(tick);
            actor["active_since"] = serde_json::json!(tick);
        }
    }
    let shared = serde_json::from_value(pool).unwrap();
    let restored = Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(restored.tick(), tick);
    restored
}

fn remaining_family(index: usize) -> (Game, ActorId, Action, u64) {
    use tor_simulation::{ConsumableSpec, EquipmentSlotId, EquipmentSpec, ItemId, ItemSpec};
    let (mut game, actor, target) = super::abilities::duel(42);
    let at = game.observe(actor).unwrap().location;
    let mut spec = ItemSpec::ordinary("boundary item".into());
    let (action, duration) = match index {
        0 => {
            spec.stackable = true;
            (
                Action::Drop {
                    item: ItemId(100),
                    quantity: Some(2),
                },
                50,
            )
        }
        1 | 2 => {
            spec.class = tor_simulation::ItemClass::Armor;
            let slot = game
                .anatomy(actor)
                .unwrap()
                .slots
                .iter()
                .position(|slot| slot.as_str() == "body_armor")
                .unwrap();
            let kind = game.anatomy(actor).unwrap().slots[slot].clone();
            let timing = game.content().rules().equipment_slot(&kind).unwrap();
            let duration = 100
                * if index == 1 {
                    timing.equip_turns
                } else {
                    timing.unequip_turns
                };
            spec.equipment = Some(EquipmentSpec {
                slot: kind,
                attack: None,
                modifiers: vec![],
            });
            let action = if index == 1 {
                Action::Equip {
                    item: ItemId(100),
                    slot: EquipmentSlotId(slot as u16),
                }
            } else {
                Action::Unequip { item: ItemId(100) }
            };
            (action, duration)
        }
        3 => {
            spec.class = tor_simulation::ItemClass::Potion;
            spec.consumable = Some(ConsumableSpec {
                effects: vec![tor_simulation::rules::Effect::Heal {
                    amount: tor_simulation::rules::AmountExpr::Fixed { amount: 1 },
                }],
            });
            (
                Action::Drink { item: ItemId(100) },
                100 * game.content().rules().constants().item_timing.consume_turns,
            )
        }
        4 | 5 => {
            let id = super::abilities::id(if index == 4 {
                "basic_melee"
            } else {
                "power_strike"
            });
            let plan = game.ability_plan(actor, &id, target).unwrap();
            assert!(plan.preparation() > 0 && plan.recovery() > 0);
            assert_eq!(plan.cost().is_some(), index == 5);
            (
                Action::UseAbility {
                    id,
                    target,
                    supplies: Default::default(),
                },
                plan.preparation() + plan.recovery(),
            )
        }
        _ => unreachable!(),
    };
    if index < 4 {
        game.place_item_stack(100, at, Some(actor), if index == 0 { 3 } else { 1 }, spec)
            .unwrap();
        if index == 2 {
            let slot = game
                .anatomy(actor)
                .unwrap()
                .slots
                .iter()
                .position(|slot| slot.as_str() == "body_armor")
                .unwrap();
            game.equip_authored(actor, ItemId(100), EquipmentSlotId(slot as u16))
                .unwrap();
        }
    }
    (game, actor, action, duration)
}

#[test]
fn drop_and_every_prepared_family_accept_minimum_and_exact_maximum_schedules() {
    use tor_simulation::IntentionOrigin;
    for family in 0..6 {
        let (base, actor, action, duration) = remaining_family(family);
        for tick in [0, u64::MAX - duration] {
            let mut game = at_tick(&base, tick);
            let before = game.clone();
            let identity = game
                .admit_intention(actor, action.clone(), IntentionOrigin::Human)
                .unwrap();
            assert_eq!(game.tick(), before.tick());
            assert_eq!(game.observe(actor).unwrap(), before.observe(actor).unwrap());
            assert_eq!(game.creature(actor), before.creature(actor));
            assert_eq!(game.combat_events(), before.combat_events());
            let queued = game.pending_intention(actor).unwrap().clone();
            assert_eq!(queued.id, identity);
            let execution = game.execute_next_intention().unwrap();
            assert_eq!(execution.intention, queued);
            let outcome = execution.outcome.unwrap();
            assert_eq!(outcome.at_tick, tick);
            assert!(game.pending_intention(actor).is_none());
            if family == 0 {
                assert_eq!(
                    game.observe(actor)
                        .unwrap()
                        .inventory
                        .iter()
                        .find(|item| item.id == tor_simulation::ItemId(100))
                        .unwrap()
                        .quantity,
                    1
                );
            } else {
                let preparation = game.preparation(actor).unwrap();
                assert_eq!(
                    preparation.plan.preparation() + preparation.plan.recovery(),
                    duration
                );
                assert_eq!(preparation.remaining, preparation.plan.preparation());
                assert_eq!(preparation.origin_intention(), Some(identity));
                if family == 5 {
                    assert_eq!(preparation.hold, Some(identity));
                    assert!(game
                        .creature(actor)
                        .unwrap()
                        .costs()
                        .reservation(identity)
                        .is_some());
                }
            }
        }
    }
}

#[test]
fn drop_and_every_prepared_family_reject_plus_one_without_side_effects() {
    use tor_simulation::IntentionOrigin;
    let error = GameError::Schedule(tor_simulation::ScheduleError::TimeExhausted);
    for family in 0..6 {
        let (base, actor, action, duration) = remaining_family(family);
        let mut game = at_tick(&base, u64::MAX - duration + 1);
        let before = game.clone();
        assert_eq!(
            game.admit_intention(actor, action.clone(), IntentionOrigin::Human),
            Err(error),
            "family {family}"
        );
        assert_eq!(
            game, before,
            "admission must preserve identities, resources, items, pending work, events and RNG"
        );
        assert_eq!(
            game.act(actor, action.clone()),
            Err(error),
            "family {family}"
        );
        assert_eq!(game, before, "direct execution is atomic");
        assert_eq!(game.combat_events(), before.combat_events());

        // Work valid when admitted can overflow when executed later. The queue
        // contract consumes the failed identity, while gameplay stays unchanged.
        let mut admitted = base.clone();
        let identity = admitted
            .admit_intention(actor, action.clone(), IntentionOrigin::Human)
            .unwrap();
        let mut queued = at_tick(&admitted, u64::MAX - duration + 1);
        let record = queued.pending_intention(actor).unwrap().clone();
        let mut expected = queued.clone();
        expected.cancel_intention(actor, identity).unwrap();
        let execution = queued.execute_next_intention().unwrap();
        assert_eq!(execution.intention, record);
        assert_eq!(execution.action, Some(action));
        assert_eq!(execution.outcome, Err(error));
        assert_eq!(queued, expected, "only the failed queue entry is consumed");
        assert!(queued.pending_intention(actor).is_none());
    }
}
