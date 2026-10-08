use tor_client_text::{parse, Input};
use tor_protocol::*;

#[test]
fn equipment_and_drink_commands_use_disclosed_affordances_and_distinct_slots() {
    let mut s = state();
    let mut item = s.observation.ground_items[0].item.clone();
    item.class = ItemClass::Ring;
    item.name = "silver ring".into();
    s.observation.inventory = vec![item.clone()];
    s.observation.interactions = Some(InteractionView {
        completed: Vec::new(),
        slots: vec![EquipmentSlot::Ring, EquipmentSlot::Ring],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item: item.id,
            slot: Some(EquipmentSlot::Ring),
            equipped_slot: None,
            known_equipment: None,
            drinkable: false,
        }],
    });
    assert!(
        matches!(parse("equip silver ring", &s).unwrap(), Input::Command(Command::Act { action: Action::Equip { item: target, slot: 0 }, .. }) if target == item.id)
    );
    assert!(parse("drink silver ring", &s).is_err());
    s.observation.interactions.as_mut().unwrap().inventory[0].equipped_slot = Some(0);
    assert!(
        matches!(parse("remove silver ring", &s).unwrap(), Input::Command(Command::Act { action: Action::Unequip { item: target }, .. }) if target == item.id)
    );
    assert!(parse("equip silver ring", &s).is_err());
    assert!(parse("drink potion of healing", &s).is_err());
}

#[test]
fn quantity_commands_and_drop_use_disclosed_ids() {
    let mut s = state();
    s.observation
        .inventory
        .push(s.observation.ground_items[0].item.clone());
    for (text, expected) in [
        (
            "take 3 #i_0a00000000000000000000000000000000000000000000000000000000000000",
            Action::Take {
                item: super::item_target(10),
                quantity: Some(3),
            },
        ),
        (
            "drop 2 #i_0a00000000000000000000000000000000000000000000000000000000000000",
            Action::Drop {
                item: super::item_target(10),
                quantity: Some(2),
            },
        ),
        (
            "drop #i_0a00000000000000000000000000000000000000000000000000000000000000",
            Action::Drop {
                item: super::item_target(10),
                quantity: None,
            },
        ),
    ] {
        assert!(
            matches!(parse(text, &s).unwrap(), Input::Command(Command::Act { action, .. }) if action == expected)
        );
    }
    assert!(parse(
        "take 0 #i_0a00000000000000000000000000000000000000000000000000000000000000",
        &s
    )
    .is_err());
    assert!(parse("take 18446744073709551616 #i_0a00000000000000000000000000000000000000000000000000000000000000", &s).is_err());
}

#[test]
fn save_is_a_protocol_barrier_request() {
    assert_eq!(
        parse("save", &state()).unwrap(),
        Input::Request(Request::Save)
    );
}

#[test]
fn developer_commands_are_opaque_revision_checked_text() {
    for text in [
        "room 3 5 5 2 Upper gallery",
        "join arbitrary future syntax",
        "teleport 2 1 3 1 0",
    ] {
        assert_eq!(
            parse(&format!("wizard {text}"), &state()).unwrap(),
            Input::Command(Command::Wizard {
                expected_revision: 7,
                operation: text.into()
            })
        );
    }
    assert!(parse("wizard", &state()).is_err());
    let mut marked = state();
    marked.wizard_game = true;
    assert!(tor_client_text::describe(&marked).contains("WIZARD GAME"));
}

#[test]
fn prose_renders_disclosed_positions_and_escapes_terminal_controls() {
    let mut state = state();
    state.observation.ground_items[0]
        .item
        .name
        .push_str("\u{1b}[2J");
    let prose = tor_client_text::describe(&state);
    assert!(!prose.contains('\u{1b}'));
    assert!(prose.contains("tick 100"));
    assert!(prose.contains("copper token"));
    assert!(prose.contains("Within reach."));
    assert_eq!(tor_client_text::inventory(&state), "Inventory: empty.");
    assert_eq!(
        tor_client_text::safe("first\nsecond\t\u{009b}"),
        "first\\nsecond\\t\\u{9b}"
    );
}

#[test]
fn movement_queries_and_control_have_distinct_intents() {
    assert_eq!(
        parse("GO East", &state()).unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Move {
                direction: Direction::East
            }
        })
    );
    assert_eq!(parse("look", &state()).unwrap(), Input::Look);
    assert_eq!(
        parse("release", &state()).unwrap(),
        Input::Request(Request::ReleaseControl)
    );
    assert_eq!(
        parse("history abc", &state()).unwrap(),
        Input::Request(Request::History {
            before: Some(EntryId("abc".into())),
            limit: 50
        })
    );
    assert!(parse("history abc extra", &state()).is_err());
}

fn state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision": "7", "observation": {
            "actor": "1", "self_target":super::actor_target(1),"tick": "100", "position": {"x":1,"y":1,"z":0},

            "places":[],"visible_cells": (0..5).flat_map(|x| (0..3).map(move |y| serde_json::json!({"key":format!("{x}:{y}"),"stairs_up":false,"stairs_down":false,"position":{"x":x,"y":y,"z":0},"wall":false,"place_hint":false}))).collect::<Vec<_>>(),
            "ground_items":[
                {"reachable":true,"item":{"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(10),"name":"copper token"},"position":{"x":1,"y":1,"z":0}},
                {"reachable":false,"item":{"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(11),"name":"silver token"},"position":{"x":2,"y":1,"z":0}}
            ], "inventory":[], "visible_actors":[], "exits":[],  "ready":true
        }
    }))
    .unwrap()
}

#[test]
fn resolves_only_disclosed_nouns_and_requires_clarification() {
    let state = state();
    assert!(parse("take token", &state)
        .unwrap_err()
        .contains("#i_0a00000000000000000000000000000000000000000000000000000000000000"));
    assert_eq!(
        parse("take the COPPER token", &state).unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Take {
                item: super::item_target(10),
                quantity: None
            }
        })
    );
    assert_eq!(
        parse(
            "take #i_0b00000000000000000000000000000000000000000000000000000000000000",
            &state
        )
        .unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Take {
                item: super::item_target(11),
                quantity: None
            }
        })
    );
    assert!(parse("take #999", &state).is_err());
    assert!(parse("take stone tablet", &state).is_err());
    assert!(parse("east extra", &state).is_err());
}

#[test]
fn annotations_preserve_text_and_explicit_scope_without_an_action() {
    assert_eq!(
        parse("note Remember the Gallery.", &state()).unwrap(),
        Input::Command(Command::Annotate {
            anchor: Anchor::State { revision: 7 },
            text: "Remember the Gallery.".into(),
            source: ClientSource::User,
            audience: Audience::Private,
            category: AnnotationCategory::Note
        })
    );
    assert_eq!(
        parse(
            "annotate frontend actor explanation entry:abc A useful hint",
            &state()
        )
        .unwrap(),
        Input::Command(Command::Annotate {
            anchor: Anchor::Entry {
                id: EntryId("abc".into())
            },
            text: "A useful hint".into(),
            source: ClientSource::Frontend,
            audience: Audience::Actor,
            category: AnnotationCategory::Explanation
        })
    );
    for bad in [
        "note   ",
        "note bad\u{1b}[2J",
        "annotate backend actor note here spoiler",
        "annotate user public note here text",
        "annotate user private note state:no text",
    ] {
        assert!(parse(bad, &state()).is_err(), "{bad}");
    }
    assert!(parse(&format!("note {}", "é".repeat(2049)), &state()).is_err());
}
