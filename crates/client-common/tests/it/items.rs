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
        "places":[],"visible_cells":[],"ground_items":[],"visible_actors":[],
        "inventory": [
            {"known_supply":null,"quantity":"1","class":"ring","appearance":"silver ring","identified":false,"id":target(1),"name":"silver ring"},
            {"known_supply":null,"quantity":"1","class":"ring","appearance":"silver ring","identified":false,"id":target(2),"name":"silver ring"}
        ]
    })).unwrap();
    view.interactions = Some(InteractionView {
        completed: Vec::new(),
        slots: vec![slot.clone(), slot.clone()],
        preparation: None,
        inventory: vec![
            ItemInteractionView {
                item: target(1),
                slot: Some(slot.clone()),
                equipped_slot: Some(0),
                known_equipment: None,
                drinkable: false,
            },
            ItemInteractionView {
                item: target(2),
                slot: Some(slot),
                equipped_slot: None,
                known_equipment: None,
                drinkable: false,
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
