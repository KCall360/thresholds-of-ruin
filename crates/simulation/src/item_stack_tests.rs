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
        let result = match outcome.outcome(taking) {
            OutcomeKind::Taken { result, .. } | OutcomeKind::Dropped { result, .. } => result,
            _ => panic!("wrong transfer outcome"),
        };
        assert_eq!(result, ItemId(game.next_item_id));
        game.apply_prepared_transfer(&outcome).unwrap();
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
    game.items.edit(source, |item| item.holds.push(hold));
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

fn ammunition_fixture() -> (Game, ActorId, ActorId, ItemId, ItemId) {
    let (mut game, owner, observer, source, destination) = fixture();
    let mut spec = (*game.items[&source].spec).clone();
    spec.class = ItemClass::Weapon;
    let component = crate::damage::DamageComponent::fixed(crate::combat::DamageType::Keen, None, 3);
    let primary = component.key();
    spec.projectile = Some(crate::projectile_profiles::ProjectileSpec {
        family: crate::content_ids::LauncherFamilyId::new("arrow").unwrap(),
        damage: crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
        recovery: crate::projectile_profiles::HitRecovery::ConsumedOnHit,
    });
    assert!(spec.valid());
    let spec = tor_world::Shared::new(spec);
    for id in [source, destination] {
        game.items.edit(id, |item| item.spec = spec.clone());
    }
    (game, owner, observer, source, destination)
}

#[test]
fn ammunition_merge_uses_fresh_identity_and_intersects_only_owner_memories() {
    for taking in [false, true] {
        let (mut game, owner, observer, source, destination) = ammunition_fixture();
        let at = game.actors[&owner].location;
        game.items.edit(source, |item| {
            item.location = if taking {
                ItemLocation::Ground(at)
            } else {
                ItemLocation::Carried(owner)
            }
        });
        game.items.edit(destination, |item| {
            item.location = if taking {
                ItemLocation::Carried(owner)
            } else {
                ItemLocation::Ground(at)
            }
        });
        let stale = crate::armor::ItemCondition {
            hit_enchantment: 4,
            ..Default::default()
        };
        for id in [source, destination] {
            game.actors
                .get_mut(&owner)
                .unwrap()
                .known_equipment
                .learn_numbers(id, stale)
                .unwrap();
        }
        game.actors
            .get_mut(&owner)
            .unwrap()
            .known_equipment
            .learn_curse(source, true)
            .unwrap();
        game.actors
            .get_mut(&owner)
            .unwrap()
            .known_equipment
            .learn_curse(destination, false)
            .unwrap();
        game.actors
            .get_mut(&observer)
            .unwrap()
            .known_equipment
            .identify(destination, stale)
            .unwrap();
        let observer_before = game.actors[&observer].known_equipment.clone();
        let old_owner = game.actors[&owner].known_equipment.clone();
        let next = ItemId(game.next_item_id);
        let plan = game
            .prepare_transfer(owner, source, Some(1), taking)
            .unwrap();
        assert_eq!(plan.result(), next);
        assert!(matches!(
            plan.outcome(taking),
            OutcomeKind::Taken { quantity: 1, .. } | OutcomeKind::Dropped { quantity: 1, .. }
        ));
        game.apply_prepared_transfer(&plan).unwrap();
        assert!(!game.items.contains_key(&destination));
        assert_eq!(game.items[&source].quantity, 2);
        assert_eq!(game.items[&next].quantity, 3);
        assert_eq!(game.items[&next].condition.hit_enchantment, 0);
        let facts = &game.actors[&owner].known_equipment;
        assert_eq!(
            facts.remembered_condition(next).unwrap().hit_enchantment,
            4,
            "shared stale memory is not refreshed from truth"
        );
        assert_eq!(
            facts.curse(next),
            None,
            "disagreeing memories are not identification"
        );
        assert_eq!(
            facts.remembered_condition(source),
            old_owner.remembered_condition(source)
        );
        assert_eq!(facts.curse(destination), Some(false));
        assert_eq!(game.actors[&observer].known_equipment, observer_before);
        assert!(game
            .prepare_transfer(owner, destination, Some(1), taking)
            .is_err());
        assert!(game.equipment_knowledge_valid(owner));
    }
}

#[test]
fn ammunition_split_copies_only_acting_owner_and_owned_transfer_copies_nobody() {
    let (mut game, owner, observer, source, destination) = ammunition_fixture();
    game.items.remove(&destination);
    game.actors
        .get_mut(&owner)
        .unwrap()
        .known_equipment
        .learn_curse(source, true)
        .unwrap();
    let observer_before = game.actors[&observer].known_equipment.clone();
    let plan = game
        .prepare_transfer(owner, source, Some(1), false)
        .unwrap();
    let split = plan.result();
    game.apply_prepared_transfer(&plan).unwrap();
    assert_eq!(game.actors[&owner].known_equipment.curse(split), Some(true));
    assert_eq!(
        game.actors[&owner]
            .known_equipment
            .remembered_condition(split),
        None
    );
    assert_eq!(game.actors[&observer].known_equipment, observer_before);
    let before_owner = game.actors[&owner].known_equipment.clone();
    let plan = game
        .prepare_owned_transfer(source, owner, observer, 1)
        .unwrap();
    let stolen = plan.result();
    game.apply_owned_transfer(plan).unwrap();
    assert_eq!(game.actors[&observer].known_equipment.curse(stolen), None);
    assert_eq!(game.actors[&owner].known_equipment, before_owner);
}

#[test]
fn ammunition_held_destination_stays_separate_and_source_holds_stay_exact() {
    let (mut game, owner, recipient, source, destination) = ammunition_fixture();
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
    for id in [source, destination] {
        game.items.edit(id, |item| item.holds.push(hold));
    }
    let expected = game.items[&destination].clone();
    let plan = game
        .prepare_owned_transfer(source, owner, recipient, 2)
        .unwrap();
    let result = plan.result();
    assert_ne!(result, destination);
    game.apply_owned_transfer(plan).unwrap();
    assert_eq!(game.items[&destination], expected);
    assert_eq!(game.items[&source].quantity, 1);
    assert_eq!(game.items[&source].holds, vec![hold]);
    assert_eq!(game.items[&result].quantity, 2);
    assert!(game.items[&result].holds.is_empty());
}

#[test]
fn ammunition_full_memory_does_not_block_split_or_mutate_existing_facts() {
    let (mut game, owner, observer, source, destination) = ammunition_fixture();
    game.items.remove(&destination);
    for raw in 1..=4096 {
        game.actors
            .get_mut(&owner)
            .unwrap()
            .known_equipment
            .learn_curse(ItemId(raw), true)
            .unwrap();
    }
    game.next_item_id = 5000;
    let before = game.actors[&owner].known_equipment.clone();
    let observer_before = game.actors[&observer].known_equipment.clone();
    let plan = game
        .prepare_transfer(owner, source, Some(1), false)
        .unwrap();
    game.apply_prepared_transfer(&plan).unwrap();
    assert_eq!(game.items[&plan.result()].quantity, 1);
    assert_eq!(game.actors[&owner].known_equipment, before);
    assert_eq!(game.actors[&observer].known_equipment, observer_before);
    assert!(
        game.identify_item(owner, plan.result()).is_err(),
        "explicit identify retains its existing memory-full refusal"
    );
}

#[test]
fn ammunition_fresh_merge_rechecks_exact_witnesses_and_allocator_without_mutation() {
    let (mut game, owner, recipient, source, destination) = ammunition_fixture();
    let plan = game
        .prepare_owned_transfer(source, owner, recipient, 1)
        .unwrap();
    game.items.edit(destination, |item| item.quantity += 1);
    let before = game.clone();
    assert!(game.apply_owned_transfer(plan).is_err());
    assert_eq!(game, before);
    game.next_item_id = u64::MAX;
    let before = game.clone();
    assert_eq!(
        game.prepare_owned_transfer(source, owner, recipient, 1)
            .unwrap_err(),
        GameError::IdentityExhausted
    );
    assert_eq!(game, before);
}

#[test]
fn ammunition_damage_references_validate_direct_placement_and_restore() {
    let (mut game, owner, _, source, _) = ammunition_fixture();
    let mut valid = (*game.items[&source].spec).clone();
    let known = game.content.rules().document().descriptors[0].id.clone();
    let component =
        crate::damage::DamageComponent::fixed(crate::combat::DamageType::Keen, Some(known), 3);
    let primary = component.key();
    valid.projectile.as_mut().unwrap().damage =
        crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap();
    let at = game.actors[&owner].location;
    game.place_item_stack(200, at, Some(owner), 2, valid.clone())
        .unwrap();
    let mut invalid = valid;
    let component = crate::damage::DamageComponent::fixed(
        crate::combat::DamageType::Keen,
        Some(crate::content_ids::DescriptorId::new("missing-projectile-descriptor").unwrap()),
        3,
    );
    let primary = component.key();
    invalid.projectile.as_mut().unwrap().damage =
        crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap();
    assert!(
        invalid.valid(),
        "numeric shape alone cannot prove content references"
    );
    let before = game.clone();
    assert!(game
        .place_item_stack(201, at, Some(owner), 2, invalid.clone())
        .is_err());
    assert_eq!(game, before);
    game.items
        .edit(source, |item| item.spec = tor_world::Shared::new(invalid));
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    assert!(Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).is_err());
}

#[test]
fn ammunition_generation_and_condition_reject_armor_and_unrelated_capabilities() {
    let (mut game, owner, _, source, _) = ammunition_fixture();
    let mut spec = (*game.items[&source].spec).clone();
    spec.equipment_generation = Some(crate::EquipmentGeneration::Weapon {
        curse_percent: 100,
        enchant_percent: 0,
        minimum: 2,
        maximum: 2,
    });
    let at = game.actors[&owner].location;
    let born = game
        .place_item_stack(200, at, Some(owner), 7, spec.clone())
        .unwrap();
    assert_eq!(game.items[&born].quantity, 7);
    assert_eq!(game.items[&born].condition.hit_enchantment, -2);
    assert!(game.items[&born].condition.cursed);
    assert!(game.items[&born].condition_valid());
    game.identify_item(owner, born).unwrap();
    assert_eq!(
        game.actors[&owner]
            .known_equipment
            .remembered_condition(born)
            .unwrap()
            .hit_enchantment,
        -2
    );
    for mutation in 0..4 {
        let mut item = game.items[&born].clone();
        match mutation {
            0 => item.condition.corrosion = 1,
            1 => item.condition.armor_penalty = 1,
            2 => item.condition.armor_enchantment = 1,
            _ => item.condition.protected = true,
        }
        assert!(!item.condition_valid());
    }
    spec.equipment_generation = Some(crate::EquipmentGeneration::Armor {
        curse_percent: 0,
        enchant_percent: 100,
        minimum: 1,
        maximum: 1,
    });
    assert!(!spec.valid());
    spec.equipment_generation = None;
    spec.edible = Some(crate::EdibleSpec {
        nutrition: crate::rules::AmountExpr::Fixed { amount: 1 },
    });
    assert!(
        !spec.valid(),
        "projectile and food are incompatible through shared capability shape validation"
    );
}
