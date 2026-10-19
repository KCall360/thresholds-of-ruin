//! Public lifecycle publication and private candidate ownership use the same
//! finalized receipts without letting inner effects seal an unfinished suffix.
use super::{expiring_resource_sequence, ready_peer_for_snapshot, round_trip, sequence};
use crate::combat::{CombatEvent, DisclosedCombatEvent};
use crate::{ActorId, Game};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Location, Position, RegionId};

fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}

fn hold_policy() -> crate::linked_hold::HoldPolicy {
    crate::linked_hold::HoldPolicy {
        sustaining: crate::content_ids::FlagId::new("magical").unwrap(),
        preparation: 100,
        recovery: 100,
        protection: 400,
    }
}

#[test]
fn public_teleport_seals_its_abandoned_sequence_before_reopen() {
    let (mut game, intention) = sequence(false);
    game.tick = 5;
    ready_peer_for_snapshot(&mut game);
    game.teleport(ActorId(1), at(1, 3)).unwrap();
    let ended: Vec<_> = game
        .combat_events()
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            matches!(event, CombatEvent::SequenceEnded {
            actor: ActorId(1), intention: Some(id), reason: crate::combat::SequenceEnd::Abandoned,
        } if *id == intention)
        })
        .collect();
    assert_eq!(ended.len(), 1);
    assert!(game.owns_event_receipt(ActorId(1), ended[0].0));
    assert!(game.preparation(ActorId(1)).is_none());
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .interactions
        .unwrap()
        .preparation
        .is_none());
    assert_eq!(round_trip(&game), game);
}

#[test]
fn public_linked_hold_publishes_settlement_but_rejected_endpoints_publish_nothing() {
    for host in [false, true] {
        let (mut game, _) = expiring_resource_sequence();
        game.tick = 5;
        ready_peer_for_snapshot(&mut game);
        game.host_region_continuation = host;
        game.begin_event_batch().unwrap();
        let mut unsupported = hold_policy();
        unsupported.sustaining = crate::content_ids::FlagId::new("unsupported_hold").unwrap();
        let before = game.clone();
        assert_eq!(
            game.apply_linked_hold(ActorId(2), ActorId(1), unsupported.clone()),
            Ok(None)
        );
        assert_eq!(game, before);
        assert_eq!(
            game.apply_linked_hold_in_transaction(ActorId(2), ActorId(1), unsupported),
            Ok(None)
        );
        assert_eq!(game, before);
        assert_eq!(game.event_observations_open(), host);
        assert!(game
            .apply_linked_hold(ActorId(2), ActorId(1), hold_policy())
            .unwrap()
            .is_some());
        assert_eq!(
            game.observe(ActorId(1))
                .unwrap()
                .combat
                .unwrap()
                .events
                .iter()
                .filter(|event| matches!(
                    event,
                    DisclosedCombatEvent::Interrupted { actor: ActorId(1) }
                ))
                .count(),
            1
        );
        if host {
            assert!(game.event_observations_open());
            game.host_region_continuation = false;
            game.seal_event_observations().unwrap();
        }
        assert_eq!(round_trip(&game), game);
    }
}

#[test]
fn public_equipment_death_publishes_own_feedback_before_reopen() {
    let (mut game, _) = crate::abilities::strike_tests::sequence_game_custom(
        &[0, 100, 200],
        |_| {},
        |_, species| {
            // The sequence fixture's fixed maximum would suppress the ring's
            // additive contribution instead of exercising rederived death.
            species.modifiers.retain(|modifier| {
                !matches!(
                    &modifier.op,
                    crate::rules::ModifierOp::Set {
                        stat: crate::rules::Stat::Health,
                        ..
                    }
                )
            });
        },
    );
    let peer = game
        .spawn_actor(at(1, 3), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let baseline = game.health(ActorId(2)).unwrap().1;
    assert!(baseline > 1);
    crate::test_creatures::injure(&mut game, ActorId(2), u32::try_from(baseline - 1).unwrap())
        .unwrap();
    assert!(game.alive(ActorId(2)));
    assert_eq!(game.health(ActorId(2)), Some((1, baseline)));
    let mut item = crate::ItemSpec::ordinary("frail ring".into());
    item.class = crate::ItemClass::Ring;
    item.equipment = Some(crate::EquipmentSpec {
        armor: None,
        slot: crate::EquipmentSlot::new("ring").unwrap(),
        attack: None,
        modifiers: vec![crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Add {
                stat: crate::rules::Stat::Health,
                value: 1 - i64::try_from(baseline).unwrap(),
            },
        }],
    });
    game.place_item_stack(100, at(1, 2), Some(ActorId(2)), 1, item)
        .unwrap();
    let slot = game
        .anatomy(ActorId(2))
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "ring")
        .unwrap();
    game.equip_authored(
        ActorId(2),
        crate::ItemId(100),
        crate::EquipmentSlotId(slot as u16),
    )
    .unwrap();
    assert_eq!(game.next_actor(), Some(peer));
    // Death drops the ring and restores the natural maximum without reviving.
    assert_eq!(game.health(ActorId(2)), Some((0, baseline)));
    assert!(game.equipment(ActorId(2)).unwrap().is_empty());
    assert_eq!(
        game.items.get(&crate::ItemId(100)).unwrap().location,
        crate::ItemLocation::Ground(at(1, 2))
    );
    assert_own_death_and_reopen(&game);
}

fn expiring_health_game(contextual: bool) -> Game {
    use crate::rules::{ConditionDef, Eviction, Modifier, ModifierOp, Scope, Stacking, Stat};
    let condition = crate::content_ids::ConditionId::new("publication_borrowed_health").unwrap();
    let (mut game, intention) = crate::abilities::strike_tests::sequence_game_custom(
        &[0, 100, 200],
        |_| {},
        |rules, species| {
            species.modifiers.retain(|modifier| {
                !matches!(
                    &modifier.op,
                    ModifierOp::Set {
                        stat: Stat::Health,
                        ..
                    }
                )
            });
            if contextual {
                species.modifiers.push(Modifier {
                    scope: Scope::WhileNoSensedHostile,
                    op: ModifierOp::Add {
                        stat: Stat::HealthRecoveryPeriodPercent,
                        value: -25,
                    },
                });
            }
            rules.conditions.push(ConditionDef {
                id: condition.clone(),
                family: crate::content_ids::ConditionFamilyId::new(condition.as_str()).unwrap(),
                lifecycle: crate::rules::ConditionLifecycle::Timed,
                timing: crate::rules::ConditionTiming::expiring(),
                restrictions: vec![],
                preparation: crate::rules::ConditionPreparation::Preserve,
                name: "Borrowed health".into(),
                modifiers: vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Health,
                        value: 10,
                    },
                }],
                stacking: Stacking::Keep,
                source_scoped: false,
                descriptors: vec![],
                capacity: Eviction::Reject,
            });
        },
    );
    game.cancel_intention(ActorId(1), intention).unwrap();
    for (id, faction) in [(ActorId(1), "left"), (ActorId(2), "right")] {
        let mut actor = game.actors.get_mut(&id).unwrap();
        actor.ready_at = 0;
        actor.combat.as_mut().unwrap().identity.faction = faction.into();
    }
    let baseline = game.health(ActorId(2)).unwrap().1;
    game.apply_condition(ActorId(2), &condition, ActorId(1), 5)
        .unwrap();
    assert_eq!(
        game.health(ActorId(2)),
        Some((baseline + 10, baseline + 10))
    );
    crate::test_creatures::injure(&mut game, ActorId(2), u32::try_from(baseline + 5).unwrap())
        .unwrap();
    assert_eq!(game.health(ActorId(2)), Some((5, baseline + 10)));
    assert_waiting_for_expiry_death(&game);
    game
}

fn assert_waiting_for_expiry_death(game: &Game) {
    assert!(game.alive(ActorId(2)));
    assert_eq!(game.health(ActorId(2)).unwrap().0, 5);
    assert!(!game.combat_events().iter().any(|event| matches!(
        event,
        CombatEvent::Died {
            actor: ActorId(2),
            ..
        }
    )));
}

fn at_expiry(game: &mut Game) {
    game.tick = 5;
    for actor in [ActorId(1), ActorId(2)] {
        game.actors.get_mut(&actor).unwrap().ready_at = 5;
    }
    assert_waiting_for_expiry_death(game);
}

fn assert_own_death_and_reopen(game: &Game) {
    assert!(!game.alive(ActorId(2)));
    let feedback = game.observe(ActorId(2)).unwrap().combat.unwrap().events;
    assert_eq!(
        feedback
            .iter()
            .filter(|event| matches!(
                event,
                DisclosedCombatEvent::Died {
                    actor: ActorId(2),
                    ..
                }
            ))
            .count(),
        1
    );
    let reopened = round_trip(game);
    assert_eq!(&reopened, game);
    assert_eq!(
        reopened.observe(ActorId(2)).unwrap().combat.unwrap().events,
        feedback
    );
}

#[test]
fn public_run_and_arena_control_publish_expiry_death_before_reopen() {
    for arena in [false, true] {
        let mut game = expiring_health_game(!arena);
        if arena {
            game.configure_arena_run_with_limits(
                ActorId(1),
                BTreeSet::from([ActorId(1), ActorId(2)]),
                BTreeMap::new(),
                crate::arena::ArenaLimits {
                    ticks: 5,
                    actions: 100,
                },
            )
            .unwrap();
        }
        if arena {
            for actor in [ActorId(1), ActorId(2)] {
                game.actors.get_mut(&actor).unwrap().ready_at = 5;
            }
            assert_eq!(game.tick(), 0);
            assert_waiting_for_expiry_death(&game);
            game.control_arena(false, 0).unwrap();
            assert_eq!(game.tick(), 5);
        } else {
            at_expiry(&mut game);
            game.configure_run(
                ActorId(1),
                BTreeSet::from([ActorId(1), ActorId(2)]),
                None,
                BTreeMap::from([
                    ("left".into(), BTreeSet::from(["right".into()])),
                    ("right".into(), BTreeSet::from(["left".into()])),
                ]),
            )
            .unwrap();
        }
        assert!(!game.alive(ActorId(2)), "arena={arena}");
        assert_own_death_and_reopen(&game);
    }
}

fn streaming_expiry() -> (Game, crate::RegionTransition) {
    let mut game = expiring_health_game(false);
    game.add_chamber(tor_world::Region {
        id: RegionId(3),
        name: "isolated active room".into(),
        bounds: tor_world::Extent::new(4, 4, 1).unwrap(),
    })
    .unwrap();
    // A nearby connected room would keep the expiring actor's region active
    // through the surviving actor's ordinary reach pin.
    game.teleport(ActorId(1), at(3, 1)).unwrap();
    at_expiry(&mut game);
    let transition = crate::RegionTransition {
        active: BTreeSet::from([RegionId(3)]),
        loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
    };
    assert_eq!(
        game.region_requirements(&transition.active, &transition.loaded),
        transition
    );
    (game, transition)
}

#[test]
fn ordinary_and_deferred_region_publication_seal_freeze_death_before_store_commit() {
    for deferred in [false, true] {
        let (mut game, transition) = streaming_expiry();
        let mut records = crate::MemoryRecords::default();
        let report = if deferred {
            game.transition_regions_counted_deferred(&transition, &mut records)
                .unwrap()
                .1
        } else {
            game.apply_region_transition(&transition, &mut records)
                .unwrap()
        };
        assert!(report.frozen.contains(&RegionId(1)));
        assert_eq!(report.detached, vec![RegionId(2)]);
        assert_eq!(records.len(), 1);
        assert_own_death_and_reopen(&game);
    }
}

#[test]
fn deferred_region_late_receipt_failure_rolls_back_game_and_record_store() {
    let (mut game, transition) = streaming_expiry();
    // Defensive counter fault injection forces the final receipt admission to
    // fail after the same real freeze/death/detach work as the success control.
    let mut receipts = serde_json::to_value(&game.event_receipts).unwrap();
    receipts["bindings"] = serde_json::json!(crate::limits::EVENT_RECEIPTS);
    game.event_receipts = serde_json::from_value(receipts).unwrap();
    let before = game.clone();
    let mut records = crate::MemoryRecords::default();
    let ids: Vec<_> = records.ids().collect();
    assert_eq!(
        game.transition_regions_counted_deferred(&transition, &mut records),
        Err(crate::TransitionError::Rule(crate::GameError::Rule(
            crate::abilities::RuleError::InvalidPlan
        )))
    );
    assert_eq!(game, before);
    assert_eq!(records.ids().collect::<Vec<_>>(), ids);
}
