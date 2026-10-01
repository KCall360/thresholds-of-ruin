//! Command, letter, and queue behavior. These tests build observations themselves.

use std::collections::BTreeSet;
use tor_client_hack::{
    bounded_repeat, bump, classify_try_send, decide_autopickup, door_command, drawn_creature,
    feet_items, fight, repeat_interrupted, take_quantity, to_action, AutoPickup, AutoQuery,
    BumpAttacks, Intent, IntentQueue, InventoryLetters, Resolved, SendFate, TrySend, QUEUE_CAP,
};
use tor_protocol::*;

fn observation() -> Observation {
    Observation {
        combat: None,
        motion: None,
        places: Vec::new(),
        actor: ActorId(1),
        tick: 0,
        position: Position { x: 0, y: 0, z: 0 },
        visible_cells: Vec::new(),
        ground_items: Vec::new(),
        inventory: Vec::new(),
        visible_actors: Vec::new(),
        ready: true,
    }
}

fn cell(x: i32, y: i32, z: i32, wall: bool) -> CellView {
    CellView {
        asset: None,
        door: None,
        material: String::new(),
        key: format!("{x}:{y}:{z}"),
        stairs_up: false,
        stairs_down: false,
        position: Position { x, y, z },
        wall,
        place_hint: false,
    }
}

fn actor(id: u64, x: i32, y: i32, z: i32) -> ActorView {
    ActorView {
        asset: None,
        name: format!("actor-{id}"),
        description: String::new(),
        id: ActorId(id),
        position: Position { x, y, z },
    }
}

fn item(id: u64, x: i32, y: i32, z: i32, reachable: bool) -> GroundItemView {
    GroundItemView {
        reachable,
        item: ItemView {
            quantity: 1,
            appearance: String::new(),
            identified: true,
            description: String::new(),
            id,
            name: format!("item-{id}"),
            asset: None,
        },
        position: Position { x, y, z },
    }
}

fn door(id: u64, open: bool, reachable: bool) -> DoorView {
    DoorView {
        id,
        name: "door".into(),
        description: String::new(),
        open,
        reachable,
        approaches: Vec::new(),
        asset: None,
    }
}

fn hostile(view: &mut Observation, id: u64, hostile: bool) {
    view.combat = Some(CombatView {
        hp: 10,
        max_hp: 10,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![CombatActorView {
            actor: ActorId(id),
            hostile,
            injury: String::new(),
        }],
        messages: Vec::new(),
        objective: None,
        victory: false,
        dead: false,
        terminal: false,
    });
}

#[test]
fn bump_and_fight_attack_the_lowest_id_at_any_z() {
    let mut view = observation();
    view.visible_cells.push(cell(1, 0, 0, false));
    view.visible_actors.push(actor(9, 1, 0, 2));
    view.visible_actors.push(actor(4, 1, 0, 1));
    view.visible_actors.push(actor(7, 1, 0, 0));
    hostile(&mut view, 4, true);
    let direction = Direction::East;
    assert_eq!(drawn_creature(&view, direction), Some(ActorId(4)));
    assert_eq!(
        to_action(&bump(&view, direction, BumpAttacks::Hostile, false, false)),
        Some(Action::Attack { target: ActorId(4) })
    );
    assert_eq!(
        to_action(&fight(&view, direction)),
        Some(Action::Attack { target: ActorId(4) })
    );
    assert_ne!(
        view.visible_actors
            .iter()
            .find(|a| a.id == ActorId(4))
            .unwrap()
            .position
            .z,
        0
    );
}

#[test]
fn a_creature_above_a_standing_wall_is_not_attacked() {
    let mut view = observation();
    view.visible_cells.push(cell(1, 0, 0, true));
    view.visible_actors.push(actor(4, 1, 0, 1));
    hostile(&mut view, 4, true);
    assert!(matches!(
        bump(&view, Direction::East, BumpAttacks::Any, false, false),
        Resolved::Move(Direction::East)
    ));
    assert!(matches!(fight(&view, Direction::East), Resolved::Miss(_)));
    assert!(to_action(&fight(&view, Direction::East)).is_none());
}

#[test]
fn an_item_at_another_z_does_not_take_on_a_bump() {
    let mut view = observation();
    view.visible_cells.push(cell(1, 0, 0, false));
    view.ground_items.push(item(3, 1, 0, 2, true));
    let action = to_action(&bump(
        &view,
        Direction::East,
        BumpAttacks::Any,
        false,
        false,
    ));
    assert_eq!(
        action,
        Some(Action::Move {
            direction: Direction::East
        })
    );
    assert!(!matches!(action, Some(Action::Take { .. })));
}

#[test]
fn a_remembered_closed_door_is_a_move_because_bump_ignores_the_chart() {
    let mut view = observation();
    view.visible_cells.push(cell(1, 0, 0, false));
    let action = to_action(&bump(
        &view,
        Direction::East,
        BumpAttacks::Off,
        false,
        false,
    ))
    .unwrap();
    assert_eq!(
        action,
        Action::Move {
            direction: Direction::East
        }
    );
    assert!(!matches!(action, Action::SetDoor { .. }));
}

#[test]
fn a_reachable_closed_door_opens_and_a_run_stops() {
    let mut view = observation();
    let mut closed = cell(1, 0, 0, false);
    closed.door = Some(door(8, false, true));
    view.visible_cells.push(closed);
    assert_eq!(
        to_action(&bump(&view, Direction::East, BumpAttacks::Off, true, false)),
        Some(Action::SetDoor {
            door: 8,
            open: true
        })
    );
    assert!(matches!(
        bump(&view, Direction::East, BumpAttacks::Off, false, true),
        Resolved::Stop
    ));
    let mut unreachable = cell(1, 0, 0, false);
    unreachable.door = Some(door(8, false, false));
    view.visible_cells = vec![unreachable];
    assert_eq!(
        to_action(&bump(
            &view,
            Direction::East,
            BumpAttacks::Off,
            false,
            false
        )),
        Some(Action::Move {
            direction: Direction::East
        })
    );
}

#[test]
fn close_requires_an_open_reachable_door() {
    let mut view = observation();
    let mut open = cell(0, 1, 0, false);
    open.door = Some(door(2, true, true));
    view.visible_cells.push(open);
    assert_eq!(
        to_action(&door_command(&view, Direction::South, false)),
        Some(Action::SetDoor {
            door: 2,
            open: false
        })
    );
    assert!(matches!(
        door_command(&view, Direction::North, false),
        Resolved::Miss(_)
    ));
}

#[test]
fn letter_inheritance_covers_take_merge_and_partial_drop() {
    let mut letters = InventoryLetters::default();
    letters.update(&[10], true);
    assert_eq!(letters.get(10), Some('a'));
    letters.update(&[10], true);
    assert_eq!(
        letters.get(10),
        Some('a'),
        "a whole take keeps its id and letter"
    );

    letters.update(&[10, 11], true);
    assert_eq!(letters.get(10), Some('a'));
    assert_eq!(
        letters.get(11),
        Some('b'),
        "a partial take allocates a new letter"
    );

    letters.adjust('a', 'c').unwrap();
    assert_eq!(letters.get(10), Some('c'));
    letters.update(&[3, 11], true);
    assert_eq!(
        letters.get(3),
        Some('c'),
        "the merge id inherits the letter that left"
    );
    assert_eq!(letters.get(11), Some('b'));
    assert_eq!(letters.get(10), None);

    let before = letters.get(3);
    letters.update(&[3, 11], true);
    assert_eq!(
        letters.get(3),
        before,
        "a partial drop keeps the source letter"
    );
}

#[test]
fn more_than_fifty_two_stacks_leave_the_rest_unlettered() {
    let mut letters = InventoryLetters::default();
    let ids: Vec<u64> = (1..=53).collect();
    letters.rebuild(&ids);
    assert_eq!(letters.get(1), Some('a'));
    assert_eq!(letters.get(52), Some('Z'));
    assert_eq!(letters.get(53), None);
}

#[test]
fn queue_holds_sixty_four_and_a_repeat_is_one_slot() {
    let mut queue = IntentQueue::new();
    for _ in 0..QUEUE_CAP {
        assert!(queue.push_back(Intent::Act {
            action: Action::Wait,
            pickup: false,
        }));
    }
    assert!(!queue.push_back(Intent::Repeat {
        direction: Direction::East,
        remaining: 2,
        pickup: true,
        suppress_attack: false,
    }));
    assert_eq!(queue.len(), 64);
    queue.pop_front();
    assert!(queue.push_back(Intent::Repeat {
        direction: Direction::North,
        remaining: bounded_repeat(250, false),
        pickup: true,
        suppress_attack: false,
    }));
    assert_eq!(queue.len(), 64);
    let repeat = {
        let mut found = None;
        while let Some(item) = queue.pop_front() {
            if matches!(item, Intent::Repeat { .. }) {
                found = Some(item);
            }
        }
        found.expect("one repeat")
    };
    let Intent::Repeat { remaining, .. } = repeat else {
        unreachable!("matched above");
    };
    assert_eq!(remaining, 100);
}

#[test]
fn repeat_count_caps_at_one_hundred_and_a_run_at_thirty_two() {
    assert_eq!(bounded_repeat(250, false), 100);
    assert_eq!(bounded_repeat(100, false), 100);
    assert_eq!(bounded_repeat(80, true), 32);
    assert_eq!(bounded_repeat(5, false), 5);
}

#[test]
fn a_run_stops_for_a_new_actor_or_a_closed_door() {
    let mut view = observation();
    view.visible_actors.push(actor(2, 3, 0, 0));
    let seen = BTreeSet::from([ActorId(2)]);
    assert!(!repeat_interrupted(&view, Direction::East, &seen));
    view.visible_actors.push(actor(5, 4, 0, 1));
    assert!(repeat_interrupted(&view, Direction::East, &seen));

    let mut door_view = observation();
    let mut next = cell(1, 0, 0, false);
    next.door = Some(door(1, false, true));
    door_view.visible_cells.push(next);
    assert!(repeat_interrupted(
        &door_view,
        Direction::East,
        &BTreeSet::new()
    ));
    let mut stair = observation();
    let mut here = cell(0, 0, 0, false);
    here.stairs_down = true;
    stair.visible_cells.push(here);
    stair.visible_cells.push(cell(1, 0, 0, false));
    assert!(repeat_interrupted(
        &stair,
        Direction::East,
        &BTreeSet::new()
    ));
}

#[test]
fn autopickup_waits_for_arrival_and_uses_feet_only() {
    let mut view = observation();
    view.ground_items.push(item(3, 0, 0, 0, true));
    let seen = BTreeSet::new();
    let skip = decide_autopickup(AutoQuery {
        session_on: true,
        movement_allows: true,
        has_control: true,
        same_branch: true,
        ready: true,
        travel: Some(TravelPhase::Active),
        actors_at_send: &seen,
        observation: &view,
    });
    assert_eq!(skip, AutoPickup::Skip);

    let take = decide_autopickup(AutoQuery {
        travel: Some(TravelPhase::Arrived),
        ..AutoQuery {
            session_on: true,
            movement_allows: true,
            has_control: true,
            same_branch: true,
            ready: true,
            travel: Some(TravelPhase::Arrived),
            actors_at_send: &seen,
            observation: &view,
        }
    });
    assert_eq!(take, AutoPickup::Take(3));

    view.ground_items.push(item(4, 0, 0, 0, true));
    let menu = decide_autopickup(AutoQuery {
        session_on: true,
        movement_allows: true,
        has_control: true,
        same_branch: true,
        ready: true,
        travel: Some(TravelPhase::Arrived),
        actors_at_send: &seen,
        observation: &view,
    });
    assert_eq!(menu, AutoPickup::Menu);
    assert!(!matches!(menu, AutoPickup::Take(_)));
}

#[test]
fn pickup_requires_reachable_and_the_actors_position() {
    let mut view = observation();
    view.ground_items.push(item(1, 0, 0, 0, false));
    view.ground_items.push(item(2, 1, 0, 0, true));
    view.ground_items.push(item(3, 0, 0, 1, true));
    assert!(feet_items(&view).is_empty());
    view.ground_items.push(item(4, 0, 0, 0, true));
    let feet = feet_items(&view);
    assert_eq!(feet.len(), 1);
    assert_eq!(feet[0].item.id, 4);
    assert!(take_quantity(4, Some(0)).is_err());
    assert!(take_quantity(4, Some(5)).is_err());
    assert_eq!(take_quantity(4, Some(3)).unwrap(), Some(3));
    assert_eq!(take_quantity(4, None).unwrap(), None);
}

#[test]
fn a_full_try_send_retries_and_does_not_disconnect() {
    let mut queue = IntentQueue::new();
    assert!(queue.push_back(Intent::Act {
        action: Action::Wait,
        pickup: false,
    }));
    assert_eq!(classify_try_send(TrySend::Full), SendFate::Retry);
    assert_eq!(queue.len(), 1, "Full leaves the intent queued");
    assert_ne!(classify_try_send(TrySend::Full), SendFate::Disconnect);
    assert_eq!(classify_try_send(TrySend::Closed), SendFate::Disconnect);
    assert_eq!(classify_try_send(TrySend::Accepted), SendFate::InFlight);
    queue.pop_front();
    assert!(queue.is_empty());
}
