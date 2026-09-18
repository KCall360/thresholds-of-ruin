use tor_client_text::adventure::{describe, Dialogue, Intent};
use tor_protocol::*;

fn state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":0,"observation":{
        "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},"ready":true,
        "visible_cells":(0..7).map(|x| serde_json::json!({
            "key":format!("cell-{x}"),"position":{"x":x,"y":0,"z":0},
            "wall":false,"material":"stone","place_hint":x==1 || x==6,
            "stairs_up":false,"stairs_down":false
        })).collect::<Vec<_>>(),
        "ground_items":[
            {"reachable":true,"item":{"id":1,"name":"copper token","description":"A small copper disc."},"position":{"x":0,"y":0,"z":0}},
            {"reachable":false,"item":{"id":2,"name":"stone tablet","description":"A weathered slab of stone."},"position":{"x":6,"y":0,"z":0}}
        ],"inventory":[],"visible_actors":[]}})).unwrap()
}

#[test]
fn diagonal_steps_and_visible_destination_bearings() {
    let mut s = state();
    let mut dialogue = Dialogue::default();
    for (short, long, direction) in [
        ("ne", "northeast", Direction::NorthEast),
        ("se", "southeast", Direction::SouthEast),
        ("sw", "southwest", Direction::SouthWest),
        ("nw", "northwest", Direction::NorthWest),
    ] {
        for name in [short, long] {
            assert_eq!(
                dialogue.interpret(&format!("step {name}"), &s),
                Intent::Action(Action::Move { direction })
            );
        }
    }
    s.observation.visible_cells.last_mut().unwrap().position = Position { x: 3, y: -3, z: 0 };
    s.observation.ground_items[1].position = Position { x: 3, y: -3, z: 0 };
    assert!(describe(&s).contains("northeast"));
    assert!(matches!(
        dialogue.interpret("ne", &s),
        Intent::Travel {
            direction: Some(Direction::NorthEast),
            ..
        }
    ));
}

#[test]
fn ordinary_prose_has_objects_and_ways_without_debug_metadata() {
    let prose = describe(&state());
    assert!(prose.contains("stone"));
    assert!(prose.contains("copper token"));
    assert!(prose.contains("east"));
    for debug in ["offset", "tick", "#1", "cell-", "region", "Ready."] {
        assert!(!prose.contains(debug), "{prose}");
    }
}

#[test]
fn enclosure_prose_uses_disclosed_surfaces_and_does_not_invent_missing_ones() {
    let mut s = state();
    for c in &mut s.observation.visible_cells {
        c.material.clear();
    }
    let here = &mut s.observation.visible_cells[0];
    here.floor = Some(SurfaceView {
        material: "stone".into(),
        distance: 1,
    });
    here.ceiling = Some(SurfaceView {
        material: "stone".into(),
        distance: 2,
    });
    assert!(describe(&s).contains("stone floor"));
    let mut dialogue = Dialogue::default();
    assert!(
        matches!(dialogue.interpret("examine ceiling", &s), Intent::Say(text) if text.contains("stone"))
    );
    s.observation.visible_cells[0].ceiling = None;
    assert!(
        matches!(dialogue.interpret("examine ceiling", &s), Intent::Say(text) if text == "You cannot see that here.")
    );
    s.observation.visible_cells[0].floor = None;
    assert!(!describe(&s).contains("floor."));
}

#[test]
fn items_in_the_current_place_are_nearby_but_other_places_keep_directions() {
    let mut s = state();
    // Move within the first place, leaving the token behind.
    for cell in &mut s.observation.visible_cells {
        cell.position.x -= 2;
    }
    for item in &mut s.observation.ground_items {
        item.position.x -= 2;
        item.reachable = false;
    }
    let prose = describe(&s);
    assert!(
        prose.contains("You see a copper token on the floor nearby."),
        "{prose}"
    );
    assert!(
        prose.contains("You see a stone tablet to the east."),
        "{prose}"
    );
    assert!(!prose.contains("token to the west"));
    for cell in &mut s.observation.visible_cells {
        cell.place_hint = false;
    }
    assert!(describe(&s).contains("copper token on the floor nearby"));
    s.observation.ground_items[0].position.z = 1;
    assert!(describe(&s).contains("copper token above you"));
}

#[test]
fn directions_travel_to_another_anchor_and_take_approaches_an_item() {
    let mut dialogue = Dialogue::default();
    assert!(
        matches!(dialogue.interpret("east", &state()), Intent::Travel { destination, take: None, .. } if destination == "cell-6")
    );
    assert!(
        matches!(dialogue.interpret("take tablet", &state()), Intent::Travel { destination, take: Some(2), .. } if destination == "cell-6")
    );
    assert!(matches!(
        dialogue.interpret("take token", &state()),
        Intent::Action(Action::Take { item: 1 })
    ));
    assert!(
        matches!(dialogue.interpret("examine tablet", &state()), Intent::Say(text) if text == "A weathered slab of stone.")
    );
    assert!(matches!(
        dialogue.interpret("take it", &state()),
        Intent::Travel { take: Some(2), .. }
    ));
}

#[test]
fn noun_clarification_is_conversational_free_and_invalidated_by_changes() {
    let mut s = state();
    let mut second = s.observation.ground_items[0].clone();
    second.item.id = 3;
    second.item.name = "silver token".into();
    s.observation.ground_items.push(second);
    let mut d = Dialogue::default();
    assert!(
        matches!(d.interpret("take token", &s), Intent::Say(text) if text.contains("Which") && !text.contains('#'))
    );
    assert!(matches!(
        d.interpret("the copper one", &s),
        Intent::Action(Action::Take { item: 1 })
    ));
    d.interpret("take token", &s);
    s.revision += 1;
    assert!(matches!(d.interpret("the silver one", &s), Intent::Say(_)));
}

#[test]
fn ordinary_floor_is_not_an_exit_and_the_current_anchor_is_not_a_destination() {
    let mut s = state();
    s.observation.visible_cells[1].place_hint = false;
    s.observation.visible_cells[2].place_hint = true;
    let prose = describe(&s);
    assert!(prose.contains("You can head east."), "{prose}");
    assert!(!prose.contains("west"));
    assert!(
        matches!(Dialogue::default().interpret("west", &s), Intent::Say(text) if text == "You can't see a way west.")
    );
    assert!(
        matches!(Dialogue::default().interpret("east", &s), Intent::Travel { destination, .. } if destination == "cell-6")
    );
    for c in &mut s.observation.visible_cells {
        c.place_hint = false;
    }
    assert!(!describe(&s).contains("You can head"));
    assert!(matches!(
        Dialogue::default().interpret("east", &s),
        Intent::Say(_)
    ));
    assert!(matches!(
        Dialogue::default().interpret("get tablet", &s),
        Intent::Travel { take: Some(2), .. }
    ));
}

#[test]
fn repeated_portal_views_do_not_create_noun_ambiguity() {
    let mut s = state();
    let mut repeated = s.observation.ground_items[1].clone();
    repeated.position.x = -4;
    s.observation.ground_items.push(repeated);
    assert!(
        matches!(Dialogue::default().interpret("examine tablet", &s), Intent::Say(text) if text == "A weathered slab of stone.")
    );
}

#[test]
fn multiple_places_ask_before_travel_and_a_missing_referent_is_not_used() {
    let mut s = state();
    s.observation.visible_cells[4].place_hint = true;
    let mut d = Dialogue::default();
    assert!(matches!(d.interpret("east", &s), Intent::Say(text) if text.contains("Which")));
    assert!(
        matches!(d.interpret("2", &s), Intent::Travel {destination,..} if destination == "cell-6")
    );
    d.interpret("examine tablet", &s);
    s.observation.ground_items.retain(|i| i.item.id != 2);
    assert!(matches!(d.interpret("take it", &s), Intent::Say(text) if text.contains("cannot see")));
    d.reset();
    assert!(matches!(d.interpret("take it", &state()), Intent::Say(_)));
}

#[test]
fn doors_are_examined_clarified_and_approached_without_entering_the_barrier() {
    let mut s = state();
    s.observation.visible_cells[3].door = Some(DoorView {
        id: 7,
        name: "wooden door".into(),
        description: "An iron handle.".into(),
        open: false,
        reachable: false,
        approaches: vec!["cell-2".into(), "cell-4".into()],
    });
    let mut d = Dialogue::default();
    assert!(describe(&s).contains("closed wooden door"));
    assert!(tor_client_text::describe(&s).contains("wooden door (#7)"));
    assert!(
        matches!(d.interpret("examine door", &s), Intent::Say(text) if text.contains("iron handle") && text.contains("closed"))
    );
    assert!(
        matches!(d.interpret("open it", &s), Intent::Travel { destination, door: Some((7, true)), .. } if destination == "cell-2")
    );
    s.observation.visible_cells[3]
        .door
        .as_mut()
        .unwrap()
        .reachable = true;
    assert_eq!(
        d.interpret("open door", &s),
        Intent::Action(Action::SetDoor {
            door: 7,
            open: true
        })
    );
    let mut second = s.observation.visible_cells[3].door.clone().unwrap();
    second.id = 8;
    s.observation.visible_cells[5].door = Some(second);
    assert!(matches!(d.interpret("open door", &s), Intent::Say(text) if text.contains("Which")));
    assert_eq!(
        d.interpret("2", &s),
        Intent::Action(Action::SetDoor {
            door: 8,
            open: true
        })
    );
    s.revision += 1;
    assert!(matches!(d.interpret("take door", &s), Intent::Say(_)));
}
