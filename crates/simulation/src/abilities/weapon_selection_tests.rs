//! Explicit selection owns one frozen check/damage/timing/cost context.
use super::*;
use crate::content_ids::{AbilityId, ExpertiseId};
use crate::resources::Resource;
use crate::rules::*;
use crate::weapon_profiles::{WeaponProfileSpec, WeaponTrait, WeaponTraitChanges};
use crate::{ActorId, EquipmentSlotId, Game, ItemId};
use serde_json::json;

fn expertise() -> ExpertiseId {
    ExpertiseId::new("fixture.weapon").unwrap()
}
fn basic() -> AbilityId {
    AbilityId::new("basic_melee").unwrap()
}
fn fixture(configure: impl FnOnce(&mut AbilityDef)) -> Game {
    fixture_sources(configure, |_, _| {})
}
fn fixture_sources(
    configure: impl FnOnce(&mut AbilityDef),
    sources: impl FnOnce(&mut RulesetDocument, &mut crate::creatures::Species),
) -> Game {
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.strikes = StrikeSource::SelectedMelee;
            ability.timing.source = TimingSource::SelectedWeapon;
            configure(ability);
        },
        |document, species| {
            document.expertises.push(crate::expertise::ExpertiseDef {
                id: expertise(),
                name: "Weapon".into(),
                category: crate::expertise::ExpertiseCategory::Equipment,
            });
            for definition in &mut document.resources {
                definition.capacity.required_flag = None;
            }
            for resource in Resource::ALL {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Set {
                        stat: Stat::ResourceMax { resource },
                        value: 20,
                    },
                });
            }
            sources(document, species);
        },
    );
    let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
    outcome.discard(game.tick).unwrap();
    game.with_effect_publication(|candidate| candidate.apply_outcome(outcome))
        .unwrap();
    game
}
fn start_plan(game: &mut Game, plan: &Plan) -> crate::IntentionId {
    if game.preparation(ActorId(1)).is_some() {
        game.pause_preparation(ActorId(1)).unwrap();
    }
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let owner = game
        .admit_intention(
            ActorId(1),
            plan.work().action(),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, owner);
    execution.outcome.unwrap();
    assert_eq!(&game.preparation(ActorId(1)).unwrap().plan, plan);
    owner
}
fn trained(game: &mut Game) {
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .add_hit_die(crate::progression::HdSource::Racial, 42)
        .unwrap();
    build
        .select_expertise(crate::HdIndex(1), expertise())
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
}
fn equip(game: &mut Game, socket: u16, expert_reduces_grip: bool) -> ItemId {
    let mut item = crate::ItemSpec::ordinary(format!("weapon {socket}"));
    item.class = crate::ItemClass::Weapon;
    let component = crate::damage::DamageComponent::fixed(
        crate::combat::DamageType::Keen,
        None,
        5 + u32::from(socket),
    );
    let primary = component.key();
    item.equipment = Some(crate::EquipmentSpec {
        slot: crate::EquipmentSlot::new("weapon").unwrap(),
        armor: None,
        attack: Some(
            crate::attacks::MeleeAttack::new(
                if socket == 0 {
                    crate::attributes::Skill::HeavyWeaponry
                } else {
                    crate::attributes::Skill::LightWeaponry
                },
                i32::from(socket),
                10 + u64::from(socket),
                20 + u64::from(socket),
                crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
            )
            .unwrap(),
        ),
        weapon_profile: Some(WeaponProfileSpec {
            expertise: expertise(),
            ranged: None,
            base_traits: if expert_reduces_grip {
                vec![WeaponTrait::TwoHanded]
            } else {
                vec![]
            },
            expert_traits: WeaponTraitChanges {
                add: vec![WeaponTrait::Offhand],
                remove: if expert_reduces_grip {
                    vec![crate::weapon_profiles::WeaponTraitKind::TwoHanded]
                } else {
                    vec![]
                },
            },
        }),
        modifiers: vec![],
    });
    let at = game.actors[&ActorId(1)].location;
    let item = game
        .spawn_item_stack(at, Some(ActorId(1)), 1, item)
        .unwrap();
    game.equip_authored(ActorId(1), item, EquipmentSlotId(socket))
        .unwrap();
    item
}
fn selected(game: &Game, item: ItemId, socket: u16) -> Result<Plan, crate::GameError> {
    game.ability_plan_with_supplies(
        ActorId(1),
        &basic(),
        AbilityInvocation {
            aim: AbilityAim::ActorTarget { target: ActorId(2) },
            weapon: WeaponSelection::Equipped {
                item,
                socket: EquipmentSlotId(socket),
            },
        },
        Default::default(),
    )
}
fn focus(plan: &Plan) -> u32 {
    plan.cost()
        .into_iter()
        .flat_map(|cost| cost.entries())
        .filter(|cost| cost.resource == Resource::Focus)
        .map(|cost| cost.resolution)
        .sum()
}
#[test]
fn selected_weapon_uses_one_check_damage_timing_context_and_primary_has_no_surcharge() {
    let mut game = fixture(|_| {});
    let primary = equip(&mut game, 0, false);
    let secondary = equip(&mut game, 10, false);
    let default = game.ability_plan(ActorId(1), &basic(), ActorId(2)).unwrap();
    let explicit_primary = selected(&game, primary, 0).unwrap();
    let explicit = selected(&game, secondary, 10).unwrap();
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .weapon_abilities
        .contains(&basic()));
    assert_eq!(focus(&default), 0);
    assert_eq!(focus(&explicit_primary), 0);
    assert_eq!(focus(&explicit), 2);
    let sheet = game.creature(ActorId(1)).unwrap();
    for (plan, socket) in [(&default, 0), (&explicit_primary, 0), (&explicit, 10)] {
        let ability = plan.as_ability().unwrap();
        let expected = &sheet.derived().equipped_attacks[&EquipmentSlotId(socket)].attack;
        assert_eq!(
            ability.strikes[0].check.as_ref().unwrap().bonus,
            expected.check_bonus
        );
        assert_eq!(ability.preparation, expected.attack.wind_up());
        assert_eq!(ability.recovery, expected.attack.recovery());
        assert_eq!(
            ability.strikes[0].on_success,
            vec![PlannedEffect::Damage {
                damage: expected.attack.damage_with_modifiers(0, 0, 0).unwrap()
            }]
        );
    }
    assert_ne!(
        default.as_ability().unwrap().strikes,
        explicit.as_ability().unwrap().strikes
    );
    assert_ne!(default.work(), explicit.work());
}
#[test]
fn empty_primary_stays_natural_and_expert_secondary_still_pays_focus() {
    let mut game = fixture(|_| {});
    trained(&mut game);
    let secondary = equip(&mut game, 10, false);
    let default = game.ability_plan(ActorId(1), &basic(), ActorId(2)).unwrap();
    assert_eq!(
        default.as_ability().unwrap().weapon,
        FrozenWeaponSelection::Natural
    );
    assert_eq!(focus(&default), 0);
    assert_eq!(focus(&selected(&game, secondary, 10).unwrap()), 1);
}
#[test]
fn offhand_surcharge_adds_to_focus_and_preserves_other_resource_costs() {
    for resource in [Resource::Focus, Resource::Mana] {
        let mut game = fixture(|ability| {
            ability.cost = Some(Cost {
                resource,
                start: 1,
                resolution: 2,
            });
        });
        let item = equip(&mut game, 10, false);
        let plan = selected(&game, item, 10).unwrap();
        let costs: Vec<_> = plan.cost().unwrap().entries().collect();
        if resource == Resource::Focus {
            assert_eq!(
                costs,
                vec![crate::costs::ResourceCost {
                    resource,
                    start: 1,
                    resolution: 4
                }]
            );
        } else {
            assert_eq!(costs.len(), 2);
            assert_eq!(
                costs[0],
                crate::costs::ResourceCost {
                    resource: Resource::Focus,
                    start: 0,
                    resolution: 2
                }
            );
            assert_eq!(
                costs[1],
                crate::costs::ResourceCost {
                    resource,
                    start: 1,
                    resolution: 2
                }
            );
        }
        assert!(plan.valid(game.content.rules()));
    }
}
#[test]
fn paid_expert_context_survives_drain_but_new_capture_observes_pool_deficit() {
    let mut game = fixture(|_| {});
    trained(&mut game);
    let _primary = equip(&mut game, 0, false);
    let secondary = equip(&mut game, 10, true);
    let captured = selected(&game, secondary, 10).unwrap();
    assert_eq!(focus(&captured), 1);
    start_plan(&mut game, &captured);
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build.remove_latest().unwrap();
    game.rebuild_creature(ActorId(1), build.clone()).unwrap();
    assert!(!game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .equipment_capacity_deficits
        .is_empty());
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
    assert!(game.preparation_still_available(game.preparation(ActorId(1)).unwrap(), None));
    assert!(selected(&game, secondary, 10).is_err());
    assert!(
        game.ability_plan(ActorId(1), &basic(), ActorId(2)).is_err(),
        "cheaper primary cannot bypass shared deficit"
    );
    build
        .add_hit_die(crate::progression::HdSource::Racial, 42)
        .unwrap();
    build
        .select_expertise(crate::HdIndex(1), expertise())
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert_eq!(focus(&selected(&game, secondary, 10).unwrap()), 1);
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
}
#[test]
fn missing_selected_item_is_structurally_valid_but_cancels_live_work() {
    let mut game = fixture(|_| {});
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    start_plan(&mut game, &plan);
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .equipment
        .remove(&EquipmentSlotId(10));
    game.items.remove(&item);
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .project_equipment_immediately_in_context(vec![], None)
        .unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut game = Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert!(plan.valid(game.content.rules()));
    assert!(game.plan_source_valid(&plan));
    assert!(!game.frozen_weapon_available(ActorId(1), &plan.as_ability().unwrap().weapon));
    assert!(!game.preparation_still_available(game.preparation(ActorId(1)).unwrap(), None));
    game.resolve_preparations().unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert!(game.start_work(plan, None).is_err());
}
#[test]
fn invocation_and_frozen_selection_require_complete_closed_named_operands() {
    let valid = json!({"aim":{"kind":"actor_target","target":2},"weapon":{"kind":"equipped","item":1,"socket":10}});
    let invocation: AbilityInvocation = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(serde_json::to_value(invocation).unwrap(), valid);
    for field in ["aim", "weapon"] {
        let mut bad = valid.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<AbilityInvocation>(bad).is_err());
    }
    for selector in [
        json!({"kind":"default","item":1}),
        json!({"kind":"equipped","item":0,"socket":10}),
        json!({"kind":"equipped","item":1,"socket":64}),
        json!({"kind":"equipped","item":1}),
        json!([1, 10]),
    ] {
        let mut bad = valid.clone();
        bad["weapon"] = selector;
        assert!(serde_json::from_value::<AbilityInvocation>(bad).is_err());
    }
    assert!(serde_json::from_str::<AbilityInvocation>(r#"{"aim":{"kind":"actor_target","target":2},"weapon":{"kind":"default"},"weapon":{"kind":"default"}}"#).is_err());
    let mut game = fixture(|_| {});
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    let mut forged = plan.as_ability().unwrap().clone();
    let FrozenWeaponSelection::Equipped { secondary, .. } = &mut forged.weapon else {
        unreachable!()
    };
    *secondary = false;
    forged.invocation.weapon = WeaponSelection::Default;
    forged.cost = None;
    assert!(
        forged.valid(game.content.rules()),
        "local shape is legal; catalog pins classification"
    );
    assert!(!game.plan_source_valid(&Plan::Ability(tor_world::Shared::new(forged))));
}

fn selected_damage() -> Effect {
    Effect::Damage {
        damage: DamageExpr::SelectedWeapon { impact_bonus: 0 },
    }
}
fn authored(check: Option<CheckDef>, effects: Vec<Effect>) -> StrikeDef {
    StrikeDef {
        offset: 0,
        unlock: None,
        check,
        on_success: effects,
        on_failure: vec![],
        descriptors: vec![],
        riders: vec![],
    }
}
#[test]
fn selected_weapon_self_target_opposed_roles_formula_and_rider_share_caster_context() {
    for role in [CheckActor::User, CheckActor::Target] {
        for formula in [false, true] {
            let probe = AbilityId::new("selection_probe").unwrap();
            let mut game = fixture_sources(
                |_| {},
                |document, species| {
                    let mut ability = document
                        .abilities
                        .iter()
                        .find(|ability| ability.basic_attack)
                        .unwrap()
                        .clone();
                    ability.id = probe.clone();
                    ability.basic_attack = false;
                    ability.reach = Reach::SelfOnly;
                    let mut strike = authored(
                        Some(CheckDef {
                            actor: role,
                            skill: SkillSelector::SelectedWeapon,
                            against: if formula {
                                Against::Difficulty {
                                    formula: Formula {
                                        base: 7,
                                        skill: Some(SkillSelector::SelectedWeapon),
                                        attribute: None,
                                    },
                                }
                            } else {
                                Against::Opposed {
                                    skill: SkillSelector::SelectedWeapon,
                                }
                            },
                        }),
                        vec![selected_damage()],
                    );
                    strike.on_failure = vec![selected_damage()];
                    strike.riders = vec![RiderDef {
                        condition: None,
                        trigger: RiderTrigger::Success,
                        check: Some(RiderCheckDef {
                            skill: SkillSelector::SelectedWeapon,
                            defense: DefenseKind::Physical,
                        }),
                        effects: vec![selected_damage()],
                        descriptors: vec![],
                    }];
                    ability.strikes = StrikeSource::Authored {
                        strikes: vec![strike],
                    };
                    document.abilities.push(ability);
                    species.modifiers.push(Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::GrantAbility {
                            ability: probe.clone(),
                        },
                    });
                },
            );
            let _primary = equip(&mut game, 0, false);
            let secondary = equip(&mut game, 10, false);
            let plan = game
                .ability_plan_with_supplies(
                    ActorId(1),
                    &probe,
                    AbilityInvocation {
                        aim: AbilityAim::ActorTarget { target: ActorId(1) },
                        weapon: WeaponSelection::Equipped {
                            item: secondary,
                            socket: EquipmentSlotId(10),
                        },
                    },
                    Default::default(),
                )
                .unwrap();
            let creature = game.creature(ActorId(1)).unwrap();
            let primary = &creature.derived().selected_attack;
            let secondary = &creature.derived().equipped_attacks[&EquipmentSlotId(10)].attack;
            let strike = &plan.as_ability().unwrap().strikes[0];
            let check = strike.check.as_ref().unwrap();
            assert_eq!(
                check.bonus,
                if role == CheckActor::User {
                    secondary.check_bonus
                } else {
                    primary.check_bonus
                }
            );
            match &check.threshold {
                CheckThreshold::Fixed { value } => assert_eq!(*value, 7 + secondary.check_bonus),
                CheckThreshold::Opposed { bonus, .. } => assert_eq!(
                    *bonus,
                    if role == CheckActor::User {
                        primary.check_bonus
                    } else {
                        secondary.check_bonus
                    }
                ),
            }
            assert_eq!(
                strike.riders[0].check.as_ref().unwrap().bonus,
                secondary.check_bonus
            );
            let expected = vec![PlannedEffect::Damage {
                damage: secondary.attack.damage_with_modifiers(0, 0, 0).unwrap(),
            }];
            assert_eq!(strike.on_success, expected);
            assert_eq!(strike.on_failure, expected);
            assert_eq!(strike.riders[0].effects, expected);
            assert_eq!(
                focus(&plan),
                2,
                "surcharge is per invocation, not per selected operand"
            );
        }
    }
}
#[test]
fn explicit_weapon_rejects_defender_only_locked_only_and_natural_abilities() {
    for mode in 0..3 {
        let mut game = fixture(|ability| {
            ability.timing.source = TimingSource::Fixed {
                prepare: 10,
                recover: 20,
            };
            ability.strikes = if mode == 2 {
                StrikeSource::NaturalMelee {
                    additional_riders: vec![],
                }
            } else {
                let mut rows = vec![authored(
                    Some(CheckDef {
                        actor: CheckActor::Target,
                        skill: SkillSelector::SelectedWeapon,
                        against: Against::Defense {
                            defense: DefenseKind::Physical,
                        },
                    }),
                    vec![],
                )];
                if mode == 1 {
                    let mut locked = authored(None, vec![selected_damage()]);
                    locked.offset = 10;
                    locked.unlock =
                        Some(crate::content_ids::TalentId::new("rattling_blow").unwrap());
                    rows.push(locked);
                }
                StrikeSource::Authored { strikes: rows }
            };
        });
        let item = equip(&mut game, 10, false);
        assert!(game.ability_plan(ActorId(1), &basic(), ActorId(2)).is_ok());
        assert!(!game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .weapon_abilities
            .contains(&basic()));
        assert!(selected(&game, item, 10).is_err());
    }
}

#[track_caller]
fn reopen(game: &Game) -> Game {
    assert!(
        game.event_receipts_valid(),
        "event receipts before checkpoint at {}",
        game.tick
    );
    assert!(
        game.combat_valid(),
        "combat state before checkpoint at {}",
        game.tick
    );
    assert!(
        game.loaded_holds_valid(),
        "holds before checkpoint at {}",
        game.tick
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}
#[test]
fn mixed_selected_cost_pending_prefix_resume_and_completed_reopen_pay_once() {
    let mut game = fixture(|ability| {
        ability.cost = Some(Cost {
            resource: Resource::Mana,
            start: 1,
            resolution: 2,
        });
        ability.timing.source = TimingSource::Fixed {
            prepare: 10,
            recover: 20,
        };
        let first = authored(None, vec![selected_damage()]);
        let mut second = first.clone();
        second.offset = 10;
        ability.strikes = StrikeSource::Authored {
            strikes: vec![first, second],
        };
    });
    let primary = equip(&mut game, 0, false);
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    let before = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .clone();
    start_plan(&mut game, &plan);
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let pending = game.creature(ActorId(1)).unwrap();
    assert_eq!(
        pending.costs().resources().balance(Resource::Mana),
        before.balance(Resource::Mana) - 1
    );
    assert_eq!(
        pending.costs().resources().balance(Resource::Focus),
        before.balance(Resource::Focus)
    );
    let owner = game.preparation(ActorId(1)).unwrap().hold.unwrap();
    assert_eq!(pending.costs().reservation(owner), plan.cost());
    let mut restored = reopen(&game);
    assert_eq!(restored, game);
    // Resuming the identical pending plan does not repeat its start payment.
    restored.start_work(plan.clone(), Some(owner)).unwrap();
    assert_eq!(restored, game);
    let mut expected = game.creature(ActorId(1)).unwrap().into_owned();
    expected.advance_active(10);
    expected.finish_cost(owner).unwrap();
    for state in [&mut game, &mut restored] {
        state
            .with_effect_publication(|candidate| {
                candidate.tick = 10;
                candidate.resolve_preparations()?;
                candidate.actors.get_mut(&ActorId(2)).unwrap().ready_at = candidate.tick;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(restored, game);
    assert_eq!(
        game.creature(ActorId(1)).unwrap().costs().resources(),
        expected.costs().resources()
    );
    assert!(game.preparation(ActorId(1)).unwrap().has_prefix());
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    let mut restored = reopen(&game);
    assert_eq!(restored, game);
    game.pause_preparation(ActorId(1)).unwrap();
    restored.pause_preparation(ActorId(1)).unwrap();
    let other = selected(&game, primary, 0).unwrap();
    let before = game.clone();
    assert!(game.start_work(other, None).is_err());
    assert_eq!(
        game, before,
        "another socket cannot resume or reuse paid prefix"
    );
    game.start_work(plan.clone(), Some(owner)).unwrap();
    restored.start_work(plan, Some(owner)).unwrap();
    expected.advance_active(10);
    for state in [&mut game, &mut restored] {
        state
            .with_effect_publication(|candidate| {
                candidate.tick = 20;
                candidate.resolve_preparations()?;
                candidate.actors.get_mut(&ActorId(2)).unwrap().ready_at = candidate.tick;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(restored, game);
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(
        game.creature(ActorId(1)).unwrap().costs().resources(),
        expected.costs().resources()
    );
    assert_eq!(reopen(&game), game);
}
#[test]
fn mixed_selected_cost_start_insufficiency_is_atomic_and_cancel_releases_both_pools() {
    let mut game = fixture(|ability| {
        ability.cost = Some(Cost {
            resource: Resource::Mana,
            start: 1,
            resolution: 2,
        });
    });
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    start_plan(&mut game, &plan);
    let owner = game.preparation(ActorId(1)).unwrap().hold.unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .costs()
            .reservation(owner),
        plan.cost()
    );
    let natural = game.ability_plan(ActorId(1), &basic(), ActorId(2)).unwrap();
    start_plan(&mut game, &natural);
    let creature = game.creature(ActorId(1)).unwrap();
    assert!(creature.costs().reservations().values().all(|cost| cost
        .entries()
        .all(|entry| entry.resource != Resource::Focus)));
    let ledger = creature.costs();
    let mut held = ledger.clone();
    let remaining = held.available(Resource::Focus);
    held.start(
        crate::IntentionId(u64::MAX - 1),
        crate::costs::CostBundle::single(crate::costs::ResourceCost {
            resource: Resource::Focus,
            start: remaining,
            resolution: 0,
        })
        .unwrap(),
    )
    .unwrap();
    let before = held.clone();
    assert!(held
        .start(crate::IntentionId(u64::MAX - 2), plan.cost().unwrap())
        .is_err());
    assert_eq!(held, before);
}
#[test]
fn historical_profile_rejects_capacity_free_two_hand_state_and_impossible_own_demand() {
    for capacity_free in [true, false] {
        let mut game = fixture_sources(
            |_| {},
            |document, species| {
                if capacity_free {
                    document
                        .equipment_slots
                        .iter_mut()
                        .find(|slot| slot.id.as_str() == "weapon")
                        .unwrap()
                        .capacity = crate::equipment_capacity::EquipmentCapacityUse::None;
                } else {
                    species.anatomy.capacities =
                        crate::equipment_capacity::EquipmentCapacities::new(
                            [(
                                crate::content_ids::EquipmentCapacityId::new("grip").unwrap(),
                                1,
                            )]
                            .into(),
                        )
                        .unwrap();
                }
            },
        );
        let item = equip(&mut game, 0, false);
        let plan = selected(&game, item, 0).unwrap();
        let mut forged = plan.as_ability().unwrap().clone();
        let FrozenWeaponSelection::Equipped {
            profile: CapturedWeaponProfile::Profile { spec, expert },
            ..
        } = &mut forged.weapon
        else {
            unreachable!()
        };
        spec.base_traits = vec![WeaponTrait::TwoHanded];
        if capacity_free {
            *expert = true;
            spec.expert_traits.remove = vec![crate::weapon_profiles::WeaponTraitKind::TwoHanded];
        }
        assert!(forged.valid(game.content.rules()));
        assert!(!game.plan_source_valid(&Plan::Ability(tor_world::Shared::new(forged))));
    }
}
#[test]
fn insufficient_secondary_focus_fails_real_preparation_without_gameplay_mutation() {
    let mut game = fixture_sources(
        |ability| {
            ability.cost = Some(Cost {
                resource: Resource::Mana,
                start: 1,
                resolution: 2,
            });
        },
        |_, species| {
            for modifier in &mut species.modifiers {
                if let ModifierOp::Set {
                    stat:
                        Stat::ResourceMax {
                            resource: Resource::Focus,
                        },
                    value,
                } = &mut modifier.op
                {
                    *value = 1;
                }
            }
        },
    );
    let item = equip(&mut game, 10, false);
    let resources = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .clone();
    assert_eq!(resources.maximum(Resource::Focus), 1);
    assert_eq!(resources.balance(Resource::Focus), 1);
    assert!(resources.balance(Resource::Mana) >= 3);
    let action = crate::Action::UseAbility {
        id: basic(),
        invocation: AbilityInvocation {
            aim: AbilityAim::ActorTarget { target: ActorId(2) },
            weapon: WeaponSelection::Equipped {
                item,
                socket: EquipmentSlotId(10),
            },
        },
        supplies: Default::default(),
    };
    let before = game.clone();
    let owner = game
        .admit_intention(ActorId(1), action.clone(), crate::IntentionOrigin::Human)
        .unwrap();
    let queued = game.pending_intention(ActorId(1)).unwrap().clone();
    assert_eq!(queued.id, owner);
    let mut expected = game.clone();
    expected.cancel_intention(ActorId(1), owner).unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention, queued);
    assert_eq!(execution.action, Some(action));
    assert_eq!(
        execution.outcome,
        Err(crate::GameError::Rule(RuleError::Unfunded))
    );
    assert_eq!(game, expected, "failed preparation changes no Mana, holds, RNG, time, equipment, health or events; only the admitted intention is consumed");
    assert!(game.pending_intention(ActorId(1)).is_none());
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(
        game.creature(ActorId(1)).unwrap().costs().resources(),
        &resources
    );
    let mut control = before;
    control.actors.get_mut(&ActorId(2)).unwrap().ready_at = control.tick;
    let owner = control
        .admit_intention(
            ActorId(1),
            crate::Action::UseAbility {
                id: basic(),
                invocation: AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = control.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, owner);
    execution.outcome.unwrap();
    assert_eq!(
        control
            .creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Mana),
        resources.balance(Resource::Mana) - 1
    );
    assert_eq!(
        control
            .creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Focus),
        1
    );
}
fn preview_for_plan(game: &Game, plan: &Plan) -> Result<AbilityWeaponPreview, crate::GameError> {
    let crate::Work::UseAbility {
        id,
        invocation,
        supplies,
    } = plan.work()
    else {
        unreachable!()
    };
    game.preview_ability_weapon(ActorId(1), &id, invocation, supplies)
}

#[test]
fn selected_weapon_preview_matches_exact_captured_terms_without_mutation() {
    let mut game = fixture(|ability| {
        ability.cost = Some(Cost {
            resource: Resource::Mana,
            start: 1,
            resolution: 2,
        });
    });
    let primary = equip(&mut game, 0, false);
    let secondary = equip(&mut game, 10, false);
    for expert in [false, true] {
        if expert {
            trained(&mut game);
        }
        for (item, socket) in [(primary, 0), (secondary, 10)] {
            let plan = selected(&game, item, socket).unwrap();
            let before = game.clone();
            let preview = preview_for_plan(&game, &plan).unwrap();
            assert_eq!(
                game, before,
                "preview changes no RNG, time, holds or equipment"
            );
            assert_eq!(preview.phase, AbilityWeaponPreviewPhase::Fresh);
            assert_eq!(preview.cost, plan.cost());
            assert_eq!(preview.preparation, plan.preparation());
            assert_eq!(preview.recovery, plan.recovery());
            let focus = preview
                .cost
                .unwrap()
                .entries()
                .find(|cost| cost.resource == Resource::Focus)
                .map(|cost| cost.resolution);
            assert_eq!(focus, (socket != 0).then_some(if expert { 1 } else { 2 }));
        }
    }
}

#[test]
fn selected_weapon_preview_validates_reference_and_is_not_a_funding_promise() {
    let mut game = fixture_sources(
        |_| {},
        |_, species| {
            for modifier in &mut species.modifiers {
                if let ModifierOp::Set {
                    stat:
                        Stat::ResourceMax {
                            resource: Resource::Focus,
                        },
                    value,
                } = &mut modifier.op
                {
                    *value = 1;
                }
            }
        },
    );
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    let before = game.clone();
    let preview = preview_for_plan(&game, &plan).unwrap();
    assert_eq!(game, before);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .costs()
            .validate_start(crate::IntentionId(u64::MAX - 2), preview.cost.unwrap()),
        Err(crate::costs::CostError::Insufficient)
    );
    let mut invocation = plan.as_ability().unwrap().invocation;
    invocation.aim = AbilityAim::ContactTarget { target: ActorId(2) };
    assert!(
        game.preview_ability_weapon(ActorId(1), &basic(), invocation, Default::default())
            .is_err(),
        "a creature reference cannot be relabeled as a contact"
    );
    assert_eq!(game, before);
    invocation = plan.as_ability().unwrap().invocation;
    invocation.weapon = WeaponSelection::Equipped {
        item,
        socket: EquipmentSlotId(0),
    };
    assert!(game
        .preview_ability_weapon(ActorId(1), &basic(), invocation, Default::default())
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn exact_preview_reuses_frozen_expertise_and_marks_paid_prefix_without_repayment() {
    let mut game = fixture(|ability| {
        ability.cost = Some(Cost {
            resource: Resource::Mana,
            start: 1,
            resolution: 2,
        });
        ability.timing.source = TimingSource::Fixed {
            prepare: 10,
            recover: 20,
        };
        let first = authored(None, vec![selected_damage()]);
        let mut second = first.clone();
        second.offset = 10;
        ability.strikes = StrikeSource::Authored {
            strikes: vec![first, second],
        };
    });
    trained(&mut game);
    let _primary = equip(&mut game, 0, false);
    let secondary = equip(&mut game, 10, true);
    let plan = selected(&game, secondary, 10).unwrap();
    assert_eq!(focus(&plan), 1);
    start_plan(&mut game, &plan);
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build.remove_latest().unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert!(
        selected(&game, secondary, 10).is_err(),
        "new capture observes current grip deficit"
    );
    let before = game.clone();
    let preview = preview_for_plan(&game, &plan).unwrap();
    assert_eq!(game, before);
    assert_eq!(preview.phase, AbilityWeaponPreviewPhase::Resuming);
    assert_eq!(preview.cost, plan.cost());
    assert_eq!(preview.preparation, 10);
    assert_eq!(preview.recovery, 20);
    game.with_effect_publication(|candidate| {
        candidate.tick = 10;
        candidate.resolve_preparations()?;
        candidate.actors.get_mut(&ActorId(2)).unwrap().ready_at = candidate.tick;
        Ok(())
    })
    .unwrap();
    let before = game.clone();
    let preview = preview_for_plan(&game, &plan).unwrap();
    assert_eq!(game, before);
    assert_eq!(preview.phase, AbilityWeaponPreviewPhase::PaidPrefix);
    assert_eq!(
        preview.cost,
        plan.cost(),
        "full historical bundle is explicitly already settled"
    );
    assert_eq!(preview.preparation, 10);
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
}

#[test]
fn paused_selected_preview_freezes_elapsed_preparation_without_charging_again() {
    let mut game = fixture(|ability| {
        ability.timing.source = TimingSource::Fixed {
            prepare: 10,
            recover: 20,
        };
    });
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    start_plan(&mut game, &plan);
    game.tick = 4;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 4;
    let active = preview_for_plan(&game, &plan).unwrap();
    assert_eq!(active.preparation, 6);
    assert_eq!(active.phase, AbilityWeaponPreviewPhase::Resuming);
    game.pause_preparation(ActorId(1)).unwrap();
    let balances = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .clone();
    game.tick = 8;
    for actor in [ActorId(1), ActorId(2)] {
        game.actors.get_mut(&actor).unwrap().ready_at = game.tick;
    }
    let before = game.clone();
    let paused = preview_for_plan(&game, &plan).unwrap();
    assert_eq!(
        paused, active,
        "paused preparation does not consume elapsed world ticks"
    );
    assert_eq!(game, before);
    assert_eq!(
        game.creature(ActorId(1)).unwrap().costs().resources(),
        &balances
    );
}
