use tor_client_common::test_support::equipment_slot;
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
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: Vec::new(),
        slots: vec![
            equipment_slot("ring", "on a finger", false),
            equipment_slot("ring", "on a finger", false),
        ],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item: item.id,
            slot: Some(equipment_slot("ring", "on a finger", false)),
            equipped_slot: None,
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            portable_light: None,
            device_activation: None,
            charges: None,
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
fn equip_commands_support_multiple_custom_attack_sockets() {
    let mut state = state();
    let mut first = state.observation.ground_items[0].item.clone();
    first.class = ItemClass::Tool;
    first.name = "brass wing fitting".into();
    let mut second = first.clone();
    second.id = ItemTarget::from_digest([127; 32]);
    second.name = "silver wing fitting".into();
    let slot = equipment_slot("fixture_wing", "fastened to a wing", true);
    state.observation.inventory = vec![first.clone(), second.clone()];
    state.observation.interactions = Some(InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![slot.clone(); 2],
        preparation: None,
        inventory: vec![
            ItemInteractionView {
                item: first.id,
                slot: Some(slot.clone()),
                equipped_slot: Some(0),
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                portable_light: None,
                device_activation: None,
                charges: None,
            },
            ItemInteractionView {
                item: second.id,
                slot: Some(slot),
                equipped_slot: None,
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                portable_light: None,
                device_activation: None,
                charges: None,
            },
        ],
    });
    assert!(
        matches!(parse("equip silver wing fitting", &state).unwrap(),
        Input::Command(Command::Act { action: Action::Equip { item, slot: 1 }, .. })
            if item == second.id)
    );
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

            "places":[],"visible_cells": (0..5).flat_map(|x| (0..3).map(move |y| serde_json::json!({"key":format!("{x}:{y}"),"trap":null,"stairs_up":false,"stairs_down":false,"position":{"x":x,"y":y,"z":0},"wall":false,"lit":true,"place_hint":false}))).collect::<Vec<_>>(),
            "ground_items":[
                {"supply_reachable":false,"reachable":true,"item":{"known_currency":null,"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(10),"name":"copper token"},"position":{"x":1,"y":1,"z":0}},
                {"supply_reachable":false,"reachable":false,"item":{"known_currency":null,"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(11),"name":"silver token"},"position":{"x":2,"y":1,"z":0}}
            ], "inventory":[], "linked_holds":[],"visible_actors":[], "exits":[],  "ready":true
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

#[test]
fn advancement_text_selects_an_exact_ordinary_proposal_before_confirmation() {
    use tor_client_common::advancement::AdvancementInput;
    assert_eq!(
        parse("advancement", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Overview)
    );
    assert_eq!(
        parse("advance remove 1 2", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Preview(
            AdvancementProposal::Replacement {
                replacement: ReplacementOperation {
                    source: ReplacementSource { owner: 0, index: 1 },
                    replacement: None
                }
            }
        ))
    );
    assert_eq!(
        parse("advancement confirm using carried", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Confirm(Some("carried".into())))
    );
    assert!(parse("advancement purchase 0 general skill athletics", &state()).is_err());
}

#[test]
fn eat_command_uses_the_exact_carried_item_and_never_infers_edibility() {
    let mut s = state();
    s.observation.combat = Some(super::creature_combat(vec![]));
    let mut item = s.observation.ground_items[0].item.clone();
    item.class = ItemClass::Food;
    item.name = "fixture meal".into();
    s.observation.inventory = vec![item.clone()];
    s.observation.interactions = Some(InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item: item.id,
            slot: None,
            equipped_slot: None,
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            portable_light: None,
            device_activation: None,
            charges: None,
        }],
    });
    assert!(parse("eat fixture meal", &s).is_err());
    s.observation.interactions.as_mut().unwrap().inventory[0].edible = true;
    assert!(
        matches!(parse("eat fixture meal", &s).unwrap(), Input::Command(Command::Act { action: Action::Eat { item: target }, .. }) if target == item.id)
    );
}

#[test]
fn wizard_creature_catalog_is_discoverable_in_text_help() {
    assert!(tor_client_text::HELP.contains("wizard creature list"));
    assert!(tor_client_text::engine::verbs::SESSION_HELP.contains("wizard creature list"));
}

#[test]
fn directional_ray_commands_select_direction_without_a_visible_creature() {
    let mut state = StateView {
        wizard_game: false,
        revision: 0,
        observation: tor_client_common::test_support::observation(),
    };
    let mut ability = tor_client_common::test_support::ability("arbitrary_ray");
    ability.name = "Arbitrary Ray".into();
    ability.target_kind = AbilityTargetKind::DirectionalRay;
    ability.reach = AbilityReach::Sight { range: 6 };
    state
        .observation
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![ability];
    state.observation.visible_actors.clear();
    state.observation.visible_cells.clear();
    let before = state.clone();
    for (token, direction) in [
        ("north", Direction::North),
        ("southeast", Direction::SouthEast),
        ("up", Direction::Up),
        ("down", Direction::Down),
    ] {
        assert!(
            matches!(parse(&format!("arbitrary ray {token}"), &state).unwrap(), Input::Command(Command::Act {
            action: Action::UseAbility { invocation: tor_protocol::AbilityInvocation { aim: tor_protocol::AbilityAim::DirectionalRay { direction: selected }, weapon: tor_protocol::WeaponSelection::Default }, supplies: SupplySelection::Carried, .. }, ..
        }) if selected == direction)
        );
    }
    for noun in ["", "me", "goblin", "north goblin", "north up"] {
        assert!(parse(&format!("arbitrary ray {noun}"), &state).is_err());
    }
    assert_eq!(state, before);
}

fn magic_state() -> StateView {
    let mut view = state();
    view.observation.combat = tor_client_common::test_support::observation().combat;
    let mut scroll = view.observation.ground_items[0].item.clone();
    scroll.class = ItemClass::Scroll;
    scroll.name = "amber scroll".into();
    scroll.identified = false;
    let mut wand = view.observation.ground_items[1].item.clone();
    wand.class = ItemClass::Wand;
    wand.name = "ivory wand".into();
    wand.identified = false;
    view.observation.inventory = vec![scroll.clone(), wand.clone()];
    view.observation.interactions = Some(InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![],
        preparation: None,
        inventory: vec![
            ItemInteractionView {
                item: scroll.id,
                slot: None,
                equipped_slot: None,
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: true,
                zappable: false,
                portable_light: None,
                device_activation: None,
                charges: None,
            },
            ItemInteractionView {
                item: wand.id,
                slot: None,
                equipped_slot: None,
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: true,
                portable_light: None,
                device_activation: Some(tor_protocol::DeviceActivation::Direction),
                charges: None,
            },
        ],
    });
    view
}

#[test]
fn ordinary_magic_commands_select_one_disclosed_item_and_direction() {
    let mut view = magic_state();
    let scroll = view.observation.inventory[0].id;
    let wand = view.observation.inventory[1].id;
    for (text, action) in [
        (
            "read amber scroll".to_owned(),
            Action::Read { item: scroll },
        ),
        (format!("read #{scroll}"), Action::Read { item: scroll }),
        (
            "zap the ivory wand northeast".to_owned(),
            Action::Zap {
                item: wand,
                aim: DeviceAim::Direction {
                    direction: Direction::NorthEast,
                },
            },
        ),
        (
            format!("zap #{wand} down"),
            Action::Zap {
                item: wand,
                aim: DeviceAim::Direction {
                    direction: Direction::Down,
                },
            },
        ),
    ] {
        assert_eq!(
            parse(&text, &view).unwrap(),
            Input::Command(Command::Act {
                expected_revision: view.revision,
                action
            })
        );
    }
    for bad in [
        "zap ivory wand",
        "zap ivory wand somewhere",
        "zap amber scroll north",
        "read ivory wand",
        "identify ivory wand",
        "identify cancel",
    ] {
        assert!(parse(bad, &view).is_err(), "{bad}");
    }
    for charges in [None, Some(0), Some(3)] {
        view.observation.interactions.as_mut().unwrap().inventory[1].charges = charges;
        assert!(
            parse("zap ivory wand east", &view).is_ok(),
            "counts do not control device eligibility"
        );
        let text = tor_client_text::inventory(&view);
        assert_eq!(text.contains("charges]"), charges.is_some());
    }
}

#[test]
fn identification_commands_bind_current_choice_and_exact_category() {
    let mut view = magic_state();
    let wand = view.observation.inventory[1].id;
    let choice = IntentionId("paid-owner-choice".into());
    view.observation
        .interactions
        .as_mut()
        .unwrap()
        .identification = Some(IdentificationChoiceView {
        choice: choice.clone(),
        classes: vec![ItemClass::Wand],
        items: vec![wand],
    });
    assert_eq!(
        parse("identify ivory wand", &view).unwrap(),
        Input::Command(Command::Act {
            expected_revision: view.revision,
            action: Action::Identify {
                choice: choice.clone(),
                item: Some(wand)
            }
        })
    );
    assert_eq!(
        parse("identify cancel", &view).unwrap(),
        Input::Command(Command::Act {
            expected_revision: view.revision,
            action: Action::Identify { choice, item: None }
        })
    );
    assert!(parse("identify amber scroll", &view).is_err());
    assert!(tor_client_text::inventory(&view).contains("already been used"));
    view.observation.inventory.pop();
    assert!(parse("identify ivory wand", &view).is_err());
    assert!(parse("identify cancel", &view).is_ok());
    view.observation
        .interactions
        .as_mut()
        .unwrap()
        .identification = None;
    assert!(parse("identify cancel", &view).is_err());
}

#[test]
fn expertise_commands_delegate_to_shared_ordinary_advancement() {
    use tor_client_common::advancement::AdvancementInput;
    assert_eq!(
        parse("advancement expertises", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Catalog(AdvancementKind::Expertise))
    );
    assert_eq!(
        parse("advancement details expertises", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Details(
            AdvancementPreviewSection::Expertises
        ))
    );
    assert!(matches!(
        parse("advancement purchase 1 general expertise spear", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Preview(AdvancementProposal::Purchase {
            purchase: PurchaseOperation {
                selection: PurchaseChoice::Expertise { .. },
                ..
            }
        }))
    ));
}

#[test]
fn intellect_expertise_commands_use_distinct_ordinary_entitlements() {
    use tor_client_common::advancement::AdvancementInput;
    assert_eq!(
        parse("advancement intellect-expertises", &state()).unwrap(),
        Input::Advancement(AdvancementInput::IntellectExpertises)
    );
    assert_eq!(
        parse("advancement intellect-expertise 3 spear", &state()).unwrap(),
        Input::Advancement(AdvancementInput::Preview(
            AdvancementProposal::IntellectExpertise {
                choice: IntellectExpertiseOperation {
                    threshold: 3,
                    expertise: "spear".into()
                }
            }
        ))
    );
    assert!(parse("advancement intellect-expertise 0 spear", &state()).is_err());
}

#[test]
fn projectile_commands_open_source_picker_without_actor_ray_or_step_fallback() {
    let mut state = StateView {
        wizard_game: false,
        revision: 0,
        observation: tor_client_common::test_support::observation(),
    };
    let mut ability = tor_client_common::test_support::ability("fixture_projectile");
    ability.name = "Fixture Projectile".into();
    ability.target_kind = AbilityTargetKind::DirectionalProjectile;
    ability.reach = AbilityReach::Projectile;
    ability.weapon_selection = false;
    state
        .observation
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![ability];
    let before = state.clone();
    assert_eq!(
        parse("fixture projectile", &state).unwrap(),
        Input::Projectile("fixture_projectile".into())
    );
    let tokens = tor_client_text::parser::tokenize("fixture projectile");
    assert!(
        matches!(tor_client_text::parser::match_disclosed_sentence(&tokens, "fixture projectile", &state.observation).unwrap(), tor_client_text::parser::grammar::ParsedCommand::Projectile { id } if id == "fixture_projectile")
    );
    for line in [
        "fixture projectile north",
        "fixture projectile me",
        "fixture projectile with weapon",
    ] {
        assert!(parse(line, &state).unwrap_err().contains("choose a source"));
        let tokens = tor_client_text::parser::tokenize(line);
        assert!(
            matches!(tor_client_text::parser::match_disclosed_sentence(&tokens, line, &state.observation).unwrap(),
            tor_client_text::parser::grammar::ParsedCommand::Refused(reason) if reason.contains("choose a source"))
        );
    }
    assert_eq!(state, before);
}

#[test]
fn identified_self_device_command_needs_no_direction() {
    let mut view = magic_state();
    view.observation.ground_items.clear();
    view.observation.inventory[1].identified = true;
    view.observation.interactions.as_mut().unwrap().inventory[1].device_activation =
        Some(DeviceActivation::SelfTarget);
    assert!(
        view.validate().is_ok(),
        "valid disclosed self device premise"
    );
    let item = view.observation.inventory[1].id;
    assert_eq!(
        parse(&format!("zap #{item}"), &view).unwrap(),
        Input::Command(Command::Act {
            expected_revision: view.revision,
            action: Action::Zap {
                item,
                aim: DeviceAim::SelfTarget {}
            }
        })
    );
}
