//! Item authority contracts; geometry/check arithmetic are owned by ray_tests.
use super::*;
use crate::{
    Action, ActorId, ChargeRange, DeviceSpec, Game, GameError, IntentionOrigin, ItemClass,
    ItemDiscovery, ItemId, ItemSpec, ReadableSpec, Work,
};
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture() -> Game {
    super::ray_tests::fixture_with(|_, ray, species| {
        species.modifiers.retain(|m|!matches!(&m.op,crate::rules::ModifierOp::GrantAbility{ability} if ability==&ray.id));
        if let crate::rules::StrikeSource::Authored { strikes } = &mut ray.strikes {
            strikes[0].check.as_mut().unwrap().against = crate::rules::Against::Opposed {
                skill: crate::rules::SkillSelector::Named {
                    skill: crate::attributes::Skill::Lore,
                },
            };
        }
    })
}
fn device(game: &mut Game, id: u64, charges: u16) -> ItemId {
    device_with_discovery(game, id, charges, ItemDiscovery::OnUse)
}
fn device_with_discovery(
    game: &mut Game,
    id: u64,
    charges: u16,
    discovery: ItemDiscovery,
) -> ItemId {
    let mut spec = ItemSpec::ordinary("plain stick".into());
    spec.identity = "fixture.elemental_device".into();
    spec.appearance = "plain stick".into();
    spec.name = "test elemental device".into();
    spec.concealed = true;
    // A Tool proves use is an explicit capability, not Wand class dispatch.
    spec.class = ItemClass::Tool;
    spec.device = Some(DeviceSpec {
        ability: crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
        initial_charges: ChargeRange {
            minimum: 0,
            maximum: 7,
        },
        discovery,
    });
    let item = game
        .place_item_stack(id, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    game.items
        .edit(item, |i| i.charges = Some(charges))
        .unwrap();
    item
}

#[test]
fn captured_zap_discovery_policy_rejects_source_mismatch_before_payment_and_restore() {
    for (discovery, forged_discovery) in [
        (ItemDiscovery::Never, ItemDiscovery::OnUse),
        (ItemDiscovery::OnUse, ItemDiscovery::Never),
    ] {
        let mut game = fixture();
        let item = device_with_discovery(&mut game, 10, 3, discovery);
        let captured = game
            .plan_work(
                ActorId(1),
                Work::Zap {
                    item,
                    aim: crate::DeviceAim::Direction {
                        direction: StepDirection::North,
                    },
                },
            )
            .unwrap();
        let mut forged = captured.clone();
        let Plan::Zap { discovery, .. } = &mut forged else {
            unreachable!()
        };
        *discovery = forged_discovery;
        assert!(forged.valid(&game.content.rules));
        let before = game.clone();
        assert!(game.item_allocation(&captured).is_ok());
        assert!(game.item_allocation(&forged).is_err());
        assert_eq!(
            game, before,
            "rejected policy cannot reserve or spend a charge"
        );

        // Keep the authentic preparation pending at the current scheduler boundary.
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
        execute(
            &mut game,
            Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North,
                },
            },
        );
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
        assert!(game.work_state_valid(ActorId(1)));
        assert_eq!(game, roundtrip(&game));
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .plan = forged;
        assert!(!game.work_state_valid(ActorId(1)));
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
        assert_eq!(game.items[&item].charges, Some(3));
        assert!(!game.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.elemental_device"));
    }
}

#[test]
fn device_never_discovery_spends_charge_without_disclosing_identity_or_count() {
    let mut game = fixture();
    let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
    let plan = game
        .plan_work(
            ActorId(1),
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North,
                },
            },
        )
        .unwrap();
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(encoded["discovery"], "never");
    let mut missing = encoded.clone();
    missing.as_object_mut().unwrap().remove("discovery");
    assert!(serde_json::from_value::<Plan>(missing).is_err());
    let mut unsupported = encoded;
    unsupported["discovery"] = serde_json::json!("self_effect");
    assert!(!serde_json::from_value::<Plan>(unsupported)
        .unwrap()
        .valid(&game.content.rules));
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North,
            },
        },
    );
    assert_eq!(game.items[&item].charges, Some(2));
    assert_eq!(game.items[&item].quantity, 1);
    assert!(game.items[&item].holds.is_empty());
    assert!(!game.actors[&ActorId(1)]
        .knowledge
        .contains("fixture.elemental_device"));
    assert!(game.actors[&ActorId(1)].known_charges.is_empty());
    assert_eq!(game, roundtrip(&game));
}
fn identify_scroll(game: &mut Game, id: u64, class: ItemClass, quantity: u64) -> ItemId {
    let mut spec = ItemSpec::ordinary("unread page".into());
    spec.class = ItemClass::Misc;
    spec.identity = format!("fixture.identify.{class:?}");
    spec.name = "identify page".into();
    spec.appearance = "unread page".into();
    spec.concealed = true;
    spec.stackable = true;
    spec.readable = Some(ReadableSpec {
        control: None,
        descriptors: vec![],
        effects: vec![],
        identify: Some(vec![class]),
        discovery: ItemDiscovery::OnUse,
    });
    game.place_item_stack(id, at(1), Some(ActorId(1)), quantity, spec)
        .unwrap()
}
fn execute(game: &mut Game, action: Action) -> crate::IntentionId {
    let issued = game
        .admit_intention(ActorId(1), action, IntentionOrigin::Human)
        .unwrap();
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, issued);
    done.outcome.unwrap();
    issued
}
fn start_owned_item_work(game: &mut Game, plan: Plan) -> crate::IntentionId {
    // Keep a real competing actor ready so admission starts the paid work but
    // does not advance its completion; each test then controls its boundary.
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let action = match plan.work() {
        Work::Read { item } => Action::Read { item },
        Work::Drink { item } => Action::Drink { item },
        Work::Zap { item, aim } => Action::Zap { item, aim },
        _ => panic!("finite item work fixture"),
    };
    let intention = execute(game, action);
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, plan);
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().intention,
        Some(intention)
    );
    intention
}
fn roundtrip(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let snapshot = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}

fn relocation_device_fixture(effect: crate::rules::Effect, hits: bool) -> Game {
    super::ray_tests::fixture_with(|_, ray, species| {
        ray.invocation = InvocationKind::DirectionalContactRay;
        ray.timing.source = crate::rules::TimingSource::Fixed {
            prepare: 100,
            recover: 100,
        };
        let crate::rules::StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        strikes[0].check.as_mut().unwrap().against = crate::rules::Against::Opposed {
            skill: crate::rules::SkillSelector::Named {
                skill: crate::attributes::Skill::Lore,
            },
        };
        strikes[0].on_failure = vec![effect];
        species.modifiers.retain(|modifier| {
            !matches!(&modifier.op,
            crate::rules::ModifierOp::GrantAbility { ability } if ability == &ray.id)
        });
        if hits {
            species.modifiers.push(crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Add {
                    stat: crate::rules::Stat::Attribute {
                        attribute: crate::attributes::Attribute::Intellect,
                    },
                    value: 100,
                },
            });
        }
    })
}

fn levitation_drink_fixture() -> (Game, ItemId, crate::content_ids::ConditionId) {
    let condition = crate::content_ids::ConditionId::new("fixture_hover").unwrap();
    let mut game = super::ray_tests::fixture_with(|document, _, _| {
        document.conditions.push(crate::rules::ConditionDef {
            hallucinating: false,
            id: condition.clone(),
            family: crate::content_ids::ConditionFamilyId::new("fixture_hover").unwrap(),
            name: "Test hover".into(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            modifiers: vec![crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Locomotion {
                    value: crate::locomotion::LocomotionValue::Levitating { enabled: true },
                },
            }],
            stacking: crate::rules::Stacking::Extend,
            source_scoped: false,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Reject,
            timing: crate::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: crate::rules::ConditionPreparation::Preserve,
        });
    });
    game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let mut spec = ItemSpec::ordinary("unmarked vial".into());
    spec.identity = "fixture.hover_drink".into();
    spec.concealed = true;
    spec.stackable = true;
    // Match the existing consumable capability schema; no item identity dispatch.
    spec.class = ItemClass::Potion;
    spec.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: ItemDiscovery::OnUse,
        effects: vec![
            crate::rules::Effect::ApplyConditionForTurns {
                condition: condition.clone(),
                duration: crate::rules::AmountExpr::Rolled {
                    dice: crate::rules::DiceExpr {
                        count: 1,
                        sides: 3,
                        bonus: 28,
                    },
                },
            },
            crate::rules::Effect::ArrestDescent,
        ],
    });
    let item = game
        .place_item_stack(50, at(1), Some(ActorId(1)), 2, spec)
        .unwrap();
    (game, item, condition)
}

#[test]
fn drink_brakes_only_at_completion_and_repeat_dose_still_arrests_imposed_descent() {
    for existing in [None, Some(10_000)] {
        let (mut game, item, condition) = levitation_drink_fixture();
        if let Some(duration) = existing {
            game.apply_condition(ActorId(1), &condition, ActorId(1), duration)
                .unwrap();
        }
        let plan = game.plan_work(ActorId(1), Work::Drink { item }).unwrap();
        let PlannedEffect::ApplyConditionForTurns {
            duration: PlannedAmount::Rolled { dice },
            turn_ticks,
            ..
        } = &plan.on_success()[0]
        else {
            panic!("captured rolled source-turn duration")
        };
        let mut expected_rng = game.rules_rng;
        let duration = u64::from(
            dice.roll(&mut expected_rng, crate::dice::Edge::default())
                .total,
        ) * turn_ticks;
        assert!((29 * turn_ticks..=31 * turn_ticks).contains(&duration));
        let due = plan.preparation();
        start_owned_item_work(&mut game, plan);
        // This isolates paid-item publication, not the separate physics/fall
        // integration owner. The imposed kinematics must survive preparation.
        game.actors.get_mut(&ActorId(1)).unwrap().motion.velocity = [17, -11, -23];
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .motion
            .displacement = [19, -13, -29];
        let mut restored = roundtrip(&game);
        for current in [&mut game, &mut restored] {
            assert_eq!(current.items[&item].quantity, 2);
            assert_eq!(current.actors[&ActorId(1)].motion.velocity, [17, -11, -23]);
            assert!(!current.actors[&ActorId(1)]
                .knowledge
                .contains("fixture.hover_drink"));
            current
                .with_recovery_transition(true, |candidate| {
                    candidate.tick = due;
                    candidate.resolve_preparations()
                })
                .unwrap();
            assert_eq!(current.items[&item].quantity, 1);
            assert!(current.items[&item].holds.is_empty());
            assert_eq!(current.actors[&ActorId(1)].motion.velocity, [17, -11, 0]);
            assert_eq!(
                current.actors[&ActorId(1)].motion.displacement,
                [19, -13, 0]
            );
            assert!(
                current
                    .creature(ActorId(1))
                    .unwrap()
                    .derived()
                    .locomotion
                    .levitating
            );
            assert_eq!(
                current
                    .creature(ActorId(1))
                    .unwrap()
                    .conditions()
                    .remaining(&condition, ActorId(1)),
                Some(existing.map_or(0, |remaining| remaining - due) + duration)
            );
            assert!(current.actors[&ActorId(1)]
                .knowledge
                .contains("fixture.hover_drink"));
            assert_eq!(current.rules_rng, expected_rng);
        }
        assert_eq!(game, restored);
    }
}

#[test]
fn overflowing_levitation_extension_rolls_back_exact_unit_rng_and_descent_brake() {
    let (mut game, item, condition) = levitation_drink_fixture();
    game.apply_condition(
        ActorId(1),
        &condition,
        ActorId(1),
        crate::limits::NUMERIC as u64,
    )
    .unwrap();
    let plan = game.plan_work(ActorId(1), Work::Drink { item }).unwrap();
    let due = plan.preparation();
    start_owned_item_work(&mut game, plan);
    game.actors.get_mut(&ActorId(1)).unwrap().motion.velocity = [17, -11, -23];
    let before = game.clone();
    assert!(game
        .with_recovery_transition(true, |candidate| {
            candidate.tick = due;
            candidate.resolve_preparations()
        })
        .is_err());
    assert_eq!(
        game, before,
        "Rejected extension cannot spend, identify, roll, or brake"
    );
}

#[test]
fn teleport_to_uses_frozen_launch_but_live_interceptor_and_restores_paid_preparation() {
    let mut game = relocation_device_fixture(crate::rules::Effect::TeleportToRayOrigin, true);
    let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
    let work = Work::Zap {
        item,
        aim: crate::DeviceAim::Direction {
            direction: StepDirection::East,
        },
    };
    let captured = game.plan_work(ActorId(1), work).unwrap();
    let frozen = captured.as_ray().unwrap().clone();
    assert_eq!(
        (frozen.preparation, frozen.recovery),
        (100, 0),
        "device use owns item preparation and no recovery, unlike the authored ability timing"
    );
    assert!(serde_json::to_value(&captured)
        .unwrap()
        .get("target")
        .is_none());
    start_owned_item_work(&mut game, captured.clone());
    game.tick = 40;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 40;
    game.pause_preparation(ActorId(1)).unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 60);
    let mut restored = roundtrip(&game);
    for current in [&mut game, &mut restored] {
        // The old recipient moves away during preparation. A different live
        // actor now intercepts the ray; no target identity was committed earlier.
        current.teleport(ActorId(2), at(5)).unwrap();
        let interceptor = current
            .spawn_actor(at(3), std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        let build = current.creature(ActorId(2)).unwrap().build().clone();
        current
            .configure_creature(
                interceptor,
                crate::CreatureIdentity {
                    name: "live interceptor".into(),
                    faction: "third".into(),
                },
                build,
            )
            .unwrap();
        current.actors.get_mut(&interceptor).unwrap().ready_at = 1000;
        let intention = current.preparation(ActorId(1)).unwrap().intention;
        current.start_work(captured.clone(), intention).unwrap();
        assert_eq!(
            *current
                .preparation(ActorId(1))
                .unwrap()
                .plan
                .as_ray()
                .unwrap(),
            frozen
        );
        current
            .with_recovery_transition(true, |candidate| {
                candidate.tick = 100;
                candidate.resolve_preparations()
            })
            .unwrap();
        assert_eq!(
            current.actors.get(&interceptor).map(|actor| actor.location),
            Some(at(2))
        );
        assert_eq!(
            current.actors.get(&ActorId(2)).map(|actor| actor.location),
            Some(at(5))
        );
        assert_eq!(current.items[&item].charges, Some(2));
        assert!(current.items[&item].holds.is_empty());
        assert_eq!(current.actors[&ActorId(1)].ready_at, 100);
        assert!(!current.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.elemental_device"));
        assert!(current.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Resolved { outcome, .. }
            if outcome.target == interceptor && outcome.effects.iter().any(|effect|
                matches!(effect, ResolvedEffect::Relocate { result: MovementResult::Moved { .. }, .. })))));
    }
    assert_eq!(game, restored);
}

#[test]
fn teleport_to_endpoint_uses_all_frozen_launch_frames_not_casters_current_placement() {
    for orientation in 0..24 {
        let mut game = relocation_device_fixture(crate::rules::Effect::TeleportToRayOrigin, true);
        let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
        *game.world = tor_world::World::new(
            vec![tor_world::Region {
                id: RegionId(1),
                name: "ray frame".into(),
                bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let origin = Location {
            region: RegionId(1),
            position: Position { x: 4, y: 4, z: 4 },
        };
        let (x, y, z) = tor_world::Direction::East.rotated(orientation).delta();
        let endpoint = Location {
            position: Position {
                x: 4 + x,
                y: 4 + y,
                z: 4 + z,
            },
            ..origin
        };
        game.teleport(ActorId(1), origin).unwrap();
        game.actors.get_mut(&ActorId(1)).unwrap().orientation = orientation;
        game.teleport(
            ActorId(2),
            Location {
                position: Position {
                    x: 4 + 2 * x,
                    y: 4 + 2 * y,
                    z: 4 + 2 * z,
                },
                ..origin
            },
        )
        .unwrap();
        let recipient_orientation = 23 - orientation;
        game.actors.get_mut(&ActorId(2)).unwrap().orientation = recipient_orientation;
        let captured = game
            .plan_work(
                ActorId(1),
                Work::Zap {
                    item,
                    aim: crate::DeviceAim::Direction {
                        direction: StepDirection::East,
                    },
                },
            )
            .unwrap();
        // Exercise the released-ray geometry kernel directly, as the existing
        // ray owner does. Ordinary relocation of *unfinished* work is tested
        // separately above; it cannot cause this resolver to refreeze origin.
        game.teleport(ActorId(1), at(1)).unwrap();
        game.actors.get_mut(&ActorId(1)).unwrap().orientation = recipient_orientation;
        game.resolve_ray(captured.as_ray().unwrap(), None).unwrap();
        assert_eq!(
            game.actors.get(&ActorId(2)).map(|actor| actor.location),
            Some(endpoint),
            "launch frame {orientation}"
        );
        assert_eq!(game.actors[&ActorId(2)].orientation, recipient_orientation);
    }
}

#[test]
fn teleport_wand_dodge_or_blocked_live_body_fit_spends_charge_without_identification() {
    for dodges in [true, false] {
        let mut game =
            relocation_device_fixture(crate::rules::Effect::TeleportToRayOrigin, !dodges);
        let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
        if dodges {
            game.apply_condition(
                ActorId(2),
                &crate::content_ids::ConditionId::new("ray_agile").unwrap(),
                ActorId(2),
                1000,
            )
            .unwrap();
        } else {
            // The beam is clear at foot height. A wall at the destination's
            // second body cell blocks only the exact teleport placement.
            game.world
                .set_wall(
                    Location {
                        position: Position {
                            z: 1,
                            ..at(3).position
                        },
                        ..at(3)
                    },
                    false,
                )
                .unwrap();
            game.set_body(
                ActorId(2),
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
        }
        let plan = game
            .plan_work(
                ActorId(1),
                Work::Zap {
                    item,
                    aim: crate::DeviceAim::Direction {
                        direction: StepDirection::East,
                    },
                },
            )
            .unwrap();
        start_owned_item_work(&mut game, plan);
        if !dodges {
            game.world
                .set_wall(
                    Location {
                        position: Position {
                            z: 1,
                            ..at(2).position
                        },
                        ..at(2)
                    },
                    true,
                )
                .unwrap();
        }
        game.with_recovery_transition(true, |candidate| {
            candidate.tick = 100;
            candidate.resolve_preparations()
        })
        .unwrap();
        assert_eq!(
            game.actors.get(&ActorId(2)).map(|actor| actor.location),
            Some(at(3))
        );
        assert_eq!(game.items[&item].charges, Some(2));
        assert!(game.items[&item].holds.is_empty());
        assert!(!game.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.elemental_device"));
        let outcome = game
            .combat_events()
            .iter()
            .find_map(|event| match event {
                crate::combat::CombatEvent::Resolved { outcome, .. }
                    if outcome.target == ActorId(2) =>
                {
                    Some(outcome)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(outcome.check.unwrap().success, dodges);
        // A successful recipient dodge selects an empty success branch, not
        // a failed outgoing attack. Both it and blocked placement are unaffected.
        assert_eq!(outcome.result, OutcomeStatus::Unaffected);
        if !dodges {
            assert_eq!(
                outcome.effects,
                [ResolvedEffect::Relocate {
                    mode: MovementMode::Forced,
                    result: MovementResult::Blocked
                }]
            );
        }
    }
}

#[test]
fn forced_caster_relocation_retires_uncompleted_zap_without_spending_a_charge() {
    let mut game = relocation_device_fixture(crate::rules::Effect::TeleportToRayOrigin, true);
    let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
    let captured = game
        .plan_work(
            ActorId(1),
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    start_owned_item_work(&mut game, captured.clone());
    assert_eq!(game.items[&item].holds.len(), 1);
    let origin = Placement::capture(&game, ActorId(1)).unwrap();
    let destination = game
        .random_relocation_candidates(ActorId(1))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    game.with_recovery_transition(true, |candidate| {
        candidate.relocate_exact_in_transaction(ActorId(1), destination, false)
    })
    .unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert!(game.items[&item].holds.is_empty());
    assert_eq!(game.items[&item].charges, Some(3));
    assert_eq!(
        captured.as_ray().unwrap().origin,
        origin,
        "Retiring work does not reinterpret the captured launch operands"
    );
    game.with_recovery_transition(true, |candidate| {
        candidate.relocate_exact_in_transaction(ActorId(1), origin, false)
    })
    .unwrap();
    game.tick = 100;
    game.resolve_preparations().unwrap();
    assert!(
        game.preparation(ActorId(1)).is_none(),
        "Moving back cannot resurrect retired work"
    );
    assert_eq!(game.items[&item].charges, Some(3));
    assert_eq!(
        game.actors.get(&ActorId(2)).map(|actor| actor.location),
        Some(at(3))
    );
}

#[test]
fn paid_self_teleport_reads_exact_unit_and_only_successful_displacement_is_discovered() {
    for blocked in [false, true] {
        let mut game = fixture();
        let mut spec = ItemSpec::ordinary("unread escape page".into());
        spec.identity = "fixture.escape_page".into();
        spec.concealed = true;
        spec.stackable = true;
        spec.readable = Some(ReadableSpec {
            control: None,
            descriptors: vec![],
            effects: vec![crate::rules::Effect::TeleportRandom],
            identify: None,
            discovery: ItemDiscovery::SelfEffect,
        });
        let scroll = game
            .place_item_stack(10, at(1), Some(ActorId(1)), 2, spec)
            .unwrap();
        if blocked {
            for to in game.random_relocation_candidates(ActorId(1)).unwrap() {
                if game.world.door(to.location).is_some() {
                    game.world.set_door(to.location, false);
                } else {
                    game.world.set_wall(to.location, true).unwrap();
                }
            }
            assert!(game
                .random_relocation_candidates(ActorId(1))
                .unwrap()
                .is_empty());
        }
        let plan = game
            .plan_work(ActorId(1), Work::Read { item: scroll })
            .unwrap();
        let due = plan.preparation();
        let ready = due + plan.recovery();
        start_owned_item_work(&mut game, plan);
        let mut restored = roundtrip(&game);
        for current in [&mut game, &mut restored] {
            current
                .with_recovery_transition(true, |candidate| {
                    candidate.tick = due;
                    candidate.resolve_preparations()
                })
                .unwrap();
            assert_eq!(current.items[&scroll].quantity, 1);
            assert!(current.items[&scroll].holds.is_empty());
            assert!(current.preparation(ActorId(1)).is_none());
            assert_eq!(current.actors[&ActorId(1)].ready_at, ready);
            assert_eq!(
                current.actors.get(&ActorId(1)).map(|actor| actor.location) == Some(at(1)),
                blocked
            );
            assert_eq!(
                current.actors[&ActorId(1)]
                    .knowledge
                    .contains("fixture.escape_page"),
                !blocked
            );
            assert!(!current.actors[&ActorId(2)]
                .knowledge
                .contains("fixture.escape_page"));
            assert!(!current.combat_events().iter().any(|event| matches!(
                event,
                crate::combat::CombatEvent::Interrupted {
                    actor: ActorId(1),
                    ..
                }
            )));
        }
        assert_eq!(game, restored);
    }
}

#[test]
fn random_teleport_callers_preserve_rng_consumed_by_ordinary_destination_contact() {
    for paid in [false, true] {
        let mut game = super::ray_tests::fixture_with(|document, _, _| {
            document.descriptors.push(crate::rules::NamedDescriptor {
                resistance: None,
                id: crate::content_ids::DescriptorId::new("poison").unwrap(),
                name: "Poison".into(),
            });
        });
        game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        let mut spec = ItemSpec::ordinary("escape page".into());
        spec.readable = Some(ReadableSpec {
            control: None,
            descriptors: vec![],
            effects: vec![crate::rules::Effect::TeleportRandom],
            identify: None,
            discovery: ItemDiscovery::SelfEffect,
        });
        let scroll = game
            .place_item_stack(10, at(1), Some(ActorId(1)), 1, spec)
            .unwrap();
        let candidates = game.random_relocation_candidates(ActorId(1)).unwrap();
        assert!(candidates.len() > 1);
        let setup_rng = game.rules_rng;
        let mut expected_rng = setup_rng;
        let index = crate::quantity::QuantityRange::new(1, candidates.len() as u64)
            .unwrap()
            .sample(&mut expected_rng)
            .unwrap()
            - 1;
        let destination = candidates[index as usize];
        let selection_rng = expected_rng;
        // A guaranteed miss still performs the ordinary three-die attack check.
        // No poison rider or damage roll is selected, so the expected suffix is exact.
        crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::default());
        game.place_trap(
            destination.location,
            tor_world::Trap {
                id: 991,
                dc: 15,
                avoidance_dc: None,
                kind: tor_world::TrapKind::PoisonDart {
                    attack_bonus: -100,
                    exposure: "poison".into(),
                },
            },
        )
        .unwrap();
        // Isolate nested contact RNG from ordinary passive discovery during
        // paid admission. Knowing this plate does not avoid it: avoidance is None.
        for actor in [ActorId(1), ActorId(2)] {
            game.reveal_feature_family(actor, tor_world::ConcealedFeatureId::Trap(991))
                .unwrap();
            assert!(game.feature_known(actor, tor_world::ConcealedFeatureId::Trap(991)));
        }
        let plan = game
            .plan_work(ActorId(1), Work::Read { item: scroll })
            .unwrap();
        let due = plan.preparation();
        if paid {
            start_owned_item_work(&mut game, plan);
        }
        assert_eq!(
            game.rules_rng, setup_rng,
            "setup must not draw before relocation"
        );
        assert_eq!(
            game.random_relocation_candidates(ActorId(1)).unwrap(),
            candidates
        );
        let mut restored = roundtrip(&game);
        for current in [&mut game, &mut restored] {
            current
                .with_recovery_transition(true, |candidate| {
                    if paid {
                        candidate.tick = due;
                        candidate.resolve_preparations()
                    } else {
                        candidate
                            .apply_effects_in_transaction(
                                ActorId(1),
                                &[crate::rules::Effect::TeleportRandom],
                            )
                            .map(|_| ())
                    }
                })
                .unwrap();
            assert_eq!(
                current.actors.get(&ActorId(1)).map(|actor| actor.location),
                Some(destination.location)
            );
            assert!(current.actors[&ActorId(1)]
                .trap_contacts
                .ids()
                .any(|id| id == 991));
            assert_eq!(current.items.contains_key(&scroll), !paid);
            assert_ne!(expected_rng, selection_rng);
            assert_eq!(
                current.rules_rng, expected_rng,
                "Publishing the parent teleport must not overwrite the nested contact's draws"
            );
        }
        assert_eq!(game, restored);
    }
}

// Both games contain both definitions. Only the concealed item's true identity
// and capability differ; no identifier spelling licenses ordinary disclosure.
fn private_utility_fixture(
    alternative: bool,
    discovery: ItemDiscovery,
    hidden_target: bool,
    dodges: bool,
) -> (Game, ItemId) {
    use crate::content_ids::{AbilityId, ConditionFamilyId, ConditionId};
    use crate::rules::*;
    let mut game = super::ray_tests::fixture_with(|document, ray, species| {
        species.modifiers.retain(|modifier| {
            !matches!(&modifier.op, ModifierOp::GrantAbility { ability } if ability == &ray.id)
        });
        let prototype = document
            .conditions
            .iter()
            .find(|condition| condition.id.as_str() == "ray_fear")
            .unwrap()
            .clone();
        for (id, rate) in [
            (
                "private_state_a",
                crate::action_duration::ActionDuration::Haste,
            ),
            (
                "private_state_b",
                crate::action_duration::ActionDuration::Slow,
            ),
        ] {
            let mut condition = prototype.clone();
            condition.id = ConditionId::new(id).unwrap();
            condition.family = ConditionFamilyId::new(id).unwrap();
            condition.name = id.into();
            condition.timing = ConditionTiming::UntilRemoved;
            condition.stacking = Stacking::Keep;
            condition.source_scoped = false;
            condition.modifiers = vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::ActionDuration { value: rate },
            }];
            document.conditions.push(condition);
        }
        for (id, op) in [
            (
                "private_lore",
                ModifierOp::Add {
                    stat: Stat::Attribute {
                        attribute: crate::attributes::Attribute::Intellect,
                    },
                    value: 100,
                },
            ),
            (
                "private_invisible",
                ModifierOp::Perception {
                    value: crate::perception::PerceptionValue::Invisible { enabled: true },
                },
            ),
        ] {
            let mut condition = prototype.clone();
            condition.id = ConditionId::new(id).unwrap();
            condition.family = ConditionFamilyId::new(id).unwrap();
            condition.name = id.into();
            condition.modifiers = vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op,
            }];
            document.conditions.push(condition);
        }
        ray.name = "First concealed utility".into();
        ray.invocation = InvocationKind::DirectionalContactRay;
        let StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        strikes[0].check.as_mut().unwrap().against = Against::Opposed {
            skill: SkillSelector::Named {
                skill: crate::attributes::Skill::Lore,
            },
        };
        strikes[0].on_failure = vec![Effect::ApplyPersistentCondition {
            condition: ConditionId::new("private_state_a").unwrap(),
            cancels: None,
        }];
        let mut other = ray.clone();
        other.id = AbilityId::new("fixture_ray_alternative").unwrap();
        other.name = "Second concealed utility".into();
        let StrikeSource::Authored { strikes } = &mut other.strikes else {
            unreachable!()
        };
        strikes[0].on_failure = vec![Effect::ApplyPersistentCondition {
            condition: ConditionId::new("private_state_b").unwrap(),
            cancels: None,
        }];
        document.abilities.push(other);
    });
    game.set_region_light(RegionId(1), true).unwrap();
    let build = game.creature(ActorId(2)).unwrap().build().clone();
    let mut location = at(1);
    location.position.y = 2;
    let bystander = game
        .spawn_actor(location, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(bystander, ActorId(3));
    game.configure_creature(
        bystander,
        crate::CreatureIdentity {
            name: "uninvolved witness".into(),
            faction: "third".into(),
        },
        build,
    )
    .unwrap();
    game.actors.get_mut(&bystander).unwrap().ready_at = 1000;
    let (actor, condition) = if dodges {
        (ActorId(2), "ray_agile")
    } else {
        (ActorId(1), "private_lore")
    };
    game.apply_condition(actor, &ConditionId::new(condition).unwrap(), actor, 500)
        .unwrap();
    if hidden_target {
        game.apply_condition(
            ActorId(2),
            &ConditionId::new("private_invisible").unwrap(),
            ActorId(2),
            500,
        )
        .unwrap();
        assert!(!game
            .observe(ActorId(1))
            .unwrap()
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(2)));
    }
    let item = device_with_discovery(&mut game, 10, 3, discovery);
    game.items
        .edit(item, |item| {
            item.spec.identity = if alternative {
                "opaque_device_b"
            } else {
                "opaque_device_a"
            }
            .into();
            item.spec.name = if alternative {
                "Second concealed utility"
            } else {
                "First concealed utility"
            }
            .into();
            item.spec.device.as_mut().unwrap().ability = AbilityId::new(if alternative {
                "fixture_ray_alternative"
            } else {
                "fixture_ray"
            })
            .unwrap();
        })
        .unwrap();
    (game, item)
}

fn private_utility_views(
    game: &mut Game,
    item: ItemId,
    direction: StepDirection,
) -> Vec<crate::Observation> {
    // Reuse the host's actual event-time observation boundary so a bystander
    // gets a witnessed receipt, rather than reconstructing history on a read.
    game.host_region_continuation = true;
    execute(
        game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction { direction },
        },
    );
    let witness = game.event_observation_witness();
    let mut views = Vec::new();
    for observer in [ActorId(1), ActorId(2), ActorId(3)] {
        let (mut view, _, proof) = game.observe_event_scene(observer).unwrap();
        game.record_event_observation(&witness, &proof, &mut view)
            .unwrap();
        views.push(view);
    }
    game.seal_event_observations().unwrap();
    game.host_region_continuation = false;
    for view in &views {
        assert_eq!(*view, game.observe(view.actor).unwrap());
    }
    views
}

fn assert_private_utility_identity(game: &Game, item: ItemId) {
    let view = game.observe(ActorId(1)).unwrap();
    let row = view.inventory.iter().find(|row| row.id == item).unwrap();
    assert!(!row.identified);
    assert_eq!(row.name, "plain stick");
    assert_eq!(
        view.interactions
            .unwrap()
            .inventory
            .iter()
            .find(|row| row.item == item)
            .unwrap()
            .charges,
        None
    );
    assert!(game.actors[&ActorId(1)].known_charges.is_empty());
}

#[test]
fn unknown_never_device_dodge_keeps_all_ordinary_disclosures_indistinguishable() {
    let (mut first, first_item) = private_utility_fixture(false, ItemDiscovery::Never, false, true);
    let (mut second, second_item) =
        private_utility_fixture(true, ItemDiscovery::Never, false, true);
    for observer in [ActorId(1), ActorId(2), ActorId(3)] {
        assert_eq!(
            first.observe(observer).unwrap(),
            second.observe(observer).unwrap()
        );
    }
    let first_views = private_utility_views(&mut first, first_item, StepDirection::East);
    let second_views = private_utility_views(&mut second, second_item, StepDirection::East);
    for game in [&first, &second] {
        let outcomes: Vec<_> = game
            .combat_events()
            .iter()
            .filter_map(|event| match event {
                crate::combat::CombatEvent::Resolved { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect();
        assert_eq!(
            outcomes.len(),
            1,
            "the first physical contact was actually checked"
        );
        assert!(
            outcomes[0].check.unwrap().success,
            "the contact actually dodged"
        );
        assert!(outcomes[0].effects.is_empty());
        assert_private_utility_identity(game, first_item);
    }
    for (first, second) in first_views.iter().zip(&second_views) {
        assert_eq!(
            first.combat.as_ref().unwrap().events,
            second.combat.as_ref().unwrap().events,
            "a dodge cannot name the concealed device ability for observer {:?}",
            first.actor
        );
        assert_eq!(
            first, second,
            "all other ordinary disclosure must also agree"
        );
    }
}

#[test]
fn unknown_never_device_contact_does_not_teach_ability_identity_to_any_observer() {
    for alternative in [false, true] {
        let (mut game, item) =
            private_utility_fixture(alternative, ItemDiscovery::Never, false, true);
        let known: Vec<_> = [ActorId(1), ActorId(2), ActorId(3)]
            .map(|actor| game.actors[&actor].recognized_abilities.clone())
            .into();
        let views = private_utility_views(&mut game, item, StepDirection::East);
        for (view, before) in views.iter().zip(&known) {
            game.record_observation_knowledge(view).unwrap();
            assert_eq!(
                game.actors[&view.actor].recognized_abilities, *before,
                "an unknown Never device must not become recognized ability knowledge"
            );
        }
        let reopened = roundtrip(&game);
        for (view, before) in views.iter().zip(&known) {
            assert_eq!(reopened.observe(view.actor).unwrap(), *view);
            assert_eq!(reopened.actors[&view.actor].recognized_abilities, *before);
        }
    }
}

#[test]
fn unknown_never_device_hidden_contact_preserves_effects_without_naming_the_source() {
    let (mut first, first_item) = private_utility_fixture(false, ItemDiscovery::Never, true, false);
    let (mut second, second_item) =
        private_utility_fixture(true, ItemDiscovery::Never, true, false);
    let first_views = private_utility_views(&mut first, first_item, StepDirection::East);
    let second_views = private_utility_views(&mut second, second_item, StepDirection::East);
    for (game, expected) in [(&first, "private_state_a"), (&second, "private_state_b")] {
        assert!(game
            .creature(ActorId(2))
            .unwrap()
            .conditions()
            .has_active(&crate::content_ids::ConditionId::new(expected).unwrap()));
        assert_private_utility_identity(game, first_item);
    }
    assert_ne!(
        first_views[1].combat.as_ref().unwrap().own_stats,
        second_views[1].combat.as_ref().unwrap().own_stats,
        "the recipient still receives its legitimate, different persistent effects"
    );
    for view in [&first_views[0], &second_views[0]] {
        assert!(
            view.combat.as_ref().unwrap().events.is_empty(),
            "an unseen contact must not leak even an anonymous event count or outcome"
        );
    }
    for index in [0, 2] {
        assert_eq!(
            first_views[index], second_views[index],
            "an unseen first contact cannot name its source ability or expose its target"
        );
    }
    assert_eq!(
        first_views[1].combat.as_ref().unwrap().events,
        second_views[1].combat.as_ref().unwrap().events,
        "the recipient's effect state is not permission to name a concealed device"
    );
    for (game, views) in [(&first, &first_views), (&second, &second_views)] {
        let reopened = roundtrip(game);
        for view in views {
            assert_eq!(reopened.observe(view.actor).unwrap(), *view);
        }
    }
}

#[test]
fn unknown_never_device_empty_launch_and_empty_charge_keep_identity_private() {
    for charges in [0, 3] {
        let (mut first, first_item) =
            private_utility_fixture(false, ItemDiscovery::Never, false, false);
        let (mut second, second_item) =
            private_utility_fixture(true, ItemDiscovery::Never, false, false);
        for (game, item) in [(&mut first, first_item), (&mut second, second_item)] {
            game.items
                .edit(item, |item| item.charges = Some(charges))
                .unwrap();
        }
        let first_views = private_utility_views(&mut first, first_item, StepDirection::North);
        let second_views = private_utility_views(&mut second, second_item, StepDirection::North);
        assert_eq!(first_views, second_views);
        for (game, item, views) in [
            (&mut first, first_item, &first_views),
            (&mut second, second_item, &second_views),
        ] {
            assert_eq!(game.items[&item].charges, Some(charges.saturating_sub(1)));
            assert_private_utility_identity(game, item);
            for view in views {
                let known = game.actors[&view.actor].recognized_abilities.clone();
                game.record_observation_knowledge(view).unwrap();
                assert_eq!(game.actors[&view.actor].recognized_abilities, known);
                assert!(view.combat.as_ref().unwrap().events.is_empty());
            }
            assert_eq!(*game, roundtrip(game));
        }
    }
}

#[test]
fn unknown_never_device_pending_zap_has_no_public_plan_or_identity_preview() {
    let (mut first, first_item) = private_utility_fixture(false, ItemDiscovery::Never, false, true);
    let (mut second, second_item) =
        private_utility_fixture(true, ItemDiscovery::Never, false, true);
    for (game, item) in [(&mut first, first_item), (&mut second, second_item)] {
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
        execute(
            game,
            Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        );
        assert!(game.preparation(ActorId(1)).is_some());
        assert_private_utility_identity(game, item);
        for observer in [ActorId(2), ActorId(3)] {
            assert_eq!(
                game.capture_public_plan_observation(observer, ActorId(1))
                    .unwrap()
                    .state,
                crate::public_plan::PublicPlanState::NoPublicPlan
            );
        }
    }
    for observer in [ActorId(1), ActorId(2), ActorId(3)] {
        assert_eq!(
            first.observe(observer).unwrap(),
            second.observe(observer).unwrap()
        );
    }
}

#[test]
fn unknown_never_device_later_identification_or_reveal_cannot_upgrade_old_receipts() {
    for hidden_target in [false, true] {
        let (mut game, item) =
            private_utility_fixture(false, ItemDiscovery::Never, hidden_target, true);
        let views = private_utility_views(&mut game, item, StepDirection::East);
        for view in &views {
            assert!(
                !view
                    .combat
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .any(|event| matches!(
                        event,
                        crate::combat::DisclosedCombatEvent::Ability { .. }
                    )),
                "the original unknown-device receipt has no licensed ability identity"
            );
        }
        game.identify_item(ActorId(1), item).unwrap();
        if hidden_target {
            // Change current perception without replacing the already sealed batch.
            let mut update = game.actor_outcome(ActorId(2)).unwrap();
            let combat = update.state.combat.as_mut().unwrap();
            let mut creature = combat.creature().clone();
            assert!(creature
                .remove_condition(
                    &crate::content_ids::ConditionId::new("private_invisible").unwrap()
                )
                .unwrap());
            *combat =
                crate::actor_creatures::CombatState::from_creature(combat.identity(), creature);
            game.apply_outcome(update).unwrap();
            assert!(game
                .observe(ActorId(1))
                .unwrap()
                .visible_actors
                .iter()
                .any(|actor| actor.id == ActorId(2)));
        }
        let reopened = roundtrip(&game);
        for current in [&game, &reopened] {
            for view in &views {
                assert_eq!(
                    current.observe(view.actor).unwrap().combat.unwrap().events,
                    view.combat.as_ref().unwrap().events,
                    "later knowledge and visibility cannot relabel a sealed receipt"
                );
            }
        }
    }
}

#[test]
fn known_never_and_on_use_devices_retain_licensed_owner_identity_feedback() {
    for discovery in [ItemDiscovery::Never, ItemDiscovery::OnUse] {
        for charges in [0, 3] {
            let (mut game, item) = private_utility_fixture(false, discovery, false, true);
            game.items
                .edit(item, |item| item.charges = Some(charges))
                .unwrap();
            if discovery == ItemDiscovery::Never {
                game.identify_item(ActorId(1), item).unwrap();
            }
            let known = game.actors[&ActorId(1)].recognized_abilities.clone();
            let views = private_utility_views(&mut game, item, StepDirection::East);
            game.record_observation_knowledge(&views[0]).unwrap();
            if charges > 0 {
                assert!(views[0]
                    .combat
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .any(|event| matches!(
                        event,
                        crate::combat::DisclosedCombatEvent::Ability { id, name, .. }
                            if id.as_str() == "fixture_ray" && name == "First concealed utility"
                    )));
                assert!(game.actors[&ActorId(1)]
                    .recognized_abilities
                    .contains(&crate::content_ids::AbilityId::new("fixture_ray").unwrap()));
            } else {
                assert!(views[0].combat.as_ref().unwrap().events.is_empty());
                assert_eq!(game.actors[&ActorId(1)].recognized_abilities, known);
            }
            assert_eq!(
                game.actors[&ActorId(1)]
                    .knowledge
                    .contains("opaque_device_a"),
                discovery == ItemDiscovery::Never || charges > 0
            );
            assert!(game.actors[&ActorId(1)].known_charges.is_empty());
            assert_eq!(game, roundtrip(&game));
        }
    }
}

#[test]
fn charged_item_uses_rank_zero_compiled_lore_without_grant_or_quantity_payment() {
    let mut game = fixture();
    let item = device(&mut game, 10, 3);
    let before = game.creature(ActorId(1)).unwrap().into_owned();
    assert!(!before
        .derived()
        .abilities
        .contains(&crate::content_ids::AbilityId::new("fixture_ray").unwrap()));
    let plan = game
        .plan_work(
            ActorId(1),
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North,
                },
            },
        )
        .unwrap();
    let ray = plan.as_ray().unwrap();
    let super::RayResolution::Opposed {
        source_skill,
        source_bonus,
        ..
    } = &ray.resolution
    else {
        panic!("fixture must capture its genuine opposed check");
    };
    assert_eq!(*source_skill, crate::attributes::Skill::Lore);
    assert_eq!(
        *source_bonus,
        super::ray::named_bonus(before.derived(), crate::attributes::Skill::Lore)
    );
    assert!(plan.cost().is_none());
    assert!(plan.ability_id().is_none());
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North,
            },
        },
    );
    assert_eq!(game.items[&item].charges, Some(2));
    assert_eq!(game.items[&item].quantity, 1);
    assert!(game.items[&item].holds.is_empty());
    assert_eq!(
        game.creature(ActorId(1)).unwrap().derived().abilities,
        before.derived().abilities
    );
    assert!(game.actors[&ActorId(1)]
        .knowledge
        .contains("fixture.elemental_device"));
    assert!(game.actors[&ActorId(1)].known_charges.is_empty());
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn unknown_empty_device_keeps_same_affordance_and_pays_time_without_rng_or_learning() {
    let mut game = fixture();
    let item = device(&mut game, 10, 0);
    let before = game
        .observe(ActorId(1))
        .unwrap()
        .interactions
        .unwrap()
        .inventory[0]
        .clone();
    assert!(before.zappable);
    assert_eq!(before.charges, None);
    let rng = game.rules_rng;
    let tick = game.tick;
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North,
            },
        },
    );
    assert!(game.tick > tick);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.items[&item].charges, Some(0));
    assert_eq!(game.items[&item].quantity, 1);
    assert!(!game.actors[&ActorId(1)]
        .knowledge
        .contains("fixture.elemental_device"));
    assert_eq!(
        game.observe(ActorId(1))
            .unwrap()
            .interactions
            .unwrap()
            .inventory[0],
        before
    );
}

#[test]
fn identify_consumes_before_choice_and_only_selected_instance_reveals_charges() {
    let mut game = fixture();
    let first = device(&mut game, 10, 2);
    let second = device(&mut game, 11, 6);
    let scroll = identify_scroll(&mut game, 12, ItemClass::Tool, 1);
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .interactions
        .unwrap()
        .identification
        .is_none());
    let origin = execute(&mut game, Action::Read { item: scroll });
    assert!(!game.items.contains_key(&scroll));
    let choice = game.identification_view(ActorId(1)).unwrap();
    assert_eq!(choice.choice, origin);
    assert_eq!(choice.classes, vec![ItemClass::Tool]);
    assert_eq!(choice.items, vec![first, second]);
    assert_eq!(game, roundtrip(&game));
    execute(
        &mut game,
        Action::Identify {
            choice: origin,
            item: Some(first),
        },
    );
    assert!(game.identification_view(ActorId(1)).is_none());
    let view = game.observe(ActorId(1)).unwrap();
    assert!(view.inventory.iter().all(|i| i.identified));
    let rows = view.interactions.unwrap().inventory;
    assert_eq!(
        rows.iter().find(|r| r.item == first).unwrap().charges,
        Some(2)
    );
    assert_eq!(
        rows.iter().find(|r| r.item == second).unwrap().charges,
        None
    );
    assert!(game.actors[&ActorId(2)].knowledge.is_empty());
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn identify_wrong_owner_category_stale_and_decline_never_refund_or_duplicate() {
    let mut game = fixture();
    let item = device(&mut game, 10, 2);
    let scroll = identify_scroll(&mut game, 11, ItemClass::Potion, 2);
    let origin = execute(&mut game, Action::Read { item: scroll });
    let before = game.clone();
    assert!(game
        .resolve_identification(ActorId(2), origin, Some(item))
        .is_err());
    assert!(game
        .resolve_identification(ActorId(1), origin, Some(item))
        .is_err());
    assert_eq!(game, before);
    assert!(game
        .plan_work(ActorId(1), Work::Read { item: scroll })
        .is_err());
    execute(
        &mut game,
        Action::Identify {
            choice: origin,
            item: None,
        },
    );
    assert_eq!(game.items[&scroll].quantity, 1);
    let before = game.clone();
    assert!(game
        .resolve_identification(ActorId(1), origin, None)
        .is_err());
    assert_eq!(game, before);
    assert!(game.identification_view(ActorId(1)).is_none());
}

#[test]
fn pending_zap_reserves_exact_device_and_replays_payment_once_after_pause() {
    let mut game = fixture();
    let item = device(&mut game, 10, 3);
    let other = device(&mut game, 11, 7);
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North,
            },
        },
    );
    assert_eq!(game.tick, 0);
    assert_eq!(game.items[&item].charges, Some(3));
    assert_eq!(game.items[&item].holds.len(), 1);
    assert!(game.items[&other].holds.is_empty());
    assert_eq!(
        game.pause_preparation(ActorId(1)),
        Some(Work::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North
            }
        })
    );
    let mut restored = roundtrip(&game);
    assert_eq!(game, restored);
    for current in [&mut game, &mut restored] {
        let p = current.preparation(ActorId(1)).unwrap().clone();
        let work = p.plan.work();
        current.start_work(p.plan, p.intention).unwrap();
        current
            .with_recovery_transition(true, |g| {
                g.tick = 100;
                g.resolve_preparations()
            })
            .unwrap();
        assert!(current.preparation(ActorId(1)).is_none());
        assert_eq!(current.items[&item].charges, Some(2));
        assert_eq!(current.items[&other].charges, Some(7));
        assert!(current.items[&item].holds.is_empty());
        assert_eq!(
            work,
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North
                }
            }
        );
    }
    assert_eq!(game, restored);
}

#[test]
fn failed_release_rolls_back_charge_effect_rng_knowledge_and_choice_together() {
    let mut game = fixture();
    let item = device(&mut game, 10, 3);
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::East,
            },
        },
    );
    let before = game.clone();
    let result = game.with_recovery_transition(true, |candidate| {
        candidate.tick = 100;
        candidate.resolve_preparations()?;
        assert_eq!(candidate.items[&item].charges, Some(2));
        Err::<(), _>(GameError::IdentityExhausted)
    });
    assert_eq!(result, Err(GameError::IdentityExhausted));
    assert_eq!(game, before);
}

#[test]
fn magic_capabilities_charge_state_and_choice_have_strict_named_current_shapes() {
    let mut game = fixture();
    let item = device(&mut game, 10, 3);
    let value = serde_json::to_value(&game.items[&item]).unwrap();
    for missing in ["charges", "condition"] {
        let mut bad = value.clone();
        bad.as_object_mut().unwrap().remove(missing);
        assert!(
            serde_json::from_value::<crate::Item>(bad).is_err(),
            "{missing}"
        );
    }
    let mut bad = value.clone();
    bad["charges"] = serde_json::json!(65536);
    assert!(serde_json::from_value::<crate::Item>(bad).is_err());
    let mut spec = (*game.items[&item].spec).clone();
    spec.stackable = true;
    assert!(!spec.valid());
    assert!(serde_json::from_str::<ChargeRange>("[3,7]").is_err());
    assert!(serde_json::from_str::<ChargeRange>(r#"{"minimum":7,"maximum":3}"#).is_err());
    assert!(
        serde_json::from_str::<crate::magic_items::IdentificationChoice>(
            r#"{"choice":0,"classes":["wand"]}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<crate::magic_items::IdentificationChoice>(r#"[1,["wand"]]"#)
            .is_err()
    );
    for _ in 0..32 {
        let sampled = ChargeRange {
            minimum: 3,
            maximum: 7,
        }
        .sample(&mut game.rules_rng)
        .unwrap();
        assert!((3..=7).contains(&sampled));
    }
}

#[test]
fn exact_charge_facts_follow_only_observed_owner_payments_not_other_actors() {
    let mut game = fixture();
    let item = device(&mut game, 10, 5);
    let scroll = identify_scroll(&mut game, 11, ItemClass::Tool, 1);
    let choice = execute(&mut game, Action::Read { item: scroll });
    execute(
        &mut game,
        Action::Identify {
            choice,
            item: Some(item),
        },
    );
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North,
            },
        },
    );
    assert_eq!(
        game.actors[&ActorId(1)]
            .known_charges
            .observed(item, game.items[&item].charges),
        Some(4)
    );

    execute(
        &mut game,
        Action::Drop {
            item,
            quantity: None,
        },
    );
    assert_eq!(game.items[&item].charges, Some(4));
    execute(
        &mut game,
        Action::Take {
            item,
            quantity: None,
        },
    );
    assert_eq!(game.items[&item].charges, Some(4));

    // Transfer preserves the physical instance but conveys no observer knowledge.
    game.items
        .edit(item, |i| {
            i.location = crate::ItemLocation::Carried(ActorId(2))
        })
        .unwrap();
    assert_eq!(
        game.observe(ActorId(2))
            .unwrap()
            .interactions
            .unwrap()
            .inventory[0]
            .charges,
        None
    );
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = game.tick + 5000;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let action = Action::Zap {
        item,
        aim: crate::DeviceAim::Direction {
            direction: StepDirection::South,
        },
    };
    let origin = game
        .admit_intention(ActorId(2), action, IntentionOrigin::Human)
        .unwrap();
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, origin);
    done.outcome.unwrap();
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(game.actors[&ActorId(2)].known_charges.is_empty());
    assert_eq!(
        game.actors[&ActorId(1)]
            .known_charges
            .observed(item, Some(4)),
        Some(4)
    );
    game.items
        .edit(item, |i| {
            i.location = crate::ItemLocation::Carried(ActorId(1))
        })
        .unwrap();
    assert_eq!(
        game.observe(ActorId(1))
            .unwrap()
            .interactions
            .unwrap()
            .inventory[0]
            .charges,
        None
    );
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn ordinary_ai_uses_known_instance_device_without_a_build_grant_or_hidden_preview() {
    let mut game = fixture();
    let item = device(&mut game, 10, 5);
    let scroll = identify_scroll(&mut game, 11, ItemClass::Tool, 1);
    let engage = |game: &mut Game| {
        game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
            .unwrap();
        let engagement = crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(2),
            location: at(3),
            last_seen: game.actor_clock(ActorId(1)),
        };
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .behavior
            .engagement = Some(engagement);
    };
    let mut probe = game.clone();
    engage(&mut probe);
    assert!(!matches!(
        probe.choose_ai(ActorId(1)).unwrap().action,
        Action::Zap { .. }
    ));
    probe.identify_item(ActorId(1), item).unwrap();
    assert!(
        !matches!(
            probe.choose_ai(ActorId(1)).unwrap().action,
            Action::Zap { .. }
        ),
        "type knowledge cannot read an unknown balance"
    );
    let choice = execute(&mut game, Action::Read { item: scroll });
    execute(
        &mut game,
        Action::Identify {
            choice,
            item: Some(item),
        },
    );
    engage(&mut game);
    let before = game.clone();
    let selected = game.choose_ai(ActorId(1)).unwrap().action;
    assert_eq!(
        selected,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::East
            }
        }
    );
    assert_eq!(
        game, before,
        "selection must not pay, reserve, roll or discover"
    );
    let mut twin = game.clone();
    twin.add_region(tor_world::Region {
        id: RegionId(3),
        name: "hidden".into(),
        bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
    })
    .unwrap();
    twin.spawn_actor(
        Location {
            region: RegionId(3),
            position: Position { x: 3, y: 3, z: 0 },
        },
        std::num::NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    assert_eq!(
        game.observe(ActorId(1)).unwrap(),
        twin.observe(ActorId(1)).unwrap()
    );
    assert_eq!(selected, twin.choose_ai(ActorId(1)).unwrap().action);
}

#[test]
fn magic_item_regression_hostile_device_reveals_guise_without_ability_grant() {
    let mut game = super::ray_tests::fixture_with(|_, ray, species| {
        species.modifiers.retain(|m| !matches!(&m.op, crate::rules::ModifierOp::GrantAbility { ability } if ability == &ray.id));
        species.modifiers.push(crate::rules::Modifier {
            cancellable: false,
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Perception {
                value: crate::perception::PerceptionValue::ObjectGuise {
                    skill: crate::attributes::Skill::Stealth,
                    edge: 0,
                    appearance: Some(crate::perception::GuiseAppearance {
                        name: "wooden box".into(),
                        description: "A closed box.".into(),
                        class: ItemClass::Tool,
                        asset: None,
                    }),
                },
            },
        });
    });
    let item = device(&mut game, 10, 3);
    assert!(game.actors[&ActorId(1)].guise.appearance().is_some());
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::North,
            },
        },
    );
    assert!(
        game.actors[&ActorId(1)].guise.appearance().is_none(),
        "hostile item invocation must reveal its source just like an ordinary attack"
    );
    assert!(!game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .abilities
        .contains(&crate::content_ids::AbilityId::new("fixture_ray").unwrap()));
}

#[test]
fn charged_dodge_and_immunity_pay_once_and_death_releases_unpaid_device() {
    for immune in [false, true] {
        let mut game = super::ray_tests::fixture_with(|_, ray, species| {
            species.modifiers.retain(|m| !matches!(&m.op, crate::rules::ModifierOp::GrantAbility { ability } if ability == &ray.id));
            if immune {
                species.modifiers.push(crate::rules::Modifier {
                    cancellable: false,
                    scope: crate::rules::Scope::Always,
                    op: crate::rules::ModifierOp::Immunity {
                        selector: crate::rules::Selector::Category {
                            category: crate::combat::DamageType::Energy,
                        },
                    },
                });
            }
        });
        let item = device(&mut game, 10, 3);
        let (actor, condition) = if immune {
            (ActorId(1), "ray_strong")
        } else {
            (ActorId(2), "ray_agile")
        };
        game.apply_condition(
            actor,
            &crate::content_ids::ConditionId::new(condition).unwrap(),
            actor,
            1000,
        )
        .unwrap();
        execute(
            &mut game,
            Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        );
        assert_eq!(game.items[&item].charges, Some(2));
        assert_eq!(game.items[&item].quantity, 1);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        assert!(game.combat_events().iter().any(|event| matches!(event,crate::combat::CombatEvent::Resolved {outcome,..} if outcome.target==ActorId(2) && outcome.check.is_some_and(|check|check.success != immune))));
    }
    let mut game = fixture();
    let item = device(&mut game, 10, 3);
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::East,
            },
        },
    );
    assert_eq!(game.items[&item].holds.len(), 1);
    crate::test_creatures::injure(&mut game, ActorId(1), 1000).unwrap();
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(game.items[&item].holds.is_empty());
    assert_eq!(
        game.items[&item].location,
        crate::ItemLocation::Ground(at(1))
    );
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn paid_choice_death_and_corrupt_restored_device_claims_cannot_duplicate_payment() {
    // Death comes from an ordinary accepted attack. Trusted no-time effect
    // injection is not a scheduler boundary and owns a separate limitation.
    let mut game = super::ray_tests::fixture_with(|_, ray, species| {
        species.modifiers.retain(|m| !matches!(&m.op, crate::rules::ModifierOp::GrantAbility { ability } if ability == &ray.id));
    });
    let item = device(&mut game, 10, 3);
    let scroll = identify_scroll(&mut game, 11, ItemClass::Tool, 2);
    let attacker_device = device(&mut game, 12, 1);
    game.items
        .edit(attacker_device, |item| {
            item.location = crate::ItemLocation::Carried(ActorId(2))
        })
        .unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    crate::test_creatures::injure(&mut game, ActorId(1), 99).unwrap();
    game.apply_condition(
        ActorId(2),
        &crate::content_ids::ConditionId::new("ray_strong").unwrap(),
        ActorId(2),
        1000,
    )
    .unwrap();
    execute(&mut game, Action::Read { item: scroll });
    game.admit_intention(ActorId(2), Action::Wait, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(game.identification_view(ActorId(1)).is_some());
    execute(&mut game, Action::Wait);
    assert!(game.identification_view(ActorId(1)).is_some());
    game.admit_intention(
        ActorId(2),
        Action::Zap {
            item: attacker_device,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::West,
            },
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(!game.alive(ActorId(1)));
    assert!(game.identification_view(ActorId(1)).is_none());
    assert_eq!(game.items[&scroll].quantity, 1);
    assert_eq!(game.items[&item].charges, Some(3));
    assert_eq!(game.items[&attacker_device].charges, Some(0));
    assert_eq!(game, roundtrip(&game));

    let mut pending = fixture();
    let item = device(&mut pending, 10, 3);
    pending.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    execute(
        &mut pending,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::East,
            },
        },
    );
    for change in 0..3 {
        let mut bad = pending.clone();
        bad.items
            .edit(item, |item| match change {
                0 => item.charges = Some(2),
                1 => item.holds.clear(),
                _ => item.charges = Some(8),
            })
            .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = bad.checkpoint(&mut shared);
        assert!(
            Game::restore_checkpoint(snapshot, &shared, bad.content().clone()).is_err(),
            "corruption {change}"
        );
    }
}

#[test]
fn identification_choices_and_owned_charge_facts_are_bounded_named_records() {
    let mut game = fixture();
    for id in 10..75 {
        device(&mut game, id, 3);
    }
    let scroll = identify_scroll(&mut game, 100, ItemClass::Tool, 1);
    let choice = execute(&mut game, Action::Read { item: scroll });
    assert_eq!(
        game.identification_view(ActorId(1)).unwrap().items.len(),
        64
    );
    assert!(game
        .validate_identification(ActorId(1), choice, Some(ItemId(74)))
        .is_err());
    #[derive(serde::Deserialize)]
    struct Facts {
        #[serde(deserialize_with = "crate::magic_items::read_known_charges")]
        _facts: tor_world::Shared<crate::magic_items::ChargeKnowledge>,
    }
    let choice = serde_json::json!({"choice":1,"classes":["tool"]});
    assert!(
        serde_json::from_value::<crate::magic_items::IdentificationChoice>(choice.clone()).is_ok()
    );
    assert!(
        serde_json::from_slice::<crate::magic_items::IdentificationChoice>(
            &serde_json::to_vec(&choice).unwrap()
        )
        .is_ok()
    );
    for raw in [
        r#"{"choice":0,"classes":["tool"]}"#,
        r#"{"choice":18446744073709551615,"classes":["tool"]}"#,
        r#"{"classes":["tool"]}"#,
        r#"{"choice":1}"#,
        r#"{"choice":1,"classes":["tool"],"extra":0}"#,
        r#"{"choice":1,"choice":2,"classes":["tool"]}"#,
        r#"{"choice":1,"classes":["tool"],"classes":["potion"]}"#,
        r#"{"choice":1,"class":"tool"}"#,
        r#"[1,["tool"]]"#,
    ] {
        assert!(
            serde_json::from_str::<crate::magic_items::IdentificationChoice>(raw).is_err(),
            "{raw}"
        );
    }
    let named = serde_json::json!({"_facts":[{"item":10,"charges":3}]});
    assert!(serde_json::from_value::<Facts>(named.clone()).is_ok());
    assert!(serde_json::from_slice::<Facts>(&serde_json::to_vec(&named).unwrap()).is_ok());
    let full = serde_json::json!({"_facts":(1..=4096)
        .map(|id| serde_json::json!({"item":id,"charges":3}))
        .collect::<Vec<_>>()});
    assert!(serde_json::from_value::<Facts>(full.clone()).is_ok());
    assert!(serde_json::from_slice::<Facts>(&serde_json::to_vec(&full).unwrap()).is_ok());
    for raw in [
        r#"{"_facts":[{"item":10,"item":11,"charges":3}]}"#,
        r#"{"_facts":[{"item":10,"charges":3,"charges":4}]}"#,
    ] {
        assert!(serde_json::from_str::<Facts>(raw).is_err(), "{raw}");
    }
    for facts in [
        serde_json::json!([[10, 3]]),
        serde_json::json!([{"item":10,"charges":3},{"item":10,"charges":2}]),
        serde_json::json!([{"item":10}]),
        serde_json::json!([{"charges":3}]),
        serde_json::json!([{"item":0,"charges":3}]),
        serde_json::json!([{"item":u64::MAX,"charges":3}]),
        serde_json::json!([{"item":11,"charges":3},{"item":10,"charges":3}]),
        serde_json::json!([{"item":10,"charges":3,"identity":"hidden"}]),
        serde_json::Value::Array(
            (1..=4097)
                .map(|id| serde_json::json!({"item":id,"charges":3}))
                .collect(),
        ),
    ] {
        let malformed = serde_json::json!({"_facts":facts});
        assert!(serde_json::from_slice::<Facts>(&serde_json::to_vec(&malformed).unwrap()).is_err());
        assert!(serde_json::from_value::<Facts>(malformed).is_err());
    }
}

#[test]
fn device_creation_samples_owned_rng_once_and_failed_placement_never_spends_it() {
    let mut game = fixture();
    let prototype = device(&mut game, 10, 3);
    let mut spec = (*game.items[&prototype].spec).clone();
    spec.identity = "fixture.sampled_device".into();
    let range = ChargeRange {
        minimum: 3,
        maximum: 7,
    };
    spec.device.as_mut().unwrap().initial_charges = range;
    let mut twin = game.clone();
    let mut expected_rng = game.rules_rng;
    let expected = range.sample(&mut expected_rng).unwrap();
    for current in [&mut game, &mut twin] {
        let item = current
            .place_item_stack(20, at(1), Some(ActorId(1)), 1, spec.clone())
            .unwrap();
        assert_eq!(current.items[&item].charges, Some(expected));
        assert_eq!(current.items[&item].quantity, 1);
        assert_eq!(current.rules_rng, expected_rng);
    }
    assert_eq!(game, twin);
    assert_eq!(
        game,
        roundtrip(&game),
        "restore must not draw initial charges again"
    );
    let before = game.clone();
    let invalid = Location {
        region: RegionId(999),
        ..at(1)
    };
    assert!(game
        .place_item_stack(21, invalid, None, 1, spec.clone())
        .is_err());
    assert_eq!(
        game, before,
        "failed placement must not consume RNG or identity"
    );
    assert_eq!(
        game.place_item_stack(21, at(1), Some(ActorId(1)), 2, spec.clone()),
        Err(GameError::InvalidQuantity)
    );
    assert_eq!(game, before);
    spec.identity = "fixture.empty_device".into();
    spec.device.as_mut().unwrap().initial_charges = ChargeRange {
        minimum: 0,
        maximum: 0,
    };
    let item = game
        .place_item_stack(21, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    assert_eq!(game.items[&item].charges, Some(0));
    assert_eq!(
        game.rules_rng, before.rules_rng,
        "explicit fixed zero is not rerolled"
    );
    assert_eq!(game, roundtrip(&game));
}
#[test]
fn read_self_sleep_while_blind_consumes_exactly_and_restores_without_protection() {
    use crate::content_ids::ConditionId;
    use crate::rules::{AmountExpr, Effect};

    // Both authored learning triggers must survive removal of the last unit.
    for discovery in [ItemDiscovery::SelfEffect, ItemDiscovery::OnUse] {
        let sleep = ConditionId::new("fixture_ink_rest").unwrap();
        let blindness = ConditionId::new("fixture_veiled_eyes").unwrap();
        let mut game = super::ray_tests::fixture_with(|document, _, _| {
            for definition in [
                serde_json::json!({
                    "id":"fixture_ink_rest","family":"fixture_ink_rest",
                    "lifecycle":"timed","name":"Ink rest","modifiers":[],
                    "stacking":{"kind":"extend"},"source_scoped":false,
                    "descriptors":[],"capacity":"reject",
                    "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                    "restrictions":["voluntary_actions"],"preparation":"pause"
                }),
                serde_json::json!({
                    "id":"fixture_veiled_eyes","family":"fixture_veiled_eyes",
                    "lifecycle":"timed","name":"Veiled eyes",
                    "modifiers":[{"scope":"always","op":{"kind":"perception",
                        "value":{"kind":"sight","enabled":false}}}],
                    "stacking":{"kind":"extend"},"source_scoped":false,
                    "descriptors":[],"capacity":"reject",
                    "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                    "restrictions":[],"preparation":"preserve"
                }),
            ] {
                document
                    .conditions
                    .push(serde_json::from_value(definition).unwrap());
            }
        });
        let mut spec = ItemSpec::ordinary("unread page".into());
        spec.class = ItemClass::Scroll;
        spec.identity = "fixture.ink_rest_page".into();
        spec.name = "ink rest page".into();
        spec.appearance = "unread page".into();
        spec.concealed = true;
        spec.stackable = true;
        spec.readable = Some(ReadableSpec {
            control: None,
            descriptors: vec![],
            effects: vec![Effect::ApplyConditionForTurns {
                condition: sleep.clone(),
                duration: AmountExpr::Fixed { amount: 1 },
            }],
            identify: None,
            discovery,
        });
        let scroll = game
            .place_item_stack(10, at(1), Some(ActorId(1)), 1, spec.clone())
            .unwrap();
        let spare = game
            .place_item_stack(11, at(1), Some(ActorId(1)), 2, spec)
            .unwrap();
        game.apply_condition(ActorId(1), &blindness, ActorId(1), 5_000)
            .unwrap();
        assert!(!game.actors[&ActorId(1)].perception().sight);
        let preparation = game
            .plan_work(ActorId(1), Work::Read { item: scroll })
            .unwrap()
            .preparation();
        // Stop at an ordinary ready peer after resolution, before sleep expires.
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = preparation;
        let origin = execute(&mut game, Action::Read { item: scroll });
        assert_eq!(game.next_actor(), Some(ActorId(2)));
        assert_eq!(game.tick(), preparation);
        assert!(!game.items.contains_key(&scroll));
        assert_eq!(game.items[&spare].quantity, 2);
        assert!(game.items[&spare].holds.is_empty());
        assert!(game.preparation(ActorId(1)).is_none());
        assert!(game.pending_intention(ActorId(1)).is_none());
        assert!(game.combat_events().iter().any(|event| matches!(
            event,
            crate::combat::CombatEvent::ItemCompleted {
                actor: ActorId(1), work: Work::Read { item }, intention: Some(id),
            } if *item == scroll && *id == origin
        )));
        assert!(game.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.ink_rest_page"));
        assert!(!game.actors[&ActorId(2)]
            .knowledge
            .contains("fixture.ink_rest_page"));
        assert!(game
            .creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .any(|entry| entry.condition == sleep
                && entry.phase == crate::conditions::ConditionPhase::Active
                && entry.source == ActorId(1)));
        assert!(game.condition_blocks_action(ActorId(1), &Action::Wait));
        let denied = game.clone();
        assert_eq!(
            game.admit_intention(
                ActorId(1),
                Action::Read { item: spare },
                IntentionOrigin::Human
            ),
            Err(GameError::ActorBusy)
        );
        assert_eq!(game, denied);
        let mut restored = roundtrip(&game);
        assert_eq!(game, restored);

        for current in [&mut game, &mut restored] {
            let peer_wait = |current: &mut Game| {
                assert_eq!(current.next_actor(), Some(ActorId(2)));
                let id = current
                    .admit_intention(ActorId(2), Action::Wait, IntentionOrigin::Human)
                    .unwrap();
                let done = current.execute_next_intention().unwrap();
                assert_eq!(done.intention.id, id);
                done.outcome.unwrap();
            };
            let wake_by = current.tick() + current.actors[&ActorId(1)].turn_ticks.get();
            while current.condition_blocks_action(ActorId(1), &Action::Wait) {
                assert!(
                    current.tick() < wake_by,
                    "denial outlived the authored turn"
                );
                peer_wait(current);
            }
            assert_eq!(current.next_actor(), Some(ActorId(1)));
            assert!(!current.condition_blocks_action(ActorId(1), &Action::Wait));
            assert!(
                current
                    .creature(ActorId(1))
                    .unwrap()
                    .conditions()
                    .instances()
                    .all(|entry| entry.condition != sleep),
                "expiry leaves no gaze-style protection"
            );
            assert!(!current.actors[&ActorId(1)].perception().sight);
            assert!(current.preparation(ActorId(1)).is_none());
            // The expired, generically named condition is immediately repeatable.
            let completes_by = current.tick()
                + current
                    .plan_work(ActorId(1), Work::Read { item: spare })
                    .unwrap()
                    .preparation();
            execute(current, Action::Read { item: spare });
            while current.preparation(ActorId(1)).is_some() {
                assert!(
                    current.tick() < completes_by,
                    "completed source retained its preparation"
                );
                peer_wait(current);
            }
            assert_eq!(current.items[&spare].quantity, 1);
            assert!(current.items[&spare].holds.is_empty());
            assert!(current.preparation(ActorId(1)).is_none());
            assert!(current.condition_blocks_action(ActorId(1), &Action::Wait));
            assert!(current
                .creature(ActorId(1))
                .unwrap()
                .conditions()
                .instances()
                .any(|entry| entry.condition == sleep
                    && entry.phase == crate::conditions::ConditionPhase::Active));
            assert_eq!(*current, roundtrip(current));
        }
        assert_eq!(game, restored);
    }
}

fn equipment_fact_fixture() -> Game {
    super::ray_tests::fixture_with(|_, _, species| {
        species.anatomy = crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("body_armor").unwrap()],
        };
    })
}
fn fact_armor(game: &mut Game, id: u64, cursed: bool) -> ItemId {
    let mut spec = ItemSpec::ordinary("test mail".into());
    spec.class = ItemClass::Armor;
    spec.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        armor: Some(crate::ArmorSpec {
            physical_reduction: 3,
            material: crate::ArmorMaterial::Metal,
            rustproof: false,
        }),
        attack: None,
        modifiers: vec![],
    });
    let item = game
        .place_item_stack(id, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    game.items
        .edit(item, |item| item.condition.cursed = cursed)
        .unwrap();
    item
}

#[test]
fn paid_equipment_identification_is_exact_and_hidden_changes_leave_stale_facts_visible() {
    let mut game = equipment_fact_fixture();
    let first = fact_armor(&mut game, 10, true);
    let second = fact_armor(&mut game, 11, false);
    game.items
        .edit(first, |item| item.condition.armor_enchantment = 2)
        .unwrap();
    assert!(game.known_equipment_view(ActorId(1), first).is_none());
    let scroll = identify_scroll(&mut game, 12, ItemClass::Armor, 1);
    let choice = execute(&mut game, Action::Read { item: scroll });
    execute(
        &mut game,
        Action::Identify {
            choice,
            item: Some(first),
        },
    );
    let remembered = game.known_equipment_view(ActorId(1), first).unwrap();
    assert!(game.known_equipment_view(ActorId(1), second).is_none());
    assert!(game.known_equipment_view(ActorId(2), first).is_none());
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.curse(first),
        Some(true)
    );
    game.items
        .edit(first, |item| {
            item.condition.armor_enchantment = 5;
            item.condition.cursed = false;
        })
        .unwrap();
    assert_eq!(
        game.known_equipment_view(ActorId(1), first),
        Some(remembered)
    );
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.curse(first),
        Some(true)
    );
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn cursed_removal_learns_only_at_paid_completion_and_pause_restore_preserves_ignorance() {
    let mut game = equipment_fact_fixture();
    let item = fact_armor(&mut game, 10, true);
    execute(
        &mut game,
        Action::Equip {
            item,
            slot: crate::EquipmentSlotId(0),
        },
    );
    assert!(game.known_equipment_view(ActorId(1), item).is_some());
    assert_eq!(game.actors[&ActorId(1)].known_equipment.curse(item), None);
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.protection(item),
        None
    );
    let before = game.tick;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = before;
    let mut ordinary = game.clone();
    ordinary
        .items
        .edit(item, |item| item.condition.cursed = false)
        .unwrap();
    execute(&mut ordinary, Action::Unequip { item });
    let intention = execute(&mut game, Action::Unequip { item });
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().remaining,
        ordinary.preparation(ActorId(1)).unwrap().remaining,
        "Hidden curse must not affect preparation timing"
    );
    assert_eq!(game.tick, before);
    assert_eq!(game.actors[&ActorId(1)].known_equipment.curse(item), None);
    assert_eq!(
        game.pause_preparation(ActorId(1)),
        Some(Work::Unequip { item })
    );
    let mut restored = roundtrip(&game);
    for current in [&mut game, &mut restored] {
        assert_eq!(
            current.actors[&ActorId(1)].known_equipment.curse(item),
            None
        );
        let preparation = current.preparation(ActorId(1)).unwrap().clone();
        let remaining = preparation.remaining;
        assert_eq!(
            remaining, 300,
            "Armor retains authored three-turn removal timing even when cursed"
        );
        current
            .start_work(preparation.plan, preparation.intention)
            .unwrap();
        current
            .with_recovery_transition(true, |game| {
                game.tick += remaining;
                game.resolve_preparations()
            })
            .unwrap();
        assert!(current.preparation(ActorId(1)).is_none());
        assert_eq!(
            current
                .equipment(ActorId(1))
                .unwrap()
                .get(&crate::EquipmentSlotId(0)),
            Some(&item)
        );
        assert_eq!(
            current.actors[&ActorId(1)].known_equipment.curse(item),
            Some(true)
        );
        assert_eq!(
            current.actors[&ActorId(1)].known_equipment.protection(item),
            None
        );
        assert!(current.combat_events().iter().any(|event| matches!(event,
            crate::combat::CombatEvent::ItemCurseBlocked { actor: ActorId(1), item: blocked, intention: Some(issued) }
                if *blocked == item && *issued == intention)));
        assert!(!current.combat_events().iter().any(|event| matches!(
            event,
            crate::combat::CombatEvent::ItemCompleted {
                work: Work::Unequip { .. },
                ..
            }
        )));
    }
    assert_eq!(game, restored);
}

#[test]
fn equipment_fact_ownership_does_not_follow_transfer_and_hidden_return_does_not_refresh() {
    let mut game = equipment_fact_fixture();
    let item = fact_armor(&mut game, 10, true);
    game.identify_item(ActorId(1), item).unwrap();
    let remembered = game.known_equipment_view(ActorId(1), item).unwrap();
    let transfer = game
        .prepare_owned_transfer(item, ActorId(1), ActorId(2), 1)
        .unwrap();
    game.apply_owned_transfer(transfer).unwrap();
    assert!(game.known_equipment_view(ActorId(2), item).is_none());
    assert_eq!(game.actors[&ActorId(2)].known_equipment.curse(item), None);
    game.items
        .edit(item, |item| {
            item.condition.cursed = false;
            item.condition.armor_enchantment = 3;
        })
        .unwrap();
    let transfer = game
        .prepare_owned_transfer(item, ActorId(2), ActorId(1), 1)
        .unwrap();
    game.apply_owned_transfer(transfer).unwrap();
    assert_eq!(
        game.known_equipment_view(ActorId(1), item),
        Some(remembered)
    );
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.curse(item),
        Some(true)
    );
    assert_eq!(game, roundtrip(&game));
}
#[test]
fn ray_and_device_capture_attack_bonus_once_per_pool_not_fixed_damage() {
    use crate::rules::*;
    let mut game = super::ray_tests::fixture_with(|_, ray, species| {
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 2,
            },
        });
        let StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        let Effect::Damage {
            damage: DamageExpr::Components { components },
        } = &mut strikes[0].on_failure[0]
        else {
            unreachable!()
        };
        components.push(DamageComponentExpr {
            category: crate::combat::DamageType::Vital,
            descriptor: None,
            amount: AmountExpr::Fixed { amount: 3 },
        });
    });
    let ray = game
        .ability_plan_with_supplies(
            ActorId(1),
            &crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
            AbilityInvocation::directional_ray(StepDirection::East),
            Default::default(),
        )
        .unwrap();
    let item = device(&mut game, 10, 3);
    let device_plan = game
        .plan_work(
            ActorId(1),
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    for plan in [&ray, &device_plan] {
        let [PlannedEffect::Damage { damage }] = plan.as_ray().unwrap().effects.as_slice() else {
            panic!("damage")
        };
        let mut rolled = 0;
        let mut fixed = 0;
        for component in damage.components() {
            match component.amount() {
                crate::damage::DamageAmount::Rolled(pool) => {
                    assert_eq!(pool.count(), 6);
                    assert_eq!(pool.bonus(), 2);
                    rolled += 1;
                }
                crate::damage::DamageAmount::Fixed(amount) => {
                    assert_eq!(amount, 3);
                    fixed += 1;
                }
            }
        }
        assert_eq!((rolled, fixed), (1, 1));
    }
}

#[test]
fn empty_self_ability_is_valid_but_device_delivery_requires_self_support() {
    let game = super::ray_tests::fixture_with(|_, ability, species| {
        ability.invocation = InvocationKind::ActorTarget;
        ability.reach = crate::rules::Reach::SelfOnly;
        ability.interaction = crate::rules::Interaction::NonHostile;
        ability.supply_costs.clear();
        let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        assert_eq!(strikes.len(), 1);
        strikes[0].check = None;
        strikes[0].on_success.clear();
        strikes[0].on_failure.clear();
        strikes[0].riders.clear();
        species.modifiers.retain(|modifier| {
            !matches!(
                &modifier.op, crate::rules::ModifierOp::GrantAbility { ability: granted }
                    if granted == &ability.id
            )
        });
    });
    // The fixture compiles ordinary rules and real creature builds before this
    // device-source assertion. Nothing uses no fake check, damage or ray.
    let ability = game
        .content
        .rules
        .ability(&crate::content_ids::AbilityId::new("fixture_ray").unwrap())
        .unwrap();
    assert_eq!(ability.reach, crate::rules::Reach::SelfOnly);
    assert_eq!(ability.interaction, crate::rules::Interaction::NonHostile);
    let crate::rules::StrikeSource::Authored { strikes } = &ability.strikes else {
        unreachable!()
    };
    assert!(strikes[0].check.is_none());
    assert!(strikes[0].on_success.is_empty() && strikes[0].on_failure.is_empty());
    let source = DeviceSpec {
        ability: ability.id.clone(),
        initial_charges: ChargeRange {
            minimum: 3,
            maximum: 3,
        },
        discovery: ItemDiscovery::Never,
    };
    assert!(source.valid());
    assert!(
        source.validate_rules(&game.content.rules).is_ok(),
        "valid empty self ability must be accepted as an item-owned device source"
    );
}

#[test]
fn device_aim_is_closed_named_and_unknown_self_request_is_free() {
    for aim in [
        crate::DeviceAim::Direction {
            direction: StepDirection::North,
        },
        crate::DeviceAim::SelfTarget {},
    ] {
        let encoded = serde_json::to_value(aim).unwrap();
        assert_eq!(
            serde_json::from_value::<crate::DeviceAim>(encoded).unwrap(),
            aim
        );
    }
    for malformed in [
        serde_json::json!(["self_target"]),
        serde_json::json!({"kind":"self_target", "direction":"north"}),
        serde_json::json!({"kind":"direction"}),
        serde_json::json!({"kind":"direction", "direction":"north", "target":2}),
    ] {
        assert!(serde_json::from_value::<crate::DeviceAim>(malformed).is_err());
    }
    let mut game = fixture();
    let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
    assert!(
        game.plan_work(
            ActorId(1),
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North
                },
            }
        )
        .is_ok(),
        "existing directional source is a valid positive control"
    );
    let before = game.clone();
    assert!(game
        .admit_intention(
            ActorId(1),
            Action::Zap {
                item,
                aim: crate::DeviceAim::SelfTarget {},
            },
            IntentionOrigin::Human
        )
        .is_err());
    assert_eq!(
        game, before,
        "unknown self request must not reserve, pay, learn or consume RNG"
    );
}

fn self_device_fixture() -> Game {
    super::ray_tests::fixture_with(|_, ability, species| {
        ability.invocation = InvocationKind::ActorTarget;
        ability.reach = crate::rules::Reach::SelfOnly;
        ability.interaction = crate::rules::Interaction::NonHostile;
        ability.supply_costs.clear();
        let crate::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        strikes[0].check = None;
        strikes[0].on_success.clear();
        strikes[0].on_failure.clear();
        strikes[0].riders.clear();
        species.modifiers.retain(|m| !matches!(&m.op, crate::rules::ModifierOp::GrantAbility { ability: id } if id == &ability.id));
    })
}

#[test]
fn self_device_capture_is_named_bounded_and_distinct_from_requested_direction() {
    let mut game = self_device_fixture();
    let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
    let work = Work::Zap {
        item,
        aim: crate::DeviceAim::Direction {
            direction: StepDirection::West,
        },
    };
    let plan = game.plan_work(ActorId(1), work.clone()).unwrap();
    assert!(plan.valid(&game.content.rules));
    assert_eq!(plan.work(), work);
    assert_eq!(plan.actor(), ActorId(1));
    assert_eq!(plan.target(), ActorId(1));
    assert!(plan.as_ray().is_none() && plan.as_ability().is_none());
    assert_eq!(plan.tokens().len(), 0);
    assert_eq!(plan.cost(), None);
    assert_eq!(plan.recovery(), 0);
    assert!(game.item_allocation(&plan).is_ok());
    let Plan::Zap {
        delivery: DeviceDeliveryPlan::SelfEffect { plan: payload },
        ..
    } = &plan
    else {
        panic!("self payload, not a ray")
    };
    assert_eq!(payload.ability.as_str(), "fixture_ray");
    assert!(payload.effects.is_empty());
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        serde_json::from_value::<Plan>(encoded.clone()).unwrap(),
        plan
    );
    for malformed in [
        serde_json::json!([
            payload.actor,
            payload.ability,
            payload.preparation,
            payload.descriptors,
            payload.effects
        ]),
        {
            let mut value = serde_json::to_value(payload).unwrap();
            value["direction"] = serde_json::json!("west");
            value
        },
    ] {
        assert!(serde_json::from_value::<SelfDevicePlan>(malformed).is_err());
    }
    let mut wrong_effects = encoded.clone();
    wrong_effects["delivery"]["plan"]["effects"] = serde_json::json!([{"kind":"restore_hit_die"}]);
    let wrong_effects: Plan = serde_json::from_value(wrong_effects).unwrap();
    assert!(!wrong_effects.valid(&game.content.rules));
    let mut wrong_delivery = encoded.clone();
    wrong_delivery["delivery"]["kind"] = serde_json::json!("ray");
    assert!(serde_json::from_value::<Plan>(wrong_delivery).is_err());
    let mut forged = encoded.clone();
    forged["delivery"]["plan"]["ability"] = serde_json::json!("unknown_ability");
    let forged: Plan = serde_json::from_value(forged).unwrap();
    assert!(!forged.valid(&game.content.rules));
    assert!(game.item_allocation(&forged).is_err());
    let mut ray_game = fixture();
    let ray_item = device_with_discovery(&mut ray_game, 10, 3, ItemDiscovery::Never);
    let ray = ray_game
        .plan_work(
            ActorId(1),
            Work::Zap {
                item: ray_item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::West,
                },
            },
        )
        .unwrap();
    assert!(ray.valid(&ray_game.content.rules));
    let mut wrong_aim = ray.clone();
    let Plan::Zap { aim, .. } = &mut wrong_aim else {
        unreachable!()
    };
    *aim = crate::DeviceAim::SelfTarget {};
    assert!(!wrong_aim.valid(&ray_game.content.rules));
    assert!(game
        .admit_intention(
            ActorId(1),
            Action::Zap {
                item,
                aim: crate::DeviceAim::SelfTarget {}
            },
            IntentionOrigin::Human
        )
        .is_err());
}

#[test]
fn admitted_nothing_device_completes_paid_without_rng_or_learning() {
    let mut game = self_device_fixture();
    let item = device_with_discovery(&mut game, 10, 3, ItemDiscovery::Never);
    let before_tick = game.tick;
    let before_rng = game.rules_rng;
    let issued = game
        .admit_intention(
            ActorId(1),
            Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::West,
                },
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, issued);
    done.outcome
        .expect("legal admitted self device must execute its captured item payload");
    assert_eq!(game.tick, before_tick + 100);
    assert_eq!(game.items[&item].charges, Some(2));
    assert_eq!(game.rules_rng, before_rng);
    assert!(!game.actors[&ActorId(1)]
        .knowledge
        .contains("fixture.elemental_device"));
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn admitted_empty_self_device_has_no_payload_and_does_not_underflow_charge() {
    let mut game = self_device_fixture();
    let item = device_with_discovery(&mut game, 10, 0, ItemDiscovery::Never);
    let before_rng = game.rules_rng;
    let before_tick = game.tick;
    execute(
        &mut game,
        Action::Zap {
            item,
            aim: crate::DeviceAim::Direction {
                direction: StepDirection::West,
            },
        },
    );
    assert_eq!(game.items[&item].charges, Some(0));
    assert_eq!(game.tick, before_tick + 100);
    assert_eq!(game.rules_rng, before_rng);
    assert!(!game.actors[&ActorId(1)]
        .knowledge
        .contains("fixture.elemental_device"));
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn self_device_source_shape_rejects_actor_ability_only_operands() {
    let game = self_device_fixture();
    let id = crate::content_ids::AbilityId::new("fixture_ray").unwrap();
    let valid = game.content.rules.ability(&id).unwrap().clone();
    assert!(super::self_device_definition_valid(&valid));
    let ray = fixture();
    let crate::rules::StrikeSource::Authored {
        strikes: ray_strikes,
    } = &ray.content.rules.ability(&id).unwrap().strikes
    else {
        unreachable!()
    };
    for mutation in 0..7 {
        let mut bad = valid.clone();
        match mutation {
            0 => bad.interaction = crate::rules::Interaction::HostileToTarget,
            1 => bad.basic_attack = true,
            2 => bad.cooldown = std::num::NonZeroU64::new(1),
            3 => bad.reach = crate::rules::Reach::Sight { range: 1 },
            _ => {
                let crate::rules::StrikeSource::Authored { strikes } = &mut bad.strikes else {
                    unreachable!()
                };
                match mutation {
                    4 => strikes[0].check = ray_strikes[0].check.clone(),
                    5 => strikes[0].offset = 1,
                    6 => strikes.push(strikes[0].clone()),
                    _ => unreachable!(),
                }
            }
        }
        assert!(
            !super::self_device_definition_valid(&bad),
            "mutation {mutation}"
        );
    }
}
