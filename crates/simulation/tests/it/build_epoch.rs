//! A permanent-build conflict token survives value-equivalent edits and restores.
use super::creature_fixture as fixture;
use tor_simulation::{
    content_ids::{AbilityId, ConditionId},
    creatures::{record::CreatureRecord, CreatureState, EquipmentSource},
    progression::HdSource,
    rules::{Modifier, ModifierOp, Scope, Stat},
    ActorId, EquipmentSlotId, ItemId,
};

fn value(creature: &CreatureState) -> serde_json::Value {
    serde_json::to_value(CreatureRecord::capture(creature)).unwrap()
}

fn epoch(creature: &CreatureState) -> u64 {
    let recorded = value(creature)["build_epoch"]
        .as_u64()
        .expect("current creature records require a positive build epoch");
    assert_eq!(recorded, creature.build_epoch().get());
    recorded
}

#[test]
fn phase5b_new_creature_has_a_persisted_positive_build_epoch() {
    let creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    assert_eq!(epoch(&creature), 1);
    let record: CreatureRecord = serde_json::from_value(value(&creature)).unwrap();
    let restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
}

#[test]
fn phase5b_permanent_edits_and_return_to_equal_values_each_advance_epoch() {
    let original = fixture::build(fixture::species());
    let mut creature = CreatureState::new(original.clone()).unwrap();
    let mut evolved = original.clone();
    evolved.add_hit_die(HdSource::Racial, 42).unwrap();
    creature.damage(3);
    let cooldown = AbilityId::new("spark_lance_fire").unwrap();
    creature.start_cooldown(&cooldown, 100).unwrap();
    let previous = creature.costs().resources().clone();
    creature.rebuild(evolved).unwrap();
    assert_eq!(epoch(&creature), 2);
    assert_eq!(creature.health().injury(), 3);
    assert_eq!(creature.cooldowns().remaining(&cooldown), 100);
    for resource in tor_simulation::resources::Resource::ALL {
        assert_eq!(
            creature.costs().resources().balance(resource),
            previous.balance(resource)
        );
    }
    creature.rebuild(original.clone()).unwrap();
    assert_eq!(creature.build(), &original);
    assert_eq!(epoch(&creature), 3);
    creature.rebuild(original).unwrap();
    assert_eq!(
        epoch(&creature),
        3,
        "an identical permanent build is no mutation"
    );
}

#[test]
fn phase5b_temporary_sources_damage_cooldowns_and_time_preserve_build_epoch() {
    let mut creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    let mut original = creature.build().clone();
    original.add_hit_die(HdSource::Racial, 42).unwrap();
    creature.rebuild(original.clone()).unwrap();
    assert_eq!(epoch(&creature), 2);
    creature.damage(3);
    assert_eq!(epoch(&creature), 2);
    creature
        .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
        .unwrap();
    assert_eq!(epoch(&creature), 2);
    creature
        .start_cooldown(&AbilityId::new("spark_lance_fire").unwrap(), 100)
        .unwrap();
    assert_eq!(epoch(&creature), 2);
    let resource = tor_simulation::resources::Resource::Stamina;
    let previous = creature.costs().resources().balance(resource);
    creature
        .start_cost(
            tor_simulation::IntentionId(7),
            tor_simulation::costs::ResourceCost {
                resource,
                start: 1,
                resolution: 0,
            },
        )
        .unwrap();
    creature
        .finish_cost(tor_simulation::IntentionId(7))
        .unwrap();
    assert_eq!(creature.costs().resources().balance(resource), previous - 1);
    assert_eq!(epoch(&creature), 2);
    assert_eq!(creature.restore_resource(resource, 1), 1);
    assert_eq!(creature.costs().resources().balance(resource), previous);
    assert_eq!(epoch(&creature), 2);
    creature
        .set_equipment(vec![EquipmentSource {
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: 4,
                },
            }],
            attack: None,
        }])
        .unwrap();
    assert_eq!(epoch(&creature), 2);
    creature.advance_active(1_200);
    assert_eq!(epoch(&creature), 2);
    creature.set_equipment(vec![]).unwrap();
    assert_eq!(creature.build(), &original);
    assert_eq!(epoch(&creature), 2);
}

#[test]
fn phase5b_permanent_purchase_advances_epoch_without_adding_a_hit_die() {
    let mut creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    let original_hd = creature.build().ledger().clone();
    let mut purchased = creature.build().clone();
    purchased
        .purchase(
            0,
            tor_simulation::content_ids::PointPoolId::new("advancement").unwrap(),
            tor_simulation::creatures::PurchaseSelection::SkillRank(
                tor_simulation::attributes::Skill::Athletics,
            ),
        )
        .unwrap();
    creature.rebuild(purchased).unwrap();
    assert_eq!(epoch(&creature), 2);
    assert_eq!(creature.build().ledger(), &original_hd);
    let record: CreatureRecord = serde_json::from_value(value(&creature)).unwrap();
    assert_eq!(
        record
            .restore(creature.build().content().clone(), vec![])
            .unwrap(),
        creature
    );
}

#[test]
fn phase5b_record_rejects_missing_zero_and_wrong_type_build_epochs() {
    let creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    let original = value(&creature);
    for replacement in [
        None,
        Some(serde_json::json!(0)),
        Some(serde_json::json!(-1)),
        Some(serde_json::json!(true)),
        Some(serde_json::json!("1")),
        Some(serde_json::json!(1.5)),
    ] {
        let mut damaged = original.clone();
        match replacement {
            Some(replacement) => {
                damaged["build_epoch"] = replacement;
            }
            None => {
                damaged.as_object_mut().unwrap().remove("build_epoch");
            }
        }
        assert!(serde_json::from_value::<CreatureRecord>(damaged).is_err());
    }
}

#[test]
fn phase5b_exhausted_epoch_rejects_actual_build_change_atomically() {
    let creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    let mut encoded = value(&creature);
    encoded["build_epoch"] = serde_json::json!(u64::MAX);
    let record: CreatureRecord = serde_json::from_value(encoded).unwrap();
    let mut exhausted = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    let before = exhausted.clone();
    let mut evolved = exhausted.build().clone();
    evolved.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(
        exhausted.rebuild(evolved),
        Err(tor_simulation::creatures::CreatureStateError::BuildEpochExhausted)
    );
    assert_eq!(exhausted, before);
    exhausted.rebuild(exhausted.build().clone()).unwrap();
    assert_eq!(exhausted, before, "no-op rebuild needs no fresh epoch");
}

#[test]
fn phase5b_last_available_epoch_commits_once_and_survives_restore() {
    let original = fixture::build(fixture::species());
    let creature = CreatureState::new(original.clone()).unwrap();
    let mut encoded = value(&creature);
    encoded["build_epoch"] = serde_json::json!(u64::MAX - 1);
    let record: CreatureRecord = serde_json::from_value(encoded).unwrap();
    let mut creature = record.restore(original.content().clone(), vec![]).unwrap();
    let mut evolved = original.clone();
    evolved.add_hit_die(HdSource::Racial, 42).unwrap();
    creature.rebuild(evolved).unwrap();
    assert_eq!(epoch(&creature), u64::MAX);
    let record: CreatureRecord = serde_json::from_value(value(&creature)).unwrap();
    let mut restored = record.restore(original.content().clone(), vec![]).unwrap();
    assert_eq!(restored, creature);
    assert!(restored.rebuild(original).is_err());
    assert_eq!(restored, creature);
}

#[test]
fn phase5b_game_rebuild_and_checkpoint_preserve_non_initial_epoch() {
    use tor_simulation::{CreatureIdentity, Game};
    use tor_world::{Location, Position, RegionId};
    let original = fixture::build(fixture::species());
    let content = original.content().clone();
    let mut game = Game::two_room_in_stone(42, content.clone());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "Epoch subject".into(),
            faction: "neutral".into(),
        },
        original.clone(),
    )
    .unwrap();
    let mut evolved = original.clone();
    evolved.add_hit_die(HdSource::Racial, 42).unwrap();
    game.rebuild_creature(actor, evolved).unwrap();
    game.rebuild_creature(actor, original).unwrap();
    assert_eq!(epoch(&game.creature(actor).unwrap()), 3);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let bytes = serde_json::to_vec(&(game.checkpoint(&mut shared), shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    let restored = Game::restore_checkpoint(snapshot, &shared, content).unwrap();
    assert_eq!(restored, game);
    assert_eq!(epoch(&restored.creature(actor).unwrap()), 3);
}

#[test]
fn phase5b_failed_combined_equipment_rebuild_does_not_increment_epoch() {
    let mut creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    creature
        .set_equipment(vec![EquipmentSource {
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: 999_950,
                },
            }],
            attack: None,
        }])
        .unwrap();
    let before = creature.clone();
    let mut larger = creature.build().clone();
    for seed in 1..=64 {
        larger.add_hit_die(HdSource::Racial, seed).unwrap();
    }
    assert!(
        creature.rebuild(larger).is_err(),
        "combined maximum must exceed the numeric bound"
    );
    assert_eq!(creature, before);
    assert_eq!(epoch(&creature), 1);
}
