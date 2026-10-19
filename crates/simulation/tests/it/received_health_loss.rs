//! Received loss has its own actor-active safety interval, independent of healing.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use tor_simulation::{
    creatures::{record::CreatureRecord, CreatureState, EquipmentSource},
    health_recovery::RecoveryNutrition,
    rules::{Modifier, ModifierOp, Ruleset, RulesetDocument, Scope, Stat},
    EquipmentSlotId, ItemId,
};

fn recorded(creature: &CreatureState) -> Value {
    serde_json::to_value(CreatureRecord::capture(creature)).unwrap()
}
fn remaining(creature: &CreatureState) -> u64 {
    let value = recorded(creature)["received_health_loss_remaining"]
        .as_u64()
        .expect("current records retain the received-Health-loss interval");
    assert_eq!(value, creature.received_health_loss_remaining());
    value
}
fn creature() -> CreatureState {
    CreatureState::new(fixture::build(fixture::species())).unwrap()
}

#[test]
fn phase5_received_loss_survives_full_heal_and_expires_at_exact_active_boundary() {
    let mut subject = creature();
    assert_eq!(remaining(&subject), 0);
    assert_eq!(subject.damage(3).applied, 3);
    assert_eq!(remaining(&subject), 600);
    assert_eq!(subject.heal(u32::MAX), 3);
    assert_eq!(subject.health().injury(), 0);
    assert_eq!(subject.health_recovery().elapsed(), 0);
    assert_eq!(subject.next_change_in(), Some(600));
    subject.advance_active(599);
    assert_eq!(remaining(&subject), 1);
    assert_eq!(subject.next_change_in(), Some(1));
    let record: CreatureRecord = serde_json::from_value(recorded(&subject)).unwrap();
    let mut restored = record
        .restore(subject.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, subject);
    for state in [&mut subject, &mut restored] {
        state.advance_active(1);
        assert_eq!(remaining(state), 0);
        assert_eq!(state.next_change_in(), None);
        state.advance_active(u64::MAX);
        assert_eq!(remaining(state), 0);
    }
    assert_eq!(restored, subject);
}

#[test]
fn phase5_repeated_received_loss_renews_but_zero_damage_does_not() {
    let mut subject = creature();
    assert_eq!(subject.damage(0).applied, 0);
    assert_eq!(remaining(&subject), 0);
    subject.damage(1);
    subject.advance_active(17);
    assert_eq!(subject.damage(0).applied, 0);
    assert_eq!(remaining(&subject), 583);
    subject.damage(1);
    assert_eq!(remaining(&subject), 600);
    subject
        .set_recovery_nutrition(RecoveryNutrition::Stopped)
        .unwrap();
    subject.advance_active(600);
    assert_eq!(remaining(&subject), 0);
    assert_eq!(subject.health().injury(), 2);
}

#[test]
fn phase5_maximum_health_changes_and_rebuild_do_not_manufacture_received_loss() {
    let mut subject = creature();
    let before = subject.health().current();
    subject
        .set_equipment(vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            attack: None,
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Health,
                    value: -1,
                },
            }],
        }])
        .unwrap();
    assert!(subject.health().current() < before);
    assert_eq!(remaining(&subject), 0);
    subject.damage(1);
    subject.advance_active(123);
    let mut changed = subject.build().clone();
    changed
        .add_hit_die(tor_simulation::progression::HdSource::Racial, 42)
        .unwrap();
    subject.rebuild(changed).unwrap();
    subject.set_equipment(vec![]).unwrap();
    assert_eq!(remaining(&subject), 477);
}

#[test]
fn phase5_death_freezes_received_loss_instead_of_resetting_it() {
    let mut subject = creature();
    subject.damage(u32::MAX);
    assert!(subject.health().dead());
    assert_eq!(remaining(&subject), 600);
    assert_eq!(subject.next_change_in(), None);
    subject.advance_active(u64::MAX);
    assert_eq!(remaining(&subject), 600);
    let record: CreatureRecord = serde_json::from_value(recorded(&subject)).unwrap();
    assert_eq!(
        record
            .restore(subject.build().content().clone(), vec![])
            .unwrap(),
        subject
    );
}

#[test]
fn phase5_received_loss_records_require_current_bounded_named_interval() {
    let subject = creature();
    let original = recorded(&subject);
    assert_eq!(original["received_health_loss_remaining"], json!(0));
    let restore = |value| {
        serde_json::from_value::<CreatureRecord>(value).is_ok_and(|record| {
            record
                .restore(subject.build().content().clone(), vec![])
                .is_ok()
        })
    };
    for remaining in [0, 1, 599, 600] {
        let mut value = original.clone();
        value["received_health_loss_remaining"] = json!(remaining);
        assert!(restore(value));
    }
    let mut missing = original.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("received_health_loss_remaining");
    assert!(!restore(missing));
    for invalid in [
        json!(null),
        json!(-1),
        json!(601),
        json!(u64::MAX),
        json!({}),
        json!([]),
    ] {
        let mut value = original.clone();
        value["received_health_loss_remaining"] = invalid;
        assert!(!restore(value));
    }
    let text = serde_json::to_string(&original).unwrap().replace(
        "\"received_health_loss_remaining\":0",
        "\"received_health_loss_remaining\":0,\"received_health_loss_remaining\":0",
    );
    assert!(serde_json::from_str::<CreatureRecord>(&text).is_err());
}

#[test]
fn phase5_advancement_policy_is_authored_required_and_bounded() {
    let mut value = serde_json::to_value(fixture::ruleset().document()).unwrap();
    value["advancement"] = json!({"purchase_recovery":0,"respec_prepare":600,"respec_recovery":0,"received_loss_interval":600,"respec_ration":value["advancement"]["respec_ration"].clone()});
    let parsed = serde_json::from_value::<RulesetDocument>(value.clone());
    assert!(
        parsed.is_ok(),
        "an explicit valid advancement policy must decode: {parsed:?}"
    );
    assert!(Ruleset::validate(parsed.unwrap()).is_ok());
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("advancement");
    assert!(serde_json::from_value::<RulesetDocument>(missing).is_err());
    for field in [
        "purchase_recovery",
        "respec_prepare",
        "respec_recovery",
        "received_loss_interval",
    ] {
        let mut missing = value.clone();
        missing["advancement"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<RulesetDocument>(missing).is_err());
    }
    let mut unknown = value.clone();
    unknown["advancement"]["unexpected"] = json!(1);
    assert!(serde_json::from_value::<RulesetDocument>(unknown).is_err());
    let mut array = value.clone();
    array["advancement"] = json!([0, 600, 0, 600]);
    assert!(serde_json::from_value::<RulesetDocument>(array).is_err());
    let duplicate = serde_json::to_string(&value).unwrap().replace(
        "\"purchase_recovery\":0",
        "\"purchase_recovery\":0,\"purchase_recovery\":0",
    );
    assert!(serde_json::from_str::<RulesetDocument>(&duplicate).is_err());
    for (field, invalid) in [
        ("received_loss_interval", 0),
        ("respec_prepare", 0),
        ("received_loss_interval", u64::MAX),
        ("respec_prepare", u64::MAX),
        ("purchase_recovery", u64::MAX),
        ("respec_recovery", u64::MAX),
    ] {
        let mut invalid_value = value.clone();
        invalid_value["advancement"][field] = json!(invalid);
        assert!(
            !serde_json::from_value::<RulesetDocument>(invalid_value)
                .is_ok_and(|document| Ruleset::validate(document).is_ok()),
            "{field}={invalid}"
        );
    }
}

use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::{
    abilities::{Outcome, OutcomeStatus, ResolvedEffect},
    Action, ActorId, CreatureIdentity, Game, MemoryRecords, RegionTransition,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn game() -> Game {
    let build = fixture::build(fixture::species());
    let world = World::new(
        [1, 3]
            .into_iter()
            .map(|id| Region {
                id: RegionId(id),
                name: format!("loss timer room {id}"),
                bounds: Extent::new(8, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, build.content().clone());
    for region in [1, 3] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(region),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(1).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {region}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        Default::default(),
    )
    .unwrap();
    game.add_default_reference_points().unwrap();
    game.refresh_navigation();
    game
}
fn rest(game: &mut Game, ticks: u64) {
    let until = game.tick().checked_add(ticks).unwrap();
    while game.tick() < until {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(game.tick(), until);
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}
fn effect(game: &mut Game, actor: ActorId, target: ActorId, effects: Vec<ResolvedEffect>) {
    game.apply(Outcome {
        actor,
        target,
        ability: None,
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects,
    })
    .unwrap();
}

#[test]
fn phase5_received_loss_resets_recipient_only_with_self_and_multicomponent_damage() {
    let mut state = game();
    effect(
        &mut state,
        ActorId(1),
        ActorId(2),
        vec![
            ResolvedEffect::Damage { amount: 1 },
            ResolvedEffect::Damage { amount: 2 },
        ],
    );
    assert_eq!(remaining(&state.creature(ActorId(1)).unwrap()), 0);
    assert_eq!(remaining(&state.creature(ActorId(2)).unwrap()), 600);
    rest(&mut state, 123);
    effect(
        &mut state,
        ActorId(1),
        ActorId(2),
        vec![ResolvedEffect::Damage { amount: 0 }],
    );
    assert_eq!(remaining(&state.creature(ActorId(2)).unwrap()), 477);
    effect(
        &mut state,
        ActorId(1),
        ActorId(1),
        vec![ResolvedEffect::Damage { amount: 1 }],
    );
    assert_eq!(remaining(&state.creature(ActorId(1)).unwrap()), 600);
    assert_eq!(remaining(&state.creature(ActorId(2)).unwrap()), 477);
    rest(&mut state, 7);
    effect(
        &mut state,
        ActorId(1),
        ActorId(2),
        vec![ResolvedEffect::Damage { amount: 1 }],
    );
    assert_eq!(remaining(&state.creature(ActorId(1)).unwrap()), 593);
    assert_eq!(remaining(&state.creature(ActorId(2)).unwrap()), 600);
}

#[test]
fn phase5_received_loss_survives_full_heal_freeze_and_restart_without_catchup() {
    for detached in [false, true] {
        let mut state = game();
        effect(
            &mut state,
            ActorId(1),
            ActorId(2),
            vec![
                ResolvedEffect::Damage { amount: 1 },
                ResolvedEffect::Heal { amount: 1 },
            ],
        );
        rest(&mut state, 137);
        let projected = state.creature(ActorId(2)).unwrap().into_owned();
        assert_eq!(remaining(&projected), 463);
        let snapshot = state.clone();
        assert_eq!(state.creature(ActorId(2)).unwrap().as_ref(), &projected);
        assert_eq!(
            state, snapshot,
            "projection must not settle the authoritative actor"
        );
        let mut records = MemoryRecords::default();
        state
            .transition_regions(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(1)]),
                    loaded: if detached {
                        BTreeSet::from([RegionId(1)])
                    } else {
                        BTreeSet::from([RegionId(1), RegionId(3)])
                    },
                },
                &mut records,
            )
            .unwrap();
        assert_eq!(state.next_creature_event_tick(), None);
        rest(&mut state, 1_200);
        state = round_trip(&state);
        state
            .transition_regions(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(1), RegionId(3)]),
                    loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
                },
                &mut records,
            )
            .unwrap();
        assert_eq!(state.creature(ActorId(2)).unwrap().as_ref(), &projected);
        assert_eq!(state.next_creature_event_tick(), Some(state.tick() + 463));
        let mut replay = round_trip(&state);
        for game in [&mut state, &mut replay] {
            rest(game, 462);
            assert_eq!(remaining(&game.creature(ActorId(2)).unwrap()), 1);
            rest(game, 1);
            assert_eq!(remaining(&game.creature(ActorId(2)).unwrap()), 0);
            assert_eq!(game.next_creature_event_tick(), None);
        }
        assert_eq!(state, replay);
    }
}

#[test]
fn phase5_received_loss_uses_authored_interval_at_numeric_limit_without_overflow() {
    let original = fixture::build(fixture::species());
    for interval in [1, tor_simulation::limits::NUMERIC as u64] {
        let mut value = serde_json::to_value(original.ruleset().document()).unwrap();
        value["advancement"] = json!({"purchase_recovery":0,"respec_prepare":600,"respec_recovery":0,"received_loss_interval":interval,"respec_ration":value["advancement"]["respec_ration"].clone()});
        let document: RulesetDocument =
            serde_json::from_value(value).expect("authored policy decodes");
        let content = tor_simulation::creatures::Content::new(
            Ruleset::validate(document).unwrap(),
            original.content().catalog().clone(),
        )
        .unwrap();
        let build = tor_simulation::creatures::CreatureBuild::from_recorded(
            content,
            original.species_id().clone(),
            original.initial_attributes(),
            original.ledger().clone(),
            original.binding(),
            original.advancement().clone(),
            vec![],
        )
        .unwrap();
        let mut subject = CreatureState::new(build).unwrap();
        subject.damage(1);
        subject.heal(1);
        assert_eq!(remaining(&subject), interval);
        subject.advance_active(interval - 1);
        assert_eq!(remaining(&subject), 1);
        subject.advance_active(u64::MAX);
        assert_eq!(remaining(&subject), 0);
        assert_eq!(subject.next_change_in(), None);
    }
}

#[test]
fn phase5_environmental_loss_resets_but_immunity_and_zero_loss_do_not() {
    use tor_simulation::{
        combat::DamageType,
        rules::{AmountExpr, DamageComponentExpr, DamageExpr, Effect, Selector},
    };
    let mut state = game();
    let actor = ActorId(1);
    let effects = |category, amount| {
        [Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount },
                }],
            },
        }]
    };
    state
        .apply_effects(actor, &effects(DamageType::Vital, 1))
        .unwrap();
    assert_eq!(remaining(&state.creature(actor).unwrap()), 600);
    rest(&mut state, 147);
    state
        .apply_effects(actor, &effects(DamageType::Vital, 0))
        .unwrap();
    assert_eq!(remaining(&state.creature(actor).unwrap()), 453);

    let mut species = fixture::species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Category {
                category: DamageType::Impact,
            },
        },
    });
    let build = fixture::build(species);
    let mut immune = Game::two_room_in_stone(42, build.content().clone());
    let id = immune
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(1).unwrap(),
        )
        .unwrap();
    immune
        .configure_creature(
            id,
            CreatureIdentity {
                name: "immune subject".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
    immune
        .apply_effects(id, &effects(DamageType::Impact, 1_000_000))
        .unwrap();
    assert_eq!(remaining(&immune.creature(id).unwrap()), 0);
    assert_eq!(immune.creature(id).unwrap().health().injury(), 0);
    immune
        .apply_effects(id, &effects(DamageType::Vital, 1))
        .unwrap();
    assert_eq!(remaining(&immune.creature(id).unwrap()), 600);
    rest(&mut immune, 17);
    immune
        .apply_effects(id, &effects(DamageType::Impact, 1_000_000))
        .unwrap();
    assert_eq!(remaining(&immune.creature(id).unwrap()), 583);
}

/// Shift a valid idle checkpoint's absolute actor/world clocks together. Relative
/// damage intervals are untouched, so no timer helper supplies the oracle.
fn shift_clock(game: &Game, offset: u64) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value((snapshot, shared)).unwrap();
    value[0]["tick"] = json!(game.tick().checked_add(offset).unwrap());
    for actors in value[1]["actors"].as_array_mut().unwrap() {
        for actor in actors.as_object_mut().unwrap().values_mut() {
            for key in ["ready_at", "active_since"] {
                actor[key] = json!(actor[key].as_u64().unwrap().checked_add(offset).unwrap());
            }
        }
    }
    let (snapshot, shared) = serde_json::from_value(value).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}

#[test]
fn phase5_received_loss_deadlines_near_global_time_limit_never_wrap_or_expire_early() {
    for exact_expiry in [false, true] {
        let mut state = game();
        effect(
            &mut state,
            ActorId(1),
            ActorId(1),
            vec![
                ResolvedEffect::Damage { amount: 1 },
                ResolvedEffect::Heal { amount: 1 },
            ],
        );
        if exact_expiry {
            rest(&mut state, 595);
        }
        let offset = (u64::MAX - 5) - state.tick();
        state = shift_clock(&state, offset);
        assert_eq!(
            state.next_creature_event_tick(),
            exact_expiry.then_some(u64::MAX)
        );
        let mut restored = round_trip(&state);
        for game in [&mut state, &mut restored] {
            rest(game, 4);
            assert_eq!(
                remaining(&game.creature(ActorId(1)).unwrap()),
                if exact_expiry { 1 } else { 596 }
            );
            rest(game, 1);
            assert_eq!(
                remaining(&game.creature(ActorId(1)).unwrap()),
                if exact_expiry { 0 } else { 595 }
            );
            assert_eq!(game.next_creature_event_tick(), None);
            let before = game.clone();
            assert_eq!(
                game.act(game.next_actor().unwrap(), Action::Wait),
                Err(tor_simulation::GameError::Schedule(
                    tor_simulation::ScheduleError::TimeExhausted
                ))
            );
            assert_eq!(*game, before);
        }
        assert_eq!(state, restored);
    }
}
