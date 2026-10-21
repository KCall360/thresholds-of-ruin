//! Checked theft executes inside the ordinary paid action transaction.
use super::{Plan, PlannedEffect};
use crate::floor_activation::{PopulationPlan, PopulationProfile, PreparedScope};
use crate::quantity::QuantityExpr;
use crate::resources::Resource;
use crate::rules::*;
use crate::theft::{PlannedTheft, RelocationGate, RelocationPolicy, TheftSelection};
use crate::{
    Action, ActorId, CurrencyKind, Game, GameError, IntentionId, ItemId, ItemLocation, ItemSpec,
    MemoryRecords, RegionState, RegionTransition,
};
use tor_world::{Location, Position, RegionId};

fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}

fn gold(name: &str) -> ItemSpec {
    let mut spec = ItemSpec::ordinary(name.into());
    spec.class = crate::ItemClass::Coin;
    spec.currency = Some(CurrencyKind::Gold);
    spec.stackable = true;
    spec
}

fn register_floor(game: &mut Game, depth: u32, members: &[u64], cells: Vec<Location>) {
    game.register_activation_scope(PreparedScope {
        scope: format!("dungeon/{depth}"),
        depth,
        profile: "empty".into(),
        members: members.iter().copied().map(RegionId).collect(),
        cells,
        protected: Default::default(),
        rooms: vec![],
    })
    .unwrap();
}

fn configure_population(game: &mut Game) {
    game.configure_scenario_policy(Default::default(), Some(ActorId(1)))
        .unwrap();
    game.configure_population(PopulationPlan {
        protagonist: ActorId(1),
        carried_item_pools: Default::default(),
        profiles: [(
            "empty".into(),
            PopulationProfile {
                version: 1,
                stages: vec![],
            },
        )]
        .into(),
    })
    .unwrap();
}

fn theft_game(
    offsets: &[u64],
    amount: QuantityExpr,
    relocate: bool,
    configure_game: impl FnOnce(&mut Game),
) -> (Game, IntentionId) {
    theft_game_in_regions(offsets, amount, relocate, 2, configure_game)
}

fn theft_game_in_regions(
    offsets: &[u64],
    amount: QuantityExpr,
    relocate: bool,
    regions: u64,
    configure_game: impl FnOnce(&mut Game),
) -> (Game, IntentionId) {
    theft_game_with_sources(
        offsets,
        amount,
        relocate,
        regions,
        |_, _| {},
        configure_game,
    )
}

fn theft_game_with_sources(
    offsets: &[u64],
    amount: QuantityExpr,
    relocate: bool,
    regions: u64,
    configure_sources: impl FnOnce(&mut RulesetDocument, &mut crate::creatures::Species),
    configure_game: impl FnOnce(&mut Game),
) -> (Game, IntentionId) {
    super::strike_tests::sequence_game_on_world_configured(
        offsets,
        |ability| {
            ability.cost = Some(Cost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 1,
            });
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            for strike in strikes {
                strike.on_success.clear();
                strike.riders = vec![RiderDef {
                    trigger: RiderTrigger::Success,
                    condition: None,
                    check: Some(RiderCheckDef {
                        skill: SkillSelector::SelectedWeapon,
                        defense: DefenseKind::Physical,
                    }),
                    effects: vec![Effect::Theft {
                        selection: TheftSelection::Currency {
                            currency: CurrencyKind::Gold,
                            amount,
                        },
                        relocation: relocate.then(|| RelocationGate {
                            flag: crate::content_ids::FlagId::new("test_vanish").unwrap(),
                            policy: RelocationPolicy::SameFloor,
                        }),
                    }],
                    descriptors: vec![],
                }];
            }
        },
        |rules, species| {
            // Riders target defenses rather than authored difficulty formulas.
            // This synthetic defense guarantees success but still rolls the check.
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::Defense {
                        defense: DefenseKind::Physical,
                    },
                    value: -100,
                },
            });
            let flag = crate::content_ids::FlagId::new("test_vanish").unwrap();
            rules.flags.push(FlagDef {
                id: flag.clone(),
                name: "Test Vanish".into(),
                modifiers: vec![],
            });
            if relocate {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Flag { flag },
                });
            }
            configure_sources(rules, species);
        },
        Some(regions),
        configure_game,
    )
}

fn frozen_theft(plan: &Plan, strike: usize) -> &PlannedTheft {
    let PlannedEffect::Theft { theft } =
        &plan.as_ability().unwrap().strikes[strike].riders[0].effects[0]
    else {
        panic!("the fixture must capture a theft operand")
    };
    theft
}

#[test]
fn paid_theft_relocation_preserves_destination_trap_rng_after_selection() {
    let (mut clear, _) = theft_game_with_sources(
        &[0],
        QuantityExpr::Fixed { quantity: 1 },
        true,
        1,
        |document, _| {
            document.descriptors.push(NamedDescriptor {
                resistance: Some(DescriptorResistance {
                    skill: crate::attributes::Skill::Athletics,
                    difficulty: 100,
                }),
                id: crate::content_ids::DescriptorId::new("poison").unwrap(),
                name: "Poison".into(),
            });
        },
        |game| {
            // Ordinary grounded contact requires a real supporting stone cell,
            // unlike this module's usual zero-height corridor fixture.
            let mut world = tor_world::World::new(vec![], vec![]).unwrap();
            world
                .add_chamber(tor_world::Region {
                    id: RegionId(1),
                    name: "supported theft floor".into(),
                    bounds: tor_world::Extent::new(5, 3, 2).unwrap(),
                })
                .unwrap();
            *game.world = world;
            configure_population(game);
            register_floor(game, 7, &[1], vec![at(1, 1), at(1, 2), at(1, 3), at(1, 4)]);
            game.activate_initial_floor().unwrap();
            game.place_item_stack(10, at(1, 2), Some(ActorId(2)), 2, gold("gold"))
                .unwrap();
        },
    );
    let mut trapped = clear.clone();
    for x in [3, 4] {
        trapped
            .place_trap(
                at(1, x),
                tor_world::Trap {
                    id: x as u64,
                    dc: 15,
                    avoidance_dc: None,
                    kind: tor_world::TrapKind::PoisonDart {
                        attack_bonus: 100,
                        exposure: "poison".into(),
                    },
                },
            )
            .unwrap();
    }
    // Isolate contact payload RNG: otherwise passive discovery of the other
    // untriggered plate legitimately adds separate perception checks.
    for actor in [ActorId(1), ActorId(2)] {
        for trap in [3, 4] {
            trapped
                .reveal_feature_family(actor, tor_world::ConcealedFeatureId::Trap(trap))
                .unwrap();
            assert!(trapped.feature_known(actor, tor_world::ConcealedFeatureId::Trap(trap)));
        }
    }
    for game in [&mut clear, &mut trapped] {
        game.with_recovery_transition(true, |candidate| {
            candidate.tick = 10;
            candidate.resolve_preparations()
        })
        .unwrap();
        assert_eq!(
            game.items[&ItemId(10)].quantity,
            1,
            "The paid transfer still completes exactly once"
        );
        assert!([at(1, 3), at(1, 4)].contains(
            &game
                .actors
                .get(&ActorId(1))
                .map(|actor| actor.location)
                .unwrap()
        ));
    }
    assert_eq!(
        clear.actors.get(&ActorId(1)).map(|actor| actor.location),
        trapped.actors.get(&ActorId(1)).map(|actor| actor.location)
    );
    let mut expected_rng = clear.rules_rng;
    crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::default());
    let damage = crate::dice::DicePool::new(1, 4, 0)
        .unwrap()
        .roll(&mut expected_rng, crate::dice::Edge::default())
        .total;
    crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::default());
    assert_eq!(
        trapped.health(ActorId(1)).unwrap().0,
        clear.health(ActorId(1)).unwrap().0 - u64::from(damage)
    );
    assert_ne!(clear.rules_rng, expected_rng);
    assert_eq!(
        trapped.rules_rng, expected_rng,
        "The theft caller must keep the destination damage and poison check draws"
    );
}

#[test]
fn late_fragment_overflow_rolls_back_the_public_paid_action_and_rng() {
    let (mut game, intention) =
        theft_game(&[0], QuantityExpr::Fixed { quantity: 4 }, false, |game| {
            game.place_item_stack(10, at(1, 2), Some(ActorId(2)), 2, gold("first gold"))
                .unwrap();
            game.place_item_stack(11, at(1, 2), Some(ActorId(2)), 2, gold("second gold"))
                .unwrap();
            game.place_item_stack(
                12,
                at(1, 1),
                Some(ActorId(1)),
                u64::MAX,
                gold("second gold"),
            )
            .unwrap();
        });
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().origin,
        Some(intention)
    );
    assert!(game.preparation(ActorId(1)).unwrap().hold.is_some());
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    let before = game.clone();
    let rng = game.rules_rng;

    // The due rider transfers the first fragment before the second merge fails.
    // Only the production outer action candidate may roll both fragments back.
    assert_eq!(
        game.act(ActorId(2), Action::Wait),
        Err(GameError::InvalidQuantity)
    );
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game, before);

    // A legal second destination reaches the same guaranteed check and proves
    // the failed request was not a free rejection before any RNG or transfer.
    let mut legal = before;
    legal
        .items
        .edit(ItemId(12), |item| item.quantity = 1)
        .unwrap();
    legal.act(ActorId(2), Action::Wait).unwrap();
    assert_ne!(legal.rules_rng, rng);
    assert_eq!(
        legal.items[&ItemId(10)].location,
        ItemLocation::Carried(ActorId(1))
    );
    assert_eq!(legal.items[&ItemId(12)].quantity, 3);
    assert!(!legal.items.contains_key(&ItemId(11)));
    assert!(legal.preparation(ActorId(1)).is_none());
}

#[test]
fn equal_offset_theft_freezes_the_paid_tail_until_destination_thaws() {
    let (mut game, intention) =
        theft_game(&[0, 0], QuantityExpr::Fixed { quantity: 1 }, true, |game| {
            configure_population(game);
            register_floor(game, 7, &[1, 2], vec![at(1, 1), at(1, 2), at(2, 1)]);
            game.activate_initial_floor().unwrap();
            game.place_item_stack(10, at(1, 2), Some(ActorId(2)), 2, gold("gold"))
                .unwrap();
            game.transition_regions(
                &RegionTransition {
                    active: [RegionId(1)].into(),
                    loaded: [RegionId(1), RegionId(2)].into(),
                },
                &mut MemoryRecords::default(),
            )
            .unwrap();
        });
    assert_eq!(game.region_state(RegionId(2)), Some(RegionState::Frozen));
    let original = game.preparation(ActorId(1)).unwrap().plan.clone();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert_eq!(game.actors[&ActorId(1)].location, at(2, 1));
    assert!(game.actor_frozen(ActorId(1)));
    let pending = game
        .preparation(ActorId(1))
        .expect("frozen equal-offset tail survives");
    assert_eq!(pending.plan, original);
    assert_eq!(pending.next_strike(), Some(1));
    assert_eq!(pending.origin, Some(intention));
    assert_eq!(pending.intention, Some(intention));
    assert_eq!(pending.remaining, 0);
    assert!(pending.hold.is_none());
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    assert_eq!(game.items[&ItemId(10)].quantity, 1);
    let resources = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .clone();
    let rng = game.rules_rng;

    game.tick = 1010;
    game.resolve_preparations().unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().next_strike(), Some(1));
    assert_eq!(game.rules_rng, rng);
    assert_eq!(
        *game.creature(ActorId(1)).unwrap().costs().resources(),
        resources
    );
    game.transition_regions(
        &RegionTransition {
            active: [RegionId(1), RegionId(2)].into(),
            loaded: [RegionId(1), RegionId(2)].into(),
        },
        &mut MemoryRecords::default(),
    )
    .unwrap();
    let pending = game.preparation(ActorId(1)).unwrap();
    assert_eq!(pending.started, 1010);
    assert_eq!(pending.next_strike(), Some(1));
    assert!(pending.hold.is_none());
    game.resolve_preparations().unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.actors[&ActorId(1)].ready_at, 1030);
    assert_eq!(
        *game.creature(ActorId(1)).unwrap().costs().resources(),
        resources
    );
    assert_eq!(game.items[&ItemId(10)].quantity, 1);
    assert_eq!(
        game.rules_rng, rng,
        "out-of-reach tail does not reroll theft"
    );
    assert_eq!(
        game.activation_count(),
        1,
        "same-floor relocation does not activate again"
    );
}

#[test]
fn contextual_theft_requires_and_freezes_current_registered_depth() {
    let (game, _) = theft_game(
        &[0],
        QuantityExpr::DepthScaledUniform {
            minimum: 2,
            maximum_base: 51,
            per_depth: 10,
        },
        false,
        |game| {
            let ability = game
                .content()
                .rules()
                .document()
                .abilities
                .iter()
                .find(|ability| ability.basic_attack)
                .unwrap()
                .id
                .clone();
            let before = game.clone();
            assert!(game.ability_plan(ActorId(1), &ability, ActorId(2)).is_err());
            assert_eq!(*game, before, "missing depth is a read-only plan rejection");
            configure_population(game);
            register_floor(game, 1, &[1], vec![at(1, 1), at(1, 2)]);
            register_floor(game, 7, &[2], vec![at(2, 1), at(2, 2)]);
            game.activate_initial_floor().unwrap();
            // Both actors were born in region 1; current registered entry wins.
            game.teleport(ActorId(1), at(2, 1)).unwrap();
            game.teleport(ActorId(2), at(2, 2)).unwrap();
        },
    );
    let pending = game.preparation(ActorId(1)).unwrap();
    let theft = frozen_theft(&pending.plan, 0);
    assert_eq!(theft.depth, Some(7));
    assert_eq!(theft.quantity_scope.as_deref(), Some("dungeon/7"));
    assert_eq!(
        theft.selection,
        crate::theft::PlannedTheftSelection::Currency {
            currency: CurrencyKind::Gold,
            amount: crate::quantity::QuantityRange::new(2, 121).unwrap(),
        }
    );
    assert!(game.plan_source_valid(&pending.plan));
}

#[test]
fn frozen_theft_rejects_mixed_registered_scopes_across_riders() {
    let (game, _) = theft_game_in_regions(
        &[0, 0],
        QuantityExpr::Fixed { quantity: 1 },
        true,
        3,
        |game| {
            configure_population(game);
            register_floor(game, 1, &[1, 2], vec![at(1, 1), at(1, 2), at(2, 1)]);
            register_floor(game, 2, &[3], vec![at(3, 1)]);
            game.activate_initial_floor().unwrap();
        },
    );
    let original = &game.preparation(ActorId(1)).unwrap().plan;
    assert!(game.plan_source_valid(original));
    let mut forged = original.clone();
    let Plan::Ability(plan) = &mut forged else {
        unreachable!()
    };
    let PlannedEffect::Theft { theft } = &mut plan.strikes[1].riders[0].effects[0] else {
        unreachable!()
    };
    theft.relocation.as_mut().unwrap().scope = "dungeon/2".into();
    assert!(
        forged.valid(game.content().rules()),
        "each frozen operand is locally valid"
    );
    assert!(game.activation_scope("dungeon/2").is_some());
    assert!(
        !game.plan_source_valid(&forged),
        "one compilation cannot capture two floors"
    );
}

#[test]
fn frozen_theft_shape_requires_a_sole_checked_success_rider() {
    let (game, _) = theft_game(&[0], QuantityExpr::Fixed { quantity: 1 }, false, |_| {});
    let original = game
        .preparation(ActorId(1))
        .unwrap()
        .plan
        .as_ability()
        .unwrap();
    assert!(original.valid(game.content().rules()));
    for malformed in 0..4 {
        let mut plan = original.clone();
        let strike = &mut plan.strikes[0];
        match malformed {
            0 => strike.on_success.push(strike.riders[0].effects.remove(0)),
            1 => strike.riders[0].check = None,
            2 => strike.riders[0].trigger = RiderTrigger::Failure,
            3 => strike.riders[0]
                .effects
                .push(PlannedEffect::ObservePublicPlan),
            _ => unreachable!(),
        }
        assert!(
            !plan.valid(game.content().rules()),
            "accepted malformed theft shape {malformed}"
        );
    }
}

#[test]
fn supplemental_loaded_thief_never_recursively_roots_its_floor() {
    let (mut game, _) =
        theft_game_in_regions(&[0], QuantityExpr::Fixed { quantity: 1 }, true, 4, |game| {
            configure_population(game);
            register_floor(game, 1, &[1, 2], vec![at(1, 1), at(1, 2), at(2, 1)]);
            game.activate_initial_floor().unwrap();
        });
    let plan = game.preparation(ActorId(1)).unwrap().plan.clone();
    // Active corridor regions load their immediate neighbor independently of
    // theft. Region 4 therefore keeps region 3 loaded, but cannot itself pin
    // region 2, which belongs to the frozen thief's captured floor.
    let (applied, _) = game
        .transition_regions(
            &RegionTransition {
                active: [RegionId(4)].into(),
                loaded: [RegionId(1), RegionId(4)].into(),
            },
            &mut MemoryRecords::default(),
        )
        .unwrap();
    assert_eq!(applied.active, [RegionId(4)].into());
    assert_eq!(
        applied.loaded,
        [RegionId(1), RegionId(3), RegionId(4)].into()
    );
    assert_eq!(game.region_state(RegionId(1)), Some(RegionState::Frozen));
    assert_eq!(game.region_state(RegionId(2)), Some(RegionState::Detached));
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, plan);
    let mut supplemental = applied.loaded.clone();
    game.relocation_loaded_closure(&applied.active, &mut supplemental);
    assert_eq!(supplemental, applied.loaded);
    // Promote the occupied region to an ordinary active root: the exact same
    // retained plan now adds its otherwise absent floor member.
    game.relocation_loaded_closure(&[RegionId(1), RegionId(4)].into(), &mut supplemental);
    assert_eq!(
        supplemental,
        [RegionId(1), RegionId(2), RegionId(3), RegionId(4)].into()
    );
}

#[test]
fn tr3_flag_gain_or_loss_does_not_rewrite_prepared_optional_relocation() {
    for initially_enabled in [false, true] {
        let talent = crate::content_ids::TalentId::new("test_vanish_at_three").unwrap();
        let flag = crate::content_ids::FlagId::new("test_vanish").unwrap();
        let (mut game, _) = theft_game_with_sources(
            &[0],
            QuantityExpr::Fixed { quantity: 1 },
            true,
            2,
            |rules, species| {
                species.modifiers.retain(|modifier| {
                    !matches!(&modifier.op,
                    ModifierOp::Flag { flag: actual } if actual == &flag)
                });
                rules.talents.push(TalentDef {
                    id: talent.clone(),
                    name: "Test Vanish at three".into(),
                    prerequisites: vec![],
                    requirement: Requirement::TotalHd {
                        minimum: crate::HdCount(3),
                    },
                    modifiers: vec![Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Flag { flag: flag.clone() },
                    }],
                    form_choices: vec![],
                    expertise_grants: vec![],
                });
            },
            |game| {
                configure_population(game);
                register_floor(game, 1, &[1, 2], vec![at(1, 1), at(1, 2), at(2, 1)]);
                game.activate_initial_floor().unwrap();
                let mut build = game.creature(ActorId(1)).unwrap().build().clone();
                build
                    .add_hit_die(crate::progression::HdSource::Racial, 42)
                    .unwrap();
                if initially_enabled {
                    build
                        .add_hit_die(crate::progression::HdSource::Racial, 42)
                        .unwrap();
                    build
                        .select_talent(crate::HdIndex(2), talent.clone())
                        .unwrap();
                }
                game.rebuild_creature(ActorId(1), build).unwrap();
                game.place_item_stack(10, at(1, 2), Some(ActorId(2)), 2, gold("gold"))
                    .unwrap();
            },
        );
        let original = game.preparation(ActorId(1)).unwrap().plan.clone();
        assert_eq!(
            frozen_theft(&original, 0).relocation.is_some(),
            initially_enabled
        );
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        if initially_enabled {
            build.remove_latest().unwrap();
        } else {
            build
                .add_hit_die(crate::progression::HdSource::Racial, 42)
                .unwrap();
            build
                .select_talent(crate::HdIndex(2), talent.clone())
                .unwrap();
        }
        game.rebuild_creature(ActorId(1), build).unwrap();
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .derived()
                .flags
                .contains(&flag),
            !initially_enabled
        );
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, original);
        assert!(game.plan_source_valid(&original));
        game.tick = 10;
        game.resolve_preparations().unwrap();
        assert_eq!(game.items[&ItemId(10)].quantity, 1);
        assert_eq!(
            game.actors[&ActorId(1)].location,
            if initially_enabled {
                at(2, 1)
            } else {
                at(1, 1)
            }
        );
    }
}
