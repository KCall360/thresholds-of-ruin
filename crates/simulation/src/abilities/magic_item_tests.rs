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
        discovery: ItemDiscovery::OnUse,
    });
    let item = game
        .place_item_stack(id, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    game.items
        .edit(item, |i| i.charges = Some(charges))
        .unwrap();
    item
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
        effects: vec![],
        identify: Some(class),
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
fn roundtrip(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let snapshot = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
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
                direction: StepDirection::North,
            },
        )
        .unwrap();
    let ray = plan.as_ray().unwrap();
    assert_eq!(ray.source_skill, crate::attributes::Skill::Lore);
    assert_eq!(
        ray.source_bonus,
        super::ray::named_bonus(before.derived(), crate::attributes::Skill::Lore)
    );
    assert!(plan.cost().is_none());
    assert!(plan.ability_id().is_none());
    execute(
        &mut game,
        Action::Zap {
            item,
            direction: StepDirection::North,
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
            direction: StepDirection::North,
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
    assert_eq!(choice.class, ItemClass::Tool);
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
            direction: StepDirection::North,
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
            direction: StepDirection::North
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
                direction: StepDirection::North
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
            direction: StepDirection::East,
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
            r#"{"choice":0,"class":"wand"}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<crate::magic_items::IdentificationChoice>(r#"[1,"wand"]"#).is_err()
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
            direction: StepDirection::North,
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
        direction: StepDirection::South,
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
            direction: StepDirection::East
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
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Perception {
                value: crate::perception::PerceptionValue::ObjectGuise {
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
            direction: StepDirection::North,
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
                direction: StepDirection::East,
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
            direction: StepDirection::East,
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
            direction: StepDirection::West,
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
            direction: StepDirection::East,
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
    let choice = serde_json::json!({"choice":1,"class":"tool"});
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
        r#"{"choice":0,"class":"tool"}"#,
        r#"{"choice":18446744073709551615,"class":"tool"}"#,
        r#"{"class":"tool"}"#,
        r#"{"choice":1}"#,
        r#"{"choice":1,"class":"tool","extra":0}"#,
        r#"{"choice":1,"choice":2,"class":"tool"}"#,
        r#"{"choice":1,"class":"tool","class":"potion"}"#,
        r#"[1,"tool"]"#,
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
