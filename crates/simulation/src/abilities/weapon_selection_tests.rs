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
    equip_configured(game, socket, expert_reduces_grip, |_| {})
}
fn equip_configured(
    game: &mut Game,
    socket: u16,
    expert_reduces_grip: bool,
    configure: impl FnOnce(&mut crate::ItemSpec),
) -> ItemId {
    let mut item = crate::ItemSpec::ordinary(format!("weapon {socket}"));
    item.class = crate::ItemClass::Weapon;
    let component = crate::damage::DamageComponent::fixed(
        crate::combat::DamageType::Keen,
        None,
        5 + u32::from(socket),
    );
    let primary = component.key();
    item.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
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
            launcher: None,
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
    configure(&mut item);
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
        game.items.remove(&item);
        assert!(
            game.plan_source_valid(&plan),
            "a missing historical item remains a legal source witness"
        );
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

fn stow_fixture_weapon(game: &mut Game, item: ItemId) {
    let plan = game
        .plan_work(ActorId(1), crate::Work::Unequip { item })
        .unwrap();
    game.with_effect_publication(|game| {
        game.finish_item_work(ActorId(1), &plan, None, false, None)
    })
    .unwrap();
}
fn finish_fixture_preparation(game: &mut Game) {
    let elapsed = game
        .preparation(ActorId(1))
        .unwrap()
        .remaining_at(game.actor_clock(ActorId(1)));
    game.tick += elapsed;
    for id in game.actors.keys().copied().collect::<Vec<_>>() {
        game.actors.get_mut(&id).unwrap().ready_at = game.tick;
    }
    game.with_effect_publication(|game| game.resolve_preparations())
        .unwrap();
}
#[test]
fn frozen_equip_retains_paid_grip_profile_after_expertise_loss_and_reopen() {
    let mut game = fixture(|_| {});
    trained(&mut game);
    equip(&mut game, 0, false);
    let target = equip(&mut game, 10, true);
    stow_fixture_weapon(&mut game, target);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item: target,
                slot: EquipmentSlotId(10),
            },
        )
        .unwrap();
    start_plan(&mut game, &plan);
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build.remove_latest().unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert!(game.preparation_still_available(game.preparation(ActorId(1)).unwrap(), None));
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.preparation(ActorId(1)).unwrap().plan, plan);
    finish_fixture_preparation(&mut restored);
    assert_eq!(
        restored
            .equipment(ActorId(1))
            .unwrap()
            .get(&EquipmentSlotId(10)),
        Some(&target)
    );
    assert!(!restored
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .equipment_capacity_deficits
        .is_empty());
    assert!(selected(&restored, target, 10).is_err());
    let remove = restored
        .plan_work(ActorId(1), crate::Work::Unequip { item: target })
        .unwrap();
    assert!(
        remove.preparation() > 0,
        "putting a weapon away remains paid"
    );
}
#[test]
fn frozen_equip_rejects_changed_pool_occupancy_and_forged_surviving_profiles() {
    let mut game = fixture(|_| {});
    let occupant = equip(&mut game, 0, false);
    let target = equip(&mut game, 10, false);
    stow_fixture_weapon(&mut game, target);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item: target,
                slot: EquipmentSlotId(10),
            },
        )
        .unwrap();
    let Plan::Equip { allocation, .. } = &plan else {
        unreachable!()
    };
    assert!(game.frozen_equip_available(ActorId(1), target, EquipmentSlotId(10), allocation));
    for target_profile in [true, false] {
        let mut bad = allocation.clone();
        if target_profile {
            bad.target = CapturedWeaponProfile::Ordinary;
        } else {
            bad.occupants[0].profile = CapturedWeaponProfile::Ordinary;
        }
        assert!(bad.valid_shape(game.content.rules(), target, EquipmentSlotId(10)));
        assert!(!bad.source_valid(&game, ActorId(1), target, EquipmentSlotId(10)));
    }
    stow_fixture_weapon(&mut game, occupant);
    assert!(!game.frozen_equip_available(ActorId(1), target, EquipmentSlotId(10), allocation));
    game.items.remove(&target);
    assert!(
        allocation.source_valid(&game, ActorId(1), target, EquipmentSlotId(10)),
        "missing historical item is loadable, but cannot complete"
    );
    assert!(!game.frozen_equip_available(ActorId(1), target, EquipmentSlotId(10), allocation));
}
#[test]
fn quickdraw_is_an_ordinary_zero_time_draw_with_no_free_replacement_or_sheathing() {
    for expert_only in [false, true] {
        let mut game = fixture(|_| {});
        if expert_only {
            trained(&mut game);
        }
        let item = equip(&mut game, 0, false);
        stow_fixture_weapon(&mut game, item);
        let mut spec = (*game.items.get(&item).unwrap().spec).clone();
        let gear = spec.equipment.as_mut().unwrap();
        let profile = gear.weapon_profile.as_mut().unwrap();
        if expert_only {
            profile.expert_traits.add.push(WeaponTrait::Quickdraw);
        } else {
            profile.base_traits.push(WeaponTrait::Quickdraw);
        }
        game.items
            .edit(item, |source| source.spec = tor_world::Shared::new(spec))
            .unwrap();
        let plan = game
            .plan_work(
                ActorId(1),
                crate::Work::Equip {
                    item,
                    slot: EquipmentSlotId(0),
                },
            )
            .unwrap();
        assert_eq!(plan.preparation(), 0);
        assert!(plan.valid(game.content.rules()));
        let mut delayed = plan.clone();
        let Plan::Equip { preparation, .. } = &mut delayed else {
            unreachable!()
        };
        *preparation = 1;
        assert!(
            !delayed.valid(game.content.rules()),
            "a captured Quickdraw draw has canonical zero preparation"
        );
        let tick = game.tick;
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
        let id = game
            .admit_intention(
                ActorId(1),
                plan.work().action(),
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, id);
        execution.outcome.unwrap();
        assert_eq!(game.tick, tick);
        assert_eq!(
            game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
            Some(&item)
        );
        assert!(game.plan_work(ActorId(1), plan.work()).is_err());
        let put_away = game
            .plan_work(ActorId(1), crate::Work::Unequip { item })
            .unwrap();
        assert!(put_away.preparation() > 0);
    }
}
#[test]
fn creatureless_draws_keep_base_traits_and_cannot_claim_expert_states() {
    let mut game = fixture(|_| {});
    let item = equip(&mut game, 0, false);
    stow_fixture_weapon(&mut game, item);
    let mut spec = (*game.items.get(&item).unwrap().spec).clone();
    let gear = spec.equipment.as_mut().unwrap();
    gear.weapon_profile.as_mut().unwrap().base_traits =
        vec![WeaponTrait::TwoHanded, WeaponTrait::Quickdraw];
    game.items
        .edit(item, |source| source.spec = tor_world::Shared::new(spec))
        .unwrap();
    game.actors.get_mut(&ActorId(1)).unwrap().combat = None;
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item,
                slot: EquipmentSlotId(0),
            },
        )
        .unwrap();
    assert_eq!(plan.preparation(), 0);
    let Plan::Equip { allocation, .. } = &plan else {
        unreachable!()
    };
    assert!(matches!(
        allocation.anatomy,
        crate::equipment_capture::EquipAnatomySource::Configured { .. }
    ));
    assert!(matches!(
        allocation.target,
        CapturedWeaponProfile::Profile { expert: false, .. }
    ));
    let mut bad = allocation.clone();
    let CapturedWeaponProfile::Profile { expert, .. } = &mut bad.target else {
        unreachable!()
    };
    *expert = true;
    assert!(!bad.valid_shape(game.content.rules(), item, EquipmentSlotId(0)));
    game.with_effect_publication(|game| {
        game.finish_item_work(ActorId(1), &plan, None, false, None)
    })
    .unwrap();
    let another = (*game.items.get(&item).unwrap().spec).clone();
    let at = game.actors[&ActorId(1)].location;
    let another = game
        .spawn_item_stack(at, Some(ActorId(1)), 1, another)
        .unwrap();
    assert!(game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item: another,
                slot: EquipmentSlotId(10)
            }
        )
        .is_err());
}

#[test]
fn frozen_equip_compares_only_its_complete_pool_and_rejects_capacity_changes() {
    let jewelry = crate::content_ids::EquipmentCapacityId::new("jewelry").unwrap();
    let ring_slot = crate::EquipmentSlot::new("fixture_ring").unwrap();
    let mut game = fixture_sources(
        |_| {},
        |document, species| {
            document.equipment_slots.push(EquipmentSlotDef {
                id: ring_slot.clone(),
                name: "ring".into(),
                equipped_phrase: "wearing".into(),
                compatible_classes: vec![crate::ItemClass::Ring],
                requires_attack: false,
                capacity: crate::equipment_capacity::EquipmentCapacityUse::Shared {
                    resource: jewelry.clone(),
                    units: 1,
                },
                equip_turns: 1,
                unequip_turns: 1,
            });
            species.anatomy.slots.push(ring_slot.clone());
            species.anatomy.capacities = crate::equipment_capacity::EquipmentCapacities::new(
                [
                    (
                        crate::content_ids::EquipmentCapacityId::new("grip").unwrap(),
                        2,
                    ),
                    (jewelry.clone(), 1),
                ]
                .into(),
            )
            .unwrap();
        },
    );
    equip(&mut game, 0, false);
    let item = equip(&mut game, 10, false);
    stow_fixture_weapon(&mut game, item);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item,
                slot: EquipmentSlotId(10),
            },
        )
        .unwrap();
    start_plan(&mut game, &plan);
    let mut ring = crate::ItemSpec::ordinary("ring".into());
    ring.class = crate::ItemClass::Ring;
    ring.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        slot: ring_slot,
        armor: None,
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
    });
    let at = game.actors[&ActorId(1)].location;
    let ring = game
        .spawn_item_stack(at, Some(ActorId(1)), 1, ring)
        .unwrap();
    game.equip_authored(ActorId(1), ring, EquipmentSlotId(11))
        .unwrap();
    let Plan::Equip { allocation, .. } = &plan else {
        unreachable!()
    };
    assert!(game.frozen_equip_available(ActorId(1), item, EquipmentSlotId(10), allocation));
    let mut changed = game.clone();
    changed
        .actors
        .get_mut(&ActorId(1))
        .unwrap()
        .anatomy
        .capacities = crate::equipment_capacity::EquipmentCapacities::new(
        [
            (
                crate::content_ids::EquipmentCapacityId::new("grip").unwrap(),
                3,
            ),
            (jewelry, 1),
        ]
        .into(),
    )
    .unwrap();
    assert!(!changed.frozen_equip_available(ActorId(1), item, EquipmentSlotId(10), allocation));
    finish_fixture_preparation(&mut game);
    assert_eq!(
        game.equipment(ActorId(1))
            .unwrap()
            .get(&EquipmentSlotId(10)),
        Some(&item)
    );
    assert_eq!(
        game.equipment(ActorId(1))
            .unwrap()
            .get(&EquipmentSlotId(11)),
        Some(&ring)
    );
}

#[test]
fn surviving_selected_weapon_requires_its_exact_authored_profile_but_not_current_expertise() {
    let mut game = fixture(|_| {});
    let item = equip(&mut game, 10, false);
    let plan = selected(&game, item, 10).unwrap();
    let mut forged = plan.as_ability().unwrap().clone();
    let FrozenWeaponSelection::Equipped { profile, .. } = &mut forged.weapon else {
        unreachable!()
    };
    *profile = CapturedWeaponProfile::Ordinary;
    assert!(forged.valid(game.content.rules()));
    assert!(!game.plan_source_valid(&Plan::Ability(tor_world::Shared::new(forged))));
    trained(&mut game);
    assert!(
        game.plan_source_valid(&plan),
        "current expertise does not rewrite the captured state"
    );
    game.items.remove(&item);
    assert!(
        game.plan_source_valid(&plan),
        "deleted historical item still has an issued identity"
    );
}

#[test]
fn frozen_equip_codec_requires_bounded_named_witness_fields_and_canonical_allocations() {
    let mut game = fixture(|_| {});
    equip(&mut game, 0, false);
    let item = equip(&mut game, 10, false);
    stow_fixture_weapon(&mut game, item);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item,
                slot: EquipmentSlotId(10),
            },
        )
        .unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    let decoded: Plan = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded, plan);
    assert!(game.plan_source_valid(&decoded));
    for field in ["anatomy", "target", "occupants"] {
        let mut bad = value.clone();
        bad["allocation"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Plan>(bad).is_err(),
            "missing {field}"
        );
    }
    let mut bad = value.clone();
    bad["allocation"]["claimed_demand"] = json!(0);
    assert!(serde_json::from_value::<Plan>(bad).is_err());
    let mut bad = value.clone();
    bad["allocation"]["occupants"] = json!(vec![value["allocation"]["occupants"][0].clone(); 65]);
    assert!(serde_json::from_value::<Plan>(bad).is_err());
    let encoded = serde_json::to_string(&plan).unwrap();
    let duplicate = encoded.replacen("\"occupants\":", "\"occupants\":[],\"occupants\":", 1);
    assert!(serde_json::from_str::<Plan>(&duplicate).is_err());
    let mut bad = value.clone();
    bad["allocation"]["occupants"]
        .as_array_mut()
        .unwrap()
        .push(value["allocation"]["occupants"][0].clone());
    let decoded: Plan = serde_json::from_value(bad).unwrap();
    assert!(
        !decoded.valid(game.content.rules()),
        "duplicate allocation has valid syntax, but no canonical plan"
    );
    let mut bad = value;
    bad["preparation"] = json!(0);
    let decoded: Plan = serde_json::from_value(bad).unwrap();
    assert!(
        !decoded.valid(game.content.rules()),
        "ordinary equipment cannot forge Quickdraw"
    );
}

fn projectile_launch_fixture() -> (Game, AbilityId, ProjectileDeliverySelection, ItemId, ItemId) {
    projectile_launch_fixture_with(|_| {})
}
fn projectile_launch_fixture_with(
    configure: impl FnOnce(&mut AbilityDef),
) -> (Game, AbilityId, ProjectileDeliverySelection, ItemId, ItemId) {
    use crate::projectile_profiles::{HitRecovery, ProjectileDeliveryKind, ProjectileSpec};
    use crate::weapon_profiles::{LauncherProfile, RangeBands};
    let launch = AbilityId::new("fixture.launch").unwrap();
    let mut game = fixture_sources(
        |_| {},
        |document, _| {
            let mut ability = document
                .abilities
                .iter()
                .find(|ability| ability.basic_attack)
                .unwrap()
                .clone();
            ability.id = launch.clone();
            ability.basic_attack = false;
            ability.invocation = InvocationKind::DirectionalProjectile;
            ability.reach = Reach::Projectile;
            ability.strikes = StrikeSource::Projectile {
                delivery: ProjectileDeliveryKind::Launch,
            };
            document.conditions.push(ConditionDef {
                id: crate::content_ids::ConditionId::new("fixture_invisible").unwrap(),
                family: crate::content_ids::ConditionFamilyId::new("fixture_invisible").unwrap(),
                lifecycle: ConditionLifecycle::Timed,
                name: "Invisible fixture".into(),
                modifiers: vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Perception {
                        value: crate::perception::PerceptionValue::Invisible { enabled: true },
                    },
                }],
                stacking: Stacking::Extend,
                source_scoped: false,
                descriptors: vec![],
                capacity: Eviction::Reject,
                timing: ConditionTiming::expiring(),
                restrictions: vec![],
                preparation: ConditionPreparation::Preserve,
            });
            configure(&mut ability);
            document.constants.default_abilities.push(launch.clone());
            document.abilities.push(ability);
        },
    );
    let family = crate::content_ids::LauncherFamilyId::new("fixture.bow").unwrap();
    let launcher = equip_configured(&mut game, 0, false, |item| {
        item.equipment
            .as_mut()
            .unwrap()
            .weapon_profile
            .as_mut()
            .unwrap()
            .launcher = Some(LauncherProfile {
            family: family.clone(),
            range: RangeBands {
                short_feet: 80,
                long_feet: 320,
            },
        });
    });
    let component =
        crate::damage::DamageComponent::fixed(crate::combat::DamageType::Keen, None, 11);
    let primary = component.key();
    let mut spec = crate::ItemSpec::ordinary("test ammunition".into());
    spec.class = crate::ItemClass::Weapon;
    spec.stackable = true;
    spec.projectile = Some(ProjectileSpec {
        family,
        damage: crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
        recovery: HitRecovery::ConsumedOnHit,
    });
    let ammo = game
        .spawn_item_stack(game.actors[&ActorId(1)].location, Some(ActorId(1)), 2, spec)
        .unwrap();
    // Fixture construction precedes the authoritative compiled equipment refresh.
    game.items
        .edit(launcher, |item| {
            item.condition.hit_enchantment = 2;
            item.condition.damage_enchantment = 3;
        })
        .unwrap();
    game.items
        .edit(ammo, |item| {
            item.condition.hit_enchantment = 4;
            item.condition.damage_enchantment = 5;
        })
        .unwrap();
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .equipment
        .remove(&EquipmentSlotId(0));
    game.equip_authored(ActorId(1), launcher, EquipmentSlotId(0))
        .unwrap();
    let selection = ProjectileDeliverySelection::Launch {
        launcher: EquippedProjectileWeapon {
            item: launcher,
            socket: EquipmentSlotId(0),
        },
        ammunition: ammo,
    };
    (game, launch, selection, launcher, ammo)
}

#[test]
fn projectile_capture_uses_exact_payload_damage_and_both_enchantments_once() {
    use crate::weapon_profiles::RangeBands;
    let (game, launch, selection, _, _) = projectile_launch_fixture();
    let before = game.clone();
    let plan = game
        .capture_projectile_plan(ActorId(1), &launch, StepDirection::East, selection)
        .unwrap();
    assert!(
        !game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .weapon_abilities
            .contains(&launch),
        "closed delivery owns selection, never outer weapon"
    );
    assert_eq!(
        game, before,
        "capture must not move, claim, pay or advance RNG"
    );
    assert_eq!(
        plan.range,
        RangeBands {
            short_feet: 80,
            long_feet: 320
        }
    );
    assert_eq!(
        plan.bonus,
        game.creature(ActorId(1))
            .unwrap()
            .derived()
            .selected_attack
            .check_bonus
            + 4
    );
    assert!(plan.valid(game.content.rules()));
    let decoded: super::ProjectilePlan =
        serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(decoded, plan);
    let [PlannedEffect::Damage { damage }] = plan.effects.as_slice() else {
        panic!("one damage payload")
    };
    assert_eq!(
        damage.components()[0].amount(),
        crate::damage::DamageAmount::Fixed(19)
    );
}

fn projectile_throw_fixture(
    qualification: Option<crate::conditions::TokenQualification>,
) -> (Game, ProjectilePlan) {
    use crate::projectile_profiles::ProjectileDeliveryKind;
    let throw = AbilityId::new("fixture.throw").unwrap();
    let mut game = fixture_sources(
        |_| {},
        |document, _| {
            let mut ability = document
                .abilities
                .iter()
                .find(|ability| ability.basic_attack)
                .unwrap()
                .clone();
            ability.id = throw.clone();
            ability.basic_attack = false;
            ability.cost = Some(Cost {
                resource: Resource::Mana,
                start: 1,
                resolution: 2,
            });
            ability.invocation = InvocationKind::DirectionalProjectile;
            ability.reach = Reach::Projectile;
            ability.strikes = StrikeSource::Projectile {
                delivery: ProjectileDeliveryKind::Throw,
            };
            document.constants.default_abilities.push(throw.clone());
            document.abilities.push(ability);
        },
    );
    let item = equip_configured(&mut game, 0, false, |item| {
        item.equipment
            .as_mut()
            .unwrap()
            .weapon_profile
            .as_mut()
            .unwrap()
            .base_traits
            .push(WeaponTrait::Thrown {
                range: crate::weapon_profiles::RangeBands {
                    short_feet: 20,
                    long_feet: 60,
                },
            });
    });
    let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
    let combat = outcome.state.combat.as_mut().unwrap();
    let mut creature = combat.creature().clone();
    creature
        .apply_qualified_condition(
            &crate::content_ids::ConditionId::new("readied").unwrap(),
            ActorId(1),
            300,
            qualification,
        )
        .unwrap();
    *combat = crate::actor_creatures::CombatState::from_creature(combat.identity(), creature);
    game.apply_outcome(outcome).unwrap();
    let plan = game
        .capture_projectile_plan(
            ActorId(1),
            &throw,
            StepDirection::East,
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Equipped {
                    item,
                    socket: EquipmentSlotId(0),
                },
            },
        )
        .unwrap();
    (game, plan)
}

#[test]
fn projectile_token_witness_rejects_duplicate_and_future_generation() {
    let (game, plan) = projectile_throw_fixture(None);
    assert_eq!(plan.tokens.len(), 1);
    assert!(game.projectile_plan_source_valid(&plan));
    let mut duplicate = plan.clone();
    let mut alias = duplicate.tokens[0].clone();
    alias.reservation.order += 1;
    duplicate.tokens.push(alias);
    assert!(!duplicate.valid(game.content.rules()));
    let mut future = plan.clone();
    future.tokens[0].reservation.generation = game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .next_generation();
    assert!(future.valid(game.content.rules()));
    assert!(!game.projectile_plan_source_valid(&future));
}

fn projectile_qualification() -> crate::conditions::TokenQualification {
    crate::conditions::TokenQualification {
        opponent: ActorId(2),
        ability_tag: crate::content_ids::AbilityTagId::new("weapon_attack").unwrap(),
    }
}

#[test]
fn projectile_candidates_are_unreserved_and_cannot_claim_reapplied_instances() {
    let (mut game, plan) = projectile_throw_fixture(Some(projectile_qualification()));
    assert!(plan.tokens.is_empty());
    assert_eq!(plan.conditional_tokens.len(), 1);
    let captured = plan.conditional_tokens[0].clone();
    assert_eq!(
        game.projectile_collision_candidates(&plan, ActorId(2))
            .unwrap(),
        vec![captured.clone()]
    );
    assert!(game
        .projectile_collision_candidates(&plan, ActorId(1))
        .unwrap()
        .is_empty());
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .instances()
        .all(|row| row.reserved_by.is_none()));
    let mut update = game.actor_outcome(ActorId(1)).unwrap();
    let combat = update.state.combat.as_mut().unwrap();
    let mut state = combat.creature().clone();
    state.remove_condition(&captured.condition).unwrap();
    state
        .apply_qualified_condition(
            &captured.condition,
            captured.source,
            300,
            captured.qualification.clone(),
        )
        .unwrap();
    *combat = crate::actor_creatures::CombatState::from_creature(combat.identity(), state);
    game.with_effect_publication(|candidate| candidate.apply_outcome(update))
        .unwrap();
    let replacement = game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .instances()
        .find(|row| row.condition == captured.condition)
        .unwrap()
        .clone();
    assert_eq!(replacement.order, captured.order, "age-order ABA control");
    assert_ne!(replacement.generation, captured.generation);
    assert!(game
        .projectile_collision_candidates(&plan, ActorId(2))
        .unwrap()
        .is_empty());
}

#[test]
fn projectile_collision_consumes_matching_candidate_for_hit_or_miss_only_once() {
    let (game, captured) = projectile_throw_fixture(Some(projectile_qualification()));
    for (bonus, success) in [(1000, true), (-1000, false)] {
        let mut game = game.clone();
        let mut plan = captured.clone();
        // Isolate collision participation from random attack success.
        plan.bonus = bonus;
        let before = game.creature(ActorId(2)).unwrap().health();
        let outcome = game
            .with_effect_publication(|candidate| {
                candidate.resolve_projectile_encounter(&plan, ActorId(2), 5, None)
            })
            .unwrap();
        assert_eq!(outcome.check.unwrap().success, success);
        assert!(game
            .projectile_collision_candidates(&plan, ActorId(2))
            .unwrap()
            .is_empty());
        assert!(!game
            .creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .any(|row| row.condition == captured.conditional_tokens[0].condition));
        if success {
            assert!(game.creature(ActorId(2)).unwrap().health().injury() > before.injury());
        } else {
            assert_eq!(game.creature(ActorId(2)).unwrap().health(), before);
            assert_eq!(outcome.result, OutcomeStatus::Miss);
        }
    }
}

#[test]
fn projectile_out_of_range_rejects_without_consuming_candidate_or_rng() {
    let (mut game, plan) = projectile_throw_fixture(Some(projectile_qualification()));
    let before = game.clone();
    assert!(game
        .with_effect_publication(|candidate| candidate.resolve_projectile_encounter(
            &plan,
            ActorId(2),
            plan.range.long_feet + 5,
            None
        ))
        .is_err());
    assert_eq!(game, before);
    assert_eq!(
        game.projectile_collision_candidates(&plan, ActorId(2))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn projectile_candidate_eligibility_survives_age_order_change_but_not_expiry_or_consumption() {
    let (mut game, original) = projectile_throw_fixture(Some(projectile_qualification()));
    let actor = ActorId(1);
    let mut record = serde_json::to_value(crate::creatures::record::CreatureRecord::capture(
        &game.creature(actor).unwrap(),
    ))
    .unwrap();
    record["conditions"][0]["order"] = json!(u64::MAX);
    let equipment = game.actors[&actor]
        .equipment
        .iter()
        .map(|(socket, item)| {
            game.items[item]
                .equipment_source(*item, *socket, &game.content.rules)
                .unwrap()
        })
        .collect();
    let restored = serde_json::from_value::<crate::creatures::record::CreatureRecord>(record)
        .unwrap()
        .restore(game.content.clone(), equipment)
        .unwrap();
    let mut update = game.actor_outcome(actor).unwrap();
    let identity = update.state.combat.as_ref().unwrap().identity();
    update.state.combat = Some(crate::actor_creatures::CombatState::from_creature(
        identity, restored,
    ));
    game.with_effect_publication(|candidate| candidate.apply_outcome(update))
        .unwrap();
    let captured = game
        .capture_projectile_plan(
            actor,
            &original.ability,
            original.direction,
            original.selection,
        )
        .unwrap();
    assert_eq!(captured.conditional_tokens[0].order, u64::MAX);
    let mut update = game.actor_outcome(actor).unwrap();
    update
        .state
        .combat
        .as_mut()
        .unwrap()
        .apply_condition(
            &crate::content_ids::ConditionId::new("exposed").unwrap(),
            actor,
            300,
            None,
        )
        .unwrap();
    game.with_effect_publication(|candidate| candidate.apply_outcome(update))
        .unwrap();
    let current = game
        .projectile_collision_candidates(&captured, ActorId(2))
        .unwrap();
    assert_eq!(current.len(), 1);
    assert_ne!(current[0].order, captured.conditional_tokens[0].order);
    assert_eq!(
        current[0].generation,
        captured.conditional_tokens[0].generation
    );
    for expire in [true, false] {
        let mut game = game.clone();
        let mut update = game.actor_outcome(ActorId(1)).unwrap();
        let state = update.state.combat.as_mut().unwrap();
        if expire {
            state.advance_active(300);
        } else {
            state.reserve_tokens(&current, ActorId(1)).unwrap();
            state.consume_tokens(&current, ActorId(1));
        }
        game.with_effect_publication(|candidate| candidate.apply_outcome(update))
            .unwrap();
        assert!(game
            .projectile_collision_candidates(&captured, ActorId(2))
            .unwrap()
            .is_empty());
    }
}

#[test]
fn projectile_miss_consumes_incoming_and_conditional_tokens_with_self_return_grouping() {
    for target in [ActorId(2), ActorId(1)] {
        let mut qualification = projectile_qualification();
        qualification.opponent = target;
        let (mut game, mut plan) = projectile_throw_fixture(Some(qualification));
        plan.bonus = -1000;
        let exposed = crate::content_ids::ConditionId::new("exposed").unwrap();
        let mut update = game.actor_outcome(target).unwrap();
        update
            .state
            .combat
            .as_mut()
            .unwrap()
            .apply_condition(&exposed, ActorId(1), 300, None)
            .unwrap();
        game.with_effect_publication(|candidate| candidate.apply_outcome(update))
            .unwrap();
        let result = game
            .with_effect_publication(|candidate| {
                candidate.resolve_projectile_encounter(&plan, target, 5, None)
            })
            .unwrap();
        assert_eq!(result.result, OutcomeStatus::Miss);
        assert!(game
            .projectile_collision_candidates(&plan, target)
            .unwrap()
            .is_empty());
        assert!(!game
            .creature(target)
            .unwrap()
            .conditions()
            .instances()
            .any(|row| row.condition == exposed));
    }
}

fn paid_throw_fixture(cursed: bool) -> (Game, Plan, ItemId) {
    let (mut game, captured) = projectile_throw_fixture(None);
    let item = captured.source.item().unwrap();
    game.items
        .edit(item, |item| {
            item.condition.hit_enchantment = 1000;
            item.condition.cursed = cursed;
        })
        .unwrap();
    let mut update = game.actor_outcome(ActorId(1)).unwrap();
    let equipment = update.state.equipment.clone();
    game.refresh_equipment_outcome(&mut update, equipment)
        .unwrap();
    game.with_effect_publication(|game| game.apply_outcome(update))
        .unwrap();
    let plan = game
        .ability_plan_with_supplies(
            ActorId(1),
            &captured.ability,
            AbilityInvocation::directional_projectile(captured.direction, captured.selection),
            Default::default(),
        )
        .unwrap();
    (game, plan, item)
}

#[test]
fn projectile_paid_equipped_release_recovers_exact_unit_after_hit_and_pending_reopen() {
    let (mut game, plan, item) = paid_throw_fixture(false);
    let target = game.actors[&ActorId(2)].location;
    let health = game.creature(ActorId(2)).unwrap().health().current();
    let owner = start_plan(&mut game, &plan);
    assert_eq!(game.items[&item].holds.len(), 1);
    assert_eq!(
        game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.preparation(ActorId(1)).unwrap().plan, plan);
    finish_fixture_preparation(&mut restored);
    assert!(restored.preparation(ActorId(1)).is_none());
    assert!(!restored
        .equipment(ActorId(1))
        .unwrap()
        .contains_key(&EquipmentSlotId(0)));
    assert_eq!(
        restored.items[&item].location,
        crate::ItemLocation::Ground(target)
    );
    assert_eq!(restored.items[&item].quantity, 1);
    assert!(restored.items[&item].holds.is_empty());
    assert!(restored.creature(ActorId(2)).unwrap().health().current() < health);
    assert!(restored
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservation(owner)
        .is_none());
    let checkpoint = restored.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(checkpoint, &shared, restored.content().clone()).unwrap(),
        restored
    );
}

#[test]
fn projectile_unknown_curse_is_paid_blocked_without_releasing_or_relocating_weapon() {
    let (mut game, plan, item) = paid_throw_fixture(true);
    assert_eq!(game.actors[&ActorId(1)].known_equipment.curse(item), None);
    let before = game.clone();
    assert!(preview_for_plan(&game, &plan).is_ok());
    assert_eq!(game, before, "preview cannot discover hidden curse");
    start_plan(&mut game, &plan);
    finish_fixture_preparation(&mut game);
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.curse(item),
        Some(true)
    );
    assert_eq!(
        game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
    assert_eq!(
        game.items[&item].location,
        crate::ItemLocation::Carried(ActorId(1))
    );
    assert!(game.items[&item].holds.is_empty());
    assert_eq!(
        game.creature(ActorId(2)).unwrap().health(),
        before.creature(ActorId(2)).unwrap().health()
    );
    assert!(game.preparation(ActorId(1)).is_none());
}

#[test]
fn projectile_interruption_before_release_preserves_equipment_and_exact_unit() {
    let (mut game, plan, item) = paid_throw_fixture(false);
    start_plan(&mut game, &plan);
    game.pause_preparation(ActorId(1)).unwrap();
    assert_eq!(
        game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
    assert_eq!(game.items[&item].quantity, 1);
    assert_eq!(game.items[&item].holds.len(), 1);
    let mut discard = game.actor_outcome(ActorId(1)).unwrap();
    discard.discard(game.tick).unwrap();
    game.with_effect_publication(|game| game.apply_outcome(discard))
        .unwrap();
    assert!(game.items[&item].holds.is_empty());
    assert_eq!(
        game.items[&item].location,
        crate::ItemLocation::Carried(ActorId(1))
    );
    assert_eq!(
        game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
}

fn paid_launch_fixture(
    recovery: crate::projectile_profiles::HitRecovery,
) -> (Game, Plan, ItemId, ItemId) {
    let (mut game, ability, selection, launcher, ammo) = projectile_launch_fixture();
    let mut spec = (*game.items[&ammo].spec).clone();
    spec.projectile.as_mut().unwrap().recovery = recovery;
    game.items
        .edit(ammo, |item| {
            item.spec = tor_world::Shared::new(spec);
            item.condition.hit_enchantment = 100;
        })
        .unwrap();
    let plan = game
        .ability_plan_with_supplies(
            ActorId(1),
            &ability,
            AbilityInvocation::directional_projectile(StepDirection::East, selection),
            Default::default(),
        )
        .unwrap();
    (game, plan, launcher, ammo)
}

#[test]
fn projectile_launch_hit_consumes_one_ammunition_unit_and_keeps_launcher() {
    let (mut game, plan, launcher, ammo) =
        paid_launch_fixture(crate::projectile_profiles::HitRecovery::ConsumedOnHit);
    game.begin_survival_diagnostics(true);
    start_plan(&mut game, &plan);
    finish_fixture_preparation(&mut game);
    assert_eq!(game.items[&ammo].quantity, 1);
    assert!(game.items[&ammo].holds.is_empty());
    assert_eq!(
        game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
        Some(&launcher)
    );
    let flows = game.take_survival_diagnostics().unwrap();
    assert_eq!(flows.actors[&ActorId(1)].items_consumed, 1);
    assert_eq!(
        flows.actors[&ActorId(1)].consumed_items.get(&ammo),
        Some(&1)
    );
    assert_eq!(
        game.items
            .values()
            .filter(|item| item.spec.projectile.is_some())
            .map(|item| item.quantity)
            .sum::<u64>(),
        1
    );
}

#[test]
fn projectile_recoverable_launch_merges_fresh_identity_without_consumption() {
    let (mut game, plan, _, ammo) =
        paid_launch_fixture(crate::projectile_profiles::HitRecovery::Recoverable);
    let target = game.actors[&ActorId(2)].location;
    let condition = game.items[&ammo].condition;
    let destination = game
        .spawn_item_stack(target, None, 3, (*game.items[&ammo].spec).clone())
        .unwrap();
    game.items
        .edit(destination, |item| item.condition = condition)
        .unwrap();
    game.begin_survival_diagnostics(true);
    start_plan(&mut game, &plan);
    finish_fixture_preparation(&mut game);
    assert_eq!(game.items[&ammo].quantity, 1);
    assert!(!game.items.contains_key(&destination));
    let landed: Vec<_> = game.items.at(crate::ItemLocation::Ground(target)).collect();
    assert_eq!(landed.len(), 1);
    assert_ne!(landed[0], destination);
    assert_ne!(landed[0], ammo);
    assert_eq!(game.items[&landed[0]].quantity, 4);
    let flows = game.take_survival_diagnostics().unwrap();
    assert_eq!(
        flows
            .actors
            .get(&ActorId(1))
            .map_or(0, |flow| flow.items_consumed),
        0
    );
}

#[test]
fn projectile_landing_merge_overflow_rolls_back_payment_damage_items_and_rng() {
    let (mut game, plan, _, ammo) =
        paid_launch_fixture(crate::projectile_profiles::HitRecovery::Recoverable);
    let target = game.actors[&ActorId(2)].location;
    let condition = game.items[&ammo].condition;
    let destination = game
        .spawn_item_stack(target, None, u64::MAX, (*game.items[&ammo].spec).clone())
        .unwrap();
    game.items
        .edit(destination, |item| item.condition = condition)
        .unwrap();
    start_plan(&mut game, &plan);
    let due = game
        .preparation(ActorId(1))
        .unwrap()
        .remaining_at(game.actor_clock(ActorId(1)));
    game.tick += due;
    for id in game.actors.keys().copied().collect::<Vec<_>>() {
        game.actors.get_mut(&id).unwrap().ready_at = game.tick;
    }
    let before = game.clone();
    assert_eq!(
        game.with_effect_publication(|game| game.resolve_preparations()),
        Err(crate::GameError::InvalidQuantity)
    );
    assert_eq!(
        game, before,
        "the landing failure rolls back the entire paid release, including check RNG and damage"
    );
}

#[test]
fn projectile_carried_secondary_throw_pays_mixed_cost_once_without_equipping() {
    let (mut game, original, item) = paid_throw_fixture(false);
    stow_fixture_weapon(&mut game, item);
    let plan = game
        .ability_plan_with_supplies(
            ActorId(1),
            original.ability_id().unwrap(),
            AbilityInvocation::directional_projectile(
                StepDirection::East,
                ProjectileDeliverySelection::Throw {
                    source: ThrownSourceSelection::Carried {
                        item,
                        socket: EquipmentSlotId(10),
                    },
                },
            ),
            Default::default(),
        )
        .unwrap();
    assert_eq!(
        focus(&plan),
        2,
        "empty primary never makes a secondary grip free"
    );
    let before = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .clone();
    start_plan(&mut game, &plan);
    assert!(game.equipment(ActorId(1)).unwrap().is_empty());
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Mana),
        before.balance(Resource::Mana) - 1
    );
    finish_fixture_preparation(&mut game);
    let after = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .clone();
    assert_eq!(
        after.balance(Resource::Mana),
        before.balance(Resource::Mana) - 3
    );
    assert_eq!(
        after.balance(Resource::Focus),
        before.balance(Resource::Focus) - 2
    );
    assert!(game.equipment(ActorId(1)).unwrap().is_empty());
    assert!(matches!(
        game.items[&item].location,
        crate::ItemLocation::Ground(_)
    ));
}

#[test]
fn projectile_durable_portal_hits_recover_in_air_and_on_returning_shooter() {
    for self_return in [false, true] {
        let (mut game, original, item) = paid_throw_fixture(false);
        let origin = game.actors[&ActorId(1)].location;
        let destination = if self_return {
            origin
        } else {
            game.world
                .add_region(tor_world::Region {
                    id: tor_world::RegionId(99),
                    name: "air".into(),
                    bounds: tor_world::Extent::new(3, 3, 3).unwrap(),
                })
                .unwrap();
            let at = tor_world::Location {
                region: tor_world::RegionId(99),
                position: tor_world::Position { x: 1, y: 1, z: 1 },
            };
            game.actors.get_mut(&ActorId(2)).unwrap().location = at;
            at
        };
        game.world
            .connect(
                tor_world::Passage {
                    from: tor_world::Location {
                        region: origin.region,
                        position: tor_world::Position {
                            x: 0,
                            ..origin.position
                        },
                    },
                    direction: tor_world::Direction::West,
                    to: destination,
                },
                0,
            )
            .unwrap();
        let selected = original.as_projectile().unwrap().selection;
        let plan = game
            .ability_plan_with_supplies(
                ActorId(1),
                original.ability_id().unwrap(),
                AbilityInvocation::directional_projectile(StepDirection::West, selected),
                Default::default(),
            )
            .unwrap();
        start_plan(&mut game, &plan);
        finish_fixture_preparation(&mut game);
        assert_eq!(
            game.items[&item].location,
            crate::ItemLocation::Ground(destination)
        );
        assert_eq!(game.items[&item].quantity, 1);
        assert!(game.item_state_valid(item, &game.items[&item]));
        let victim = if self_return { ActorId(1) } else { ActorId(2) };
        assert!(game.creature(victim).unwrap().health().injury() > 0);
    }
}

#[test]
fn projectile_no_legal_miss_landing_records_one_discard_without_random_draw() {
    let (mut game, original, _, ammo) =
        paid_launch_fixture(crate::projectile_profiles::HitRecovery::ConsumedOnHit);
    let origin = game.actors[&ActorId(1)].location;
    game.actors.get_mut(&ActorId(2)).unwrap().location = tor_world::Location {
        region: origin.region,
        position: tor_world::Position {
            x: 4,
            ..origin.position
        },
    };
    for y in -1..=1 {
        for x in -1..=1 {
            if x == 0 && y == 0 {
                continue;
            }
            let at = tor_world::Location {
                region: origin.region,
                position: tor_world::Position {
                    x: origin.position.x + x,
                    y: origin.position.y + y,
                    z: origin.position.z,
                },
            };
            game.world.set_wall(at, true).unwrap();
        }
    }
    let selection = original.as_projectile().unwrap().selection;
    let plan = game
        .ability_plan_with_supplies(
            ActorId(1),
            original.ability_id().unwrap(),
            AbilityInvocation::directional_projectile(StepDirection::West, selection),
            Default::default(),
        )
        .unwrap();
    let rng = game.rules_rng;
    game.begin_survival_diagnostics(true);
    start_plan(&mut game, &plan);
    finish_fixture_preparation(&mut game);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.items[&ammo].quantity, 1);
    let flows = game.take_survival_diagnostics().unwrap();
    assert_eq!(flows.actors[&ActorId(1)].items_consumed, 1);
    assert_eq!(
        flows.actors[&ActorId(1)].consumed_items.get(&ammo),
        Some(&1)
    );
}

#[test]
fn projectile_combined_hit_uses_wide_check_lane_and_keeps_shared_final_budget() {
    let (mut game, ability, selection, _, ammo) = projectile_launch_fixture();
    game.items
        .edit(ammo, |item| item.condition.hit_enchantment = 1000)
        .unwrap();
    let before = game.clone();
    let plan = game
        .capture_projectile_plan(ActorId(1), &ability, StepDirection::East, selection)
        .unwrap();
    assert_eq!(
        plan.bonus,
        game.creature(ActorId(1))
            .unwrap()
            .derived()
            .selected_attack
            .check_bonus
            + 1000
    );
    assert!(plan.bonus > crate::limits::STAT_MAGNITUDE);
    assert!(plan.valid(game.content.rules()));
    assert_eq!(game, before);
    game.items
        .edit(ammo, |item| {
            item.condition.hit_enchantment = crate::limits::NUMERIC as i32
        })
        .unwrap();
    assert!(
        game.items[&ammo].condition_valid(),
        "each payload is individually legal"
    );
    let before = game.clone();
    assert!(game
        .capture_projectile_plan(ActorId(1), &ability, StepDirection::East, selection)
        .is_err());
    assert_eq!(
        game, before,
        "final shared check budget refusal is read-only and exposes no numeric operands"
    );
}
#[test]
fn projectile_choices_ignore_hidden_instance_values_and_do_not_mutate_state() {
    let (mut game, ability, selection, launcher, ammunition) = projectile_launch_fixture();
    let ProjectileDeliverySelection::Launch {
        launcher: selected, ..
    } = selection
    else {
        unreachable!()
    };
    let queries = [
        (ProjectileChoiceQuery::Launchers, launcher),
        (
            ProjectileChoiceQuery::Ammunition { launcher: selected },
            ammunition,
        ),
    ];
    let baseline: Vec<_> = queries
        .iter()
        .map(|(query, item)| game.projectile_choice_for_item(ActorId(1), &ability, *query, *item))
        .collect();
    assert!(baseline.iter().all(Option::is_some));
    for item in [launcher, ammunition] {
        game.items
            .edit(item, |item| {
                item.condition.cursed = true;
                item.condition.hit_enchantment = crate::limits::NUMERIC as i32;
                item.condition.damage_enchantment = -(crate::limits::NUMERIC as i32);
            })
            .unwrap();
    }
    let before = game.clone();
    let changed: Vec<_> = queries
        .iter()
        .map(|(query, item)| game.projectile_choice_for_item(ActorId(1), &ability, *query, *item))
        .collect();
    assert_eq!(changed, baseline);
    assert_eq!(game, before);
    assert!(game
        .projectile_choice_for_item(
            ActorId(1),
            &basic(),
            ProjectileChoiceQuery::Launchers,
            launcher
        )
        .is_none());
    assert!(game
        .projectile_choice_for_item(
            ActorId(1),
            &ability,
            ProjectileChoiceQuery::ThrowSources,
            launcher
        )
        .is_none());
    assert!(game
        .projectile_choice_for_item(
            ActorId(2),
            &ability,
            ProjectileChoiceQuery::Launchers,
            launcher
        )
        .is_none());
}

#[test]
fn projectile_throw_choices_use_primary_first_real_free_grips_and_active_expertise() {
    let (mut game, plan, item) = paid_throw_fixture(false);
    let ability = plan.ability_id().unwrap().clone();
    stow_fixture_weapon(&mut game, item);
    let query = |game: &Game| {
        game.projectile_choice_for_item(
            ActorId(1),
            &ability,
            ProjectileChoiceQuery::ThrowSources,
            item,
        )
    };
    assert_eq!(
        query(&game),
        Some(ProjectileChoice::Throw {
            source: ThrownSourceSelection::Carried {
                item,
                socket: EquipmentSlotId(0)
            }
        })
    );
    let primary = equip_configured(&mut game, 0, false, |_| {});
    assert_eq!(
        query(&game),
        Some(ProjectileChoice::Throw {
            source: ThrownSourceSelection::Carried {
                item,
                socket: EquipmentSlotId(10)
            }
        })
    );
    let secondary = equip_configured(&mut game, 10, false, |_| {});
    assert_eq!(query(&game), None);
    stow_fixture_weapon(&mut game, primary);
    stow_fixture_weapon(&mut game, secondary);
    game.items
        .edit(item, |item| {
            let profile = item
                .spec
                .equipment
                .as_mut()
                .unwrap()
                .weapon_profile
                .as_mut()
                .unwrap();
            let index = profile
                .base_traits
                .iter()
                .position(|value| matches!(value, WeaponTrait::Thrown { .. }))
                .unwrap();
            profile
                .expert_traits
                .add
                .push(profile.base_traits.remove(index));
        })
        .unwrap();
    assert_eq!(
        query(&game),
        None,
        "carried dagger must actually own active expertise"
    );
    trained(&mut game);
    assert!(
        query(&game).is_some(),
        "ordinary paid expertise unlocks the same carried source"
    );
}

fn ai_projectile_fixture() -> (Game, AbilityId, ProjectileDeliverySelection, ItemId, ItemId) {
    let (mut game, ability, selection, launcher, ammo) =
        projectile_launch_fixture_with(|ability| {
            ability.ai.priority = 100;
            ability.cooldown = std::num::NonZeroU64::new(200);
        });
    let target = tor_world::Location {
        position: tor_world::Position { x: 4, y: 1, z: 0 },
        ..game.actors[&ActorId(2)].location
    };
    game.teleport(ActorId(2), target).unwrap();
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .behavior
        .engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(2),
        location: target,
        last_seen: game.tick,
    });
    game.refresh_navigation();
    (game, ability, selection, launcher, ammo)
}

#[test]
fn projectile_ai_request_is_identical_under_unknown_numeric_and_curse_changes() {
    let (game, ability, selection, _, ammo) = ai_projectile_fixture();
    let expected = crate::Action::UseAbility {
        id: ability,
        invocation: AbilityInvocation::directional_projectile(StepDirection::East, selection),
        supplies: Default::default(),
    };
    let ordinary = game.choose_ai(ActorId(1)).unwrap();
    assert_eq!(ordinary.action, expected);
    assert!(
        ordinary.plan.is_none(),
        "fresh numeric capture belongs to execution"
    );
    let mut twin = game.clone();
    twin.items
        .edit(ammo, |item| {
            item.condition.hit_enchantment = crate::limits::NUMERIC as i32;
            item.condition.cursed = true;
        })
        .unwrap();
    assert!(twin.items[&ammo].condition_valid());
    let before = twin.clone();
    let changed = twin.choose_ai(ActorId(1)).unwrap();
    assert_eq!(changed.action, expected);
    assert!(changed.plan.is_none());
    assert_eq!(twin, before);
    let crate::Action::UseAbility {
        id,
        invocation,
        supplies,
    } = expected
    else {
        unreachable!()
    };
    assert_eq!(
        twin.ability_plan_with_supplies(ActorId(1), &id, invocation, supplies),
        Err(crate::GameError::Rule(RuleError::NumericBudget))
    );
}

#[test]
fn projectile_ai_numeric_refusal_pays_wait_without_failed_capture_side_effects() {
    let (mut game, _, _, launcher, ammo) = ai_projectile_fixture();
    game.items
        .edit(ammo, |item| {
            item.condition.hit_enchantment = crate::limits::NUMERIC as i32
        })
        .unwrap();
    let resources = game.creature(ActorId(1)).unwrap().costs().clone();
    let items = game.items.clone();
    let rng = game.rules_rng;
    let next_item = game.next_item_id;
    let health = game.health(ActorId(2));
    let ready = game.actors[&ActorId(1)].ready_at;
    game.begin_survival_diagnostics(true);
    game.admit_ai_intention(ActorId(1)).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut reopened = Game::restore_checkpoint(checkpoint, &shared, game.content.clone()).unwrap();
    reopened.begin_survival_diagnostics(true);
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(crate::Action::Wait));
    execution.outcome.unwrap();
    assert!(game.actors[&ActorId(1)].ready_at > ready);
    assert_eq!(game.creature(ActorId(1)).unwrap().costs(), &resources);
    assert_eq!(game.items, items);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.next_item_id, next_item);
    assert_eq!(game.health(ActorId(2)), health);
    assert_eq!(
        game.equipment(ActorId(1)).unwrap()[&EquipmentSlotId(0)],
        launcher
    );
    assert!(game.preparation(ActorId(1)).is_none());
    let replayed = reopened.execute_next_intention().unwrap();
    assert_eq!(replayed.action, Some(crate::Action::Wait));
    replayed.outcome.unwrap();
    assert_eq!(
        game.take_survival_diagnostics().unwrap().actors[&ActorId(1)]
            .projectile_numeric_budget_refusals,
        1
    );
    assert_eq!(
        reopened.take_survival_diagnostics().unwrap().actors[&ActorId(1)]
            .projectile_numeric_budget_refusals,
        1
    );
    assert_eq!(game, reopened);
}

#[test]
fn projectile_ai_fresh_cooldown_skips_request_without_execution_failure() {
    let (mut game, ability, _, _, _) = ai_projectile_fixture();
    let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
    let combat = outcome.state.combat.as_mut().unwrap();
    let mut creature = combat.creature().clone();
    creature.start_cooldown(&ability, 200).unwrap();
    *combat = crate::actor_creatures::CombatState::from_creature(combat.identity(), creature);
    game.apply_outcome(outcome).unwrap();
    assert!(
        !matches!(game.choose_ai(ActorId(1)).unwrap().action, crate::Action::UseAbility { id, .. } if id == ability)
    );
}

#[test]
fn projectile_ai_retains_paid_profile_before_expertise_and_cooldown_gates() {
    let (mut game, ability, selection, _, _) = ai_projectile_fixture();
    trained(&mut game);
    let plan = game
        .ability_plan_with_supplies(
            ActorId(1),
            &ability,
            AbilityInvocation::directional_projectile(StepDirection::East, selection),
            Default::default(),
        )
        .unwrap();
    start_plan(&mut game, &plan);
    game.pause_preparation(ActorId(1)).unwrap();
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build.remove_latest().unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .validate_cooldown(&ability, 200),
        Err(crate::cooldowns::CooldownError::AlreadyCoolingDown),
        "preparation already started the frozen cooldown"
    );
    let before = game.clone();
    let decision = game.choose_ai(ActorId(1)).unwrap();
    assert_eq!(decision.action, plan.work().action());
    assert_eq!(decision.plan, Some(plan));
    assert_eq!(game, before);
}

#[test]
fn projectile_ai_request_does_not_scan_invisible_intervening_occupancy() {
    let (mut game, _, _, _, _) = ai_projectile_fixture();
    let expected = game.choose_ai(ActorId(1)).unwrap().action;
    let blocker = game
        .spawn_actor(
            tor_world::Location {
                position: tor_world::Position { x: 2, y: 1, z: 0 },
                ..game.actors[&ActorId(1)].location
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let build = game.creature(ActorId(2)).unwrap().build().clone();
    game.configure_creature(
        blocker,
        crate::actor_creatures::CreatureIdentity {
            name: "Hidden interceptor".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    let mut outcome = game.actor_outcome(blocker).unwrap();
    let combat = outcome.state.combat.as_mut().unwrap();
    let mut creature = combat.creature().clone();
    creature
        .apply_condition(
            &crate::content_ids::ConditionId::new("fixture_invisible").unwrap(),
            blocker,
            300,
        )
        .unwrap();
    *combat = crate::actor_creatures::CombatState::from_creature(combat.identity(), creature);
    game.apply_outcome(outcome).unwrap();
    assert!(
        game.creature(blocker)
            .unwrap()
            .derived()
            .perception
            .invisible
    );
    assert!(!game
        .observe(ActorId(1))
        .unwrap()
        .visible_actors
        .iter()
        .any(|actor| {
            actor.id == blocker && actor.disclosure == crate::perception::ActorDisclosure::Creature
        }));
    let before = game.clone();
    assert_eq!(game.choose_ai(ActorId(1)).unwrap().action, expected);
    assert_eq!(game, before);
}

#[test]
fn projectile_ai_refusal_diagnostic_is_not_published_when_wait_fails() {
    let (mut game, _, _, _, ammo) = ai_projectile_fixture();
    game.items
        .edit(ammo, |item| {
            item.condition.hit_enchantment = crate::limits::NUMERIC as i32;
        })
        .unwrap();
    let memory = game.actors[&ActorId(1)]
        .behavior_policy()
        .unwrap()
        .memory_ticks;
    assert!(memory > 0);
    game.tick = u64::MAX - memory;
    game.actors.get_mut(&ActorId(1)).unwrap().turn_ticks =
        std::num::NonZeroU64::new(memory + 1).unwrap();
    for id in [ActorId(1), ActorId(2)] {
        let mut actor = game.actors.get_mut(&id).unwrap();
        actor.active_since = game.tick;
        actor.ready_at = game.tick;
    }
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .behavior
        .engagement
        .as_mut()
        .unwrap()
        .last_seen = game.tick;
    let crate::Action::UseAbility {
        id,
        invocation,
        supplies,
    } = game.choose_ai(ActorId(1)).unwrap().action
    else {
        panic!("fixture must still select the same projectile at the exhausted clock");
    };
    assert_eq!(
        game.ability_plan_with_supplies(ActorId(1), &id, invocation, supplies),
        Err(crate::GameError::Rule(RuleError::NumericBudget))
    );
    game.begin_survival_diagnostics(true);
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(1), crate::Action::Wait),
        Err(crate::GameError::Schedule(
            crate::ScheduleError::TimeExhausted
        ))
    );
    assert_eq!(
        game, before,
        "failed fallback must not publish diagnostics or AI state"
    );
    assert!(game
        .take_survival_diagnostics()
        .unwrap()
        .actors
        .values()
        .all(|row| row.projectile_numeric_budget_refusals == 0));
}

#[test]
fn projectile_ai_discovers_cursed_equipped_throw_only_after_paid_attempt() {
    let (mut game, _, item) = paid_throw_fixture(true);
    let target = tor_world::Location {
        position: tor_world::Position { x: 4, y: 1, z: 0 },
        ..game.actors[&ActorId(2)].location
    };
    game.teleport(ActorId(2), target).unwrap();
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .behavior
        .engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(2),
        location: target,
        last_seen: game.tick,
    });
    game.refresh_navigation();
    assert_eq!(game.actors[&ActorId(1)].known_equipment.curse(item), None);
    let decision = game.choose_ai(ActorId(1)).unwrap();
    let crate::Action::UseAbility {
        id,
        invocation,
        supplies,
    } = decision.action
    else {
        panic!("unknown curse must not suppress the available throw");
    };
    let plan = game
        .ability_plan_with_supplies(ActorId(1), &id, invocation, supplies)
        .unwrap();
    start_plan(&mut game, &plan);
    finish_fixture_preparation(&mut game);
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.curse(item),
        Some(true)
    );
    assert_eq!(
        game.equipment(ActorId(1)).unwrap()[&EquipmentSlotId(0)],
        item
    );
    assert!(
        !matches!(game.choose_ai(ActorId(1)).unwrap().action, crate::Action::UseAbility { id: next, .. } if next == id)
    );
}

#[test]
fn remembered_ammunition_bonuses_require_identity_and_exact_numeric_memory() {
    let (mut game, _, _, launcher, ammo) = projectile_launch_fixture();
    let actor = ActorId(1);
    assert!(game.known_projectile_bonus_view(actor, launcher).is_none());
    assert!(game.known_projectile_bonus_view(actor, ammo).is_none());
    game.actors
        .get_mut(&actor)
        .unwrap()
        .known_equipment
        .learn_curse(ammo, true)
        .unwrap();
    assert!(
        game.known_projectile_bonus_view(actor, ammo).is_none(),
        "curse alone is not numeric knowledge"
    );
    let remembered = game.items[&ammo].condition;
    game.actors
        .get_mut(&actor)
        .unwrap()
        .known_equipment
        .learn_numbers(ammo, remembered)
        .unwrap();
    game.items
        .edit(ammo, |item| item.spec.concealed = true)
        .unwrap();
    assert!(
        game.known_projectile_bonus_view(actor, ammo).is_none(),
        "unknown type hides remembered instance values"
    );
    let identity = game.items[&ammo].spec.identity.clone();
    game.actors
        .get_mut(&actor)
        .unwrap()
        .knowledge
        .insert(identity);
    game.items
        .edit(ammo, |item| {
            item.condition.hit_enchantment = -17;
            item.condition.damage_enchantment = 31;
        })
        .unwrap();
    let bonus = game.known_projectile_bonus_view(actor, ammo).unwrap();
    assert_eq!(
        (bonus.hit, bonus.damage),
        (4, 5),
        "hidden changes must not refresh memory"
    );
    let observed = game.observe(actor).unwrap();
    let row = observed
        .interactions
        .unwrap()
        .inventory
        .into_iter()
        .find(|row| row.item == ammo)
        .unwrap();
    assert_eq!(row.known_projectile_bonus, Some(bonus));
    assert_eq!(
        row.known_cursed,
        Some(true),
        "plain ammunition retains independent curse memory"
    );
    assert_eq!(row.known_protected, None);
    assert!(row.slot.is_none());
    assert!(
        row.known_equipment.is_none(),
        "ammunition must not invent equipment"
    );
    for other in game.actors.keys().copied().filter(|other| *other != actor) {
        assert!(game.known_projectile_bonus_view(other, ammo).is_none());
        assert!(game
            .observe(other)
            .unwrap()
            .interactions
            .into_iter()
            .flat_map(|view| view.inventory)
            .all(|row| row.item != ammo));
    }
    game.items
        .edit(ammo, |item| item.spec.projectile = None)
        .unwrap();
    assert!(
        game.known_projectile_bonus_view(actor, ammo).is_none(),
        "numeric memory requires projectile capability"
    );
}

fn detached_source_records(game: &mut Game) -> crate::MemoryRecords {
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(&crate::RegionTransition::default(), &mut records)
        .unwrap();
    assert!(!game.has_actor(ActorId(1)));
    records
}

fn edit_detached_source_plan(
    game: &Game,
    original: &mut crate::MemoryRecords,
    edit: impl Fn(&mut Plan),
) -> crate::MemoryRecords {
    use crate::streaming::RecordStore;
    let mut records = crate::MemoryRecords::default();
    for (_, id) in game.detached_records() {
        let record = original.get(id).unwrap();
        let mut value = serde_json::to_value(crate::RegionRecordData::capture(&record)).unwrap();
        if value["actors"].get("1").is_some() {
            let encoded = &mut value["actors"]["1"]["pending"]["plan"];
            let mut plan: Plan = serde_json::from_value(encoded.clone()).unwrap();
            edit(&mut plan);
            assert!(
                plan.valid(game.content.rules()),
                "isolate source validation"
            );
            *encoded = serde_json::to_value(plan).unwrap();
        }
        let restored = serde_json::from_value::<crate::RegionRecordData>(value)
            .unwrap()
            .restore(game.content().clone())
            .unwrap();
        records.put(id, tor_world::Shared::new(restored));
    }
    records
}

fn all_source_fixture_regions() -> crate::RegionTransition {
    let regions = [tor_world::RegionId(1), tor_world::RegionId(2)]
        .into_iter()
        .collect();
    crate::RegionTransition {
        active: regions,
        loaded: [tor_world::RegionId(1), tor_world::RegionId(2)]
            .into_iter()
            .collect(),
    }
}

fn assert_detached_source_rejected(game: &Game, records: &mut crate::MemoryRecords) {
    assert!(!game.detached_records_valid(records));
    let mut candidate = game.clone();
    assert_eq!(
        candidate.transition_regions(&all_source_fixture_regions(), records),
        Err(crate::TransitionError::InvalidRecord(tor_world::RegionId(
            1
        )))
    );
    assert_eq!(candidate, *game, "malformed arrival must remain atomic");
}

#[test]
fn detached_equip_sources_bind_retained_anatomy_owner_and_surviving_profiles() {
    for configured in [false, true] {
        let mut game = fixture(|_| {});
        equip(&mut game, 0, false);
        let item = equip(&mut game, 10, false);
        stow_fixture_weapon(&mut game, item);
        if configured {
            game.actors.get_mut(&ActorId(1)).unwrap().combat = None;
        }
        let plan = game
            .plan_work(
                ActorId(1),
                crate::Work::Equip {
                    item,
                    slot: EquipmentSlotId(10),
                },
            )
            .unwrap();
        start_plan(&mut game, &plan);
        let pending = game.preparation(ActorId(1)).unwrap().clone();
        let mut original = detached_source_records(&mut game);
        let mut records = edit_detached_source_plan(&game, &mut original, |_| {});
        assert!(game.detached_records_valid(&mut records));
        for target in [false, true] {
            let mut damaged = edit_detached_source_plan(&game, &mut original, |plan| {
                let Plan::Equip { allocation, .. } = plan else {
                    panic!("Equip")
                };
                let profile = if target {
                    &mut allocation.target
                } else {
                    &mut allocation.occupants[0].profile
                };
                let CapturedWeaponProfile::Profile { spec, .. } = profile else {
                    panic!("profile")
                };
                spec.expert_traits.add.clear();
            });
            assert_detached_source_rejected(&game, &mut damaged);
        }
        let mut alias = edit_detached_source_plan(&game, &mut original, |plan| {
            let Plan::Equip { actor, .. } = plan else {
                panic!("Equip")
            };
            *actor = ActorId(2);
        });
        assert_detached_source_rejected(&game, &mut alias);
        if configured {
            let mut damaged = edit_detached_source_plan(&game, &mut original, |plan| {
                let Plan::Equip { allocation, .. } = plan else {
                    panic!("Equip")
                };
                let crate::equipment_capture::EquipAnatomySource::Configured { anatomy } =
                    &mut allocation.anatomy
                else {
                    panic!("configured anatomy")
                };
                anatomy.primary_attack_socket =
                    crate::equipment_capacity::PrimaryAttackSocket::None;
            });
            assert_detached_source_rejected(&game, &mut damaged);
        }
        game.transition_regions(&all_source_fixture_regions(), &mut records)
            .unwrap();
        assert_eq!(game.preparation(ActorId(1)), Some(&pending));
        finish_fixture_preparation(&mut game);
        assert_eq!(
            game.equipment(ActorId(1))
                .unwrap()
                .get(&EquipmentSlotId(10)),
            Some(&item)
        );
    }
}

#[test]
fn merged_equip_source_context_never_falls_back_and_retains_deleted_item_semantics() {
    let mut game = fixture(|_| {});
    let item = equip(&mut game, 0, false);
    stow_fixture_weapon(&mut game, item);
    game.actors.get_mut(&ActorId(1)).unwrap().combat = None;
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Equip {
                item,
                slot: EquipmentSlotId(0),
            },
        )
        .unwrap();
    let actors = game.actors.iter().map(|(id, actor)| (*id, actor)).collect();
    let mut items: std::collections::BTreeMap<_, _> =
        game.items.iter().map(|(id, item)| (*id, item)).collect();
    let absent_actors = Default::default();
    assert!(!game.plan_source_valid_with_context(
        &plan,
        PlanSourceContext::Records {
            actors: &absent_actors,
            items: &items
        },
    ));
    let mut historical = plan.clone();
    let Plan::Equip { allocation, .. } = &mut historical else {
        panic!("Equip")
    };
    let CapturedWeaponProfile::Profile { spec, .. } = &mut allocation.target else {
        panic!("profile")
    };
    spec.expert_traits.add.clear();
    assert!(!game.plan_source_valid(&historical));
    items.remove(&item);
    assert!(game.plan_source_valid_with_context(
        &historical,
        PlanSourceContext::Records {
            actors: &actors,
            items: &items
        },
    ));
    let mut forged = plan;
    let Plan::Equip { item, .. } = &mut forged else {
        panic!("Equip")
    };
    *item = ItemId(game.next_item_id);
    assert!(!game.plan_source_valid_with_context(
        &forged,
        PlanSourceContext::Records {
            actors: &actors,
            items: &items
        },
    ));
}

#[test]
fn detached_projectile_sources_retain_generation_and_exact_weapon_witnesses() {
    for (qualified, carried) in [(false, false), (true, false), (false, true), (true, true)] {
        let (mut game, mut captured) =
            projectile_throw_fixture(qualified.then(projectile_qualification));
        if carried {
            let item = captured.source.item().unwrap();
            stow_fixture_weapon(&mut game, item);
            captured = game
                .capture_projectile_plan(
                    ActorId(1),
                    &captured.ability,
                    captured.direction,
                    ProjectileDeliverySelection::Throw {
                        source: ThrownSourceSelection::Carried {
                            item,
                            socket: EquipmentSlotId(0),
                        },
                    },
                )
                .unwrap();
        }
        let plan = Plan::Projectile(tor_world::Shared::new(captured));
        start_plan(&mut game, &plan);
        let pending = game.preparation(ActorId(1)).unwrap().clone();
        let next_generation = game
            .creature(ActorId(1))
            .unwrap()
            .conditions()
            .next_generation();
        let mut original = detached_source_records(&mut game);
        let mut records = edit_detached_source_plan(&game, &mut original, |_| {});
        assert!(game.detached_records_valid(&mut records));
        let mut future = edit_detached_source_plan(&game, &mut original, |plan| {
            let Plan::Projectile(plan) = plan else {
                panic!("Projectile")
            };
            if qualified {
                plan.conditional_tokens[0].generation = next_generation;
            } else {
                plan.tokens[0].reservation.generation = next_generation;
            }
        });
        assert_detached_source_rejected(&game, &mut future);
        let mut profile = edit_detached_source_plan(&game, &mut original, |plan| {
            let Plan::Projectile(plan) = plan else {
                panic!("Projectile")
            };
            let profile = match &mut plan.source {
                ProjectileSource::ThrowEquipped {
                    weapon: FrozenWeaponSelection::Equipped { profile, .. },
                } => profile,
                ProjectileSource::ThrowCarried { allocation, .. } => &mut allocation.target,
                _ => panic!("throw"),
            };
            let CapturedWeaponProfile::Profile { spec, .. } = profile else {
                panic!("profile")
            };
            spec.expert_traits.add.clear();
        });
        assert_detached_source_rejected(&game, &mut profile);
        game.transition_regions(&all_source_fixture_regions(), &mut records)
            .unwrap();
        assert_eq!(game.preparation(ActorId(1)), Some(&pending));
    }
}

#[test]
fn detached_launcher_source_rejects_a_forged_surviving_ammunition_payload() {
    let (mut game, ability, selection, _, _) = projectile_launch_fixture();
    let plan = game
        .ability_plan_with_supplies(
            ActorId(1),
            &ability,
            AbilityInvocation::directional_projectile(StepDirection::East, selection),
            Default::default(),
        )
        .unwrap();
    start_plan(&mut game, &plan);
    let mut original = detached_source_records(&mut game);
    let mut records = edit_detached_source_plan(&game, &mut original, |_| {});
    assert!(game.detached_records_valid(&mut records));
    let mut damaged = edit_detached_source_plan(&game, &mut original, |plan| {
        let Plan::Projectile(plan) = plan else {
            panic!("Projectile")
        };
        let ProjectileSource::Launch { payload, .. } = &mut plan.source else {
            panic!("Launch")
        };
        payload.recovery = crate::projectile_profiles::HitRecovery::Recoverable;
    });
    assert_detached_source_rejected(&game, &mut damaged);
}

fn ranged_loot_choice(game: &Game) -> Option<crate::Action> {
    let actor = ActorId(1);
    let view = game.observe(actor).unwrap();
    game.choose_nearby_loot(
        actor,
        &view,
        &crate::ai::AiProfile::general(),
        &[],
        &mut |_| None,
    )
}

#[test]
fn ranged_loot_collects_only_six_known_compatible_shots_across_stacks() {
    let (mut game, _, _, _, ammo) = projectile_launch_fixture();
    let actor = ActorId(1);
    let at = game.actors[&actor].location;
    let payload = (*game.items[&ammo].spec).clone();
    let ground = game
        .spawn_item_stack(at, None, 20, payload.clone())
        .unwrap();
    assert_eq!(
        ranged_loot_choice(&game),
        Some(crate::Action::Take {
            item: ground,
            quantity: Some(4),
        })
    );
    game.spawn_item_stack(at, Some(actor), 4, payload).unwrap();
    assert_eq!(ranged_loot_choice(&game), None);
}

#[test]
fn ranged_loot_unknown_payload_values_and_incompatible_families_do_not_steer_pickup() {
    let (mut game, _, _, launcher, ammo) = projectile_launch_fixture();
    let at = game.actors[&ActorId(1)].location;
    let mut payload = (*game.items[&ammo].spec).clone();
    payload.projectile.as_mut().unwrap().family =
        crate::content_ids::LauncherFamilyId::new("wrong.family").unwrap();
    game.spawn_item_stack(at, None, 20, payload).unwrap();
    assert_eq!(ranged_loot_choice(&game), None);
    let ground = game
        .spawn_item_stack(at, None, 20, (*game.items[&ammo].spec).clone())
        .unwrap();
    let expected = ranged_loot_choice(&game);
    assert_eq!(
        expected,
        Some(crate::Action::Take {
            item: ground,
            quantity: Some(4)
        })
    );
    game.items
        .edit(ground, |item| {
            item.condition.hit_enchantment = -100;
            item.condition.damage_enchantment = 100;
            item.condition.cursed = true;
        })
        .unwrap();
    assert_eq!(ranged_loot_choice(&game), expected);
    game.items
        .edit(launcher, |item| {
            item.condition.hit_enchantment = -100;
            item.condition.damage_enchantment = 100;
            item.condition.cursed = true;
        })
        .unwrap();
    assert_eq!(ranged_loot_choice(&game), expected);
}

fn clear_ranged_fixture_equipment(game: &mut Game) {
    let mut owner = game.actors.get_mut(&ActorId(1)).unwrap();
    owner.equipment.clear();
    owner
        .combat
        .as_mut()
        .unwrap()
        .project_equipment_immediately_in_context(vec![], None)
        .unwrap();
}

#[test]
fn ranged_loot_bootstraps_a_disclosed_pair_without_free_equipment() {
    let (mut game, _, _, launcher, ammo) =
        projectile_launch_fixture_with(|def| def.ai.priority = 100);
    let actor = ActorId(1);
    let at = game.actors[&actor].location;
    clear_ranged_fixture_equipment(&mut game);
    for id in [launcher, ammo] {
        game.items
            .edit(id, |item| item.location = crate::ItemLocation::Ground(at))
            .unwrap();
    }
    let before = game.clone();
    assert_eq!(
        ranged_loot_choice(&game),
        Some(crate::Action::Take {
            item: launcher,
            quantity: Some(1)
        })
    );
    assert_eq!(game, before, "preview neither equips nor collects the pair");
    game.items
        .edit(launcher, |item| {
            item.location = crate::ItemLocation::Carried(actor)
        })
        .unwrap();
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        None,
        "empty launcher waits for payload"
    );
    assert_eq!(
        ranged_loot_choice(&game),
        Some(crate::Action::Take {
            item: ammo,
            quantity: Some(2)
        })
    );
    game.items
        .edit(ammo, |item| {
            item.location = crate::ItemLocation::Carried(actor)
        })
        .unwrap();
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        Some(crate::Action::Equip {
            item: launcher,
            slot: EquipmentSlotId(0),
        })
    );
    game.equip_authored(actor, launcher, EquipmentSlotId(0))
        .unwrap();
    assert_eq!(game.choose_gear(actor, &game.observe(actor).unwrap()), None);
    assert_eq!(ranged_loot_choice(&game), None);
}

#[test]
fn ranged_loot_respects_idle_equipment_knowledge_and_inventory_bounds() {
    let (mut game, _, _, _, ammo) = projectile_launch_fixture();
    let actor = ActorId(1);
    let at = game.actors[&actor].location;
    let mut payload = (*game.items[&ammo].spec).clone();
    payload.concealed = true;
    payload.identity = "unknown projectile".into();
    payload.appearance = "unfamiliar shaft".into();
    let ground = game.spawn_item_stack(at, None, 20, payload).unwrap();
    assert_eq!(ranged_loot_choice(&game), None);
    game.identify_item(actor, ground).unwrap();
    assert_eq!(
        ranged_loot_choice(&game),
        Some(crate::Action::Take {
            item: ground,
            quantity: Some(4)
        })
    );
    let view = game.observe(actor).unwrap();
    let mut profile = crate::ai::AiProfile::general();
    profile.equipment = false;
    assert_eq!(
        game.choose_nearby_loot(actor, &view, &profile, &[], &mut |_| None),
        None
    );
    profile.equipment = true;
    profile.idle = crate::behavior::IdleBehavior::Wait;
    assert!(game
        .choose_nearby_loot(actor, &view, &profile, &[], &mut |_| None)
        .is_some());
    for _ in 0..128 {
        game.spawn_item_stack(
            at,
            Some(actor),
            1,
            crate::ItemSpec::ordinary("clutter".into()),
        )
        .unwrap();
    }
    assert_eq!(
        ranged_loot_choice(&game),
        None,
        "a bounded scan must not undercount the reserve"
    );
}

fn ranged_trade_fixture(priority: i16) -> (Game, ItemId, ItemId, ItemId) {
    let (mut game, _, _, launcher, ammo) =
        projectile_launch_fixture_with(|def| def.ai.priority = priority);
    clear_ranged_fixture_equipment(&mut game);
    let mut spec = (*game.items[&launcher].spec).clone();
    let gear = spec.equipment.as_mut().unwrap();
    gear.weapon_profile.as_mut().unwrap().base_traits = vec![WeaponTrait::TwoHanded];
    let component = crate::damage::DamageComponent::fixed(crate::combat::DamageType::Keen, None, 1);
    let primary = component.key();
    gear.attack = Some(
        crate::attacks::MeleeAttack::new(
            crate::attributes::Skill::HeavyWeaponry,
            0,
            10,
            20,
            crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
        )
        .unwrap(),
    );
    game.items
        .edit(launcher, |item| item.spec = tor_world::Shared::new(spec))
        .unwrap();
    let melee = equip(&mut game, 0, false);
    (game, launcher, ammo, melee)
}

#[test]
fn ranged_gear_uses_authored_priority_not_expertise_and_stops_replacement_oscillation() {
    let actor = ActorId(1);
    let (mut conservative, _, _, _) = ranged_trade_fixture(0);
    assert_eq!(
        conservative.choose_gear(actor, &conservative.observe(actor).unwrap()),
        None
    );
    trained(&mut conservative);
    assert_eq!(
        conservative.choose_gear(actor, &conservative.observe(actor).unwrap()),
        None,
        "expertise grants capability, not ranged preference"
    );
    let (mut game, launcher, ammo, melee) = ranged_trade_fixture(100);
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        Some(crate::Action::Unequip { item: melee })
    );
    clear_ranged_fixture_equipment(&mut game);
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        Some(crate::Action::Equip {
            item: launcher,
            slot: EquipmentSlotId(0),
        })
    );
    game.equip_authored(actor, launcher, EquipmentSlotId(0))
        .unwrap();
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        None,
        "a preferred loaded launcher must not immediately be replaced by melee"
    );
    let at = game.actors[&actor].location;
    game.items
        .edit(ammo, |item| item.location = crate::ItemLocation::Ground(at))
        .unwrap();
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        Some(crate::Action::Unequip { item: launcher }),
        "depleted carried reserve permits safe return to better melee"
    );
}

#[test]
fn ranged_gear_unknown_conditions_do_not_change_choice_and_known_curse_blocks_removal() {
    let actor = ActorId(1);
    let (mut game, launcher, ammo, melee) = ranged_trade_fixture(100);
    let expected = game.choose_gear(actor, &game.observe(actor).unwrap());
    assert_eq!(expected, Some(crate::Action::Unequip { item: melee }));
    for id in [launcher, ammo, melee] {
        game.items
            .edit(id, |item| {
                item.condition.hit_enchantment = 100;
                item.condition.damage_enchantment = -100;
                item.condition.cursed = true;
            })
            .unwrap();
    }
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        expected
    );
    game.actors
        .get_mut(&actor)
        .unwrap()
        .known_equipment
        .learn_curse(melee, true)
        .unwrap();
    assert_eq!(game.choose_gear(actor, &game.observe(actor).unwrap()), None);
}

#[test]
fn ranged_gear_pair_scan_reuses_one_workspace_and_never_compiles_projectiles() {
    let (mut game, _, _, launcher, _) = projectile_launch_fixture_with(|def| def.ai.priority = 100);
    let actor = ActorId(1);
    let at = game.actors[&actor].location;
    clear_ranged_fixture_equipment(&mut game);
    let mut spec = (*game.items[&launcher].spec).clone();
    spec.equipment.as_mut().unwrap().modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Defense {
                defense: DefenseKind::Physical,
            },
            value: -1,
        },
    });
    game.items
        .edit(launcher, |item| {
            item.spec = tor_world::Shared::new(spec.clone())
        })
        .unwrap();
    for _ in 0..31 {
        game.spawn_item_stack(at, Some(actor), 1, spec.clone())
            .unwrap();
    }
    let view = game.observe(actor).unwrap();
    let before = crate::diagnostics::work_counts();
    assert_eq!(
        game.choose_ranged_gear(actor, &view),
        None,
        "ranged preference cannot buy lower defense"
    );
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.game_clones - before.game_clones, 1);
    assert_eq!(after.ability_plans - before.ability_plans, 0);
    assert!(
        after.dynamic_creature_folds - before.dynamic_creature_folds <= 16 * (1 + 2 * 2),
        "sixteen candidates, one baseline and at most two two-phase weapon socket projections each"
    );
}

#[test]
fn ranged_gear_conservative_two_grip_secondary_preserves_natural_melee() {
    let (mut game, launcher, _, melee) = ranged_trade_fixture(0);
    let actor = ActorId(1);
    clear_ranged_fixture_equipment(&mut game);
    let at = game.actors[&actor].location;
    // Keep the existing melee weapon away from this unarmed fixture. Its
    // two-grip body and ordinary Focus pool are the production humanoid shape.
    game.items
        .edit(melee, |item| {
            item.location = crate::ItemLocation::Ground(at)
        })
        .unwrap();
    let mut spec = (*game.items[&launcher].spec).clone();
    spec.equipment.as_mut().unwrap().attack = Some(
        crate::attacks::MeleeAttack::fixed(
            crate::attributes::Skill::HeavyWeaponry,
            -20,
            10,
            20,
            crate::combat::DamageType::Keen,
            None,
            1,
        )
        .unwrap(),
    );
    game.items
        .edit(launcher, |item| item.spec = tor_world::Shared::new(spec))
        .unwrap();
    let before = game.selected_melee_attack(actor).unwrap().clone();
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        Some(crate::Action::Equip {
            item: launcher,
            slot: EquipmentSlotId(10),
        })
    );
    game.equip_authored(actor, launcher, EquipmentSlotId(10))
        .unwrap();
    assert_eq!(game.selected_melee_attack(actor), Some(&before));
    assert!(game.ranged_loadout_has_budget(actor, launcher, EquipmentSlotId(10)));
}

#[test]
fn ranged_gear_depleted_secondary_releases_grip_for_a_better_carried_melee() {
    let (mut game, launcher, ammo, melee) = ranged_trade_fixture(0);
    let actor = ActorId(1);
    clear_ranged_fixture_equipment(&mut game);
    game.equip_authored(actor, launcher, EquipmentSlotId(10))
        .unwrap();
    let at = game.actors[&actor].location;
    game.items
        .edit(ammo, |item| item.location = crate::ItemLocation::Ground(at))
        .unwrap();
    // The carried melee alternative is strengthened explicitly in its known
    // archetype, not through an undisclosed instance enchantment.
    let mut spec = (*game.items[&melee].spec).clone();
    let attack = game.selected_melee_attack(actor).unwrap();
    spec.equipment.as_mut().unwrap().attack = Some(
        crate::attacks::MeleeAttack::new(
            attack.skill(),
            20,
            1,
            1,
            attack.damage_with_modifiers(0, 100, 0).unwrap(),
        )
        .unwrap(),
    );
    game.items
        .edit(melee, |item| item.spec = tor_world::Shared::new(spec))
        .unwrap();
    assert_eq!(
        game.choose_gear(actor, &game.observe(actor).unwrap()),
        Some(crate::Action::Unequip { item: launcher })
    );
}

fn settle_ranged_ai_turn(game: &mut Game, actor: ActorId) {
    for _ in 0..32 {
        if game.next_actor() == Some(actor) && game.preparation(actor).is_none() {
            return;
        }
        game.act(game.next_actor().unwrap(), crate::Action::Wait)
            .unwrap();
    }
    panic!("bounded fixture did not reach the next ready AI turn");
}

#[test]
fn ranged_ai_acquires_equips_reopens_and_fires_through_ordinary_intentions() {
    let (mut game, ability, _, launcher, ammo) =
        projectile_launch_fixture_with(|def| def.ai.priority = 100);
    let actor = ActorId(1);
    let at = game.actors[&actor].location;
    clear_ranged_fixture_equipment(&mut game);
    for id in [launcher, ammo] {
        game.items
            .edit(id, |item| item.location = crate::ItemLocation::Ground(at))
            .unwrap();
    }
    game.configure_ai(
        actor,
        crate::ai::AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            ..crate::ai::AiProfile::general()
        },
    )
    .unwrap();
    game.refresh_navigation();
    for item in [launcher, ammo] {
        settle_ranged_ai_turn(&mut game, actor);
        let expected = crate::Action::Take {
            item,
            quantity: Some(if item == launcher { 1 } else { 2 }),
        };
        assert_eq!(game.next_ai_action(), Some((actor, expected.clone())));
        game.admit_ai_intention(actor).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(expected));
        execution.outcome.unwrap();
    }
    settle_ranged_ai_turn(&mut game, actor);
    assert_eq!(
        game.next_ai_action(),
        Some((
            actor,
            crate::Action::Equip {
                item: launcher,
                slot: EquipmentSlotId(0)
            }
        ))
    );
    game.admit_ai_intention(actor).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.equipment(actor).unwrap()[&EquipmentSlotId(0)],
        launcher
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.preparation(actor), game.preparation(actor));
    settle_ranged_ai_turn(&mut restored, actor);
    assert_eq!(
        restored.equipment(actor).unwrap()[&EquipmentSlotId(0)],
        launcher
    );
    let target = tor_world::Location {
        position: tor_world::Position { x: 4, y: 1, z: 0 },
        ..at
    };
    restored.teleport(ActorId(2), target).unwrap();
    restored.actors.get_mut(&actor).unwrap().behavior.engagement =
        Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(2),
            location: target,
            last_seen: restored.tick,
        });
    restored.refresh_navigation();
    let expected = crate::Action::UseAbility {
        id: ability,
        invocation: AbilityInvocation::directional_projectile(
            StepDirection::East,
            ProjectileDeliverySelection::Launch {
                launcher: EquippedProjectileWeapon {
                    item: launcher,
                    socket: EquipmentSlotId(0),
                },
                ammunition: ammo,
            },
        ),
        supplies: Default::default(),
    };
    assert_eq!(restored.next_ai_action(), Some((actor, expected.clone())));
    restored.admit_ai_intention(actor).unwrap();
    let execution = restored.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(expected));
    execution.outcome.unwrap();
    settle_ranged_ai_turn(&mut restored, actor);
    assert_eq!(
        restored
            .observe(actor)
            .unwrap()
            .inventory
            .iter()
            .find(|item| item.id == ammo)
            .unwrap()
            .quantity,
        1,
        "one actual release consumes one carried unit after restoring the acquired equipment"
    );
}

#[test]
fn ranged_loot_requires_fundable_maxima_and_retains_existing_capacity_deficits() {
    let (mut impossible, _, _, _, ammo) = projectile_launch_fixture_with(|def| {
        def.cost = Some(Cost {
            resource: Resource::Focus,
            start: 21,
            resolution: 0,
        });
    });
    let actor = ActorId(1);
    let at = impossible.actors[&actor].location;
    impossible
        .spawn_item_stack(at, None, 6, (*impossible.items[&ammo].spec).clone())
        .unwrap();
    assert_eq!(ranged_loot_choice(&impossible), None);
    let (mut game, _, _, _, ammo) = projectile_launch_fixture();
    trained(&mut game);
    equip(&mut game, 10, true);
    let mut build = game.creature(actor).unwrap().build().clone();
    build.remove_latest().unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert!(!game
        .creature(actor)
        .unwrap()
        .derived()
        .equipment_capacity_deficits
        .is_empty());
    let at = game.actors[&actor].location;
    game.spawn_item_stack(at, None, 6, (*game.items[&ammo].spec).clone())
        .unwrap();
    let before = game.clone();
    assert_eq!(game.choose_gear(actor, &game.observe(actor).unwrap()), None);
    assert_eq!(ranged_loot_choice(&game), None);
    assert_eq!(
        game, before,
        "current deficits neither drop existing gear nor admit new draws"
    );
}

#[test]
fn slow_quickdraw_bypasses_unused_large_equip_duration_but_ordinary_draw_rejects() {
    let mut game = fixture_sources(
        |_| {},
        |_, species| {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::ActionDuration {
                    value: crate::action_duration::ActionDuration::Slow,
                },
            });
        },
    );
    let actor = ActorId(1);
    let item = equip(&mut game, 0, false);
    stow_fixture_weapon(&mut game, item);
    let turns = game
        .content
        .rules()
        .equipment_slot(&crate::EquipmentSlot::new("weapon").unwrap())
        .unwrap()
        .equip_turns;
    assert_eq!(
        turns, 1,
        "this boundary witness uses the ordinary weapon draw"
    );
    game.actors.get_mut(&actor).unwrap().turn_ticks =
        std::num::NonZeroU64::new(crate::limits::NUMERIC as u64 / 2 + 1).unwrap();
    let work = crate::Work::Equip {
        item,
        slot: EquipmentSlotId(0),
    };
    let before = game.clone();
    assert!(matches!(
        game.plan_work(actor, work.clone()),
        Err(crate::GameError::Build(
            crate::creatures::BuildError::DerivedLimit
        ))
    ));
    assert_eq!(
        game, before,
        "ordinary over-bound equipment work rejects atomically"
    );
    let mut spec = (*game.items.get(&item).unwrap().spec).clone();
    spec.equipment
        .as_mut()
        .unwrap()
        .weapon_profile
        .as_mut()
        .unwrap()
        .base_traits
        .push(WeaponTrait::Quickdraw);
    game.items
        .edit(item, |source| source.spec = tor_world::Shared::new(spec))
        .unwrap();
    let before = game.clone();
    let plan = game
        .plan_work(actor, work.clone())
        .expect("quickdraw does not use the scaled equip duration");
    assert_eq!(game, before);
    assert_eq!(plan.preparation(), 0);
    assert_eq!(plan.recovery(), 0);
    assert!(plan.valid(game.content.rules()));
    let restored: Plan = serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(restored, plan);
    let tick = game.tick();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = tick;
    let intention = game
        .admit_intention(actor, work.action(), crate::IntentionOrigin::Human)
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    execution.outcome.unwrap();
    assert_eq!(game.tick(), tick);
    assert_eq!(
        game.equipment(actor).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
    assert!(
        game.plan_work(actor, crate::Work::Unequip { item })
            .is_err(),
        "quickdraw never makes sheathing free"
    );
}
#[test]
fn projectile_attack_bonus_augments_only_rolled_pool_once() {
    let (mut game, launch, selection, launcher, ammo) = projectile_launch_fixture();
    let rolled = crate::damage::DamageComponent::rolled(
        crate::combat::DamageType::Keen,
        None,
        crate::dice::DicePool::new(2, 6, 0).unwrap(),
    );
    let primary = rolled.key();
    let damage = crate::damage::DamageSpec::new(
        vec![
            rolled,
            crate::damage::DamageComponent::fixed(crate::combat::DamageType::Vital, None, 3),
        ],
        Some(primary),
    )
    .unwrap();
    game.items
        .edit(ammo, |item| {
            item.spec.projectile.as_mut().unwrap().damage = damage
        })
        .unwrap();
    let baseline = game
        .capture_projectile_plan(ActorId(1), &launch, StepDirection::East, selection)
        .unwrap();
    game.items
        .edit(launcher, |item| {
            item.spec
                .equipment
                .as_mut()
                .unwrap()
                .modifiers
                .push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::AttackDamageRollBonus,
                        value: 2,
                    },
                })
        })
        .unwrap();
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .equipment
        .remove(&EquipmentSlotId(0));
    game.equip_authored(ActorId(1), launcher, EquipmentSlotId(0))
        .unwrap();
    let captured = game
        .capture_projectile_plan(ActorId(1), &launch, StepDirection::East, selection)
        .unwrap();
    let [PlannedEffect::Damage { damage: before }] = baseline.effects.as_slice() else {
        panic!("before")
    };
    let [PlannedEffect::Damage { damage: after }] = captured.effects.as_slice() else {
        panic!("after")
    };
    let mut rolled_count = 0;
    let mut fixed_count = 0;
    for (before, after) in before.components().iter().zip(after.components()) {
        match (before.amount(), after.amount()) {
            (crate::damage::DamageAmount::Rolled(a), crate::damage::DamageAmount::Rolled(b)) => {
                assert_eq!(b.bonus(), a.bonus() + 2);
                assert_eq!(a.count(), b.count());
                rolled_count += 1;
            }
            (crate::damage::DamageAmount::Fixed(a), crate::damage::DamageAmount::Fixed(b)) => {
                assert_eq!(a, b);
                fixed_count += 1;
            }
            _ => panic!("shape changed"),
        }
    }
    assert_eq!((rolled_count, fixed_count), (1, 1));
}
