//! Whole sentences through the parser, and what the resolver binds them to.

use tor_client_common::Palette;
use tor_client_text::{
    engine::{
        resolve::{resolve, Domain, Referents, Resolution},
        scene::{Key, Kind, Scene},
    },
    parser::{parse_input, NounPhrase, ParsedCommand, Preposition, Verb},
};
use tor_protocol::*;

fn sample_state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game": false,
        "revision": "1",
        "observation": {
            "actor": "1",
            "tick": "10",
            "position": {"x": 0, "y": 0, "z": 0},
            "ready": true,
            "places": [],
            "visible_cells": [
                {
                    "key": "cell-0",
                    "position": {"x": 0, "y": 0, "z": 0},
                    "wall": false,
                    "material": "stone",
                    "place_hint": true,
                    "stairs_up": false,
                    "stairs_down": false,
                    "door": null
                },
                {
                    "key": "cell-1",
                    "position": {"x": 1, "y": 0, "z": 0},
                    "wall": false,
                    "material": "stone",
                    "place_hint": false,
                    "stairs_up": false,
                    "stairs_down": false,
                    "door": {
                        "id": "101",
                        "name": "oak door",
                        "description": "A heavy wooden door with iron hinges.",
                        "open": false,
                        "reachable": true,
                        "approaches": ["cell-0"]
                    }
                },
                {
                    "key": "cell-2",
                    "position": {"x": 2, "y": 0, "z": 0},
                    "wall": false,
                    "material": "stone",
                    "place_hint": false,
                    "stairs_up": false,
                    "stairs_down": false,
                    "door": {
                        "id": "102",
                        "name": "iron gate",
                        "description": "A barred iron portcullis.",
                        "open": true,
                        "reachable": false,
                        "approaches": ["cell-1"]
                    }
                }
            ],
            "ground_items": [
                {
                    "reachable": true,
                    "position": {"x": 0, "y": 0, "z": 0},
                    "item": {
                        "id": "1",
                        "name": "copper token",
                        "description": "A worn copper disc.",
                        "quantity": "1",
                        "appearance": "token",
                        "identified": true
                    }
                },
                {
                    "reachable": true,
                    "position": {"x": 0, "y": 0, "z": 0},
                    "item": {
                        "id": "2",
                        "name": "silver token",
                        "description": "A polished silver disc.",
                        "quantity": "1",
                        "appearance": "token",
                        "identified": true
                    }
                },
                {
                    "reachable": false,
                    "position": {"x": 1, "y": 0, "z": 0},
                    "item": {
                        "id": "3",
                        "name": "stone tablet",
                        "description": "An inscribed slab of granite.",
                        "quantity": "1",
                        "appearance": "tablet",
                        "identified": true
                    }
                }
            ],
            "inventory": [
                {
                    "id": "10",
                    "name": "iron sword",
                    "description": "A sharp iron shortsword.",
                    "quantity": "1",
                    "appearance": "sword",
                    "identified": true
                },
                {
                    "id": "11",
                    "name": "brass key",
                    "description": "An ornate brass key.",
                    "quantity": "1",
                    "appearance": "key",
                    "identified": true
                }
            ],
            "visible_actors": [
                {
                    "id": "2",
                    "name": "goblin scout",
                    "description": "A snarling creature clad in scavenged leather.",
                    "position": {"x": 1, "y": 0, "z": 0}
                }
            ]
        }
    }))
    .unwrap()
}

#[test]
fn sentences_parse_into_commands_in_order() {
    let input = "take copper token. go east. open the oak door";
    let commands = parse_input(input).unwrap();
    assert_eq!(commands.len(), 3);

    assert!(matches!(
        &commands[0],
        ParsedCommand::Transitive {
            verb: Verb::Take,
            direct
        } if direct.head.as_deref() == Some("token")
    ));

    assert!(matches!(
        &commands[1],
        ParsedCommand::Directional {
            direction: Direction::East
        }
    ));

    assert!(matches!(
        &commands[2],
        ParsedCommand::Transitive {
            verb: Verb::Open,
            direct
        } if direct.head.as_deref() == Some("door")
    ));
}

/// The direct object of a one-sentence command.
fn object(line: &str) -> NounPhrase {
    match parse_input(line).unwrap().remove(0) {
        ParsedCommand::Transitive { direct, .. } | ParsedCommand::Ditransitive { direct, .. } => {
            direct
        }
        other => panic!("not transitive: {other:?}"),
    }
}

#[test]
fn the_scene_lists_what_the_state_discloses() {
    let state = sample_state();
    let palette = Palette::default();
    let scene = Scene::new(&state, &palette);
    let count = |kind| scene.of(kind).count();
    assert_eq!(count(Kind::Thing), 5);
    assert_eq!(scene.of(Kind::Thing).filter(|r| r.carried).count(), 2);
    assert_eq!(count(Kind::Door), 2);
    assert_eq!(count(Kind::Figure), 1);
    assert_eq!(count(Kind::Me), 1);
    // A portcullis is a kind of door, and a scout a creature.
    let gate = scene.get(Key::Door(102)).unwrap();
    assert!(gate.heads.contains(&"door".to_owned()));
    let goblin = scene.get(Key::Actor(ActorId(2))).unwrap();
    assert!(goblin.words.contains(&"creature".to_owned()));
}

#[test]
fn distinguishable_things_need_a_choice_and_alike_ones_dont() {
    let mut state = sample_state();
    let palette = Palette::default();
    let referents = Referents::default();
    let scene = Scene::new(&state, &palette);
    assert_eq!(
        resolve(&object("take token"), &scene, &referents, Domain::Ground),
        Resolution::Ask(vec![Key::Item(1), Key::Item(2)])
    );
    assert_eq!(
        resolve(
            &object("take the silver one"),
            &scene,
            &referents,
            Domain::Ground
        ),
        Resolution::One(Key::Item(2))
    );
    assert_eq!(
        resolve(&object("take tokens"), &scene, &referents, Domain::Ground),
        Resolution::Many(vec![Key::Item(1), Key::Item(2)])
    );
    // Two copper tokens can't be told apart, so either will do: the one in
    // reach.
    let mut twin = state.observation.ground_items[0].clone();
    twin.item.id = 4;
    twin.reachable = false;
    twin.position.x = 1;
    state.observation.ground_items[1] = twin;
    let scene = Scene::new(&state, &palette);
    assert_eq!(
        resolve(&object("take token"), &scene, &referents, Domain::Ground),
        Resolution::One(Key::Item(1))
    );
    // "The second token" still counts them both.
    assert_eq!(
        resolve(
            &object("take the second token"),
            &scene,
            &referents,
            Domain::Ground
        ),
        Resolution::One(Key::Item(4))
    );
}

#[test]
fn verbs_prefer_what_they_can_act_on() {
    let mut state = sample_state();
    state.observation.ground_items[2].item.name = "goblin scout corpse".into();
    let palette = Palette::default();
    let referents = Referents::default();
    let scene = Scene::new(&state, &palette);
    let scout = object("take scout");
    // Taking means the corpse; attacking and examining, the living scout.
    assert_eq!(
        resolve(&scout, &scene, &referents, Domain::Ground),
        Resolution::One(Key::Item(3))
    );
    assert_eq!(
        resolve(&scout, &scene, &referents, Domain::Figures),
        Resolution::One(Key::Actor(ActorId(2)))
    );
    assert_eq!(
        resolve(&scout, &scene, &referents, Domain::Any),
        Resolution::One(Key::Actor(ActorId(2)))
    );
    assert_eq!(
        resolve(&object("take body"), &scene, &referents, Domain::Ground),
        Resolution::One(Key::Item(3))
    );
    assert_eq!(
        resolve(&object("drop sword"), &scene, &referents, Domain::Carried),
        Resolution::One(Key::Item(10))
    );
    assert_eq!(
        resolve(&object("examine me"), &scene, &referents, Domain::Any),
        Resolution::One(Key::Me)
    );
    assert_eq!(
        resolve(&object("take lamp"), &scene, &referents, Domain::Ground),
        Resolution::Missing("You can't see any lamp here.".into())
    );
}

#[test]
fn it_is_the_last_thing_mentioned() {
    let state = sample_state();
    let palette = Palette::default();
    let scene = Scene::new(&state, &palette);
    let mut referents = Referents::default();
    assert_eq!(
        resolve(&object("take it"), &scene, &referents, Domain::Ground),
        Resolution::Missing("I'm not sure what \"it\" refers to.".into())
    );
    referents.mention(scene.get(Key::Item(3)).unwrap());
    assert_eq!(
        resolve(&object("take it"), &scene, &referents, Domain::Ground),
        Resolution::One(Key::Item(3))
    );
    referents.mention(scene.get(Key::Actor(ActorId(2))).unwrap());
    assert_eq!(
        resolve(&object("attack him"), &scene, &referents, Domain::Figures),
        Resolution::One(Key::Actor(ActorId(2)))
    );
    // Gone from view, it can't be acted on.
    let mut later = state.clone();
    later.observation.visible_actors.clear();
    let scene = Scene::new(&later, &palette);
    assert_eq!(
        resolve(&object("attack it"), &scene, &referents, Domain::Figures),
        Resolution::Missing("You can't see it any more.".into())
    );
}

#[test]
fn ditransitive_commands_bind_both_objects() {
    let state = sample_state();
    let palette = Palette::default();
    let scene = Scene::new(&state, &palette);
    let referents = Referents::default();
    let ParsedCommand::Ditransitive {
        verb,
        direct,
        preposition,
        indirect,
    } = parse_input("attack the goblin with my iron sword")
        .unwrap()
        .remove(0)
    else {
        panic!("expected two objects");
    };
    assert_eq!((verb, preposition), (Verb::Attack, Preposition::With));
    assert_eq!(
        resolve(&direct, &scene, &referents, Domain::Figures),
        Resolution::One(Key::Actor(ActorId(2)))
    );
    assert_eq!(
        resolve(&indirect, &scene, &referents, Domain::Carried),
        Resolution::One(Key::Item(10))
    );
}

#[test]
fn all_except_leaves_out_everything_it_names() {
    let mut state = sample_state();
    let mut twin = state.observation.ground_items[0].clone();
    twin.item.id = 4;
    state.observation.ground_items.push(twin);
    let palette = Palette::default();
    let scene = Scene::new(&state, &palette);
    let referents = Referents::default();
    assert_eq!(
        resolve(
            &object("take all except copper token"),
            &scene,
            &referents,
            Domain::Ground
        ),
        // In reach first, then the nearest.
        Resolution::Many(vec![Key::Item(2), Key::Item(3)])
    );
    assert_eq!(
        resolve(
            &object("drop everything but the key"),
            &scene,
            &referents,
            Domain::Carried
        ),
        Resolution::Many(vec![Key::Item(10)])
    );
    assert_eq!(
        resolve(
            &object("take all tokens"),
            &scene,
            &referents,
            Domain::Ground
        ),
        Resolution::Many(vec![Key::Item(1), Key::Item(2), Key::Item(4)])
    );
}
