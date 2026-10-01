//! End-to-end integration tests for the Interactive Fiction parser.

use tor_client_text::parser::{
    match_noun_phrase, parse_input, ConversationContext, Entity, MatchResult, ParsedCommand,
    Preposition, Referents, Scope, Verb,
};
use tor_protocol::*;

fn test_state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game": false,
        "revision": 1,
        "observation": {
            "actor": 1,
            "tick": 10,
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
                        "id": 101,
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
                        "id": 102,
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
                        "id": 1,
                        "name": "copper token",
                        "description": "A worn copper disc.",
                        "quantity": 1,
                        "appearance": "token",
                        "identified": true
                    }
                },
                {
                    "reachable": true,
                    "position": {"x": 0, "y": 0, "z": 0},
                    "item": {
                        "id": 2,
                        "name": "silver token",
                        "description": "A polished silver disc.",
                        "quantity": 1,
                        "appearance": "token",
                        "identified": true
                    }
                },
                {
                    "reachable": false,
                    "position": {"x": 1, "y": 0, "z": 0},
                    "item": {
                        "id": 3,
                        "name": "stone tablet",
                        "description": "An inscribed slab of granite.",
                        "quantity": 1,
                        "appearance": "tablet",
                        "identified": true
                    }
                }
            ],
            "inventory": [
                {
                    "id": 10,
                    "name": "iron sword",
                    "description": "A sharp iron shortsword.",
                    "quantity": 1,
                    "appearance": "sword",
                    "identified": true
                },
                {
                    "id": 11,
                    "name": "brass key",
                    "description": "An ornate brass key.",
                    "quantity": 1,
                    "appearance": "key",
                    "identified": true
                }
            ],
            "visible_actors": [
                {
                    "id": 2,
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
fn test_parse_multi_sentence_pipeline() {
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

#[test]
fn test_scope_extraction_from_state() {
    let state = test_state();
    let scope = Scope::from_state(&state);

    // Should include: 2 inventory items + 3 ground items + 1 actor + 2 doors + surfaces
    let items: Vec<_> = scope.entities.iter().filter(|e| e.is_item()).collect();
    assert_eq!(items.len(), 5);

    let carried: Vec<_> = scope.entities.iter().filter(|e| e.is_carried()).collect();
    assert_eq!(carried.len(), 2);

    let doors: Vec<_> = scope.entities.iter().filter(|e| e.is_door()).collect();
    assert_eq!(doors.len(), 2);

    let actors: Vec<_> = scope.entities.iter().filter(|e| e.is_actor()).collect();
    assert_eq!(actors.len(), 1);
}

#[test]
fn test_conversational_disambiguation_flow() {
    let state = test_state();
    let scope = Scope::from_state(&state);
    let mut context = ConversationContext::default();

    // 1. Player says "take token"
    let commands = parse_input("take token").unwrap();
    assert_eq!(commands.len(), 1);

    let np = match &commands[0] {
        ParsedCommand::Transitive { direct, .. } => direct,
        other => panic!("Expected Transitive, got: {other:?}"),
    };

    // 2. Resolver finds two candidates: copper token and silver token
    let result = match_noun_phrase(np, &scope, &context.referents);
    let candidates = match result {
        MatchResult::Multiple(candidates) => candidates,
        other => panic!("Expected Multiple, got: {other:?}"),
    };
    assert_eq!(candidates.len(), 2);

    // 3. System asks disambiguation question
    let question = context.ask_disambiguation("take", candidates, state.revision);
    assert_eq!(
        question,
        "Which do you mean: the copper token or the silver token?"
    );

    // 4. Player replies with clarification: "the copper one"
    let reply_commands = parse_input("the copper one").unwrap();
    assert_eq!(reply_commands.len(), 1);

    let clarification_np = match &reply_commands[0] {
        ParsedCommand::Clarification(np) => np,
        other => panic!("Expected Clarification, got: {other:?}"),
    };

    // 5. Context resolves the pending action to the copper token
    let (verb_phrase, chosen) = context
        .resolve_clarification(clarification_np, state.revision)
        .expect("Should resolve clarification");

    assert_eq!(verb_phrase, "take");
    assert_eq!(chosen.name(), "copper token");
    if let Entity::Item { id, .. } = chosen {
        assert_eq!(id, 1);
    } else {
        panic!("Expected item");
    }

    // 6. Confirm pronoun "it" is now updated to the copper token
    assert_eq!(
        context.referents.it.as_ref().map(|e| e.name()),
        Some("copper token")
    );
}

#[test]
fn test_pronoun_reference_flow() {
    let state = test_state();
    let scope = Scope::from_state(&state);
    let mut context = ConversationContext::default();

    // Set initial referent by examining the stone tablet
    let tablet = scope
        .entities
        .iter()
        .find(|e| e.name() == "stone tablet")
        .unwrap()
        .clone();
    context.mention(&tablet);

    // Player types "take it"
    let commands = parse_input("take it").unwrap();
    assert_eq!(commands.len(), 1);

    let np = match &commands[0] {
        ParsedCommand::Transitive { direct, .. } => direct,
        other => panic!("Expected Transitive, got: {other:?}"),
    };

    let result = match_noun_phrase(np, &scope, &context.referents);
    match result {
        MatchResult::Single(entity) => {
            assert_eq!(entity.name(), "stone tablet");
            if let Entity::Item { id, .. } = entity {
                assert_eq!(id, 3);
            }
        }
        other => panic!("Expected Single match, got: {other:?}"),
    }
}

#[test]
fn test_ditransitive_attack_goblin_with_sword() {
    let state = test_state();
    let scope = Scope::from_state(&state);
    let referents = Referents::default();

    let commands = parse_input("attack the goblin with my iron sword").unwrap();
    assert_eq!(commands.len(), 1);

    let (direct, prep, indirect) = match &commands[0] {
        ParsedCommand::Ditransitive {
            verb,
            direct,
            preposition,
            indirect,
        } => {
            assert_eq!(*verb, Verb::Attack);
            (direct, *preposition, indirect)
        }
        other => panic!("Expected Ditransitive, got: {other:?}"),
    };

    assert_eq!(prep, Preposition::With);

    // Resolve target goblin
    match match_noun_phrase(direct, &scope, &referents) {
        MatchResult::Single(Entity::Actor { id, name, .. }) => {
            assert_eq!(id, ActorId(2));
            assert_eq!(name, "goblin scout");
        }
        other => panic!("Expected goblin match, got: {other:?}"),
    }

    // Resolve weapon sword
    match match_noun_phrase(indirect, &scope, &referents) {
        MatchResult::Single(Entity::Item { id, carried, .. }) => {
            assert_eq!(id, 10);
            assert!(carried);
        }
        other => panic!("Expected sword match, got: {other:?}"),
    }
}

#[test]
fn test_take_all_except_token() {
    let state = test_state();
    let scope = Scope::from_state(&state);
    let referents = Referents::default();

    let commands = parse_input("take all except copper token").unwrap();
    assert_eq!(commands.len(), 1);

    let np = match &commands[0] {
        ParsedCommand::Transitive { direct, .. } => direct,
        other => panic!("Expected Transitive, got: {other:?}"),
    };

    let result = match_noun_phrase(np, &scope, &referents);
    match result {
        MatchResult::All(entities) => {
            // Should contain all items except the copper token
            assert!(!entities.iter().any(|e| e.name() == "copper token"));
            assert!(entities.iter().any(|e| e.name() == "silver token"));
        }
        other => panic!("Expected All, got: {other:?}"),
    }
}
