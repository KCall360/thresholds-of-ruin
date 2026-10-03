//! Descriptions of the scene, and what single commands mean in it.
use tor_client_common::Palette;
use tor_client_text::narrative::NO_PLACES;
use tor_client_text::{
    adventure::{describe, describe_with},
    engine::{
        resolve::Referents,
        scene::Scene,
        verbs::{interpret as understand, Goal, Interpretation},
    },
    parser::{match_sentence_with_raw, parse_input, tokenize},
    Input,
};
use tor_protocol::*;

fn state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":0,"observation":{
        "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":(0..7).map(|x| serde_json::json!({
            "key":format!("cell-{x}"),"position":{"x":x,"y":0,"z":0},
            "wall":false,"material":"stone","place_hint":x==1 || x==6,
            "stairs_up":false,"stairs_down":false
        })).collect::<Vec<_>>(),
        "ground_items":[
            {"reachable":true,"item":{"quantity":1,"appearance":"item","identified":true,"id":1,"name":"copper token","description":"A small copper disc."},"position":{"x":0,"y":0,"z":0}},
            {"reachable":false,"item":{"quantity":1,"appearance":"item","identified":true,"id":2,"name":"stone tablet","description":"A weathered slab of stone."},"position":{"x":6,"y":0,"z":0}}
        ],"inventory":[],"visible_actors":[]}})).unwrap()
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
        prose.contains("A copper token lies on the floor nearby; a stone tablet lies to the east."),
        "{prose}"
    );
    assert!(!prose.contains("token to the west"));
    for cell in &mut s.observation.visible_cells {
        cell.place_hint = false;
    }
    // Without hints or walls it's all one place.
    assert!(describe(&s).contains("A copper token and a stone tablet lie on the floor nearby."));
    s.observation.ground_items[0].position.z = 1;
    assert!(describe(&s).contains("a copper token is above you"));
}

#[test]
fn actors_occupying_several_cells_are_described_once_by_size() {
    let mut s = state();
    // Add player's own multi-cell body (ActorId(1), height 2, cells (0,0,0) and (0,0,1))
    s.observation.visible_actors.extend([
        ActorView {
            id: ActorId(1),
            name: "delver".into(),
            description: "Your own physical body.".into(),
            position: Position { x: 0, y: 0, z: 0 },
            asset: Some("creature.delver".into()),
        },
        ActorView {
            id: ActorId(1),
            name: "delver".into(),
            description: "Your own physical body.".into(),
            position: Position { x: 0, y: 0, z: 1 },
            asset: Some("creature.delver".into()),
        },
    ]);
    // Add 2-cell humanoid (ActorId(2), height 2, cells (3,0,0) and (3,0,1))
    s.observation.visible_actors.extend([
        ActorView {
            id: ActorId(2),
            name: "scout".into(),
            description: "A nimble scout.".into(),
            position: Position { x: 3, y: 0, z: 0 },
            asset: Some("creature.scout".into()),
        },
        ActorView {
            id: ActorId(2),
            name: "scout".into(),
            description: "A nimble scout.".into(),
            position: Position { x: 3, y: 0, z: 1 },
            asset: Some("creature.scout".into()),
        },
    ]);
    // Add 3-cell high giant (ActorId(3), height 3, cells at (5,0,0), (5,0,1), (5,0,2))
    s.observation.visible_actors.extend([
        ActorView {
            id: ActorId(3),
            name: "giant".into(),
            description: "A huge giant.".into(),
            position: Position { x: 5, y: 0, z: 0 },
            asset: Some("creature.giant".into()),
        },
        ActorView {
            id: ActorId(3),
            name: "giant".into(),
            description: "A huge giant.".into(),
            position: Position { x: 5, y: 0, z: 1 },
            asset: Some("creature.giant".into()),
        },
        ActorView {
            id: ActorId(3),
            name: "giant".into(),
            description: "A huge giant.".into(),
            position: Position { x: 5, y: 0, z: 2 },
            asset: Some("creature.giant".into()),
        },
    ]);
    // Add 2-cell wide beast (ActorId(4), width 2, height 1, cells at (0,3,0) and (1,3,0))
    s.observation.visible_actors.extend([
        ActorView {
            id: ActorId(4),
            name: "beast".into(),
            description: "A wide beast.".into(),
            position: Position { x: 0, y: 3, z: 0 },
            asset: Some("creature.beast".into()),
        },
        ActorView {
            id: ActorId(4),
            name: "beast".into(),
            description: "A wide beast.".into(),
            position: Position { x: 1, y: 3, z: 0 },
            asset: Some("creature.beast".into()),
        },
    ]);

    let prose = describe(&s);
    // Player's own local physical body is filtered out
    assert!(!prose.contains("yourself"));
    assert!(!prose.contains("delver"));
    // A 2-cell humanoid is just a scout, a 3-cell high actor "towering" and
    // a 2-cell wide one "massive"; figures in one place are told together.
    assert!(
        prose.contains(
            "There is a scout and a towering giant to the east, and a massive beast to the south."
        ),
        "{prose}"
    );
}

#[test]
fn your_own_body_is_omitted_but_seen_through_a_portal() {
    let mut s = state();
    // Player local body cells at (0,0,0) and (0,0,1)
    s.observation.visible_actors.extend([
        ActorView {
            id: ActorId(1),
            name: "delver".into(),
            description: "Your own body.".into(),
            position: Position { x: 0, y: 0, z: 0 },
            asset: Some("creature.delver".into()),
        },
        ActorView {
            id: ActorId(1),
            name: "delver".into(),
            description: "Your own body.".into(),
            position: Position { x: 0, y: 0, z: 1 },
            asset: Some("creature.delver".into()),
        },
    ]);
    // Portal self-observation cells seen at (4, 0, 0) and (4, 0, 1)
    s.observation.visible_actors.extend([
        ActorView {
            id: ActorId(1),
            name: "delver".into(),
            description: "You recognize your own appearance from another angle.".into(),
            position: Position { x: 4, y: 0, z: 0 },
            asset: Some("creature.delver".into()),
        },
        ActorView {
            id: ActorId(1),
            name: "delver".into(),
            description: "You recognize your own appearance from another angle.".into(),
            position: Position { x: 4, y: 0, z: 1 },
            asset: Some("creature.delver".into()),
        },
    ]);

    let prose = describe(&s);
    // Local body is not listed as "yourself at your feet" or "yourself above you"
    assert!(!prose.contains("yourself at your feet"));
    assert!(!prose.contains("yourself above you"));
    // Portal loop sighting IS preserved!
    assert!(
        prose.contains("You can see yourself to the east."),
        "{prose}"
    );
}

fn palette(revision: u64, body: PaletteBody) -> PaletteUpdate {
    PaletteUpdate { revision, body }
}

fn carrying(s: &mut StateView) {
    for (id, name, description) in [
        (10, "healing potion", "A vial of bubbling red draught."),
        (11, "iron ration", "Hard tack and dried meat."),
        (12, "iron ring", "A band of cold wrought iron."),
        (13, "iron sword", "A sharp steel blade."),
    ] {
        s.observation.inventory.push(ItemView {
            id,
            name: name.into(),
            appearance: "item".into(),
            identified: true,
            description: description.into(),
            quantity: 1,
            asset: None,
        });
    }
}

fn goblin(s: &mut StateView) {
    s.observation.visible_actors.push(ActorView {
        id: ActorId(42),
        name: "goblin sentry".into(),
        position: Position { x: 1, y: 0, z: 0 },
        description: "A small, snarling goblin.".into(),
        asset: None,
    });
}

/// What one command means in `s`, with the palette given.
fn meaning_with(line: &str, s: &StateView, palette: &Palette) -> Interpretation {
    let scene = Scene::new(s, palette);
    let tokens = tokenize(line);
    let command = match_sentence_with_raw(&tokens, Some(line)).unwrap();
    understand(&command, &scene, &mut Referents::default())
}

fn meaning(line: &str, s: &StateView) -> Interpretation {
    meaning_with(line, s, &Palette::default())
}

fn said(line: &str, s: &StateView) -> String {
    match meaning(line, s) {
        Interpretation::Say(text) => text,
        other => panic!("{line}: expected an answer, got {other:?}"),
    }
}

fn goals(line: &str, s: &StateView) -> Vec<Goal> {
    match meaning(line, s) {
        Interpretation::Goals(goals) => goals,
        other => panic!("{line}: expected goals, got {other:?}"),
    }
}

fn question(line: &str, s: &StateView) -> Vec<String> {
    match meaning(line, s) {
        Interpretation::Ask(q) => q.choices.into_iter().map(|c| c.label).collect(),
        other => panic!("{line}: expected a question, got {other:?}"),
    }
}

fn take(item: u64) -> Goal {
    Goal::Take {
        item,
        quantity: None,
    }
}

#[test]
fn many_verbs_share_an_action_and_the_executor_decides_whether_to_walk() {
    let s = state();
    for line in [
        "take token",
        "get the token",
        "pick up token",
        "grab copper token",
    ] {
        assert_eq!(goals(line, &s), [take(1)], "{line}");
    }
    // Out of reach is still one goal; the journey is the executor's step.
    assert_eq!(goals("take tablet", &s), [take(2)]);
    assert_eq!(said("examine tablet", &s), "A weathered slab of stone.");
    assert_eq!(said("read tablet", &s), "A weathered slab of stone.");
    // A portal's second view of the same thing is not a second thing.
    let mut twice = s.clone();
    let mut repeated = twice.observation.ground_items[1].clone();
    repeated.position.x = -4;
    twice.observation.ground_items.push(repeated);
    assert_eq!(said("examine tablet", &twice), "A weathered slab of stone.");
}

#[test]
fn directions_head_for_ways_onward_and_floor_is_not_one() {
    let mut s = state();
    assert!(matches!(
        goals("east", &s).as_slice(),
        [Goal::Go { direction: Direction::East, destination }] if destination == "cell-6"
    ));
    assert_eq!(said("west", &s), "You can't see a way west.");
    let mut two = state();
    two.observation.visible_cells[4].place_hint = true;
    assert_eq!(question("east", &two).len(), 2);
    for (short, direction) in [("ne", Direction::NorthEast), ("sw", Direction::SouthWest)] {
        assert_eq!(
            goals(&format!("step {short}"), &s),
            [Goal::Step { direction }]
        );
    }
    for c in &mut s.observation.visible_cells {
        c.place_hint = false;
    }
    // With no walls or hints in sight, a direction crosses open ground as far
    // as can be seen that way, and nothing seen is no way.
    assert!(
        describe(&s).contains("You can head east."),
        "{}",
        describe(&s)
    );
    assert!(matches!(
        goals("east", &s).as_slice(),
        [Goal::Go { direction: Direction::East, destination }] if destination == "cell-6"
    ));
    assert_eq!(said("west", &s), "You can't see a way west.");
}

#[test]
fn doors_open_and_close_and_say_when_they_already_are() {
    let mut s = state();
    s.observation.visible_cells[3].door = Some(DoorView {
        asset: None,
        id: 7,
        name: "wooden door".into(),
        description: "An iron handle.".into(),
        open: false,
        reachable: false,
        approaches: vec!["cell-2".into(), "cell-4".into()],
    });
    assert!(describe(&s).contains("A closed wooden door leads east."));
    assert_eq!(said("examine door", &s), "An iron handle. It is closed.");
    assert_eq!(
        goals("open door", &s),
        [Goal::Door {
            door: 7,
            open: true
        }]
    );
    assert_eq!(
        said("close the door", &s),
        "The wooden door is already closed."
    );
    assert_eq!(said("take door", &s), "You can't take the wooden door.");
    let mut second = s.observation.visible_cells[3].door.clone().unwrap();
    second.id = 8;
    s.observation.visible_cells[5].door = Some(second);
    assert_eq!(
        question("open door", &s),
        ["the wooden door to the east", "the wooden door to the east"]
    );
}

#[test]
fn attacks_choose_figures_and_never_an_unseen_id() {
    let mut s = state();
    for id in [2, 3] {
        s.observation.visible_actors.push(ActorView {
            asset: None,
            id: ActorId(id),
            name: "ruin guard".into(),
            description: String::new(),
            position: Position {
                x: id as i32,
                y: 0,
                z: 0,
            },
        });
    }
    assert_eq!(question("attack guard", &s).len(), 2);
    assert_eq!(
        goals("attack the second guard", &s),
        [Goal::Attack { target: ActorId(3) }]
    );
    assert_eq!(said("attack #99", &s), "You can't see any #99 here.");
    assert_eq!(
        said("attack token", &s),
        "Attacking the copper token would achieve nothing."
    );
    assert_eq!(said("attack me", &s), "You'd rather not hurt yourself.");
}

#[test]
fn surfaces_and_unnamed_figures_use_asset_words_the_palette_holds() {
    let mut s = state();
    let here = s.observation.visible_cells[0].clone();
    let solid = |key: &str, position: Position, asset: &str| CellView {
        wall: true,
        key: key.into(),
        position,
        asset: Some(asset.into()),
        door: None,
        ..here.clone()
    };
    s.observation.visible_cells.extend([
        solid(
            "floor",
            Position {
                z: -1,
                ..here.position
            },
            "terrain.floor.cave",
        ),
        solid(
            "wall",
            Position {
                y: 1,
                ..here.position
            },
            "terrain.wall.stone",
        ),
    ]);
    s.observation.visible_actors.push(ActorView {
        name: String::new(),
        description: String::new(),
        id: ActorId(2),
        position: Position {
            x: 3,
            ..here.position
        },
        asset: Some("creature.rat".into()),
    });
    let plain = describe(&s);
    assert!(plain.contains("passage of stone."), "{plain}");
    assert!(plain.contains("a figure"), "{plain}");
    let mut held = Palette::default();
    held.apply(&palette(
        1,
        PaletteBody::Full {
            assets: ["terrain.floor.cave", "terrain.wall.stone", "creature.rat"]
                .map(String::from)
                .into(),
        },
    ));
    let worded = describe_with(&s, &held);
    assert!(worded.contains("a packed earth floor"), "{worded}");
    assert!(worded.contains("walls of dressed stone"), "{worded}");
    assert!(worded.contains("a rat"), "{worded}");
    assert_eq!(
        meaning_with("examine floor", &s, &held),
        Interpretation::Say("The floor is made of packed earth.".into())
    );
    assert_eq!(
        meaning_with("examine rat", &s, &held),
        Interpretation::Say("You see nothing special about the rat.".into())
    );
    assert_eq!(
        said("examine figure", &s),
        "You see nothing special about the figure."
    );
}

#[test]
fn chains_split_into_sentences_and_lists_into_goals() {
    let mut s = state();
    let mut second = s.observation.ground_items[0].clone();
    second.item.id = 5;
    second.item.name = "silver coin".into();
    s.observation.ground_items.push(second);
    assert_eq!(
        parse_input("take token. east. take tablet").unwrap().len(),
        3
    );
    assert_eq!(goals("take token and tablet", &s), [take(1), take(2)]);
    // Within reach first, then the nearest.
    assert_eq!(goals("take all", &s), [take(1), take(5), take(2)]);
    assert_eq!(
        goals("take everything except the coin", &s),
        [take(1), take(2)]
    );
}

#[test]
fn carried_things_drop_and_unbacked_verbs_say_so_plainly() {
    let mut s = state();
    carrying(&mut s);
    goblin(&mut s);
    let drop_sword = [Goal::Drop {
        item: 13,
        quantity: None,
    }];
    assert_eq!(goals("drop the sword", &s), drop_sword);
    assert_eq!(goals("put sword on floor", &s), drop_sword);
    assert_eq!(
        said("put ring in potion", &s),
        "You can't put things anywhere but the floor yet."
    );
    assert_eq!(
        said("drop tablet", &s),
        "You aren't carrying the stone tablet."
    );
    assert_eq!(said("take sword", &s), "You already have the iron sword.");
    assert_eq!(
        said("take goblin", &s),
        "You can't carry the goblin sentry."
    );
    for (line, answer) in [
        ("drink potion", "You can't drink anything yet."),
        ("eat ration", "You can't eat anything yet."),
        ("wear ring", "You can't wear anything yet."),
        ("wield sword", "You can't wield anything yet."),
        ("take off ring", "You can't take anything off yet."),
        ("give ring to goblin", "You can't give anything away yet."),
        ("talk to goblin", "You can't talk with anyone yet."),
        ("ask goblin about key", "You can't ask anyone anything yet."),
        ("unlock door with key", "You can't see any door here."),
        ("push token", "You can't push anything yet."),
        ("throw ring at goblin", "You can't throw anything yet."),
        ("light the lamp", "You can't see any lamp here."),
        ("pray", "You can't pray yet."),
        ("search", "You can't search for hidden things yet."),
    ] {
        assert_eq!(said(line, &s), answer, "{line}");
    }
    // The weapon must be carried; the game has no weapon choice yet.
    assert_eq!(
        goals("attack goblin with sword", &s),
        [Goal::Attack {
            target: ActorId(42)
        }]
    );
    assert_eq!(
        said("attack goblin with axe", &s),
        "You aren't carrying any axe."
    );
}

#[test]
fn condition_and_inventory_are_told_without_numbers_beyond_hp() {
    let mut s = state();
    assert_eq!(said("diagnose", &s), "You feel fine.");
    s.observation.combat = Some(CombatView {
        hp: 18,
        max_hp: 20,
        preparation_remaining: Some(7),
        preparation_active: true,
        recovery_remaining: 3,
        actors: vec![],
        events: vec![],
        objective: None,
        victory: false,
        dead: false,
        terminal: false,
    });
    assert_eq!(
        said("diagnose", &s),
        "You have a few cuts and bruises. (HP 18/20)"
    );
    assert_eq!(
        said("examine me", &s),
        "You have a few cuts and bruises. (HP 18/20)"
    );
    assert_eq!(said("inventory", &s), "You are empty-handed.");
    carrying(&mut s);
    assert_eq!(
        said("i", &s),
        "You are carrying a healing potion, an iron ration, an iron ring and an iron sword."
    );
    assert!(!describe(&s).contains("tick"));
}

#[test]
fn session_commands_and_naming_are_tools() {
    let s = state();
    assert_eq!(
        meaning("name room Vault of Souls", &s),
        Interpretation::Tool(Input::Command(Command::RenamePlace {
            expected_revision: s.revision,
            key: "cell-1".into(),
            name: "Vault of Souls".into(),
        }))
    );
    assert_eq!(
        meaning("note Beware the shadows", &s),
        Interpretation::Tool(Input::Command(Command::Annotate {
            anchor: Anchor::State {
                revision: s.revision
            },
            text: "Beware the shadows".into(),
            source: ClientSource::User,
            audience: Audience::Actor,
            category: AnnotationCategory::Note,
        }))
    );
    for (line, request) in [
        ("save", Request::Save),
        ("sync", Request::Snapshot),
        ("control", Request::AcquireControl),
        ("release", Request::ReleaseControl),
    ] {
        assert_eq!(
            meaning(line, &s),
            Interpretation::Tool(Input::Request(request))
        );
    }
    assert_eq!(meaning("places", &s), Interpretation::Tool(Input::Places));
    assert_eq!(
        meaning("pace 100", &s),
        Interpretation::Tool(Input::Pace(Some(100)))
    );
    let mut named = s.clone();
    named.observation.places.push(PlaceView {
        key: "cell-1".into(),
        name: "Hallowed Crypt".into(),
    });
    assert!(describe(&named).contains("Hallowed Crypt"));
    assert_eq!(
        tor_client_text::places(&named),
        "1. Hallowed Crypt (in sight)"
    );
}

#[test]
fn rooms_are_described_with_an_article_that_fits() {
    // Every epithet, including "echoing", gets the right article.
    let mut s = state();
    for key in 0..64 {
        s.observation.visible_cells[1].key = format!("anchor-{key}");
        let prose = describe(&s);
        assert!(!prose.contains(" a echoing"), "{prose}");
    }
}

/// A walled view from a map: `#` wall, `.` open, `+` closed door, `'` open
/// door, `@` the character, `i` a copper token, `r` a rat, space unseen.
/// Cell keys are map coordinates, so they stay with the cell wherever the
/// character stands, as the server's do.
fn walled(map: &[&str]) -> StateView {
    let at = map
        .iter()
        .enumerate()
        .find_map(|(y, row)| row.find('@').map(|x| (x as i32, y as i32)))
        .unwrap();
    let mut cells = Vec::new();
    let mut items = Vec::new();
    let mut actors = Vec::new();
    for (y, row) in map.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let (x, y) = (x as i32 - at.0, y as i32 - at.1);
            if ch == ' ' {
                continue;
            }
            if ch == 'r' {
                actors.push(serde_json::json!({"id": 50 + actors.len(), "name": "rat",
                    "description": "", "position": {"x": x, "y": y, "z": 0}}));
            }
            if ch == 'i' {
                items.push(serde_json::json!({"reachable": x == 0 && y == 0,
                    "item": {"quantity": 1, "appearance": "item", "identified": true,
                        "id": 1, "name": "copper token", "description": ""},
                    "position": {"x": x, "y": y, "z": 0}}));
            }
            for z in [-1, 0, 1] {
                let solid = ch == '#' || z == -1;
                cells.push(serde_json::json!({
                    "key": format!("{},{},{z}", x + at.0, y + at.1),
                    "position": {"x": x, "y": y, "z": z},
                    "wall": solid,
                    "material": "stone",
                    "place_hint": false, "stairs_up": false, "stairs_down": false,
                    "door": (matches!(ch, '+' | '\'') && z == 0).then(|| serde_json::json!({
                        "id": 9, "name": "oak door", "description": "",
                        "open": ch == '\'', "reachable": false, "approaches": []
                    })),
                }));
            }
        }
    }
    serde_json::from_value(serde_json::json!({
        "wizard_game": false, "revision": 0, "observation": {
            "actor": 1, "tick": 0, "position": {"x": 0, "y": 0, "z": 0},
            "ready": true, "places": [], "visible_cells": cells,
            "ground_items": items, "inventory": [], "visible_actors": actors
        }
    }))
    .unwrap()
}

#[test]
fn a_room_is_described_from_its_own_extent_and_its_openings_are_its_ways() {
    let s = walled(&[
        "#########     ",
        "#.......#     ",
        "#..@....... i ",
        "#.......#     ",
        "#########     ",
    ]);
    let prose = describe(&s);
    assert!(prose.contains(" chamber of stone."), "{prose}");
    // Only the opening is a way; the token beyond it is in another place.
    assert!(prose.contains("A passage leads east."), "{prose}");
    assert!(
        prose.contains("A copper token lies to the east."),
        "{prose}"
    );
    assert!(matches!(
        goals("east", &s).as_slice(),
        [Goal::Go { destination, .. }] if destination == "10,2,0"
    ));
    assert_eq!(said("north", &s), "You can't see a way north.");
}

#[test]
fn a_closed_door_is_a_way_that_is_shut() {
    let s = walled(&["#####", "#.@.+", "#####"]);
    assert_eq!(said("east", &s), "The oak door to the east is closed.");
    assert!(describe(&s).contains("A closed oak door leads east."));
    let open = walled(&["#####  ", "#.@.'..", "#####  "]);
    assert!(describe(&open).contains("An open oak door leads east."));
    assert!(matches!(goals("east", &open).as_slice(), [Goal::Go { .. }]));
}

#[test]
fn a_room_seen_in_part_goes_on_out_of_sight() {
    let s = walled(&["#####", "#.@..", "#...."]);
    let prose = describe(&s);
    assert!(prose.contains("It goes on out of sight"), "{prose}");
    // It isn't called a dead end.
    assert!(!prose.contains("no way out"), "{prose}");
}

#[test]
fn atmosphere_colours_a_place_the_same_way_every_time() {
    let s = walled(&["#####", "#.@.#", "#####"]);
    let prose = describe(&s);
    let mood = tor_client_text::narrative::atmosphere(&s, &Palette::default(), &NO_PLACES);
    // A mood word and the sentences of its theme.
    assert!(
        prose.starts_with(&format!(
            "You are in a narrow, {} passage of stone.",
            mood.mood
        )),
        "{prose}"
    );
    for sentence in &mood.description {
        assert!(prose.contains(sentence), "{prose}");
    }
    assert_eq!(describe(&s), prose);
    // Smell and sound are the same place's.
    assert_eq!(said("smell", &s), mood.smell);
    assert_eq!(said("listen", &s), mood.sound);
    // Listening with others about names them, and claims nothing of them.
    let rats = walled(&["#######", "#.@r.r#", "#######"]);
    let mood = tor_client_text::narrative::atmosphere(&rats, &Palette::default(), &NO_PLACES);
    assert_eq!(
        said("listen", &rats),
        format!("{} You keep an ear on the two rats.", mood.sound)
    );
}

#[test]
fn a_place_reads_the_same_from_anywhere_in_it() {
    // Regression: the atmosphere followed the cell nearest the middle of what
    // was seen, so it changed as the character walked across the room.
    let room = |at: usize| {
        let mut row: Vec<char> = "#.......#".chars().collect();
        row[at] = '@';
        let row: String = row.into_iter().collect();
        walled(&["#########", "#.......#", &row, "#.......#", "#########"])
    };
    let first = tor_client_text::narrative::atmosphere(&room(1), &Palette::default(), &NO_PLACES);
    for at in 2..8 {
        assert_eq!(
            tor_client_text::narrative::atmosphere(&room(at), &Palette::default(), &NO_PLACES),
            first
        );
    }
}

#[test]
fn places_have_varied_atmospheres() {
    // Rooms that differ only in where they are read differently.
    let moods: std::collections::BTreeSet<String> = (0..40)
        .map(|i| {
            let mut s = walled(&["#####", "#.@.#", "#####"]);
            for cell in &mut s.observation.visible_cells {
                cell.key = format!("{i}/{}", cell.key);
            }
            let mood = tor_client_text::narrative::atmosphere(&s, &Palette::default(), &NO_PLACES);
            format!("{} {}", mood.mood, mood.description.join(" "))
        })
        .collect();
    assert!(moods.len() >= 20, "{moods:?}");
}

#[test]
fn ways_are_named_by_kind_and_a_closed_room_has_none() {
    let s = walled(&[
        "####.####",
        "#.......#",
        "#...@...'..",
        "#.......#",
        "####+####",
    ]);
    let prose = describe(&s);
    assert!(
        prose
            .contains("A passage leads north, an open oak door east, and a closed oak door south."),
        "{prose}"
    );
    let shut = walled(&["#####", "#.@.#", "#####"]);
    assert!(describe(&shut).contains("You see no way out."));
}

#[test]
fn things_and_figures_are_told_in_sentences() {
    let mut s = walled(&[
        "#########",
        "#.r.r...#",
        "#..@....#",
        "#.......#",
        "#########",
    ]);
    let token = |id: u64, x: i32, quantity: u64| {
        serde_json::from_value::<GroundItemView>(serde_json::json!({
            "reachable": x == 0, "position": {"x": x, "y": 0, "z": 0},
            "item": {"quantity": quantity, "appearance": "item", "identified": true,
                "id": id, "name": "copper token", "description": ""}}))
        .unwrap()
    };
    s.observation.ground_items = vec![token(1, 0, 1), token(2, 0, 2), token(3, 3, 1)];
    let prose = describe(&s);
    // Two rats to the north: one is up and left, one up and right, so each
    // has its own bearing.
    assert!(
        prose.contains("There is a rat to the northwest, and a rat to the northeast."),
        "{prose}"
    );
    assert!(
        prose.contains(
            "Three copper tokens lie at your feet; another copper token lies on the floor nearby."
        ),
        "{prose}"
    );
    // Injuries go with the names.
    s.observation.combat = Some(
        serde_json::from_value(serde_json::json!({
            "hp": 10, "max_hp": 10, "dead": false, "victory": false, "terminal": false,
            "preparation_remaining": null, "preparation_active": false,
            "recovery_remaining": 0, "events": [], "objective": null,
            "actors": [{"actor": 50, "hostile": true, "injury": "badly_wounded"}]
        }))
        .unwrap(),
    );
    assert!(
        describe(&s).contains("There is a badly wounded rat to the northwest"),
        "{}",
        describe(&s)
    );
}

#[test]
fn a_place_seen_before_is_named_briefly() {
    let s = walled(&["#####", "#.@.'", "#####"]);
    let brief =
        tor_client_text::adventure::brief_place_with(&s, &Palette::default(), &NO_PLACES, true);
    let mood = tor_client_text::narrative::atmosphere(&s, &Palette::default(), &NO_PLACES);
    assert_eq!(
        brief,
        format!(
            "You are back in the narrow, {} passage. An open oak door leads east.",
            mood.mood
        )
    );
    let superbrief =
        tor_client_text::adventure::brief_place_with(&s, &Palette::default(), &NO_PLACES, false);
    assert!(superbrief.starts_with("You are back in"), "{superbrief}");
    assert!(!superbrief.contains("door"), "{superbrief}");
}

#[test]
fn look_leaves_the_objective_to_status() {
    let mut s = walled(&["#####", "#.@.#", "#####"]);
    s.observation.combat = Some(
        serde_json::from_value(serde_json::json!({
            "hp": 7, "max_hp": 10, "dead": false, "victory": false, "terminal": false,
            "preparation_remaining": null, "preparation_active": false,
            "recovery_remaining": 0, "events": [], "objective": "reach_exit", "actors": []
        }))
        .unwrap(),
    );
    assert!(describe(&s).contains("Reach the exit."));
    let look = tor_client_text::adventure::look_with(&s, &Palette::default(), &NO_PLACES);
    assert!(look.starts_with("HP 7/10\n"), "{look}");
    assert!(!look.contains("Reach the exit."), "{look}");
    for line in ["status", "score", "objective"] {
        assert_eq!(
            said(line, &s),
            "You are wounded. (HP 7/10) Reach the exit.",
            "{line}"
        );
    }
    assert_eq!(said("examine me", &s), "You are wounded. (HP 7/10)");
}

#[test]
fn a_carried_thing_and_its_twin_on_the_floor_need_no_question_to_examine() {
    // Regression: "x token" asked "the copper token you're carrying or the
    // copper token at your feet?" though they look the same.
    let mut s = state();
    let token = s.observation.ground_items[0].item.clone();
    s.observation.inventory.push(ItemView { id: 9, ..token });
    assert_eq!(said("examine token", &s), "A small copper disc.");
    // Going to it means the one on the floor; taking it too.
    assert_eq!(
        goals("take token", &s),
        [Goal::Take {
            item: 1,
            quantity: None
        }]
    );
    assert_eq!(said("go to token", &s), "The copper token is right here.");
}

#[test]
fn going_to_a_remembered_place_travels_to_where_it_was_learned() {
    let mut s = state();
    s.observation.places = vec![
        PlaceView {
            key: "cell-1".into(),
            name: "Hollow Promise".into(),
        },
        PlaceView {
            key: "far-away".into(),
            name: "Vault of Whispers".into(),
        },
    ];
    for line in ["go to vault of whispers", "go to the Vault of Whispers"] {
        assert_eq!(
            goals(line, &s),
            [Goal::Visit {
                destination: "far-away".into(),
                name: "Vault of Whispers".into()
            }],
            "{line}"
        );
    }
    // The place the character is in needs no journey.
    assert_eq!(
        said("go to hollow promise", &s),
        "You're already in Hollow Promise."
    );
    // Things in sight still come first when no place has the name.
    assert!(matches!(
        goals("go to tablet", &s).as_slice(),
        [Goal::Approach { .. }]
    ));
}

#[test]
fn a_place_keeps_the_key_it_was_first_seen_under() {
    // A corridor seen in part, then all of it: its lowest cell key changes,
    // but the place remembered keeps its first key, and so its atmosphere.
    let part = walled(&["   ######   ", "   ..@...   ", "   ######   "]);
    let whole = walled(&["############", "........@...", "############"]);
    assert_ne!(NO_PLACES.key(&part), NO_PLACES.key(&whole));
    let mut places = tor_client_text::narrative::Places::default();
    places.learn(&part);
    assert_eq!(places.key(&whole), places.key(&part));
    places.learn(&whole);
    let palette = Palette::default();
    assert_eq!(
        tor_client_text::narrative::atmosphere(&whole, &palette, &places),
        tor_client_text::narrative::atmosphere(&part, &palette, &places)
    );
}
