use tor_client_common::Palette;
use tor_client_text::adventure::{describe, describe_with, Dialogue, Intent};
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
fn remembered_places_list_and_name_without_becoming_travel_destinations() {
    let mut state = state();
    state.observation.places.push(PlaceView {
        key: "offscreen".into(),
        name: "Quiet Reverie".into(),
    });
    assert_eq!(
        tor_client_text::places(&state),
        "1. Quiet Reverie (remembered)"
    );
    let mut dialogue = Dialogue::default();
    assert_eq!(
        dialogue.interpret("places", &state),
        Intent::Tools(tor_client_text::Input::Places)
    );
    assert_eq!(
        dialogue.interpret("name 1 Hearth of Echoes", &state),
        Intent::Tools(tor_client_text::Input::Command(Command::RenamePlace {
            expected_revision: state.revision,
            key: "offscreen".into(),
            name: "Hearth of Echoes".into()
        }))
    );
    assert!(!matches!(
        dialogue.interpret("go to Quiet Reverie", &state),
        Intent::Travel { .. }
    ));
    assert!(tor_client_text::parse("name 9 Missing", &state).is_err());
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
    // Floors and ceilings are seen solid cells: stone below the player, open
    // headroom, then a stone ceiling two cells up.
    let here = s.observation.visible_cells[0].clone();
    let column = |dz: i32, wall: bool| CellView {
        wall,
        material: if wall { "stone".into() } else { String::new() },
        key: format!("column{dz}"),
        position: Position {
            z: here.position.z + dz,
            ..here.position
        },
        door: None,
        ..here.clone()
    };
    s.observation
        .visible_cells
        .extend([column(-1, true), column(1, false), column(2, true)]);
    assert!(describe(&s).contains("stone floor"));
    let mut dialogue = Dialogue::default();
    assert!(
        matches!(dialogue.interpret("examine ceiling", &s), Intent::Say(text) if text.contains("stone"))
    );
    s.observation.visible_cells.retain(|c| c.key != "column2");
    assert!(
        matches!(dialogue.interpret("examine ceiling", &s), Intent::Say(text) if text == "You cannot see that here.")
    );
    s.observation.visible_cells.retain(|c| c.key != "column-1");
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
        matches!(dialogue.interpret("take tablet", &state()), Intent::Travel { destination, take: Some((2, None)), .. } if destination == "cell-6")
    );
    assert!(matches!(
        dialogue.interpret("take token", &state()),
        Intent::Action(Action::Take {
            item: 1,
            quantity: None
        })
    ));
    assert!(
        matches!(dialogue.interpret("examine tablet", &state()), Intent::Say(text) if text == "A weathered slab of stone.")
    );
    assert!(matches!(
        dialogue.interpret("take it", &state()),
        Intent::Travel {
            take: Some((2, None)),
            ..
        }
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
        Intent::Action(Action::Take {
            item: 1,
            quantity: None
        })
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
        Intent::Travel {
            take: Some((2, None)),
            ..
        }
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
        asset: None,
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

#[test]
fn attacks_clarify_visible_names_and_never_select_an_unknown_id() {
    let mut state = state();
    for id in [2, 3] {
        state.observation.visible_actors.push(ActorView {
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
    let mut dialogue = Dialogue::default();
    assert!(matches!(
        dialogue.interpret("attack guard", &state),
        Intent::Say(_)
    ));
    assert_eq!(
        dialogue.interpret("2", &state),
        Intent::Action(Action::Attack { target: ActorId(3) })
    );
    assert_eq!(
        dialogue.interpret("attack #2", &state),
        Intent::Action(Action::Attack { target: ActorId(2) })
    );
    assert!(matches!(
        dialogue.interpret("attack #99", &state),
        Intent::Say(_)
    ));
}

fn palette(revision: u64, body: PaletteBody) -> PaletteUpdate {
    PaletteUpdate { revision, body }
}

#[test]
fn surfaces_and_unnamed_figures_use_asset_words_the_palette_holds() {
    let mut s = state();
    // A cave floor below, a stone wall beside, and an unnamed rat.
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
    // Without a palette, the disclosed materials and the default figure.
    let plain = describe(&s);
    assert!(plain.contains("a stone floor"), "{plain}");
    assert!(plain.contains("walls of stone"), "{plain}");
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
    // No word for the stone wall itself: terrain.wall's.
    assert!(worded.contains("walls of dressed stone"), "{worded}");
    assert!(worded.contains("a rat"), "{worded}");
    assert!(matches!(
        Dialogue::default().interpret_with("examine floor", &s, &held),
        Intent::Say(text) if text == "The visible floor is made of packed earth."
    ));

    // An asset the palette lacks keeps the client's own look.
    held.apply(&palette(
        2,
        PaletteBody::Delta {
            base: 1,
            added: Default::default(),
            removed: ["creature.rat".to_string()].into(),
        },
    ));
    let partial = describe_with(&s, &held);
    assert!(
        partial.contains("a figure") && partial.contains("packed earth"),
        "{partial}"
    );
    // After a missed revision, nothing is drawn from the palette.
    held.apply(&palette(
        9,
        PaletteBody::Delta {
            base: 8,
            added: Default::default(),
            removed: Default::default(),
        },
    ));
    assert_eq!(describe_with(&s, &held), plain);
}

#[test]
fn multi_command_sentence_chains_and_queues_remaining_commands() {
    let s = state();
    let mut dialogue = Dialogue::default();
    let first = dialogue.interpret("take token. east. take tablet", &s);
    assert_eq!(
        first,
        Intent::Action(Action::Take {
            item: 1,
            quantity: None,
        })
    );
    assert_eq!(dialogue.queue.len(), 2);
    assert_eq!(dialogue.queue[0], "east");
    assert_eq!(dialogue.queue[1], "take tablet");
}

#[test]
fn ditransitive_attack_and_unlock_with_carried_items() {
    let mut s = state();
    s.observation.visible_actors.push(ActorView {
        asset: None,
        id: ActorId(2),
        name: "goblin scout".into(),
        description: "A goblin.".into(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let mut dialogue = Dialogue::default();

    // Weapon not carried
    assert_eq!(
        dialogue.interpret("attack goblin with iron sword", &s),
        Intent::Say("You don't have the iron sword.".into())
    );

    // Carry weapon
    s.observation.inventory.push(ItemView {
        asset: None,
        id: 10,
        name: "iron sword".into(),
        description: "A sword.".into(),
        quantity: 1,
        appearance: "sword".into(),
        identified: true,
    });

    // Weapon carried -> attacks goblin
    assert_eq!(
        dialogue.interpret("attack goblin with iron sword", &s),
        Intent::Action(Action::Attack { target: ActorId(2) })
    );

    // Door and key
    s.observation.visible_cells[1].door = Some(DoorView {
        asset: None,
        id: 101,
        name: "oak door".into(),
        description: "A wooden door.".into(),
        open: false,
        reachable: true,
        approaches: vec!["cell-0".into()],
    });

    // Key not carried
    assert_eq!(
        dialogue.interpret("unlock door with brass key", &s),
        Intent::Say("You don't have the brass key.".into())
    );

    // Carry key
    s.observation.inventory.push(ItemView {
        asset: None,
        id: 11,
        name: "brass key".into(),
        description: "A key.".into(),
        quantity: 1,
        appearance: "key".into(),
        identified: true,
    });

    // Key carried -> opens door
    assert_eq!(
        dialogue.interpret("unlock door with brass key", &s),
        Intent::Action(Action::SetDoor {
            door: 101,
            open: true,
        })
    );
}

#[test]
fn take_all_queues_remaining_place_items() {
    let mut s = state();
    let mut second = s.observation.ground_items[0].clone();
    second.item.id = 5;
    second.item.name = "silver coin".into();
    s.observation.ground_items.push(second);

    let mut dialogue = Dialogue::default();
    let first = dialogue.interpret("take all", &s);
    assert_eq!(
        first,
        Intent::Action(Action::Take {
            item: 1,
            quantity: None,
        })
    );
    assert_eq!(dialogue.queue.len(), 1);
    assert_eq!(dialogue.queue[0], "take silver coin");
}

#[test]
fn narrative_place_title_sensory_and_verbosity() {
    let mut s = state();
    let prose = describe(&s);
    assert!(
        prose.contains("Stone Chamber")
            || prose.contains("Stone Hall")
            || prose.contains("Stone Passage")
    );
    assert!(prose.contains("stone floor"));
    assert!(
        prose.contains("cool")
            || prose.contains("chill")
            || prose.contains("Shadows")
            || prose.contains("quiet")
            || prose.contains("air")
    );

    // Authored place name overrides procedural
    s.observation.places.push(PlaceView {
        key: "cell-1".into(),
        name: "Hallowed Crypt".into(),
    });
    assert!(describe(&s).contains("Hallowed Crypt"));

    // Scenery examination
    let mut dialogue = Dialogue::default();
    assert_eq!(
        dialogue.interpret("examine room", &s),
        Intent::Say(describe(&s))
    );
    assert!(matches!(
        dialogue.interpret("listen", &s),
        Intent::Say(text) if text.contains("quiet") || text.contains("air")
    ));
    assert!(matches!(
        dialogue.interpret("smell", &s),
        Intent::Say(text) if text.contains("cool") || text.contains("stone")
    ));
    assert!(matches!(
        dialogue.interpret("search", &s),
        Intent::Say(text) if text.contains("copper token")
    ));

    // Verbosity modes
    assert_eq!(
        dialogue.interpret("verbose", &s),
        Intent::Say("Maximum verbosity.".into())
    );
    assert_eq!(
        dialogue.interpret("brief", &s),
        Intent::Say("Brief descriptions.".into())
    );
    assert_eq!(
        dialogue.interpret("superbrief", &s),
        Intent::Say("Superbrief descriptions.".into())
    );

    // Interactive place naming and notes
    assert_eq!(
        dialogue.interpret("name room Vault of Souls", &s),
        Intent::Tools(tor_client_text::Input::Command(Command::RenamePlace {
            expected_revision: s.revision,
            key: "cell-1".into(),
            name: "Vault of Souls".into(),
        }))
    );
    assert_eq!(
        dialogue.interpret("note Beware the lurking shadows", &s),
        Intent::Tools(tor_client_text::Input::Command(Command::Annotate {
            anchor: Anchor::State {
                revision: s.revision
            },
            text: "Beware the lurking shadows".into(),
            source: ClientSource::User,
            audience: Audience::Actor,
            category: AnnotationCategory::Note,
        }))
    );
}

#[test]
fn test_expanded_if_interactions() {
    let mut s = state();
    let mut dialogue = Dialogue::default();

    // 1. again / g
    assert_eq!(
        dialogue.interpret("again", &s),
        Intent::Say("There is no previous command to repeat.".into())
    );
    assert_eq!(
        dialogue.interpret("g", &s),
        Intent::Say("There is no previous command to repeat.".into())
    );
    assert_eq!(dialogue.interpret("wait", &s), Intent::Action(Action::Wait));
    assert_eq!(
        dialogue.interpret("again", &s),
        Intent::Action(Action::Wait)
    );
    assert_eq!(dialogue.interpret("g", &s), Intent::Action(Action::Wait));

    // 2. diagnose
    assert_eq!(
        dialogue.interpret("diagnose", &s),
        Intent::Say("You are in good health, with no apparent injuries or afflictions.".into())
    );
    s.observation.combat = Some(CombatView {
        hp: 18,
        max_hp: 20,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![],
        messages: vec![],
        objective: None,
        victory: false,
        dead: false,
        terminal: false,
    });
    assert!(matches!(
        dialogue.interpret("diagnose", &s),
        Intent::Say(text) if text.contains("minor cuts") && text.contains("18/20")
    ));
    s.observation.combat = None;

    // 3. read
    assert_eq!(
        dialogue.interpret("read copper token", &s),
        Intent::Say("A small copper disc.".into())
    );
    assert_eq!(
        dialogue.interpret("read floor", &s),
        Intent::Say("There is nothing written there.".into())
    );

    // Give player an inventory
    s.observation.inventory.push(ItemView {
        id: 10,
        name: "healing potion".into(),
        appearance: "item".into(),
        identified: true,
        description: "A vial of bubbling red draught.".into(),
        quantity: 1,
        asset: None,
    });
    s.observation.inventory.push(ItemView {
        id: 11,
        name: "iron ration".into(),
        appearance: "item".into(),
        identified: true,
        description: "Hard tack and dried meat.".into(),
        quantity: 1,
        asset: None,
    });
    s.observation.inventory.push(ItemView {
        id: 12,
        name: "iron ring".into(),
        appearance: "item".into(),
        identified: true,
        description: "A band of cold wrought iron.".into(),
        quantity: 1,
        asset: None,
    });
    s.observation.inventory.push(ItemView {
        id: 13,
        name: "iron sword".into(),
        appearance: "item".into(),
        identified: true,
        description: "A sharp steel blade.".into(),
        quantity: 1,
        asset: None,
    });

    // 4. drink and eat
    assert!(matches!(
        dialogue.interpret("drink healing potion", &s),
        Intent::Say(text) if text.contains("refreshing")
    ));
    assert_eq!(
        dialogue.interpret("drink iron sword", &s),
        Intent::Say("You cannot drink the iron sword.".into())
    );
    assert!(matches!(
        dialogue.interpret("eat iron ration", &s),
        Intent::Say(text) if text.contains("sustains you")
    ));
    assert_eq!(
        dialogue.interpret("eat iron sword", &s),
        Intent::Say("The iron sword is not edible.".into())
    );

    // 5. wear, wield, remove
    assert_eq!(
        dialogue.interpret("wear iron ring", &s),
        Intent::Say("You put on the iron ring.".into())
    );
    assert_eq!(
        dialogue.interpret("put on iron ring", &s),
        Intent::Say("You put on the iron ring.".into())
    );
    assert_eq!(
        dialogue.interpret("remove iron ring", &s),
        Intent::Say("You take off the iron ring.".into())
    );
    assert_eq!(
        dialogue.interpret("take off iron ring", &s),
        Intent::Say("You take off the iron ring.".into())
    );
    assert_eq!(
        dialogue.interpret("wield iron sword", &s),
        Intent::Say("You ready the iron sword for combat.".into())
    );

    // 6. put <item> on floor / ground
    assert_eq!(
        dialogue.interpret("put iron sword on floor", &s),
        Intent::Action(Action::Drop {
            item: 13,
            quantity: None,
        })
    );
    assert_eq!(
        dialogue.interpret("put iron sword in chest", &s),
        Intent::Say("You cannot put the iron sword in the chest.".into())
    );

    // 7. give and talk with actor
    s.observation.visible_actors.push(ActorView {
        id: ActorId(42),
        name: "goblin sentry".into(),
        position: Position { x: 1, y: 0, z: 0 },
        description: "A small, snarling goblin.".into(),
        asset: None,
    });

    assert_eq!(
        dialogue.interpret("give iron ring to goblin", &s),
        Intent::Say("The goblin sentry does not seem interested in the iron ring.".into())
    );
    assert_eq!(
        dialogue.interpret("talk to goblin", &s),
        Intent::Say("The goblin sentry glares warily and offers no reply.".into())
    );
    assert_eq!(
        dialogue.interpret("ask goblin about dungeon", &s),
        Intent::Say(
            "The goblin sentry remains silent, offering no response about the dungeon.".into()
        )
    );
    assert_eq!(
        dialogue.interpret("talk to myself", &s),
        Intent::Say("Talking to yourself is a sure sign of madness.".into())
    );

    // 8. push, pull, turn on door
    s.observation.visible_cells[1].door = Some(DoorView {
        id: 99,
        open: false,
        name: "oak door".into(),
        description: "A heavy timber door.".into(),
        reachable: true,
        approaches: vec!["cell-0".into()],
        asset: None,
    });

    assert_eq!(
        dialogue.interpret("push oak door", &s),
        Intent::Action(Action::SetDoor {
            door: 99,
            open: true,
        })
    );
    assert_eq!(
        dialogue.interpret("turn oak door", &s),
        Intent::Say("Turning the handle does nothing unusual.".into())
    );

    s.observation.visible_cells[1].door.as_mut().unwrap().open = true;
    assert_eq!(
        dialogue.interpret("pull oak door", &s),
        Intent::Action(Action::SetDoor {
            door: 99,
            open: false,
        })
    );
}

#[test]
fn test_conversational_clarification_with_noun_phrases() {
    let mut s = state();
    let mut dialogue = Dialogue::default();

    // Add silver token next to copper token (both reachable at 0,0,0)
    s.observation.ground_items.push(GroundItemView {
        position: Position { x: 0, y: 0, z: 0 },
        reachable: true,
        item: ItemView {
            id: 20,
            name: "silver token".into(),
            appearance: "item".into(),
            identified: true,
            description: "A small silver disc.".into(),
            quantity: 1,
            asset: None,
        },
    });

    // 1. "take token" triggers disambiguation
    let prompt = dialogue.interpret("take token", &s);
    assert_eq!(
        prompt,
        Intent::Say(
            "Which do you mean? 1) copper token (count 1); 2) silver token (count 1)".into()
        )
    );

    // 2. Answer with "the first one"
    assert_eq!(
        dialogue.interpret("the first one", &s),
        Intent::Action(Action::Take {
            item: 1,
            quantity: None,
        })
    );

    // 3. Trigger disambiguation again
    let _ = dialogue.interpret("take token", &s);
    // Answer with adjective/noun "silver"
    assert_eq!(
        dialogue.interpret("silver", &s),
        Intent::Action(Action::Take {
            item: 20,
            quantity: None,
        })
    );

    // 4. Trigger disambiguation again
    let _ = dialogue.interpret("take token", &s);
    // Answer with ordinal + head noun "the 2nd token"
    assert_eq!(
        dialogue.interpret("the 2nd token", &s),
        Intent::Action(Action::Take {
            item: 20,
            quantity: None,
        })
    );

    // 5. Trigger disambiguation again and answer with invalid option
    let _ = dialogue.interpret("take token", &s);
    assert_eq!(
        dialogue.interpret("gold", &s),
        Intent::Say("There is no matching option. Which do you mean? 1) copper token (count 1); 2) silver token (count 1)".into())
    );
    // Then answer with numeric choice "1"
    assert_eq!(
        dialogue.interpret("1", &s),
        Intent::Action(Action::Take {
            item: 1,
            quantity: None,
        })
    );
}

#[test]
fn test_pronoun_reference_flow() {
    let mut s = state();
    let mut dialogue = Dialogue::default();

    s.observation.visible_actors.push(ActorView {
        id: ActorId(42),
        name: "goblin sentry".into(),
        position: Position { x: 1, y: 0, z: 0 },
        description: "A small, snarling goblin.".into(),
        asset: None,
    });

    // Examine goblin sentry establishes actor pronoun
    assert_eq!(
        dialogue.interpret("examine goblin sentry", &s),
        Intent::Say("A small, snarling goblin.".into())
    );
    assert_eq!(dialogue.actor, Some(ActorId(42)));

    // Attack him uses established actor pronoun
    assert_eq!(
        dialogue.interpret("attack him", &s),
        Intent::Action(Action::Attack {
            target: ActorId(42),
        })
    );

    // Ask her about treasure
    assert_eq!(
        dialogue.interpret("ask her about treasure", &s),
        Intent::Say(
            "The goblin sentry remains silent, offering no response about the treasure.".into()
        )
    );

    // Plural items pronoun tracking
    s.observation.ground_items.push(GroundItemView {
        position: Position { x: 0, y: 0, z: 0 },
        reachable: true,
        item: ItemView {
            id: 50,
            name: "iron arrows".into(),
            appearance: "item".into(),
            identified: true,
            description: "A bundle of arrows.".into(),
            quantity: 5,
            asset: None,
        },
    });

    // Take iron arrows sets plural_items
    assert_eq!(
        dialogue.interpret("take iron arrows", &s),
        Intent::Action(Action::Take {
            item: 50,
            quantity: None,
        })
    );
    assert_eq!(dialogue.plural_items, vec![50]);

    // Simulate item now being in player's inventory
    s.observation.inventory.push(ItemView {
        id: 50,
        name: "iron arrows".into(),
        appearance: "item".into(),
        identified: true,
        description: "A bundle of arrows.".into(),
        quantity: 5,
        asset: None,
    });

    // Drop them uses plural_items
    assert_eq!(
        dialogue.interpret("drop them", &s),
        Intent::Action(Action::Drop {
            item: 50,
            quantity: None,
        })
    );
}

#[test]
fn test_session_commands_unified_routing() {
    let s = state();
    let mut dialogue = Dialogue::default();

    assert_eq!(
        dialogue.interpret("save", &s),
        Intent::Tools(tor_client_text::Input::Request(Request::Save))
    );
    assert_eq!(
        dialogue.interpret("sync", &s),
        Intent::Tools(tor_client_text::Input::Request(Request::Snapshot))
    );
    assert_eq!(
        dialogue.interpret("control", &s),
        Intent::Tools(tor_client_text::Input::Request(Request::AcquireControl))
    );
    assert_eq!(
        dialogue.interpret("release", &s),
        Intent::Tools(tor_client_text::Input::Request(Request::ReleaseControl))
    );
    assert_eq!(
        dialogue.interpret("places", &s),
        Intent::Tools(tor_client_text::Input::Places)
    );
    assert_eq!(
        dialogue.interpret("history", &s),
        Intent::Tools(tor_client_text::Input::Request(Request::History {
            before: None,
            limit: 50,
        }))
    );
    assert_eq!(
        dialogue.interpret("history 100", &s),
        Intent::Tools(tor_client_text::Input::Request(Request::History {
            before: Some(EntryId("100".into())),
            limit: 50,
        }))
    );
    assert_eq!(
        dialogue.interpret("note Secret Passage Behind Rug", &s),
        Intent::Tools(tor_client_text::Input::Command(Command::Annotate {
            anchor: Anchor::State {
                revision: s.revision
            },
            text: "Secret Passage Behind Rug".into(),
            source: ClientSource::User,
            audience: Audience::Actor,
            category: AnnotationCategory::Note,
        }))
    );
    assert_eq!(
        dialogue.interpret("wizard teleport 1 2 3", &s),
        Intent::Tools(tor_client_text::Input::Command(Command::Wizard {
            expected_revision: s.revision,
            operation: "teleport 1 2 3".into(),
        }))
    );
}
