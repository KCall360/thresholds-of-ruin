//! Per-theft merge indexing is value-keyed, local, and bounded by inventory work.
use super::{PlannedTheftSelection, QuantityRange};
use crate::{ActorId, CurrencyKind, Game, ItemId, ItemLocation, ItemSpec};
use std::num::NonZeroU64;
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}

fn game() -> Game {
    let mut game = Game::region_corridor(42, 1, crate::test_creatures::content());
    for id in [1, 2] {
        game.spawn_authored_actor(id, at(id as i32), NonZeroU64::new(100).unwrap())
            .unwrap();
    }
    game
}

fn gold(name: &str) -> ItemSpec {
    let mut spec = ItemSpec::ordinary(name.into());
    spec.class = crate::ItemClass::Coin;
    spec.currency = Some(CurrencyKind::Gold);
    spec.stackable = true;
    spec
}

fn steal(game: &mut Game, amount: u64) -> usize {
    let selection = PlannedTheftSelection::Currency {
        currency: CurrencyKind::Gold,
        amount: QuantityRange::new(amount, amount).unwrap(),
    };
    let mut rng = 9;
    let before = crate::diagnostics::work_counts().stack_candidates;
    let receipt = game
        .transfer_theft(ActorId(1), ActorId(2), &selection, &mut rng)
        .unwrap()
        .unwrap();
    let visited = crate::diagnostics::work_counts().stack_candidates - before;
    assert_eq!(receipt.quantity(), amount);
    assert_eq!(rng, 9, "fixed theft does not draw quantity randomness");
    visited
}

fn quantity_at(game: &Game, location: ItemLocation) -> u64 {
    game.items
        .at(location)
        .map(|id| game.items[&id].quantity)
        .sum()
}

#[test]
fn currency_merge_work_is_recipient_entries_plus_transferred_fragments() {
    for entries in [16_usize, 128] {
        let mut game = game();
        for index in 0..entries {
            let suffix = index as u64;
            game.place_item_stack(
                10 + suffix,
                at(1),
                Some(ActorId(1)),
                1,
                ItemSpec::ordinary(format!("recipient gear {index}")),
            )
            .unwrap();
            game.place_item_stack(
                1000 + suffix,
                at(2),
                Some(ActorId(2)),
                2,
                gold(&format!("victim mint {index}")),
            )
            .unwrap();
            // Another indexed bucket must not enlarge the recipient pass.
            game.place_item_stack(
                2000 + suffix,
                at(3),
                None,
                5,
                gold(&format!("ground mint {index}")),
            )
            .unwrap();
        }
        let total = game.items.values().map(|item| item.quantity).sum::<u64>();
        let visited = steal(&mut game, 2 * entries as u64);
        assert_eq!(
            visited,
            entries + entries,
            "each of {entries} recipient entries is indexed once and each fragment looks up once"
        );
        assert_eq!(
            game.items.values().map(|item| item.quantity).sum::<u64>(),
            total
        );
        assert_eq!(
            quantity_at(&game, ItemLocation::Carried(ActorId(1))),
            3 * entries as u64
        );
        assert_eq!(quantity_at(&game, ItemLocation::Carried(ActorId(2))), 0);
        assert_eq!(
            quantity_at(&game, ItemLocation::Ground(at(3))),
            5 * entries as u64
        );
        for index in 0..entries {
            let id = ItemId(1000 + index as u64);
            assert_eq!(game.items[&id].location, ItemLocation::Carried(ActorId(1)));
            assert_eq!(game.items[&id].quantity, 2);
        }
    }
}

#[test]
fn separately_allocated_equal_specs_merge_into_the_lowest_recipient_id() {
    let mut game = game();
    // Insertion order must not select the surviving recipient stack.
    for (id, owner, amount) in [(20, 1, 11), (10, 1, 7), (30, 2, 2), (31, 2, 3)] {
        game.place_item_stack(
            id,
            at(owner as i32),
            Some(ActorId(owner)),
            amount,
            gold("equal gold"),
        )
        .unwrap();
    }
    for id in [ItemId(20), ItemId(30), ItemId(31)] {
        assert_eq!(game.items[&ItemId(10)].spec, game.items[&id].spec);
        assert!(!game.items[&ItemId(10)]
            .spec
            .shares_storage(&game.items[&id].spec));
    }
    steal(&mut game, 5);
    assert_eq!(game.items[&ItemId(10)].quantity, 12);
    assert_eq!(game.items[&ItemId(20)].quantity, 11);
    assert!(!game.items.contains_key(&ItemId(30)));
    assert!(!game.items.contains_key(&ItemId(31)));
    assert_eq!(quantity_at(&game, ItemLocation::Carried(ActorId(1))), 23);
    assert_eq!(quantity_at(&game, ItemLocation::Carried(ActorId(2))), 0);
}

#[test]
fn full_spec_difference_stays_separate_and_new_destinations_are_reused() {
    let mut game = game();
    let plain = gold("same identity and name");
    let mut different = plain.clone();
    different
        .properties
        .insert("mint".into(), "distinct full spec".into());
    assert_eq!(plain.identity, different.identity);
    assert_eq!(plain.name, different.name);
    assert_ne!(plain, different);
    game.place_item_stack(100, at(1), Some(ActorId(1)), 3, plain.clone())
        .unwrap();
    game.place_item_stack(10, at(2), Some(ActorId(2)), 2, different.clone())
        .unwrap();
    game.place_item_stack(20, at(2), Some(ActorId(2)), 3, different.clone())
        .unwrap();
    game.place_item_stack(30, at(2), Some(ActorId(2)), 4, plain.clone())
        .unwrap();

    steal(&mut game, 9);
    assert_eq!(game.items[&ItemId(100)].quantity, 7);
    assert_eq!(*game.items[&ItemId(100)].spec, plain);
    assert_eq!(game.items[&ItemId(10)].quantity, 5);
    assert_eq!(*game.items[&ItemId(10)].spec, different);
    assert_eq!(
        game.items[&ItemId(10)].location,
        ItemLocation::Carried(ActorId(1))
    );
    assert!(!game.items.contains_key(&ItemId(20)));
    assert!(!game.items.contains_key(&ItemId(30)));
    assert_eq!(game.items.at(ItemLocation::Carried(ActorId(1))).count(), 2);
    assert_eq!(quantity_at(&game, ItemLocation::Carried(ActorId(1))), 12);
    assert_eq!(quantity_at(&game, ItemLocation::Carried(ActorId(2))), 0);
}

#[test]
fn empty_victim_does_not_build_a_recipient_merge_index() {
    let mut game = game();
    for index in 0..8 {
        let spec = if index % 2 == 0 {
            gold(&format!("recipient gold {index}"))
        } else {
            ItemSpec::ordinary(format!("recipient gear {index}"))
        };
        game.place_item_stack(10 + index, at(1), Some(ActorId(1)), 1, spec)
            .unwrap();
    }
    let before = game.clone();
    let count = crate::diagnostics::work_counts().stack_candidates;
    let mut rng = 9;
    assert!(game
        .transfer_theft(
            ActorId(1),
            ActorId(2),
            &PlannedTheftSelection::Currency {
                currency: CurrencyKind::Gold,
                amount: QuantityRange::new(3, 3).unwrap(),
            },
            &mut rng,
        )
        .unwrap()
        .is_none());
    assert_eq!(
        crate::diagnostics::work_counts().stack_candidates - count,
        0
    );
    assert_eq!(rng, 9);
    assert_eq!(game, before);
}

#[test]
fn currency_merge_index_distinguishes_condition_and_reuses_matching_fragments() {
    let mut game = game();
    for (id, owner) in [(10, 1), (11, 1), (20, 2), (21, 2)] {
        game.place_item_stack(
            id,
            at(owner as i32),
            Some(ActorId(owner)),
            2,
            gold("same gold"),
        )
        .unwrap();
    }
    // Exercise exact-state indexing without authorizing mutable currency or
    // stackable equipment content. The transfer kernel must not discard state.
    for id in [11, 20, 21] {
        game.items
            .edit(ItemId(id), |item| item.condition.hit_enchantment = 1);
    }
    assert_eq!(steal(&mut game, 4), 4);
    assert_eq!(game.items[&ItemId(10)].quantity, 2);
    assert_eq!(game.items[&ItemId(11)].quantity, 6);
    assert_eq!(game.items[&ItemId(11)].condition.hit_enchantment, 1);
    assert!(!game.items.contains_key(&ItemId(20)));
    assert!(!game.items.contains_key(&ItemId(21)));
}
