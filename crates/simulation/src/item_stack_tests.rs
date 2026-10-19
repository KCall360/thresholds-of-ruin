//! Owning transfer tests for homogeneous per-unit stack state. Internal mutations
//! exercise the merge boundary without enabling stackable equipment or devices.
use super::*;
use std::num::NonZeroU64;
use tor_world::{Position, RegionId};

fn fixture() -> (Game, ActorId, ActorId, ItemId, ItemId) {
    let mut game = Game::region_corridor(42, 1, crate::test_creatures::content());
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let owner = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    let recipient = game
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
    let mut spec = ItemSpec::ordinary("homogeneous units".into());
    spec.stackable = true;
    let source = game
        .place_item_stack(100, at, Some(owner), 3, spec.clone())
        .unwrap();
    let destination = game
        .place_item_stack(101, at, Some(recipient), 2, spec)
        .unwrap();
    (game, owner, recipient, source, destination)
}

#[test]
fn stack_keys_reject_each_distinct_per_unit_field_and_nonstackable_items() {
    let (game, _, _, source, destination) = fixture();
    let original = &game.items[&source];
    assert!(original.stacks_with(&game.items[&destination]));
    for field in 0..10 {
        let mut changed = game.items[&destination].clone();
        match field {
            0 => changed.condition.corrosion = 1,
            1 => changed.condition.cursed = true,
            2 => changed.condition.protected = true,
            3 => changed.condition.armor_penalty = 1,
            4 => changed.condition.armor_enchantment = 1,
            5 => changed.condition.hit_enchantment = -1,
            6 => changed.condition.damage_enchantment = 1,
            7 => changed.charges = Some(1),
            8 => {
                changed.corpse = Some(crate::CorpseAppearance {
                    actor: ActorId(9),
                    asset: None,
                })
            }
            9 => changed.spec.stackable = false,
            _ => unreachable!(),
        }
        assert!(!original.stacks_with(&changed), "field {field}");
        assert!(!changed.stacks_with(original), "symmetric field {field}");
    }
}

#[test]
fn owned_transfer_skips_different_condition_and_preserves_exact_split_state() {
    let (mut game, owner, recipient, source, destination) = fixture();
    game.items
        .edit(source, |item| item.condition.hit_enchantment = 2);
    let condition = game.items[&source].condition;
    let expected = ItemId(game.next_item_id);
    let plan = game
        .prepare_owned_transfer(source, owner, recipient, 1)
        .unwrap();
    assert_eq!(plan.result(), expected);
    game.apply_owned_transfer(plan).unwrap();
    assert_eq!(game.items[&source].quantity, 2);
    assert_eq!(game.items[&destination].quantity, 2);
    assert_eq!(game.items[&expected].quantity, 1);
    assert_eq!(game.items[&expected].condition, condition);
    assert_eq!(game.items[&source].condition, condition);
    let plan = game
        .prepare_owned_transfer(source, owner, recipient, 1)
        .unwrap();
    assert_eq!(
        plan.result(),
        expected,
        "later equal units reuse the compatible stack"
    );
    game.apply_owned_transfer(plan).unwrap();
    assert_eq!(game.items[&expected].quantity, 2);
}

#[test]
fn selected_or_low_level_incompatible_merge_is_atomic() {
    let (mut game, owner, recipient, source, destination) = fixture();
    game.items
        .edit(destination, |item| item.condition.cursed = true);
    let before = game.clone();
    assert!(matches!(
        game.prepare_owned_transfer_into(source, owner, recipient, 1, Some(destination)),
        Err(GameError::ItemUnavailable)
    ));
    assert!(matches!(
        game.apply_transfer(source, destination, 1, ItemLocation::Carried(recipient)),
        Err(GameError::ItemUnavailable)
    ));
    assert_eq!(game, before);
}

#[test]
fn low_level_merge_rejects_wrong_destination_location_without_mutation() {
    let (mut game, owner, _, source, destination) = fixture();
    let before = game.clone();
    assert!(matches!(
        game.apply_transfer(source, destination, 1, ItemLocation::Carried(owner)),
        Err(GameError::ItemUnavailable)
    ));
    assert_eq!(game, before);
}

#[test]
fn take_and_drop_choose_only_equal_condition_stacks() {
    for taking in [false, true] {
        let (mut game, owner, _, source, destination) = fixture();
        let at = game.actors[&owner].location;
        let (from, to) = if taking {
            (ItemLocation::Ground(at), ItemLocation::Carried(owner))
        } else {
            (ItemLocation::Carried(owner), ItemLocation::Ground(at))
        };
        game.items.edit(source, |item| {
            item.location = from;
            item.condition.damage_enchantment = -2;
        });
        game.items.edit(destination, |item| item.location = to);
        let outcome = game
            .prepare_transfer(owner, source, Some(1), taking)
            .unwrap();
        let result = match outcome {
            OutcomeKind::Taken { result, .. } | OutcomeKind::Dropped { result, .. } => result,
            _ => panic!("wrong transfer outcome"),
        };
        assert_eq!(result, ItemId(game.next_item_id));
        game.apply_transfer(source, result, 1, to).unwrap();
        assert_eq!(game.items[&destination].quantity, 2);
        assert_eq!(game.items[&result].condition.damage_enchantment, -2);
    }
}

#[test]
fn compatible_merge_preserves_holds_and_does_not_copy_instance_knowledge() {
    let (mut game, owner, recipient, source, destination) = fixture();
    let hold = crate::supplies::ItemHold {
        key: crate::supplies::ItemClaimKey {
            lineage: crate::supplies::PreparationLineage {
                actor: owner,
                origin: crate::IntentionId(1),
            },
            cost_ordinal: 0,
        },
        quantity: 1,
    };
    game.items
        .edit(source, |item| item.holds.push(hold.clone()));
    game.actors
        .get_mut(&owner)
        .unwrap()
        .known_equipment
        .learn_curse(source, true)
        .unwrap();
    let owner_before = game.actors[&owner].known_equipment.clone();
    let recipient_before = game.actors[&recipient].known_equipment.clone();
    let plan = game
        .prepare_owned_transfer(source, owner, recipient, 2)
        .unwrap();
    assert_eq!(plan.result(), destination);
    game.apply_owned_transfer(plan).unwrap();
    assert_eq!(game.items[&source].quantity, 1);
    assert_eq!(game.items[&source].holds, vec![hold]);
    assert_eq!(game.items[&destination].quantity, 4);
    assert!(game.items[&destination].holds.is_empty());
    assert_eq!(game.actors[&owner].known_equipment, owner_before);
    assert_eq!(game.actors[&recipient].known_equipment, recipient_before);
}
