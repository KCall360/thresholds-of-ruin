//! Visible equipment disclosure: only what the items would show lying on the
//! ground, in canonical order, and nothing that depends on private facts.
use super::*;
use crate::interactions::{AnatomySpec, EquipmentSlot, EquipmentSlotId};
use std::num::NonZeroU64;
use tor_world::{Position, RegionId, Shared};

const SLOTS: [&str; 5] = ["weapon", "body_armor", "ring", "ring", "weapon"];

fn spec(name: &str, appearance: &str, class: ItemClass, asset: &str) -> ItemSpec {
    let mut spec = ItemSpec::ordinary(name.into());
    spec.appearance = appearance.into();
    spec.class = class;
    spec.asset = Some(asset.into());
    spec
}

/// A wearer with two weapon sockets, body armour and two ring sockets, and an
/// observer beside it. Returns the game, wearer and observer.
fn fixture() -> (Game, ActorId, ActorId) {
    let mut game = Game::region_corridor(42, 1, crate::test_creatures::content());
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let wearer = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    let observer = game
        .spawn_actor(
            Location {
                position: Position {
                    x: 2,
                    ..at.position
                },
                ..at
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.actors.get_mut(&wearer).unwrap().anatomy = Shared::new(AnatomySpec {
        slots: SLOTS
            .iter()
            .map(|s| EquipmentSlot::new(s).unwrap())
            .collect(),
        ..AnatomySpec::default()
    });
    (game, wearer, observer)
}

fn wear(game: &mut Game, wearer: ActorId, id: u64, socket: u16, spec: ItemSpec) -> ItemId {
    let at = game.actors[&wearer].location;
    let item = game
        .place_item_stack(id, at, Some(wearer), 1, spec)
        .unwrap();
    game.actors
        .get_mut(&wearer)
        .unwrap()
        .equipment
        .insert(EquipmentSlotId(socket), item);
    item
}

fn worn(game: &Game, wearer: ActorId) -> Vec<(String, u8, String)> {
    game.worn_view(&game.actors[&wearer])
        .into_iter()
        .map(|w| (w.slot.as_str().to_owned(), w.occurrence, w.appearance))
        .collect()
}

#[test]
fn weapons_and_armour_are_worn_visibly_and_rings_are_not() {
    let (mut game, wearer, _) = fixture();
    wear(
        &mut game,
        wearer,
        10,
        4,
        spec("dagger", "dagger", ItemClass::Weapon, "item.weapon"),
    );
    wear(
        &mut game,
        wearer,
        11,
        1,
        spec(
            "ring mail",
            "ring mail",
            ItemClass::Armor,
            "item.armor.mail",
        ),
    );
    wear(
        &mut game,
        wearer,
        12,
        3,
        spec("ring of stealth", "jade ring", ItemClass::Ring, "item.ring"),
    );
    wear(
        &mut game,
        wearer,
        13,
        0,
        spec("mace", "mace", ItemClass::Weapon, "item.weapon.mace"),
    );
    assert_eq!(
        worn(&game, wearer),
        [
            ("body_armor".into(), 0, "ring mail".into()),
            ("weapon".into(), 0, "mace".into()),
            ("weapon".into(), 1, "dagger".into()),
        ],
        "canonical slot order; the second weapon socket is occurrence 1; rings hidden"
    );
    let view = game.worn_view(&game.actors[&wearer]);
    assert_eq!(view[0].asset.as_deref(), Some("item.armor.mail"));
    assert_eq!(view[0].class, ItemClass::Armor);
}

#[test]
fn private_item_facts_never_change_the_worn_record() {
    let (mut game, wearer, _) = fixture();
    let mail = wear(
        &mut game,
        wearer,
        11,
        1,
        spec("ring mail", "ring mail", ItemClass::Armor, "item.armor"),
    );
    let before = game.worn_view(&game.actors[&wearer]);
    // Enchantment, true name and the hidden ring beneath are private.
    game.items
        .edit(mail, |item| item.condition.hit_enchantment = 3);
    wear(
        &mut game,
        wearer,
        12,
        2,
        spec(
            "ring of teleportation",
            "opal ring",
            ItemClass::Ring,
            "item.ring",
        ),
    );
    assert_eq!(game.worn_view(&game.actors[&wearer]), before);
    // A different item with the same look is indistinguishable.
    game.actors
        .get_mut(&wearer)
        .unwrap()
        .equipment
        .remove(&EquipmentSlotId(1));
    wear(
        &mut game,
        wearer,
        14,
        1,
        spec(
            "cursed ring mail",
            "ring mail",
            ItemClass::Armor,
            "item.armor",
        ),
    );
    assert_eq!(game.worn_view(&game.actors[&wearer]), before);
}

#[test]
fn onlookers_and_the_wearer_receive_the_same_worn_record() {
    let (mut game, wearer, observer) = fixture();
    wear(
        &mut game,
        wearer,
        11,
        1,
        spec("ring mail", "ring mail", ItemClass::Armor, "item.armor"),
    );
    let own = game.observe(wearer).unwrap();
    assert_eq!(own.worn, game.worn_view(&game.actors[&wearer]));
    let seen = game.observe(observer).unwrap();
    let views: Vec<_> = seen
        .visible_actors
        .iter()
        .filter(|actor| actor.id == wearer)
        .collect();
    assert!(!views.is_empty(), "the observer sees the wearer");
    for view in views {
        assert_eq!(
            view.worn, own.worn,
            "every visible cell carries the same record"
        );
    }
}
