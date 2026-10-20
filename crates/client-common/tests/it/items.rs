use tor_client_common::items::{item_action, item_choices, ItemOperation};
use tor_protocol::*;

fn equipment_slot(id: &str, equipped_phrase: &str, requires_attack: bool) -> EquipmentSlot {
    EquipmentSlot {
        id: id.into(),
        name: "Fixture socket".into(),
        equipped_phrase: equipped_phrase.into(),
        requires_attack,
    }
}

fn target(byte: u8) -> ItemTarget {
    ItemTarget::from_digest([byte; 32])
}

fn paired_equipment(slot: EquipmentSlot) -> Observation {
    let mut view: Observation = serde_json::from_value(serde_json::json!({
        "actor":"1", "self_target":ActorTarget::from_digest([1; 32]), "tick":"0",
        "position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":[],"ground_items":[],"linked_holds":[],"visible_actors":[],
        "inventory": [
            {"known_currency":null,"known_supply":null,"quantity":"1","class":"ring","appearance":"silver ring","identified":false,"id":target(1),"name":"silver ring"},
            {"known_currency":null,"known_supply":null,"quantity":"1","class":"ring","appearance":"silver ring","identified":false,"id":target(2),"name":"silver ring"}
        ]
    })).unwrap();
    view.interactions = Some(InteractionView {
        curse_blocked: vec![],
        identification: None,
        completed: Vec::new(),
        slots: vec![slot.clone(), slot.clone()],
        preparation: None,
        inventory: vec![
            ItemInteractionView {
                item: target(1),
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
                charges: None,
            },
            ItemInteractionView {
                item: target(2),
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
                charges: None,
            },
        ],
    });
    view
}

#[test]
fn equipment_uses_free_anatomy_socket_without_knowledge_and_requires_removal() {
    let mut view = paired_equipment(equipment_slot("ring", "on a finger", false));
    assert_eq!(
        item_action(&view, target(2), ItemOperation::Equip).unwrap(),
        Action::Equip {
            item: target(2),
            slot: 1
        }
    );
    assert_eq!(
        item_action(&view, target(1), ItemOperation::Unequip).unwrap(),
        Action::Unequip { item: target(1) }
    );
    assert!(item_action(&view, target(3), ItemOperation::Equip).is_err());
    assert!(item_action(&view, target(2), ItemOperation::Drink).is_err());
    view.interactions.as_mut().unwrap().slots.pop();
    assert_eq!(
        item_choices(&view, ItemOperation::Equip)
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![target(2)]
    );
    assert!(item_action(&view, target(2), ItemOperation::Equip)
        .unwrap_err()
        .contains("Remove"));
}

#[test]
fn every_disclosed_attack_socket_can_be_equipped() {
    for id in ["weapon", "fixture_wing"] {
        let view = paired_equipment(equipment_slot(id, "fastened to a wing", true));
        assert_eq!(
            item_action(&view, target(2), ItemOperation::Equip).unwrap(),
            Action::Equip {
                item: target(2),
                slot: 1
            },
            "{id}"
        );
    }
}

#[test]
fn equipment_compatibility_uses_content_ids_instead_of_display_names() {
    let mut view = paired_equipment(equipment_slot("fixture_wing", "fastened to a wing", false));
    view.interactions.as_mut().unwrap().inventory[1]
        .slot
        .as_mut()
        .unwrap()
        .name = "An alternate display label".into();
    assert_eq!(
        item_action(&view, target(2), ItemOperation::Equip).unwrap(),
        Action::Equip {
            item: target(2),
            slot: 1
        }
    );
    let interactions = view.interactions.as_mut().unwrap();
    interactions.inventory[1].slot =
        Some(equipment_slot("fixture_horn", "fastened to a wing", false));
    interactions.inventory[1].slot.as_mut().unwrap().name = interactions.slots[1].name.clone();
    assert!(item_action(&view, target(2), ItemOperation::Equip).is_err());
}

#[test]
fn eating_uses_only_explicit_disclosed_edibility_and_one_exact_item() {
    let mut view = paired_equipment(equipment_slot("ring", "on a finger", false));
    view.combat = tor_client_common::test_support::observation().combat;
    view.inventory[1].class = ItemClass::Food;
    assert!(item_action(&view, target(2), ItemOperation::Eat).is_err());
    view.interactions.as_mut().unwrap().inventory[1].edible = true;
    assert_eq!(
        item_choices(&view, ItemOperation::Eat)
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![target(2)]
    );
    assert_eq!(
        item_action(&view, target(2), ItemOperation::Eat).unwrap(),
        Action::Eat { item: target(2) }
    );
    view.inventory[1].class = ItemClass::Corpse;
    assert_eq!(
        item_action(&view, target(2), ItemOperation::Eat).unwrap(),
        Action::Eat { item: target(2) }
    );
    view.interactions.as_mut().unwrap().inventory[1].edible = false;
    assert!(item_action(&view, target(2), ItemOperation::Eat).is_err());
}

#[test]
fn magic_items_use_only_disclosed_affordances_and_never_test_unknown_charges() {
    use tor_client_common::items::{charges, zap_action};
    let mut view = paired_equipment(equipment_slot("ring", "on a finger", false));
    view.combat = tor_client_common::test_support::observation().combat;
    view.inventory[0].class = ItemClass::Scroll;
    view.inventory[1].class = ItemClass::Wand;
    assert!(item_action(&view, target(1), ItemOperation::Read).is_err());
    assert!(zap_action(&view, target(2), Direction::North).is_err());
    view.interactions.as_mut().unwrap().inventory[0].readable = true;
    view.interactions.as_mut().unwrap().inventory[1].zappable = true;
    assert_eq!(
        item_action(&view, target(1), ItemOperation::Read).unwrap(),
        Action::Read { item: target(1) }
    );
    for known in [None, Some(0), Some(7)] {
        view.interactions.as_mut().unwrap().inventory[1].charges = known;
        assert_eq!(charges(&view, target(2)), known);
        assert_eq!(
            zap_action(&view, target(2), Direction::North).unwrap(),
            Action::Zap {
                item: target(2),
                direction: Direction::North
            }
        );
        assert_eq!(
            item_choices(&view, ItemOperation::Zap)
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![target(2)]
        );
    }
    view.inventory.pop();
    assert!(zap_action(&view, target(2), Direction::North).is_err());
}

#[test]
fn identification_binds_exact_pending_choice_category_and_carried_instance() {
    use tor_client_common::items::identification_action;
    let mut view = paired_equipment(equipment_slot("ring", "on a finger", false));
    let choice = IntentionId("opaque-paid-choice".into());
    view.interactions.as_mut().unwrap().identification = Some(IdentificationChoiceView {
        choice: choice.clone(),
        class: ItemClass::Ring,
        items: vec![target(2)],
    });
    assert_eq!(
        identification_action(&view, Some(target(2))).unwrap(),
        Action::Identify {
            choice: choice.clone(),
            item: Some(target(2))
        }
    );
    assert_eq!(
        identification_action(&view, None).unwrap(),
        Action::Identify { choice, item: None }
    );
    assert!(identification_action(&view, Some(target(1))).is_err());
    view.inventory[1].class = ItemClass::Potion;
    assert!(identification_action(&view, Some(target(2))).is_err());
    view.inventory.pop();
    assert!(identification_action(&view, Some(target(2))).is_err());
    view.interactions.as_mut().unwrap().identification = None;
    assert!(identification_action(&view, None).is_err());
}

#[test]
fn projectile_details_label_remembered_bonuses_without_inventing_attack() {
    let mut view = paired_equipment(equipment_slot("fixture", "worn", false));
    let row = &mut view.interactions.as_mut().unwrap().inventory[0];
    row.slot = None;
    row.equipped_slot = None;
    row.known_projectile_bonus = Some(ProjectileBonusView { hit: -2, damage: 3 });
    assert_eq!(
        tor_client_common::items::equipment_details(&view, target(1)).as_deref(),
        Some("remembered projectile hit bonus -2, damage bonus +3")
    );
    view.interactions.as_mut().unwrap().inventory[0].known_projectile_bonus = None;
    assert!(tor_client_common::items::equipment_details(&view, target(1)).is_none());
}
