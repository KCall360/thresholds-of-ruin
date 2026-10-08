use tor_client_common::items::{item_action, ItemOperation};
use tor_protocol::*;

fn target(byte: u8) -> ItemTarget {
    ItemTarget::from_digest([byte; 32])
}

#[test]
fn equipment_uses_free_anatomy_socket_without_knowledge_and_requires_removal() {
    let mut view: Observation = serde_json::from_value(serde_json::json!({
        "actor":"1", "self_target":ActorTarget::from_digest([1; 32]), "tick":"0",
        "position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":[],"ground_items":[],"visible_actors":[],
        "inventory": [
            {"quantity":"1","class":"ring","appearance":"silver ring","identified":false,"id":target(1),"name":"silver ring"},
            {"quantity":"1","class":"ring","appearance":"silver ring","identified":false,"id":target(2),"name":"silver ring"}
        ]
    })).unwrap();
    view.interactions = Some(InteractionView {
        slots: vec![EquipmentSlot::Ring, EquipmentSlot::Ring],
        preparation: None,
        inventory: vec![
            ItemInteractionView {
                item: target(1),
                slot: Some(EquipmentSlot::Ring),
                equipped_slot: Some(0),
                known_equipment: None,
                drinkable: false,
            },
            ItemInteractionView {
                item: target(2),
                slot: Some(EquipmentSlot::Ring),
                equipped_slot: None,
                known_equipment: None,
                drinkable: false,
            },
        ],
    });
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
    assert!(item_action(&view, target(2), ItemOperation::Equip)
        .unwrap_err()
        .contains("Remove"));
}
