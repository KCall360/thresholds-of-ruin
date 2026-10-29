use tor_client_3d_model::{
    view, Carried, InventoryBrowser, InventoryFilter, ItemModel, View, MAX_INVENTORY_SEARCH,
};
use tor_client_common::{test_support::observation, ClientState, Palette};
use tor_protocol::{ItemClass, ItemTarget, Snapshot, StreamContext, StreamId};

fn item(n: u8, name: &str, class: ItemClass) -> Carried {
    Carried {
        id: ItemTarget::from_digest([n; 32]),
        model: ItemModel::Placeholder,
        name: name.into(),
        class,
        quantity: 1,
        description: "Disclosed description".into(),
        equipped: None,
        can_equip: false,
        drinkable: false,
        portable_light: None,
        edible: false,
        readable: false,
        device: None,
    }
}
fn fixture() -> View {
    let observed = observation();
    let snapshot: Snapshot=serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},
        "context":StreamContext{stream:StreamId("inventory".into()),epoch:0},
        "actor":observed.actor,"branch":"play","cursor":{"sequence":"0","tick":observed.tick.to_string()},
        "has_control":true,"intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":"0","observation":observed}
    })).unwrap();
    let mut v = view(
        &ClientState::from_snapshot(snapshot).unwrap(),
        &Palette::default(),
    );
    v.inventory = vec![
        item(1, "Iron sword", ItemClass::Weapon),
        Carried {
            drinkable: true,
            ..item(2, "Émeraude draught", ItemClass::Potion)
        },
        item(3, "Curious token", ItemClass::Misc),
    ];
    v.items.clear();
    v
}

#[test]
fn filters_follow_disclosed_gear_and_use_capabilities() {
    let v = fixture();
    let mut browser = InventoryBrowser::default();
    browser.set_filter(InventoryFilter::Gear);
    assert_eq!(
        browser
            .carried(&v)
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["Iron sword"]
    );
    browser.set_filter(InventoryFilter::Usable);
    assert_eq!(
        browser
            .carried(&v)
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["Émeraude draught"]
    );
    browser.set_filter(InventoryFilter::All);
    assert_eq!(browser.carried(&v).count(), 3);
}

#[test]
fn search_is_bounded_unicode_and_never_uses_an_undisclosed_identity() {
    let v = fixture();
    let mut browser = InventoryBrowser::default();
    browser.set_query("ÉMERAUDE");
    assert_eq!(browser.carried(&v).count(), 1);
    browser.set_query("healing");
    assert_eq!(browser.carried(&v).count(), 0);
    browser.set_query(&"é".repeat(MAX_INVENTORY_SEARCH + 1));
    assert_eq!(browser.query().chars().count(), MAX_INVENTORY_SEARCH);
    browser.set_query("\nIron\t");
    assert_eq!(browser.query(), "Iron");
    assert_eq!(browser.carried(&v).count(), 1);
}

#[test]
fn selection_tracks_identity_and_clears_when_removed_or_filtered() {
    let mut v = fixture();
    let mut browser = InventoryBrowser::default();
    let target = v.inventory[0].id;
    assert!(browser.select(&v, target));
    v.inventory.reverse();
    browser.reconcile(&v);
    assert_eq!(browser.selected(), Some(target));
    browser.set_filter(InventoryFilter::Usable);
    browser.reconcile(&v);
    assert_eq!(browser.selected(), None);
    browser.set_filter(InventoryFilter::All);
    assert!(browser.select(&v, target));
    v.inventory.retain(|i| i.id != target);
    browser.reconcile(&v);
    assert_eq!(browser.selected(), None);
    assert!(!browser.select(&v, target));
    assert_eq!(browser.selected(), None);
}

#[test]
fn keyboard_selection_is_bounded_and_never_mutates_disclosed_inventory() {
    let v = fixture();
    let mut browser = InventoryBrowser::default();
    browser.shift(&v, 1);
    assert_eq!(browser.selected(), Some(v.inventory[0].id));
    browser.shift(&v, 20);
    assert_eq!(browser.selected(), Some(v.inventory[2].id));
    browser.shift(&v, -20);
    assert_eq!(browser.selected(), Some(v.inventory[0].id));
    browser.set_query("no matches");
    browser.shift(&v, 1);
    assert_eq!(browser.selected(), None);
    assert_eq!(v.inventory.len(), 3);
}

#[test]
fn only_reachable_ground_targets_can_be_selected_and_they_do_not_get_hidden_by_pack_filters() {
    let mut v = fixture();
    let id = ItemTarget::from_digest([88; 32]);
    v.items.push(tor_client_3d_model::GroundItem {
        corpse_seed: None,
        model: ItemModel::Potion,
        corpse_model: tor_client_3d_model::CreatureModel::Placeholder,
        id,
        name: "Blue vial".into(),
        class: ItemClass::Potion,
        quantity: 2,
        description: "A blue vial.".into(),
        position: v.feet,
        reachable: true,
    });
    let mut browser = InventoryBrowser::default();
    browser.set_filter(InventoryFilter::Gear);
    browser.set_query("sword");
    assert!(browser.select(&v, id));
    browser.reconcile(&v);
    assert_eq!(browser.selected(), Some(id));
    v.items[0].reachable = false;
    browser.reconcile(&v);
    assert_eq!(browser.selected(), None);
    assert!(!browser.select(&v, id));
}

#[test]
fn a_potion_class_alone_does_not_invent_a_use_affordance() {
    let mut v = fixture();
    v.inventory[1].drinkable = false;
    let mut browser = InventoryBrowser::default();
    browser.set_filter(InventoryFilter::Usable);
    assert_eq!(browser.carried(&v).count(), 0);
}
