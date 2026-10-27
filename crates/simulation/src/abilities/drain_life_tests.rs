//! Paid Drain fixtures use real native creatures and immutable structural scope.
//! Canonical source completeness and managed residency are server-owned.
use super::*;
use crate::spatial_scope::registry::{Budget, CellRun, PrimaryRun, RoomSurfaces, Scope};
use crate::spatial_scope::{Membership, PrimarySpace, RoomId};
use crate::{ActorId, Game, ItemId};
use tor_world::{Location, Position, RegionId};
fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(health: i64, targets: usize, discovery: crate::ItemDiscovery) -> (Game, ItemId) {
    fixture_with_traits(health, targets, discovery, 1000, false)
}
fn fixture_with_traits(
    health: i64,
    targets: usize,
    discovery: crate::ItemDiscovery,
    memory: u64,
    guise: bool,
) -> (Game, ItemId) {
    fixture_with_options(health, targets, discovery, memory, guise, false, false)
}
fn fixture_with_options(
    health: i64,
    targets: usize,
    discovery: crate::ItemDiscovery,
    memory: u64,
    guise: bool,
    wide: bool,
    keep_ability: bool,
) -> (Game, ItemId) {
    let mut game = super::ray_tests::fixture_with(|document, ability, species| {
        if wide {
            let template = document
                .conditions
                .iter()
                .find(|condition| condition.id.as_str() == "distracted")
                .unwrap()
                .clone();
            for index in 0..68 {
                let mut condition = template.clone();
                let id = format!("wide_health_{index}");
                condition.id = crate::content_ids::ConditionId::new(id.clone()).unwrap();
                condition.family = crate::content_ids::ConditionFamilyId::new(id.clone()).unwrap();
                condition.name = id;
                condition.source_scoped = false;
                // `distracted` is a NextCheck token: Health modifiers require
                // an ordinary timed source, not token-only edge semantics.
                condition.lifecycle = crate::rules::ConditionLifecycle::Timed;
                assert_eq!(condition.timing, crate::rules::ConditionTiming::expiring());
                assert_eq!(condition.stacking, crate::rules::Stacking::Longest);
                assert!(!condition.protected_lifecycle());
                condition.modifiers = vec![
                    crate::rules::Modifier {
                        cancellable: false,
                        scope: crate::rules::Scope::Always,
                        op: crate::rules::ModifierOp::Add {
                            stat: crate::rules::Stat::Health,
                            value: 1_000_000
                        }
                    };
                    128
                ];
                document.conditions.push(condition);
            }
        }
        ability.invocation = InvocationKind::ActorTarget;
        ability.reach = crate::rules::Reach::SelfOnly;
        ability.interaction = crate::rules::Interaction::NonHostile;
        ability.supply_costs.clear();
        let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        strikes[0].check = None;
        strikes[0].on_success = vec![crate::rules::Effect::DrainLife];
        strikes[0].on_failure.clear();
        strikes[0].riders.clear();
        for modifier in &mut species.modifiers {
            if let crate::rules::ModifierOp::Set {
                stat: crate::rules::Stat::Health,
                value,
            } = &mut modifier.op
            {
                *value = health;
            }
        }
        species.behavior.memory_ticks = memory;
        species.behavior.disposition = crate::behavior::Disposition::Passive;
        species.modifiers.extend([
            crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Set {
                    stat: crate::rules::Stat::HealthRecoveryPeriod,
                    value: 1000,
                },
            },
            crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Set {
                    stat: crate::rules::Stat::HealthRecoveryResetPercent,
                    value: 50,
                },
            },
        ]);
        if guise {
            species.modifiers.push(crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Perception {
                    value: crate::perception::contract_fixtures::guise(),
                },
            });
        }
        if wide {
            // The reused unit_subject source contributes exactly +22 Health.
            let added: i64 = species
                .modifiers
                .iter()
                .filter_map(|modifier| match modifier.op {
                    crate::rules::ModifierOp::Add {
                        stat: crate::rules::Stat::Health,
                        value,
                    } => Some(value),
                    _ => None,
                })
                .sum();
            assert_eq!(added, 22);
            // Health Set overrides, rather than adds to, every source. The wide
            // fixture uses ordinary sampled birth Health plus bounded additions.
            species.modifiers.retain(|modifier| {
                !matches!(
                    modifier.op,
                    crate::rules::ModifierOp::Set {
                        stat: crate::rules::Stat::Health,
                        ..
                    }
                )
            });
        }
        if !keep_ability {
            species.modifiers.retain(|m|!matches!(&m.op,crate::rules::ModifierOp::GrantAbility{ability:id} if id==&ability.id));
        }
    });
    if wide {
        let creature = game.creature(ActorId(1)).unwrap();
        let expected = creature
            .derived()
            .health_contributions
            .iter()
            .map(|entry| u64::from(entry.health))
            .sum::<u64>()
            + 22
            + u64::from(
                creature
                    .derived()
                    .attributes
                    .get(crate::attributes::Attribute::Strength),
            );
        assert_eq!(game.health(ActorId(1)).unwrap(), (expected, expected));
    } else {
        assert_eq!(game.health(ActorId(1)).unwrap().0, health as u64);
    }
    assert!(targets <= 3);
    if targets > 1 {
        let build = game.creature(ActorId(1)).unwrap().build().clone();
        for x in [2, 4].into_iter().take(targets - 1) {
            let actor = game
                .spawn_actor(at(x), std::num::NonZeroU64::new(100).unwrap())
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: format!("friend{x}"),
                    faction: "side1".into(),
                },
                build.clone(),
            )
            .unwrap();
            game.actors.get_mut(&actor).unwrap().ready_at = 1000;
        }
    }
    let room = RoomId::new(1).unwrap();
    let cells = CellRun {
        region: RegionId(1),
        z: 0,
        y: 1,
        first_x: 1,
        last_x: if targets == 0 { 1 } else { 4 },
    };
    let scope = Scope {
        mapping: crate::spatial_scope::mapping::MappingCertification::Uncertified,
        id: "drain-fixture".into(),
        members: vec![RegionId(1)],
        primary: vec![PrimaryRun {
            cells,
            membership: Membership::new(PrimarySpace::Room(room), None).unwrap(),
        }],
        rooms: vec![RoomSurfaces {
            room,
            runs: vec![cells],
        }],
    };
    game.register_spatial_scope(
        scope,
        &Budget {
            scopes: 1,
            members: 1,
            rooms: 1,
            passages: 0,
            runs: 2,
            label_bytes: 64,
            expanded_refs: 8,
        },
    )
    .unwrap();
    let mut spec = crate::ItemSpec::ordinary("plain stick".into());
    spec.class = crate::ItemClass::Tool;
    spec.identity = "fixture.drain".into();
    spec.concealed = true;
    spec.device = Some(crate::DeviceSpec {
        ability: crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
        initial_charges: crate::ChargeRange {
            minimum: 3,
            maximum: 3,
        },
        discovery,
    });
    let item = game
        .place_item_stack(10, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    (game, item)
}
fn admit(game: &mut Game, item: ItemId) -> crate::IntentionId {
    game.admit_intention(
        ActorId(1),
        crate::Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::West,
            },
        },
        crate::IntentionOrigin::Human,
    )
    .unwrap()
}
fn complete(game: &mut Game, item: ItemId) -> crate::IntentionId {
    let tick = game.tick;
    let rng = game.rules_rng;
    let intention = admit(game, item);
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    execution.outcome.unwrap();
    assert_eq!(game.tick, tick + 100);
    assert_eq!(game.rules_rng, rng);
    intention
}
#[test]
fn drain_life_weak_paid_refusal_releases_charge_without_discovery_or_completed_disguise() {
    let (mut game, item) = fixture(1, 1, crate::ItemDiscovery::OnUse);
    let intention = complete(&mut game, item);
    assert_eq!(game.health(ActorId(1)).unwrap().0, 1);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 1);
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(game.items[&item].holds.is_empty());
    assert!(!game.actors[&ActorId(1)].knowledge.contains("fixture.drain"));
    assert!(game.combat_events().iter().any(|event| matches!(event,crate::combat::CombatEvent::ItemRefused{actor:ActorId(1),intention:Some(id),reason:crate::ItemRefusal::TooWeak,..} if *id==intention)));
    assert!(!game
        .combat_events()
        .iter()
        .any(|event| matches!(event, crate::combat::CombatEvent::ItemCompleted { .. })));
    let event = game
        .combat_events()
        .iter()
        .find(|event| matches!(event, crate::combat::CombatEvent::ItemRefused { .. }))
        .unwrap();
    let encoded = serde_json::to_value(event).unwrap();
    assert_eq!(
        serde_json::from_value::<crate::combat::CombatEvent>(encoded.clone()).unwrap(),
        *event
    );
    for reason in [
        serde_json::json!({"too_weak":null}),
        serde_json::json!("other"),
        serde_json::json!(0),
    ] {
        let mut malformed = encoded.clone();
        malformed["ItemRefused"]["reason"] = reason;
        assert!(serde_json::from_value::<crate::combat::CombatEvent>(malformed).is_err());
    }
}
#[test]
fn drain_life_empty_census_spends_charge_without_halving_or_identity() {
    let (mut game, item) = fixture(5, 0, crate::ItemDiscovery::Never);
    complete(&mut game, item);
    assert_eq!(game.items[&item].charges, Some(2));
    assert_eq!(game.health(ActorId(1)).unwrap().0, 5);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 5);
    assert!(!game.actors[&ActorId(1)].knowledge.contains("fixture.drain"));
}
#[test]
fn drain_life_paid_odd_health_snapshot_splits_across_friendly_and_other_creatures() {
    let (mut game, item) = fixture(11, 3, crate::ItemDiscovery::Never);
    complete(&mut game, item);
    assert_eq!(game.items[&item].charges, Some(2));
    assert_eq!(game.health(ActorId(1)).unwrap().0, 5);
    for id in [ActorId(2), ActorId(3), ActorId(4)] {
        assert_eq!(game.health(id).unwrap().0, 10);
    }
    assert!(!game.actors[&ActorId(1)].knowledge.contains("fixture.drain"));
    let item_losses: Vec<_> =
        game.combat_events()
            .iter()
            .filter_map(|event| match event {
                crate::combat::CombatEvent::Resolved { outcome, .. }
                    if outcome.effects.iter().any(|effect| {
                        matches!(effect, ResolvedEffect::DirectHealthLoss { .. })
                    }) =>
                {
                    Some(outcome)
                }
                _ => None,
            })
            .collect();
    assert_eq!(item_losses.len(), 4);
    assert!(item_losses.iter().all(|outcome| outcome.ability.is_none()));

    // The generic self-ability route retains its actual source attribution;
    // item Exposure has no public ability identity to invent or disclose.
    let (mut ordinary, unused_item) =
        fixture_with_options(11, 3, crate::ItemDiscovery::Never, 1000, false, false, true);
    let ability = crate::content_ids::AbilityId::new("fixture_ray").unwrap();
    let issued = ordinary
        .admit_intention(
            ActorId(1),
            crate::Action::UseAbility {
                id: ability.clone(),
                invocation: AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = ordinary.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, issued);
    execution.outcome.unwrap();
    assert_eq!(ordinary.health(ActorId(1)).unwrap().0, 5);
    assert_eq!(ordinary.items[&unused_item].charges, Some(3));
    let ordinary_losses: Vec<_> = ordinary
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. }
                if outcome
                    .effects
                    .iter()
                    .any(|effect| matches!(effect, ResolvedEffect::DirectHealthLoss { .. })) =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .collect();
    assert_eq!(ordinary_losses.len(), 4);
    assert!(ordinary_losses
        .iter()
        .all(|outcome| outcome.ability.as_ref() == Some(&ability)
            && outcome.intention == Some(issued)));
}
#[test]
fn drain_life_zero_quotient_still_halves_caster_without_rounding_target_loss_up() {
    let (mut game, item) = fixture(3, 3, crate::ItemDiscovery::Never);
    complete(&mut game, item);
    assert_eq!(game.health(ActorId(1)).unwrap().0, 1);
    for id in [ActorId(2), ActorId(3), ActorId(4)] {
        assert_eq!(game.health(id).unwrap().0, 3);
    }
}
#[test]
fn drain_life_unit_effect_and_direct_health_loss_are_closed_and_wide() {
    assert_eq!(
        serde_json::from_str::<crate::rules::Effect>(r#"{"kind":"drain_life"}"#).unwrap(),
        crate::rules::Effect::DrainLife
    );
    assert!(
        serde_json::from_str::<crate::rules::Effect>(r#"{"kind":"drain_life","radius":1}"#)
            .is_err()
    );
    let effect = ResolvedEffect::DirectHealthLoss { amount: u64::MAX };
    assert_eq!(
        serde_json::from_value::<ResolvedEffect>(serde_json::to_value(&effect).unwrap()).unwrap(),
        effect
    );
    let mut health = crate::health::Health::new(u64::MAX);
    let loss = u64::MAX - u64::MAX / 2;
    assert_eq!(health.direct_loss(loss), loss);
    assert_eq!(health.current(), u64::MAX / 2);
    assert_eq!(health.direct_loss(u64::MAX), u64::MAX / 2);
    assert!(health.dead());
    assert_eq!(health.direct_loss(1), 0);
}

fn direct_loss(game: &mut Game, target: ActorId, amount: u64) {
    game.apply(Outcome {
        actor: target,
        target,
        ability: None,
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        resistance: None,
        participation: AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![ResolvedEffect::DirectHealthLoss { amount }],
    })
    .unwrap();
}
#[test]
fn drain_life_zero_share_releases_genuine_guise_and_uses_only_authored_anonymous_memory() {
    for memory in [0, 37] {
        let (mut game, _) = fixture_with_traits(3, 3, crate::ItemDiscovery::Never, memory, true);
        for id in [ActorId(2), ActorId(3), ActorId(4)] {
            game.configure_ai(id, crate::ai::AiProfile::general())
                .unwrap();
            assert!(game.actors[&id].guise.appearance().is_some());
            assert!(game.actors[&id].awareness.alert.is_none());
        }
        let rng = game.rules_rng;
        let tick = game.tick;
        assert!(game
            .apply_effects(ActorId(1), &[crate::rules::Effect::DrainLife])
            .unwrap());
        assert_eq!(game.rules_rng, rng);
        assert_eq!(game.tick, tick);
        for id in [ActorId(2), ActorId(3), ActorId(4)] {
            assert_eq!(game.health(id).unwrap().0, 3);
            assert!(game.actors[&id].guise.appearance().is_none());
            assert!(game.actors[&id].behavior.engagement.is_none());
            assert_eq!(
                game.actors[&id].awareness.alert,
                if memory == 0 {
                    None
                } else {
                    Some(crate::awareness::LocationAlert {
                        location: at(1),
                        expires: game.actor_clock(id) + memory,
                    })
                }
            );
        }
        assert!(!game.combat_events().iter().any(|event|matches!(event,
            crate::combat::CombatEvent::Resolved{outcome,..} if outcome.target!=ActorId(1)
                && outcome.effects.iter().any(|effect|matches!(effect,ResolvedEffect::DirectHealthLoss{..})))));
    }
}
#[test]
fn drain_life_snapshots_denominator_before_death_and_rolls_back_late_cleanup_failure() {
    let (mut game, item) = fixture(100, 3, crate::ItemDiscovery::Never);
    direct_loss(&mut game, ActorId(2), 99);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 1);
    let mut positive = game.clone();
    complete(&mut positive, item);
    assert!(!positive.alive(ActorId(2)));
    assert_eq!(positive.health(ActorId(3)).unwrap().0, 84);
    assert_eq!(positive.health(ActorId(4)).unwrap().0, 84);
    assert!(positive
        .items
        .iter()
        .any(|(_, item)| item.spec.class == crate::ItemClass::Corpse));
    game.next_item_id = u64::MAX;
    let before = game.clone();
    // The successful control above is an actual paid device completion. This
    // failure isolates the direct effect transaction after staged life loss;
    // ordinary action admission correctly reserves corpse identities earlier.
    assert_eq!(
        game.apply_effects(ActorId(1), &[crate::rules::Effect::DrainLife]),
        Err(crate::GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
}
#[test]
fn drain_life_resets_actual_damage_activity_once_for_more_than_two_recipients() {
    let (mut game, _) = fixture(100, 3, crate::ItemDiscovery::Never);
    for id in [ActorId(1), ActorId(2), ActorId(3), ActorId(4)] {
        direct_loss(&mut game, id, 1);
        let mut creature = game.creature(id).unwrap().into_owned();
        assert_eq!(creature.derived().health_recovery_reset_percent, 50);
        creature
            .restore_health_recovery(
                800,
                crate::health_recovery::RecoveryNutrition::Normal,
                crate::health_recovery::RecoveryContext::Known {
                    sensed_hostile: false,
                },
            )
            .unwrap();
        let identity = game.actors[&id].combat.as_ref().unwrap().identity.clone();
        game.actors.get_mut(&id).unwrap().combat = Some(crate::combat::CombatState::from_creature(
            identity, creature,
        ));
        assert_eq!(game.creature(id).unwrap().health_recovery().elapsed(), 800);
    }
    assert!(game
        .apply_effects(ActorId(1), &[crate::rules::Effect::DrainLife])
        .unwrap());
    for id in [ActorId(1), ActorId(2), ActorId(3), ActorId(4)] {
        assert_eq!(game.creature(id).unwrap().health_recovery().elapsed(), 400);
    }
}

#[test]
fn drain_life_valid_condition_sources_preserve_health_and_loss_beyond_u32() {
    let (mut game, item) =
        fixture_with_options(11, 1, crate::ItemDiscovery::Never, 1000, false, true, false);
    let base = {
        let creature = game.creature(ActorId(1)).unwrap();
        creature
            .derived()
            .health_contributions
            .iter()
            .map(|entry| u64::from(entry.health))
            .sum::<u64>()
            + 22
            + u64::from(
                creature
                    .derived()
                    .attributes
                    .get(crate::attributes::Attribute::Strength),
            )
    };
    let expected = base + 68 * 128 * 1_000_000_u64;
    assert!(expected / 2 > u64::from(u32::MAX));
    for target in [ActorId(1), ActorId(2)] {
        game.apply(Outcome {
            actor: target,
            target,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            resistance: None,
            participation: AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: (0..68)
                .map(|index| ResolvedEffect::ApplyCondition {
                    id: crate::content_ids::ConditionId::new(format!("wide_health_{index}"))
                        .unwrap(),
                    duration: 1000,
                })
                .collect(),
        })
        .unwrap();
        assert_eq!(game.health(target).unwrap(), (expected, expected));
    }
    complete(&mut game, item);
    assert_eq!(game.health(ActorId(1)).unwrap().0, expected / 2);
    assert_eq!(game.health(ActorId(2)).unwrap().0, expected - expected / 2);
    assert!(game.combat_events().iter().any(|event|matches!(event,
        crate::combat::CombatEvent::Resolved{outcome,..} if outcome.effects.iter().any(|effect|
            matches!(effect,ResolvedEffect::DirectHealthLoss{amount} if *amount>u64::from(u32::MAX))))));
}
#[test]
fn drain_life_ignores_noncombat_bodies_in_the_real_scope_census() {
    let (mut game, item) = fixture(5, 1, crate::ItemDiscovery::Never);
    let body = game
        .spawn_actor(at(2), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.actors.get_mut(&body).unwrap().ready_at = 1000;
    assert!(game.health(body).is_none());
    assert!(game.alive(body));
    complete(&mut game, item);
    assert_eq!(game.health(ActorId(1)).unwrap().0, 2);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 3);
    assert!(game.health(body).is_none());
    assert!(game.alive(body));
}
#[test]
fn drain_life_rejects_mixed_or_hostile_authored_sources_before_admission() {
    let (game, _) = fixture(5, 1, crate::ItemDiscovery::Never);
    let valid = game.content.rules().document().clone();
    assert!(crate::rules::Ruleset::validate(valid.clone()).is_ok());
    for hostile in [false, true] {
        let mut invalid = valid.clone();
        let ability = invalid
            .abilities
            .iter_mut()
            .find(|ability| ability.id.as_str() == "fixture_ray")
            .unwrap();
        if hostile {
            ability.interaction = crate::rules::Interaction::HostileToTarget;
        } else {
            let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].on_success.push(crate::rules::Effect::RoomLight);
        }
        assert!(crate::rules::Ruleset::validate(invalid).is_err());
    }
}

#[test]
fn drain_life_completion_uses_settled_health_not_admission_weakness() {
    let (mut game, item) = fixture(5, 1, crate::ItemDiscovery::Never);
    direct_loss(&mut game, ActorId(1), 4);
    let mut creature = game.creature(ActorId(1)).unwrap().into_owned();
    creature
        .restore_health_recovery(
            950,
            crate::health_recovery::RecoveryNutrition::Normal,
            crate::health_recovery::RecoveryContext::Known {
                sensed_hostile: false,
            },
        )
        .unwrap();
    let identity = game.actors[&ActorId(1)]
        .combat
        .as_ref()
        .unwrap()
        .identity
        .clone();
    game.actors.get_mut(&ActorId(1)).unwrap().combat = Some(
        crate::combat::CombatState::from_creature(identity, creature),
    );
    assert_eq!(game.health(ActorId(1)).unwrap().0, 1);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .health_recovery()
            .elapsed(),
        950
    );
    complete(&mut game, item);
    assert_eq!(game.items[&item].charges, Some(2));
    assert!(!game
        .combat_events()
        .iter()
        .any(|event| matches!(event, crate::combat::CombatEvent::ItemRefused { .. })));
    assert_eq!(game.health(ActorId(1)).unwrap().0, 1);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 4);
}
