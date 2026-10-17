//! Descriptions of the scene, and what single commands mean in it.
use tor_client_common::test_support::equipment_slot;
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
        "wizard_game":false,"revision":"0","observation":{
        "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":(0..7).map(|x| serde_json::json!({
            "key":format!("cell-{x}"),"position":{"x":x,"y":0,"z":0},
            "wall":false,"material":"stone","place_hint":x==1 || x==6,
            "stairs_up":false,"stairs_down":false
        })).collect::<Vec<_>>(),
        "ground_items":[
            {"supply_reachable":false,"reachable":true,"item":{"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(1),"name":"copper token","description":"A small copper disc."},"position":{"x":0,"y":0,"z":0}},
            {"supply_reachable":false,"reachable":false,"item":{"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(2),"name":"stone tablet","description":"A weathered slab of stone."},"position":{"x":6,"y":0,"z":0}}
        ],"inventory":[],"linked_holds":[],"visible_actors":[]}})).unwrap()
}

#[test]
fn stats_inspection_is_a_local_command_in_both_text_parsers() {
    let s = state();
    assert_eq!(tor_client_text::parse("stats", &s), Ok(Input::Stats));
    let parsed = parse_input("stats").unwrap();
    let palette = Palette::default();
    let scene = Scene::new(&s, &palette);
    assert_eq!(
        understand(&parsed[0], &scene, &mut Referents::default()),
        Interpretation::Say("No creature stats are available.".into())
    );
}

#[test]
fn starter_catalog_help_promotes_current_granted_techniques() {
    let s = state();
    let before = s.clone();
    let help = said("help", &s);
    for command in [
        "rattling blow <creature>",
        "measured cut <creature>",
        "brace for impact",
        "spark lance fire <creature>",
        "falter <creature>",
        "shell impact",
    ] {
        assert!(
            help.contains(command),
            "starter catalog help must advertise current command: {command}"
        );
    }
    assert!(help.contains("if granted"));
    for retired in ["power strike", "magic bolt", "fear <creature>"] {
        assert!(!help.contains(retired), "retired help command: {retired}");
    }
    assert_eq!(s, before);
}

#[test]
fn starter_catalog_names_select_only_disclosed_variants_in_both_text_modes() {
    let mut s = state();
    let mut abilities = Vec::new();
    for (id, name, self_only) in [
        ("rattling_blow", "Rattling Blow", false),
        ("measured_cut", "Measured Cut", false),
        ("brace_for_impact", "Brace for Impact", true),
        ("spark_lance_fire", "Spark Lance (Fire)", false),
        ("falter", "Falter", false),
        ("shell_impact", "Shell (Impact)", true),
    ] {
        let mut ability = tor_client_common::test_support::ability(id);
        ability.name = name.into();
        if self_only {
            ability.reach = AbilityReach::SelfOnly;
            ability.target_kind = AbilityTargetKind::SelfOnly;
        }
        abilities.push(ability);
    }
    s.observation.combat = Some(super::creature_combat(abilities));
    goblin(&mut s);
    let before = s.clone();
    for (command, ability, target) in [
        (
            "rattling blow goblin",
            "rattling_blow",
            super::actor_target(42),
        ),
        (
            "measured cut goblin",
            "measured_cut",
            super::actor_target(42),
        ),
        (
            "brace for impact",
            "brace_for_impact",
            s.observation.self_target,
        ),
        (
            "brace for impact me",
            "brace_for_impact",
            s.observation.self_target,
        ),
        (
            "spark lance fire goblin",
            "spark_lance_fire",
            super::actor_target(42),
        ),
        ("falter goblin", "falter", super::actor_target(42)),
        ("shell impact", "shell_impact", s.observation.self_target),
        ("shell impact me", "shell_impact", s.observation.self_target),
    ] {
        assert_eq!(
            goals(command, &s),
            [Goal::UseAbility {
                ability: ability.into(),
                invocation: AbilityInvocation::ActorTarget { target },
                supplies: Default::default(),
            }],
            "{command}"
        );
        assert!(
            matches!(tor_client_text::parse(command, &s),
            Ok(Input::Command(Command::Act { action: Action::UseAbility {
                id, invocation: AbilityInvocation::ActorTarget { target: selected }, ..
            }, .. })) if id == ability && selected == target),
            "{command}"
        );
    }
    for undisclosed in [
        "spark lance cold goblin",
        "shell energy me",
        "power strike goblin",
    ] {
        assert!(tor_client_text::parse(undisclosed, &s).is_err());
        assert!(!said(undisclosed, &s).is_empty());
    }
    assert_eq!(s, before);
}

#[test]
fn ability_verbs_select_disclosed_figures_and_keep_unfunded_admission_possible() {
    let mut s = state();
    s.observation.combat = Some(super::creature_combat(vec![
        tor_client_common::test_support::ability("basic_melee"),
        tor_client_common::test_support::ability("power_strike"),
        tor_client_common::test_support::ability("magic_bolt"),
        tor_client_common::test_support::ability("fear"),
    ]));
    let target = super::actor_target(42);
    s.observation.visible_actors.push(ActorView {
        asset: None,
        id: target,
        name: "goblin".into(),
        description: String::new(),
        position: Position { x: 6, y: 0, z: 0 },
    });
    let before = s.clone();
    for (verb, ability) in [
        ("powerstrike", "power_strike"),
        ("magic bolt", "magic_bolt"),
        ("fear", "fear"),
    ] {
        assert_eq!(
            goals(&format!("{verb} goblin"), &s),
            [Goal::UseAbility {
                ability: ability.into(),
                invocation: tor_protocol::AbilityInvocation::ActorTarget { target },
                supplies: Default::default(),
            }]
        );
        assert!(
            matches!(tor_client_text::parse(&format!("{verb} goblin"), &s), Ok(Input::Command(Command::Act { action: Action::UseAbility { id: selected, invocation: tor_protocol::AbilityInvocation::ActorTarget { target: selected_target }, ..}, .. })) if selected == ability && selected_target == target)
        );
        assert!(
            tor_client_text::parse(&format!("{verb} #{}", super::actor_target(99)), &s).is_err()
        );
    }
    assert_eq!(s, before);
    for (command, ability) in [
        ("power strike goblin", "power_strike"),
        ("magic bolt goblin", "magic_bolt"),
    ] {
        assert_eq!(
            goals(command, &s),
            [Goal::UseAbility {
                ability: ability.into(),
                invocation: tor_protocol::AbilityInvocation::ActorTarget { target },
                supplies: Default::default(),
            }]
        );
        assert!(
            matches!(tor_client_text::parse(command, &s), Ok(Input::Command(Command::Act { action: Action::UseAbility { id: selected, .. }, .. })) if selected == ability)
        );
    }
    s.observation
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![tor_client_common::test_support::ability("basic_melee")];
    assert!(!said("fear goblin", &s).is_empty());
    assert!(tor_client_text::parse("fear goblin", &s).is_err());
}

#[test]
fn carried_equipment_verbs_resolve_disclosed_items_and_validate_current_slots() {
    let mut s = state();
    carrying(&mut s);
    let ring = super::item_target(12);
    s.observation.interactions = Some(InteractionView {
        completed: Vec::new(),
        slots: vec![
            equipment_slot("ring", "on a finger", false),
            equipment_slot("ring", "on a finger", false),
        ],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item: ring,
            slot: Some(equipment_slot("ring", "on a finger", false)),
            equipped_slot: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
        }],
    });
    assert_eq!(
        goals("wear ring", &s),
        [Goal::UseItem {
            item: ring,
            operation: tor_client_common::items::ItemOperation::Equip,
        }]
    );
    assert_eq!(said("drink ring", &s), "You can't drink that item.");
    assert_eq!(said("remove ring", &s), "That item isn't equipped.");
    s.observation.interactions.as_mut().unwrap().inventory[0].equipped_slot = Some(0);
    assert_eq!(
        goals("take off ring", &s),
        [Goal::UseItem {
            item: ring,
            operation: tor_client_common::items::ItemOperation::Unequip,
        }]
    );
    assert_eq!(said("wear ring", &s), "That item is already equipped.");
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
            id: super::actor_target(1),
            name: "delver".into(),
            description: "Your own physical body.".into(),
            position: Position { x: 0, y: 0, z: 0 },
            asset: Some("creature.delver".into()),
        },
        ActorView {
            id: super::actor_target(1),
            name: "delver".into(),
            description: "Your own physical body.".into(),
            position: Position { x: 0, y: 0, z: 1 },
            asset: Some("creature.delver".into()),
        },
    ]);
    // Add 2-cell humanoid (ActorId(2), height 2, cells (3,0,0) and (3,0,1))
    s.observation.visible_actors.extend([
        ActorView {
            id: super::actor_target(2),
            name: "scout".into(),
            description: "A nimble scout.".into(),
            position: Position { x: 3, y: 0, z: 0 },
            asset: Some("creature.scout".into()),
        },
        ActorView {
            id: super::actor_target(2),
            name: "scout".into(),
            description: "A nimble scout.".into(),
            position: Position { x: 3, y: 0, z: 1 },
            asset: Some("creature.scout".into()),
        },
    ]);
    // Add 3-cell high giant (ActorId(3), height 3, cells at (5,0,0), (5,0,1), (5,0,2))
    s.observation.visible_actors.extend([
        ActorView {
            id: super::actor_target(3),
            name: "giant".into(),
            description: "A huge giant.".into(),
            position: Position { x: 5, y: 0, z: 0 },
            asset: Some("creature.giant".into()),
        },
        ActorView {
            id: super::actor_target(3),
            name: "giant".into(),
            description: "A huge giant.".into(),
            position: Position { x: 5, y: 0, z: 1 },
            asset: Some("creature.giant".into()),
        },
        ActorView {
            id: super::actor_target(3),
            name: "giant".into(),
            description: "A huge giant.".into(),
            position: Position { x: 5, y: 0, z: 2 },
            asset: Some("creature.giant".into()),
        },
    ]);
    // Add 2-cell wide beast (ActorId(4), width 2, height 1, cells at (0,3,0) and (1,3,0))
    s.observation.visible_actors.extend([
        ActorView {
            id: super::actor_target(4),
            name: "beast".into(),
            description: "A wide beast.".into(),
            position: Position { x: 0, y: 3, z: 0 },
            asset: Some("creature.beast".into()),
        },
        ActorView {
            id: super::actor_target(4),
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
            id: super::actor_target(1),
            name: "delver".into(),
            description: "Your own body.".into(),
            position: Position { x: 0, y: 0, z: 0 },
            asset: Some("creature.delver".into()),
        },
        ActorView {
            id: super::actor_target(1),
            name: "delver".into(),
            description: "Your own body.".into(),
            position: Position { x: 0, y: 0, z: 1 },
            asset: Some("creature.delver".into()),
        },
    ]);
    // Portal self-observation cells seen at (4, 0, 0) and (4, 0, 1)
    s.observation.visible_actors.extend([
        ActorView {
            id: super::actor_target(1),
            name: "delver".into(),
            description: "You recognize your own appearance from another angle.".into(),
            position: Position { x: 4, y: 0, z: 0 },
            asset: Some("creature.delver".into()),
        },
        ActorView {
            id: super::actor_target(1),
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
            class: Default::default(),
            id: super::item_target(id),
            name: name.into(),
            appearance: "item".into(),
            identified: true,
            description: description.into(),
            quantity: 1,
            asset: None,
            known_supply: None,
        });
    }
}

fn goblin(s: &mut StateView) {
    s.observation.visible_actors.push(ActorView {
        id: super::actor_target(42),
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
    let command =
        tor_client_text::parser::match_disclosed_sentence(&tokens, line, &s.observation).unwrap();
    understand(&command, &scene, &mut Referents::default())
}

fn meaning(line: &str, s: &StateView) -> Interpretation {
    meaning_with(line, s, &Palette::default())
}

fn said(line: &str, s: &StateView) -> String {
    match meaning(line, s) {
        Interpretation::Say(text) | Interpretation::Refuse(text) => text,
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
        item: super::item_target(item),
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
    assert_eq!(
        said("examine tablet", &s),
        "A weathered slab of stone. It lies to the east."
    );
    assert_eq!(said("read tablet", &s), "A weathered slab of stone.");
    // A portal's second view of the same thing is not a second thing.
    let mut twice = s.clone();
    let mut repeated = twice.observation.ground_items[1].clone();
    repeated.position.x = -4;
    twice.observation.ground_items.push(repeated);
    assert_eq!(
        said("examine tablet", &twice),
        "A weathered slab of stone. It lies to the west."
    );
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
        describe(&s).contains("To the east it continues beyond your view."),
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
        id: super::door_target(7),
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
            door: super::door_target(7),
            open: true
        }]
    );
    assert_eq!(
        said("close the door", &s),
        "The wooden door is already closed."
    );
    assert_eq!(said("take door", &s), "You can't take the wooden door.");
    let mut second = s.observation.visible_cells[3].door.clone().unwrap();
    second.id = super::door_target(8);
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
            id: super::actor_target(id),
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
        [Goal::Attack {
            target: super::actor_target(3)
        }]
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
        id: super::actor_target(2),
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
        Interpretation::Say("The rat is to the east.".into())
    );
    assert_eq!(said("examine figure", &s), "The figure is to the east.");
}

#[test]
fn shared_asset_nouns_preserve_authored_names_and_parser_handles() {
    let mut s = state();
    s.observation.visible_actors.push(ActorView {
        name: String::new(),
        description: String::new(),
        id: super::actor_target(2),
        position: Position { x: 3, y: 0, z: 0 },
        asset: Some("creature.equine.quagga.unfamiliar".into()),
    });
    let mut held = Palette::default();
    held.apply(&palette(
        1,
        PaletteBody::Full {
            assets: ["creature.equine.quagga.unfamiliar".into()].into(),
        },
    ));
    assert!(describe_with(&s, &held).contains("a quagga"));
    assert_eq!(
        meaning_with("examine quagga", &s, &held),
        Interpretation::Say("The quagga is to the east.".into())
    );
    s.observation.visible_actors[0].name = "Copper King".into();
    assert!(describe_with(&s, &held).contains("Copper King"));
    assert_eq!(
        meaning_with("examine copper king", &s, &held),
        Interpretation::Say("The copper king is to the east.".into())
    );
    held.apply(&palette(
        3,
        PaletteBody::Delta {
            base: 2,
            added: Default::default(),
            removed: Default::default(),
        },
    ));
    assert!(describe_with(&s, &held).contains("Copper King"));
    s.observation.visible_actors[0].name.clear();
    assert!(describe_with(&s, &held).contains("a figure"));
}

#[test]
fn chains_split_into_sentences_and_lists_into_goals() {
    let mut s = state();
    let mut second = s.observation.ground_items[0].clone();
    second.item.id = super::item_target(5);
    second.item.name = "silver coin".into();
    s.observation.ground_items.push(second);
    assert_eq!(
        parse_input("take token. east. take tablet").unwrap().len(),
        3
    );
    assert_eq!(goals("take token and tablet", &s), [take(1), take(2)]);
    // Within reach first, then the nearest; only from this place, so one
    // command never walks off to another. Naming a thing there still works.
    assert_eq!(goals("take all", &s), [take(1), take(5)]);
    assert_eq!(goals("take everything except the coin", &s), [take(1)]);
    assert_eq!(goals("take tablet", &s), [take(2)]);
}

#[test]
fn carried_things_drop_and_unusable_actions_are_refused() {
    let mut s = state();
    carrying(&mut s);
    goblin(&mut s);
    let drop_sword = [Goal::Drop {
        item: super::item_target(13),
        quantity: None,
    }];
    assert_eq!(goals("drop the sword", &s), drop_sword);
    assert_eq!(goals("put sword on floor", &s), drop_sword);
    assert_eq!(
        said("put ring in potion", &s),
        "There's nowhere here to put things but the floor."
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
        ("drink potion", "That item has no usable interaction."),
        ("eat ration", "That item has no usable interaction."),
        ("wear ring", "That item has no usable interaction."),
        ("wield sword", "That item has no usable interaction."),
        ("take off ring", "That item has no usable interaction."),
        ("give ring to goblin", "No one here is interested."),
        ("talk to goblin", "No one here seems inclined to talk."),
        (
            "ask goblin about key",
            "No one here seems inclined to talk.",
        ),
        ("unlock door with key", "You can't see any door here."),
        ("push token", "That would achieve nothing."),
        (
            "throw ring at goblin",
            "You'd rather keep hold of it; use attack to fight.",
        ),
        ("light the lamp", "You can't see any lamp here."),
        (
            "pray",
            "You murmur a prayer. If anything hears it, it gives no sign.",
        ),
        (
            "search",
            "You search, but find nothing you hadn't already seen.",
        ),
    ] {
        assert_eq!(said(line, &s), answer, "{line}");
    }
    // The weapon must be carried; the game has no weapon choice yet.
    assert_eq!(
        goals("attack goblin with sword", &s),
        [Goal::Attack {
            target: super::actor_target(42)
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
        public_plan: None,
        sequence: None,
        own_stats: None,
        hp: 18,
        max_hp: 20,
        preparation_remaining: Some(7),
        preparation_active: true,
        recovery_remaining: 3,
        actors: vec![],
        events: vec![],
        objective: None,
        exit: None,
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
        origin: PlaceNameOrigin::Authored,
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
pub(crate) fn walled(map: &[&str]) -> StateView {
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
                actors.push(
                    serde_json::json!({"id": super::actor_target(50 + actors.len() as u64), "name": "rat",
                    "description": "", "position": {"x": x, "y": y, "z": 0}}),
                );
            }
            if ch == 'i' {
                items.push(serde_json::json!({"supply_reachable":false,"reachable": x == 0 && y == 0,
                    "item": {"known_supply":null,"quantity": "1", "class":"misc","appearance": "item", "identified": true,
                        "id":super::item_target(1), "name": "copper token", "description": ""},
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
                        "id":super::door_target(9), "name": "oak door", "description": "",
                        "open": ch == '\'', "reachable": false, "approaches": []
                    })),
                }));
            }
        }
    }
    serde_json::from_value(serde_json::json!({
        "wizard_game": false, "revision": "0", "observation": {
            "actor": "1", "self_target":super::actor_target(1),"tick": "0", "position": {"x": 0, "y": 0, "z": 0},
            "ready": true, "places": [], "visible_cells": cells,
            "ground_items": items, "inventory": [], "linked_holds":[],"visible_actors": actors
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
    assert!(prose.contains("it continues beyond your view."), "{prose}");
    // Each way into the dark can be walked, toward the farthest seen floor.
    assert!(matches!(
        goals("east", &s).as_slice(),
        [Goal::Go {
            direction: Direction::East,
            ..
        }]
    ));
    // It isn't called a dead end.
    assert!(!prose.contains("no way onward"), "{prose}");
}

#[test]
fn atmosphere_colours_a_place_the_same_way_every_time() {
    let s = walled(&["#####", "#.@.#", "#####"]);
    let described = describe(&s);
    let mood = tor_client_text::narrative::atmosphere(&s, &Palette::default(), &NO_PLACES);
    // An unnamed place is titled by what it is, then described.
    let (title, prose) = described.split_once('\n').unwrap();
    assert_eq!(title, format!("Narrow, {} passage", mood.mood));
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
    assert_eq!(describe(&s), described);
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
    assert!(describe(&shut).contains("You see no way onward."));
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
        serde_json::from_value::<GroundItemView>(serde_json::json!({"supply_reachable":false,
            "reachable": x == 0, "position": {"x": x, "y": 0, "z": 0},
            "item": {"known_supply":null,"quantity": quantity.to_string(), "class":"misc","appearance": "item", "identified": true,
                "id": super::item_target(id), "name": "copper token", "description": ""}}))
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
            "public_plan":null,"hp": "10", "max_hp": "10", "dead": false, "victory": false, "terminal": false,
            "sequence":null,"preparation_remaining": null, "preparation_active": false,
            "recovery_remaining": "0", "events": [], "objective": null,
            "actors": [{"actor": super::actor_target(50), "hostile": true, "allied": false, "injury": "badly_wounded"}]
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
            "public_plan":null,"hp": "7", "max_hp": "10", "dead": false, "victory": false, "terminal": false,
            "sequence":null,"preparation_remaining": null, "preparation_active": false,
            "recovery_remaining": "0", "events": [], "objective": "reach_exit", "actors": []
        }))
        .unwrap(),
    );
    assert!(describe(&s).contains("Find your way to the exit."));
    let look = tor_client_text::adventure::look_with(&s, &Palette::default(), &NO_PLACES);
    assert!(look.starts_with("HP 7/10\n"), "{look}");
    assert!(!look.contains("Find your way to the exit."), "{look}");
    for line in ["status", "score", "objective"] {
        assert_eq!(
            said(line, &s),
            "You are wounded. (HP 7/10) Find your way to the exit.",
            "{line}"
        );
    }
    assert_eq!(said("examine me", &s), "You are wounded. (HP 7/10)");
}

#[test]
fn a_carried_thing_and_its_twin_on_the_floor_are_told_apart_by_where_they_are() {
    // They look alike, but they're two things in two places, and the player
    // can't know they're identical: ask which, by where each is.
    let mut s = state();
    let token = s.observation.ground_items[0].item.clone();
    s.observation.inventory.push(ItemView {
        id: super::item_target(9),
        ..token
    });
    assert_eq!(
        question("examine token", &s),
        [
            "the copper token you're carrying",
            "the copper token at your feet"
        ]
    );
    // Taking prefers what isn't carried, so it needs no question.
    assert_eq!(
        goals("take token", &s),
        [Goal::Take {
            item: super::item_target(1),
            quantity: None
        }]
    );
}

#[test]
fn going_to_a_remembered_place_travels_to_where_it_was_learned() {
    let mut s = state();
    s.observation.places = vec![
        PlaceView {
            key: "cell-1".into(),
            name: "Hollow Promise".into(),
            origin: PlaceNameOrigin::Authored,
        },
        PlaceView {
            key: "far-away".into(),
            name: "Vault of Whispers".into(),
            origin: PlaceNameOrigin::Authored,
        },
    ];
    for line in ["go to vault of whispers", "go to the Vault of Whispers"] {
        assert_eq!(
            goals(line, &s),
            [Goal::Visit {
                destination: "far-away".into(),
                name: "Vault of Whispers".into(),
                back: false,
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

#[test]
fn going_to_the_start_returns_to_where_the_character_began() {
    let begun = state();
    let mut places = tor_client_text::narrative::Places::default();
    places.begin(&begun);
    let palette = Palette::default();
    let meaning = |line: &str, s: &StateView| {
        let scene = Scene::remembering(s, &palette, &places);
        let command = match_sentence_with_raw(&tokenize(line), Some(line)).unwrap();
        understand(&command, &scene, &mut Referents::default())
    };
    assert_eq!(
        meaning("go to start", &begun),
        Interpretation::Say("You're already where you started.".into())
    );
    // Two steps east, the start is a journey back.
    let mut moved = state();
    for cell in &mut moved.observation.visible_cells {
        cell.position.x -= 2;
    }
    for line in ["go to start", "go back to the start", "go to the beginning"] {
        assert_eq!(
            meaning(line, &moved),
            Interpretation::Goals(vec![Goal::Visit {
                destination: "cell-0".into(),
                name: "where you started".into(),
                back: true,
            }]),
            "{line}"
        );
    }
    // A client that never saw the start can't go back to it.
    assert_eq!(said("go to start", &moved), "You can't see any start here.");
}

#[test]
fn a_walk_follows_a_passage_and_stops_where_it_opens_out() {
    use tor_client_text::adventure::onward;
    // Map coordinates: the character walks a passage that bends south.
    let before = walled(&["##########", "#@.....  #", "#######. #", "      #. #"]);
    let along = walled(&[
        "##########",
        "#......@##",
        "#######.##",
        "      #.# ",
        "      #.# ",
    ]);
    // The only way on bends south; the walk follows it.
    assert!(matches!(
        onward(&before, &along, Direction::East),
        Some((Direction::South, _))
    ));
    // A side passage makes a junction: stop there.
    let junction = walled(&["##########", "#......@..", "#######.##", "      #.# "]);
    assert_eq!(onward(&before, &junction, Direction::East), None);
    // Coming out into a room: stop to look at it.
    let room = walled(&["#######", "#.....#", "...@..#", "#.....#", "#######"]);
    assert_eq!(onward(&before, &room, Direction::East), None);
}

#[test]
fn intentional_retrieval_exit_requires_up_and_does_not_invent_a_landing() {
    let mut s = state();
    s.observation.visible_cells[0].stairs_up = true;
    s.observation.combat = Some(
        serde_json::from_value(serde_json::json!({
            "public_plan":null,"hp":"10","max_hp":"10","dead":false,"victory":false,"terminal":false,
            "sequence":null,"preparation_remaining":null,"preparation_active":false,
            "recovery_remaining":"0","events":[],"objective":"retrieve_and_exit",
            "exit":"cell-0","actors":[]
        }))
        .unwrap(),
    );
    // A visible cell above the actor is not a landing for the terminal action.
    let mut unrelated = s.observation.visible_cells[0].clone();
    unrelated.key = "unrelated-upper-cell".into();
    unrelated.position.z = 1;
    unrelated.stairs_up = false;
    s.observation.visible_cells.push(unrelated);

    let place = tor_client_text::engine::place::survey(&s);
    let upward: Vec<_> = place.ways(Direction::Up).collect();
    assert_eq!(upward.len(), 1);
    assert_eq!(upward[0].destination, None);
    assert_eq!(
        upward[0].kind,
        tor_client_text::engine::place::Opening::SurfaceExit
    );
    assert_eq!(said("go to exit", &s), "You're already at the exit.");
    assert_eq!(
        goals("up", &s),
        [Goal::Step {
            direction: Direction::Up
        }]
    );
    assert!(describe(&s).contains("carry it up through the surface exit"));
    assert!(said("objective", &s).contains("carry it up through the surface exit"));
    let palette = Palette::default();
    let scene = Scene::new(&s, &palette);
    assert_eq!(
        scene.referents.iter().filter(|r| r.name == "exit").count(),
        1
    );
    assert!(!s.observation.combat.as_ref().unwrap().victory);

    // An ordinary disclosed stair retains its actual visible landing.
    s.observation.combat.as_mut().unwrap().objective = Some(ObjectiveKind::ReachExit);
    let ordinary = tor_client_text::engine::place::survey(&s);
    assert_eq!(
        ordinary
            .ways(Direction::Up)
            .next()
            .unwrap()
            .destination
            .as_deref(),
        Some("unrelated-upper-cell")
    );
}

#[test]
fn a_disclosed_exit_is_described_and_can_be_gone_to() {
    let mut s = state();
    assert_eq!(said("go to exit", &s), "You can't see any exit here.");
    s.observation.combat = Some(
        serde_json::from_value(serde_json::json!({
            "public_plan":null,"hp": "10", "max_hp": "10", "dead": false, "victory": false, "terminal": false,
            "sequence":null,"preparation_remaining": null, "preparation_active": false,
            "recovery_remaining": "0", "events": [], "objective": "reach_exit",
            "exit": "cell-3", "actors": []
        }))
        .unwrap(),
    );
    assert!(
        describe(&s).contains("The exit is to the east."),
        "{}",
        describe(&s)
    );
    for line in ["go to exit", "go back to the exit", "go to the way out"] {
        assert_eq!(
            goals(line, &s),
            [Goal::Visit {
                destination: "cell-3".into(),
                name: "the exit".into(),
                back: false,
            }],
            "{line}"
        );
    }
    s.observation.combat.as_mut().unwrap().exit = Some("cell-0".into());
    assert_eq!(said("go to exit", &s), "You're already at the exit.");
    assert!(describe(&s).contains("You are standing at the exit."));
}

#[test]
fn names_the_game_made_up_are_left_unsaid() {
    let mut s = state();
    s.observation.places.push(PlaceView {
        key: "cell-1".into(),
        name: "Hollow Promise".into(),
        origin: PlaceNameOrigin::Invented,
    });
    assert!(!describe(&s).contains("Hollow Promise"), "{}", describe(&s));
    assert_eq!(
        tor_client_text::places(&s),
        "1. An unnamed place (in sight)\nName one with name <number> <name>."
    );
    assert_eq!(
        said("go to hollow promise", &s),
        "You can't see any hollow promise here."
    );
    // Once the player names it, it's theirs, and said.
    s.observation.places[0].name = "Antechamber".into();
    s.observation.places[0].origin = PlaceNameOrigin::Player;
    assert!(
        describe(&s).starts_with("Antechamber\n"),
        "{}",
        describe(&s)
    );
    assert_eq!(tor_client_text::places(&s), "1. Antechamber (in sight)");
}

#[test]
fn a_place_joined_with_the_next_room_is_known_by_its_nearest_hint() {
    // Regression: seen across a doorway, two rooms can survey as one place
    // holding both their hints; keyed by the lowest key, stepping into the
    // next room didn't count as arriving anywhere.
    let mut s = walled(&["###########", "#.......@..", "###########"]);
    for cell in &mut s.observation.visible_cells {
        let (x, y, z) = (cell.position.x, cell.position.y, cell.position.z);
        // "7,1,0" is next to the character; "2,1,0" is farther, with the
        // lower key.
        cell.place_hint = z == 0 && y == 0 && (x == -1 || x == -6);
    }
    let near = s
        .observation
        .visible_cells
        .iter()
        .find(|c| c.place_hint && c.position.x == -1)
        .unwrap()
        .key
        .clone();
    assert_eq!(NO_PLACES.key(&s), Some(near));
}

#[test]
fn ways_alike_in_one_wall_are_asked_about_by_where_each_lies() {
    // Regression: "Which way do you mean, a passage to the east or a
    // passage to the east?"
    let s = walled(&["#####", "#...'..", "#.@.#", "#...'..", "#####"]);
    assert_eq!(
        question("east", &s),
        [
            "an open oak door to the northeast",
            "an open oak door to the southeast"
        ]
    );
}

#[test]
fn a_place_dark_on_most_sides_names_the_sides_that_are_not() {
    let s = walled(&["#######", ".......", "...@...", "......."]);
    let prose = describe(&s);
    assert!(
        prose.contains("It continues beyond your view on every side but the north"),
        "{prose}"
    );
}

#[test]
fn stairs_underfoot_are_one_step_up() {
    let mut s = state();
    s.observation.visible_cells[0].stairs_up = true;
    assert_eq!(
        goals("up", &s),
        [Goal::Step {
            direction: Direction::Up
        }]
    );
    assert!(describe(&s).contains("Stairs lead up."), "{}", describe(&s));
}

#[test]
fn things_can_be_named_by_where_they_lie() {
    // Regression: "open the east door" wasn't understood.
    let mut s = walled(&["#######", "'..@..+", "#######"]);
    // Two doors, one each way: give them their own ids.
    for cell in &mut s.observation.visible_cells {
        if let Some(door) = &mut cell.door {
            if cell.position.x < 0 {
                door.id = super::door_target(8);
            }
        }
    }
    let door = |id| Goal::Door {
        door: super::door_target(id),
        open: true,
    };
    assert_eq!(goals("open the east door", &s), [door(9)]);
    assert_eq!(goals("open the door to the east", &s), [door(9)]);
    assert_eq!(
        said("open the west door", &s),
        "The oak door is already open."
    );
}

#[test]
fn a_verb_keeps_its_preposition() {
    // Regression: "knock on door" answered "You can't see any on door here."
    let s = walled(&["#####", "#.@.+", "#####"]);
    assert_eq!(said("knock on the door", &s), "Nothing would come of that.");
}

#[test]
fn an_arbitrary_disclosed_ability_needs_no_lexicon_entry() {
    let mut s = state();
    s.observation.combat = Some(super::creature_combat(vec![
        tor_client_common::test_support::ability("shimmer_wave"),
    ]));
    let target = super::actor_target(42);
    s.observation.visible_actors.push(ActorView {
        asset: None,
        id: target,
        name: "sentinel".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    assert_eq!(
        goals("shimmer wave sentinel", &s),
        vec![Goal::UseAbility {
            ability: "shimmer_wave".into(),
            invocation: tor_protocol::AbilityInvocation::ActorTarget { target },
            supplies: Default::default(),
        }]
    );
    assert!(
        matches!(tor_client_text::parse("shimmer_wave sentinel",&s),Ok(Input::Command(Command::Act{action:Action::UseAbility{id,invocation: tor_protocol::AbilityInvocation::ActorTarget { target: selected },..},..})) if id=="shimmer_wave"&&selected==target)
    );
}

#[test]
fn a_supportive_ability_name_never_implies_a_restorative_mechanic() {
    for (id, name) in [("secure_ward", "Secure Ward"), ("mend", "Mend")] {
        let mut s = state();
        let mut ability = tor_client_common::test_support::ability(id);
        ability.name = name.into();
        ability.reach = AbilityReach::SelfOrAlly {
            range: AbilityAllyRange::Touch,
        };
        ability.target_kind = AbilityTargetKind::SelfOrAlly;
        s.observation.combat = Some(super::creature_combat(vec![ability]));
        let actual = meaning(id, &s);
        assert!(
            matches!(actual, Interpretation::Goals(_)),
            "{name}: {actual:?}"
        );
    }
}

#[test]
fn phase5_supportive_verbs_select_self_or_an_explicit_disclosed_ally() {
    let mut s = state();
    let mut ability = tor_client_common::test_support::ability("mend");
    ability.reach = AbilityReach::SelfOrAlly {
        range: AbilityAllyRange::Touch,
    };
    ability.target_kind = AbilityTargetKind::SelfOrAlly;
    s.observation.combat = Some(super::creature_combat(vec![ability]));
    let ally = super::actor_target(42);
    let neutral = super::actor_target(43);
    for (id, name) in [(ally, "companion"), (neutral, "stranger")] {
        s.observation.visible_actors.push(ActorView {
            asset: None,
            id,
            name: name.into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        s.observation
            .combat
            .as_mut()
            .unwrap()
            .actors
            .push(CombatActorView {
                actor: id,
                hostile: false,
                allied: id == ally,
                injury: Injury::Healthy,
            });
    }
    // A disclosed name and target kind do not promise which effects it has.
    assert!(matches!(meaning("mend", &s), Interpretation::Goals(_)));
    s.observation.combat.as_mut().unwrap().hp = 5;
    for command in ["mend", "mend me", "mend self", "mend myself"] {
        assert!(
            matches!(tor_client_text::parse(command, &s), Ok(Input::Command(Command::Act { action: Action::UseAbility { invocation: tor_protocol::AbilityInvocation::ActorTarget { target }, .. }, .. })) if target == s.observation.self_target),
            "{command}"
        );
    }
    assert_eq!(
        goals("mend", &s),
        vec![Goal::UseAbility {
            ability: "mend".into(),
            invocation: tor_protocol::AbilityInvocation::ActorTarget {
                target: s.observation.self_target
            },
            supplies: Default::default(),
        }]
    );
    assert_eq!(
        goals("mend me", &s),
        vec![Goal::UseAbility {
            ability: "mend".into(),
            invocation: tor_protocol::AbilityInvocation::ActorTarget {
                target: s.observation.self_target
            },
            supplies: Default::default(),
        }]
    );
    assert_eq!(
        goals("mend companion", &s),
        vec![Goal::UseAbility {
            ability: "mend".into(),
            invocation: tor_protocol::AbilityInvocation::ActorTarget { target: ally },
            supplies: Default::default(),
        }]
    );
    assert!(tor_client_text::parse("mend stranger", &s).is_err());
    assert!(tor_client_text::parse("mend #42", &s).is_err());
}

#[test]
fn phase5_using_clause_keeps_exact_ground_supply_through_both_text_modes() {
    let mut s = state();
    let mut ability = tor_client_common::test_support::ability("mend");
    ability.reach = AbilityReach::SelfOnly;
    ability.target_kind = AbilityTargetKind::SelfOnly;
    ability.supply_costs = vec![AbilitySupplyCost {
        supply: "bandage".into(),
        name: "Bandage".into(),
        quantity: 1,
        access: SupplyAccess::CarriedOrGroundTouch,
    }];
    s.observation.combat = Some(super::creature_combat(vec![ability]));
    s.observation.combat.as_mut().unwrap().hp = 5;
    let item = super::item_target(9);
    s.observation.ground_items.push(GroundItemView {
        reachable: false,
        supply_reachable: true,
        position: Position { x: 1, y: 0, z: 0 },
        item: ItemView {
            id: item,
            name: "clean bandage".into(),
            description: String::new(),
            class: ItemClass::Misc,
            quantity: 1,
            appearance: "bandage".into(),
            identified: true,
            asset: None,
            known_supply: Some(SupplyView {
                id: "bandage".into(),
                name: "Bandage".into(),
            }),
        },
    });
    let expected = SupplySelection::Explicit {
        items: vec![SelectedSupply {
            cost_ordinal: 0,
            item,
        }],
    };
    for command in [
        "mend me using clean bandage".to_owned(),
        format!("mend using #{item}"),
    ] {
        assert!(
            matches!(tor_client_text::parse(&command, &s), Ok(Input::Command(Command::Act { action: Action::UseAbility { invocation: tor_protocol::AbilityInvocation::ActorTarget { target }, supplies, .. }, .. })) if target == s.observation.self_target && supplies == expected)
        );
        assert_eq!(
            goals(&command, &s),
            vec![Goal::UseAbility {
                ability: "mend".into(),
                invocation: tor_protocol::AbilityInvocation::ActorTarget {
                    target: s.observation.self_target
                },
                supplies: expected.clone()
            }]
        );
    }
    assert!(tor_client_text::parse("mend using #9", &s).is_err());
    assert!(tor_client_text::parse("mend using clean bandage, clean bandage", &s).is_err());
    s.observation
        .ground_items
        .last_mut()
        .unwrap()
        .supply_reachable = false;
    assert!(tor_client_text::parse("mend using clean bandage", &s).is_err());
}

#[test]
fn phase5_scripted_opaque_self_works_without_a_scene_occurrence() {
    let mut s = state();
    let mut ability = tor_client_common::test_support::ability("mend");
    ability.reach = AbilityReach::SelfOrAlly {
        range: AbilityAllyRange::Touch,
    };
    ability.target_kind = AbilityTargetKind::SelfOrAlly;
    s.observation.combat = Some(super::creature_combat(vec![ability]));
    assert!(s
        .observation
        .visible_actors
        .iter()
        .all(|actor| actor.id != s.observation.self_target));
    let command = format!("mend #{}", s.observation.self_target);
    assert!(
        matches!(tor_client_text::parse(&command, &s), Ok(Input::Command(Command::Act {
        action: Action::UseAbility { invocation: tor_protocol::AbilityInvocation::ActorTarget { target }, supplies: SupplySelection::Carried, .. }, ..
    })) if target == s.observation.self_target)
    );
}

#[test]
fn personal_stats_distinguish_active_fainting_and_protection_without_hidden_causers() {
    let mut state = state();
    let mut combat = super::creature_combat(vec![]);
    combat.own_stats.as_mut().unwrap().conditions = vec![
        PersonalCondition {
            condition: ContentRef {
                id: "fixture_faint".into(),
                name: "Fixture Faint".into(),
            },
            phase: ConditionPhase::Active,
            remaining_ticks: 200,
        },
        PersonalCondition {
            condition: ContentRef {
                id: "fixture_ice".into(),
                name: "Fixture Ice".into(),
            },
            phase: ConditionPhase::Protection,
            remaining_ticks: 400,
        },
    ];
    state.observation.combat = Some(combat);
    let before = state.clone();
    let rows = tor_client_text::stats(&state);
    assert!(rows.contains("Fixture Faint: 200 active ticks remaining"));
    assert!(rows.contains("Protection against Fixture Ice: 400 active ticks remaining"));
    assert!(!rows.contains("Protection against Fixture Faint"));
    assert!(!rows.contains("from actor"));
    assert_eq!(state, before);
}

#[test]
fn personal_stats_display_restorable_losses_from_disclosed_facts() {
    let mut state = state();
    let mut combat = super::creature_combat(vec![]);
    let mut value = serde_json::to_value(combat.own_stats.as_ref().unwrap()).unwrap();
    value["restorable_losses"] = serde_json::json!({
        "attributes":{"strength":1,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "hit_dice":"0","maximum_health":2
    });
    combat.own_stats = Some(serde_json::from_value(value).unwrap());
    state.observation.combat = Some(combat);
    let before = state.clone();
    assert!(
        tor_client_text::stats(&state).contains("Restorable losses: Strength 1, maximum Health 2")
    );
    assert_eq!(state, before);
}

#[test]
fn view_limit_referents_and_ambiguity_labels_do_not_claim_darkness() {
    let s = walled(&[".....", ".@...", "....."]);
    let palette = Palette::default();
    let scene = Scene::new(&s, &palette);
    let limits: Vec<_> = scene
        .of(tor_client_text::engine::scene::Kind::Feature)
        .filter(|r| r.name.starts_with("view limit to the "))
        .collect();
    assert!(limits.len() >= 2);
    let choices = question("examine darkness", &s);
    assert!(choices.len() >= 2);
    assert!(choices
        .iter()
        .all(|label| label.contains("view limit to the ") && !label.contains("dark")));
    let names: std::collections::BTreeSet<_> = limits.iter().map(|r| &r.name).collect();
    assert_eq!(
        names.len(),
        limits.len(),
        "each view limit names its direction"
    );
    for r in limits {
        let prose = tor_client_text::engine::verbs::examine(r, &scene);
        assert!(prose.contains("continues beyond your view"), "{prose}");
        assert!(!prose.contains("dark"), "{prose}");
    }
}
