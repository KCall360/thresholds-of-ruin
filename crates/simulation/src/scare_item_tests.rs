//! Pickup is an ordinary paid transfer: exact requested units either become
//! cursed in inventory or crumble on the floor. Previews do neither.
use super::*;
use std::num::NonZeroU64;
use tor_world::{Position, RegionId};

fn fixture() -> (Game, ActorId, ItemId) {
    let mut document = crate::test_creatures::ruleset().document().clone();
    // The ordinary Fear condition is source-scoped (its penalty is versus its
    // source). Ground wards require a source-independent finite objective, so
    // author a distinct fixture condition instead of repurposing Fear.
    let fear = crate::content_ids::ConditionId::new("fear").unwrap();
    let id = crate::content_ids::ConditionId::new("fixture.scared").unwrap();
    let mut scared = document
        .conditions
        .iter()
        .find(|def| def.id == fear)
        .unwrap()
        .clone();
    scared.id = id.clone();
    scared.family = crate::content_ids::ConditionFamilyId::new("fixture.scared").unwrap();
    scared.name = "Fixture Scared".into();
    scared.source_scoped = false;
    scared.modifiers.clear();
    scared.timing = crate::rules::ConditionTiming::ActorActive {
        on_expiry: crate::rules::ConditionExpiry::Remove,
    };
    document.conditions.push(scared);
    let rules = crate::rules::Ruleset::validate(document).unwrap();
    let species = crate::test_creatures::species();
    let content = crate::creatures::Content::new(
        rules,
        crate::creatures::ScenarioCatalog {
            species: [(species.id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let mut game = Game::region_corridor(42, 1, content);
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let owner = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    let mut spec = ItemSpec::ordinary("unread scroll".into());
    spec.class = ItemClass::Scroll;
    spec.stackable = true;
    spec.ground_scare = Some(crate::control_scrolls::ScareSpec {
        condition: id,
        duration: 1000,
    });
    spec.validate_rules(game.content.rules()).unwrap();
    let item = game.place_item_stack(100, at, None, 3, spec).unwrap();
    (game, owner, item)
}

#[test]
fn scare_pickup_preview_is_pure_and_partial_take_curses_only_transferred_units() {
    let (mut game, owner, source) = fixture();
    let before = game.clone();
    let plan = game.prepare_transfer(owner, source, Some(1), true).unwrap();
    assert_eq!(game, before);
    let result = plan.result();
    game.apply_prepared_transfer(&plan).unwrap();
    assert_eq!(game.items[&source].quantity, 2);
    assert!(!game.items[&source].condition.cursed);
    assert_eq!(game.items[&result].quantity, 1);
    assert!(game.items[&result].condition.cursed);
    assert_eq!(game.items[&result].location, ItemLocation::Carried(owner));
    assert!(game.items[&result].condition_valid());
}

#[test]
fn scare_pickups_merge_only_after_the_new_per_unit_curse_and_drop_preserves_it() {
    let (mut game, owner, source) = fixture();
    let first = game.prepare_transfer(owner, source, Some(1), true).unwrap();
    game.apply_prepared_transfer(&first).unwrap();
    let second = game.prepare_transfer(owner, source, Some(1), true).unwrap();
    let result = second.result();
    game.apply_prepared_transfer(&second).unwrap();
    assert_eq!(game.items[&result].quantity, 2);
    assert!(game.items[&result].condition.cursed);
    assert!(!game.items.contains_key(&first.result()));
    assert_eq!(game.items[&source].quantity, 1);
    assert!(!game.items[&source].condition.cursed);
    let dropped = game
        .prepare_transfer(owner, result, Some(1), false)
        .unwrap();
    game.apply_prepared_transfer(&dropped).unwrap();
    assert!(game.items[&dropped.result()].condition.cursed);
    assert_ne!(
        dropped.result(),
        source,
        "uncursed floor units cannot absorb cursed units"
    );
}

#[test]
fn cursed_scare_pickup_crumbles_requested_quantity_without_transfer_or_id_allocation() {
    let (mut game, owner, source) = fixture();
    game.items
        .edit(source, |item| item.condition.cursed = true)
        .unwrap();
    let next = game.next_item_id;
    let before = game.clone();
    let plan = game.prepare_transfer(owner, source, Some(2), true).unwrap();
    assert_eq!(game, before);
    assert_eq!(
        plan.outcome(true),
        OutcomeKind::Crumbled {
            item: source,
            quantity: 2
        }
    );
    game.apply_prepared_transfer(&plan).unwrap();
    assert_eq!(game.items[&source].quantity, 1);
    assert_eq!(game.items.at(ItemLocation::Carried(owner)).count(), 0);
    assert_eq!(game.next_item_id, next);
    let final_unit = game.prepare_transfer(owner, source, None, true).unwrap();
    game.apply_prepared_transfer(&final_unit).unwrap();
    assert!(!game.items.contains_key(&source));
}

#[test]
fn scare_stale_pickup_witness_rejects_before_mutating_the_source_or_destination() {
    let (mut game, owner, source) = fixture();
    let plan = game.prepare_transfer(owner, source, Some(1), true).unwrap();
    game.items
        .edit(source, |item| item.condition.cursed = true)
        .unwrap();
    let before = game.clone();
    assert_eq!(
        game.apply_prepared_transfer(&plan),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
}

#[test]
fn scare_curse_and_quantity_survive_checkpoint_and_requested_crumble_costs_normal_take_time() {
    let (mut game, owner, source) = fixture();
    game.items
        .edit(source, |item| item.condition.cursed = true)
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut reopened =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(reopened, game);
    let before = reopened.tick;
    let outcome = reopened
        .act(
            owner,
            crate::Action::Take {
                item: source,
                quantity: Some(1),
            },
        )
        .unwrap();
    assert_eq!(
        outcome.kind,
        OutcomeKind::Crumbled {
            item: source,
            quantity: 1
        }
    );
    assert_eq!(reopened.actors[&owner].ready_at - before, 50);
    assert_eq!(reopened.items[&source].quantity, 2);
    assert!(reopened.items[&source].condition.cursed);
}

#[test]
fn remove_curse_freezes_and_uncurses_carried_scare_stacks_then_pickup_curses_again() {
    let (mut game, owner, source) = fixture();
    let taken = game.prepare_transfer(owner, source, Some(2), true).unwrap();
    game.apply_prepared_transfer(&taken).unwrap();
    let carried = taken.result();
    let plan = game
        .compile_equipment_alteration(
            owner,
            &crate::EquippedSelection::All,
            crate::EquipmentAlteration::RemoveCurse,
        )
        .unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(value["targets"], serde_json::json!([]));
    assert_eq!(value["carried_targets"].as_array().unwrap().len(), 1);
    let resolved = crate::ResolvedEquipmentAlteration { plan, hit: false };
    game.validate_equipment_alterations(
        owner,
        &[crate::abilities::ResolvedEffect::AlterEquipment {
            alteration: resolved.clone(),
        }],
    )
    .unwrap();
    let mut outcome = game.actor_outcome(owner).unwrap();
    assert!(game
        .alter_equipment_in_outcome(&mut outcome, &resolved)
        .unwrap());
    game.apply_outcome(outcome).unwrap();
    assert!(!game.items[&carried].condition.cursed);
    assert_eq!(game.items[&carried].quantity, 2);
    let drop = game
        .prepare_transfer(owner, carried, Some(1), false)
        .unwrap();
    game.apply_prepared_transfer(&drop).unwrap();
    let retake = game
        .prepare_transfer(owner, drop.result(), Some(1), true)
        .unwrap();
    assert!(matches!(
        retake.outcome(true),
        OutcomeKind::Taken { quantity: 1, .. }
    ));
    game.apply_prepared_transfer(&retake).unwrap();
    assert!(game.items[&retake.result()].condition.cursed);
}

#[test]
fn remove_curse_carried_witness_is_strict_and_rejects_changed_or_dropped_sources() {
    let (mut game, owner, source) = fixture();
    let taken = game.prepare_transfer(owner, source, Some(1), true).unwrap();
    game.apply_prepared_transfer(&taken).unwrap();
    let plan = game
        .compile_equipment_alteration(
            owner,
            &crate::EquippedSelection::All,
            crate::EquipmentAlteration::RemoveCurse,
        )
        .unwrap();
    let positive = serde_json::to_value(&plan).unwrap();
    assert!(serde_json::from_value::<crate::PlannedEquipmentAlteration>(positive.clone()).is_ok());
    let mut missing = positive.clone();
    missing.as_object_mut().unwrap().remove("carried_targets");
    assert!(serde_json::from_value::<crate::PlannedEquipmentAlteration>(missing).is_err());
    let mut duplicate = positive;
    let entry = duplicate["carried_targets"][0].clone();
    duplicate["carried_targets"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    assert!(serde_json::from_value::<crate::PlannedEquipmentAlteration>(duplicate).is_err());
    let drop = game
        .prepare_transfer(owner, taken.result(), None, false)
        .unwrap();
    game.apply_prepared_transfer(&drop).unwrap();
    let before = game.clone();
    let resolved = crate::ResolvedEquipmentAlteration { plan, hit: false };
    assert_eq!(
        game.validate_equipment_alterations(
            owner,
            &[crate::abilities::ResolvedEffect::AlterEquipment {
                alteration: resolved
            }]
        ),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
}

#[test]
fn scare_pickup_rejects_bad_quantity_without_cursing_or_crumbling_any_units() {
    for cursed in [false, true] {
        let (mut game, owner, source) = fixture();
        game.items
            .edit(source, |item| item.condition.cursed = cursed)
            .unwrap();
        let before = game.clone();
        for quantity in [0, 4, u64::MAX] {
            assert_eq!(
                game.prepare_transfer(owner, source, Some(quantity), true),
                Err(GameError::InvalidQuantity)
            );
            assert_eq!(game, before);
        }
    }
}

#[test]
fn cursed_scare_crumble_does_not_require_a_fresh_identity() {
    let (mut game, owner, source) = fixture();
    game.items
        .edit(source, |item| item.condition.cursed = true)
        .unwrap();
    game.next_item_id = u64::MAX;
    let plan = game.prepare_transfer(owner, source, Some(1), true).unwrap();
    game.apply_prepared_transfer(&plan).unwrap();
    assert_eq!(game.items[&source].quantity, 2);
    assert_eq!(game.next_item_id, u64::MAX);
}
