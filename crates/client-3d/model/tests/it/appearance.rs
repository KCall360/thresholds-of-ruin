use tor_client_3d_model::{actor_model, item_model, CreatureModel, ItemModel};
use tor_client_common::Palette;
use tor_protocol::{
    ActorTarget, ActorView, ItemClass, ItemTarget, ItemView, PaletteBody, PaletteUpdate, Position,
};

fn palette(assets: &[&str]) -> Palette {
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: assets.iter().map(|asset| (*asset).into()).collect(),
        },
    });
    palette
}

fn actor(asset: Option<&str>, name: &str) -> ActorView {
    ActorView {
        appearance_seed: None,
        id: ActorTarget::from_digest([7; 32]),
        name: name.into(),
        description: String::new(),
        position: Position { x: 0, y: 0, z: 0 },
        asset: asset.map(str::to_owned),
    }
}

#[test]
fn an_unknown_equine_variant_falls_back_to_the_supported_family() {
    let asset = "creature.equine.authored";
    assert_eq!(
        actor_model(&palette(&[asset]), &actor(Some(asset), "Alice")),
        CreatureModel::Equine
    );
}

#[test]
fn display_names_never_choose_the_creature_anatomy() {
    let asset = "creature.equine.quagga";
    let held = palette(&[asset]);
    for name in ["quagga", "novice", "hobgoblin", ""] {
        assert_eq!(
            actor_model(&held, &actor(Some(asset), name)),
            CreatureModel::Quagga
        );
    }
}

#[test]
fn absent_unknown_and_unforecast_assets_use_a_visible_neutral_placeholder() {
    for asset in [None, Some("author.unknown"), Some("creature.equine.quagga")] {
        assert_eq!(
            actor_model(&palette(&[]), &actor(asset, "ice monster")),
            CreatureModel::Placeholder
        );
    }
    let asset = "creature.authored";
    assert_eq!(
        actor_model(&palette(&[asset]), &actor(Some(asset), "human")),
        CreatureModel::Placeholder
    );
}

#[test]
fn an_unidentified_potion_uses_only_its_disclosed_appearance_and_class() {
    let asset = "item.potion.authored";
    let item = ItemView {
        corpse: None,
        id: ItemTarget::from_digest([8; 32]),
        name: "mace of concealed power".into(),
        description: String::new(),
        class: ItemClass::Potion,
        quantity: 1,
        appearance: "cloudy potion".into(),
        identified: false,
        asset: Some(asset.into()),
        known_currency: None,
        known_supply: None,
    };
    assert_eq!(item_model(&palette(&[asset]), &item), ItemModel::Potion);
}

#[test]
fn an_unknown_item_has_a_class_fallback() {
    let item = ItemView {
        corpse: None,
        id: ItemTarget::from_digest([9; 32]),
        name: "mace".into(),
        description: String::new(),
        class: ItemClass::Misc,
        quantity: 1,
        appearance: "wrapped object".into(),
        identified: false,
        asset: None,
        known_currency: None,
        known_supply: None,
    };
    assert_eq!(item_model(&palette(&[]), &item), ItemModel::Placeholder);
}

#[test]
fn an_unknown_miscellaneous_item_does_not_claim_to_be_a_container() {
    let item = ItemView {
        corpse: None,
        id: ItemTarget::from_digest([10; 32]),
        name: "unfamiliar object".into(),
        description: String::new(),
        class: ItemClass::Misc,
        quantity: 1,
        appearance: "unfamiliar object".into(),
        identified: false,
        asset: None,
        known_currency: None,
        known_supply: None,
    };
    assert_ne!(item_model(&palette(&[]), &item), ItemModel::Pouch);
}
