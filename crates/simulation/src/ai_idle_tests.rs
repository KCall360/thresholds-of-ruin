//! Actor-owned idle decisions, bounded interests and snapshot-sharing contracts.
use super::*;
use crate::{behavior::IdleBehavior, ItemId, ItemSpec};
use tor_world::{Position, RegionId};

fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}
fn fixture(profile: AiProfile) -> Game {
    fixture_with_rules(profile, crate::test_creatures::ruleset())
}
fn fixture_with_rules(profile: AiProfile, rules: std::sync::Arc<crate::rules::Ruleset>) -> Game {
    let species = crate::test_creatures::species();
    let mut game = super::tests::configured_game(species.clone(), species, rules);
    game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
    game.set_region_light(RegionId(2), false).unwrap();
    game.configure_ai(ActorId(2), profile).unwrap();
    game.refresh_navigation();
    game
}
fn potion(name: &str, concealed: bool) -> ItemSpec {
    let mut item = ItemSpec::ordinary(name.into());
    item.class = crate::ItemClass::Potion;
    item.concealed = concealed;
    item.stackable = true;
    item.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![crate::rules::Effect::Heal {
            amount: crate::rules::AmountExpr::Fixed { amount: 2 },
        }],
    });
    item
}
fn loot(game: &Game) -> Option<Action> {
    let view = game.observe(ActorId(2)).unwrap();
    let mut search = game.route_search(ActorId(2)).unwrap();
    game.choose_nearby_loot(
        ActorId(2),
        &view,
        &game.combat.ai[&ActorId(2)].profile,
        &[],
        &mut |at| search.route(at).ok(),
    )
}

#[test]
fn idle_wait_forbids_loot_detours_but_permits_underfoot_useful_pickup() {
    let mut game = fixture(AiProfile {
        idle: IdleBehavior::Wait,
        ..AiProfile::general()
    });
    game.place_item_stack(20, at(1, 3, 1), None, 9, potion("known", false))
        .unwrap();
    game.refresh_navigation();
    assert_eq!(loot(&game), None);
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
    game.place_item_stack(21, at(1, 2, 1), None, 9, potion("near", false))
        .unwrap();
    assert_eq!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(21),
            quantity: Some(1)
        })
    );
}

#[test]
fn loot_respects_care_and_equipment_overrides_independently() {
    let mut game = fixture(AiProfile {
        care: false,
        equipment: false,
        ..AiProfile::general()
    });
    game.place_item_stack(20, at(1, 2, 1), None, 4, potion("known", false))
        .unwrap();
    assert_eq!(loot(&game), None);
    game.combat.ai.get_mut(&ActorId(2)).unwrap().profile.care = true;
    assert_eq!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(20),
            quantity: Some(1)
        })
    );
    let mut armor = ItemSpec::ordinary("known armor".into());
    armor.class = crate::ItemClass::Armor;
    armor.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: Some(crate::ArmorSpec {
            physical_reduction: 2,
            material: crate::ArmorMaterial::Metal,
            rustproof: false,
        }),
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        attack: None,
        modifiers: vec![],
    });
    game.place_item_stack(21, at(1, 2, 1), None, 1, armor)
        .unwrap();
    let profile = &mut game.combat.ai.get_mut(&ActorId(2)).unwrap().profile;
    profile.care = false;
    assert_eq!(loot(&game), None);
    game.combat
        .ai
        .get_mut(&ActorId(2))
        .unwrap()
        .profile
        .equipment = true;
    assert_eq!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(21),
            quantity: Some(1)
        })
    );
}

#[test]
fn unknown_item_semantics_and_unseen_piles_never_change_idle_choices() {
    let mut game = fixture(AiProfile::general());
    let mut other = game.clone();
    game.place_item_stack(20, at(1, 2, 1), None, 1, potion("unknown", true))
        .unwrap();
    let mut unknown = potion("unknown", true);
    unknown.consumable.as_mut().unwrap().effects = vec![crate::rules::Effect::RestoreAttribute {
        attribute: crate::attributes::Attribute::Strength,
        amount: crate::rules::AmountExpr::Fixed { amount: 1 },
    }];
    other
        .place_item_stack(20, at(1, 2, 1), None, 1, unknown)
        .unwrap();
    other
        .place_item_stack(21, at(2, 3, 2), None, 1, potion("hidden", false))
        .unwrap();
    assert!(
        game.observe(ActorId(2)) == other.observe(ActorId(2)),
        "hidden facts must not change the own observation"
    );
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        other.choose_ai(ActorId(2)).unwrap().action
    );
    assert_eq!(loot(&game), None);
    game.identify_item(ActorId(2), ItemId(20)).unwrap();
    assert!(matches!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(20),
            ..
        })
    ));
}

#[test]
fn repeated_previews_leave_visits_rng_and_time_unchanged() {
    let game = fixture(AiProfile::general());
    let before = game.clone();
    let first = game.choose_ai(ActorId(2)).unwrap();
    for _ in 0..8 {
        let next = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(next.action, first.action);
        assert_eq!(next.ai, first.ai);
        assert_eq!(next.behavior, first.behavior);
    }
    assert_eq!(game, before);
}

fn fill_history(game: &mut Game) {
    for region in [1, 2] {
        for x in 1..=4 {
            for y in 1..=2 {
                game.combat
                    .ai
                    .get_mut(&ActorId(2))
                    .unwrap()
                    .visits
                    .insert(at(region, x, y), 64);
            }
        }
    }
}
fn shares_region(a: &Game, b: &Game, region: u64) -> bool {
    let bucket = |g: &Game| {
        g.combat.ai[&ActorId(2)]
            .visits
            .shared_regions()
            .find(|(id, _)| **id == RegionId(region))
            .unwrap()
            .1
            .clone()
    };
    bucket(a).shares_storage(&bucket(b))
}

#[test]
fn ordinary_player_wait_shares_mature_ai_histories_without_copying_entries() {
    let mut game = fixture(AiProfile::general());
    fill_history(&mut game);
    let before = game.clone();
    assert!(shares_region(&game, &before, 1));
    assert!(shares_region(&game, &before, 2));
    game.act(ActorId(1), Action::Wait).unwrap();
    assert!(shares_region(&game, &before, 1));
    assert!(shares_region(&game, &before, 2));
    assert_eq!(
        game.combat.ai[&ActorId(2)].visits,
        before.combat.ai[&ActorId(2)].visits
    );
}

#[test]
fn one_idle_decision_copies_only_its_current_region_bucket() {
    let mut game = fixture(AiProfile::general());
    fill_history(&mut game);
    let before = game.clone();
    game.act(ActorId(1), Action::Wait).unwrap();
    game.admit_ai_intention(ActorId(2)).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(!shares_region(&game, &before, 1));
    assert!(shares_region(&game, &before, 2));
    assert_eq!(
        before.combat.ai[&ActorId(2)].visits.get(&at(1, 2, 1)),
        Some(&64)
    );
    assert_eq!(
        game.combat.ai[&ActorId(2)].visits.get(&at(1, 2, 1)),
        Some(&65)
    );
}

#[test]
fn reconfiguring_a_task_preserves_earned_visits() {
    let mut game = fixture(AiProfile::general());
    fill_history(&mut game);
    let before = game.combat.ai[&ActorId(2)].visits.clone();
    game.configure_ai(
        ActorId(2),
        AiProfile {
            idle: IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(game.combat.ai[&ActorId(2)].visits, before);
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
}

#[test]
fn simple_idle_roaming_needs_one_scene_and_no_route_searches() {
    let mut game = fixture(AiProfile::general());
    fill_history(&mut game);
    let before = crate::diagnostics::work_counts();
    assert!(matches!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Move(_)
    ));
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.scenes - before.scenes, 1);
    assert_eq!(after.route_searches - before.route_searches, 0);
    assert_eq!(after.ability_plans - before.ability_plans, 0);
}

#[test]
fn visit_encoding_is_ordered_strict_and_does_not_depend_on_shared_storage() {
    let mut game = fixture(AiProfile::general());
    fill_history(&mut game);
    let ai = &game.combat.ai[&ActorId(2)];
    let value = serde_json::to_value(ai).unwrap();
    let restored: Ai = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(&restored, ai);
    let mut duplicate = value.clone();
    let rows = duplicate["visits"].as_array_mut().unwrap();
    rows.push(rows[0].clone());
    assert!(serde_json::from_value::<Ai>(duplicate).is_err());
    let mut reversed = value;
    reversed["visits"].as_array_mut().unwrap().reverse();
    assert!(serde_json::from_value::<Ai>(reversed).is_err());
}

fn supply(id: &str) -> ItemSpec {
    let mut item = ItemSpec::ordinary(id.into());
    item.stackable = true;
    item.supply = Some(crate::content_ids::SupplyId::new(id).unwrap());
    item
}
fn supply_game() -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    for name in ["herb", "swab"] {
        document.supplies.push(crate::rules::SupplyDef {
            id: crate::content_ids::SupplyId::new(name).unwrap(),
            name: name.into(),
        });
    }
    let primary = document
        .abilities
        .iter_mut()
        .find(|ability| ability.id.as_str() == "bind_wounds")
        .unwrap();
    let cost = primary.supply_costs[0].clone();
    let costs = |herbs, swabs| {
        [("herb", herbs), ("swab", swabs)]
            .into_iter()
            .map(|(name, quantity)| {
                let mut value = cost.clone();
                value.supply = crate::content_ids::SupplyId::new(name).unwrap();
                value.quantity = quantity;
                value
            })
            .collect()
    };
    primary.supply_costs = costs(1, 2);
    primary.ai.priority = 20;
    let mut alternative = primary.clone();
    alternative.id = crate::content_ids::AbilityId::new("alternate_heal").unwrap();
    alternative.supply_costs = costs(2, 1);
    alternative.ai.priority = 10;
    document
        .constants
        .default_abilities
        .push(alternative.id.clone());
    document.abilities.push(alternative);
    fixture_with_rules(
        AiProfile::general(),
        crate::rules::Ruleset::validate(document).unwrap(),
    )
}

#[test]
fn useful_supply_reserve_keeps_a_complete_recipe_instead_of_unpayable_component_minima() {
    let mut game = supply_game();
    game.place_item_stack(20, at(1, 2, 1), None, 99, supply("swab"))
        .unwrap();
    game.place_item_stack(21, at(1, 2, 1), None, 99, supply("herb"))
        .unwrap();
    assert_eq!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(20),
            quantity: Some(2)
        })
    );
    game.place_item_stack(22, at(1, 2, 1), Some(ActorId(2)), 2, supply("swab"))
        .unwrap();
    assert_eq!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(21),
            quantity: Some(1)
        })
    );
    game.place_item_stack(23, at(1, 2, 1), Some(ActorId(2)), 1, supply("herb"))
        .unwrap();
    assert_eq!(
        loot(&game),
        None,
        "one complete use is enough; alternatives must not provoke hoarding"
    );
}

#[test]
fn unknown_supplies_and_supplies_without_a_safe_owned_use_are_ignored() {
    let mut game = supply_game();
    let mut concealed = supply("swab");
    concealed.concealed = true;
    game.place_item_stack(20, at(1, 2, 1), None, 99, concealed)
        .unwrap();
    game.place_item_stack(21, at(1, 2, 1), None, 99, supply("bandage"))
        .unwrap();
    assert_eq!(loot(&game), None);
    game.identify_item(ActorId(2), ItemId(20)).unwrap();
    assert_eq!(
        loot(&game),
        Some(Action::Take {
            item: ItemId(20),
            quantity: Some(2)
        })
    );
}

fn food(concealed: bool) -> ItemSpec {
    let mut item = ItemSpec::ordinary("meal".into());
    item.class = crate::ItemClass::Food;
    item.stackable = true;
    item.concealed = concealed;
    item.edible = Some(crate::EdibleSpec {
        nutrition: crate::rules::AmountExpr::Fixed { amount: 5 },
    });
    item
}
fn hungry_game(required: bool) -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.nutrition = crate::nutrition::NutritionPolicy::Enabled {
        initial: 2,
        maximum: 10,
        hungry_below: 7,
        weak_below: 3,
        starve_at: -3,
        unit_ticks: 1000,
        hungry_recovery: crate::health_recovery::RecoveryNutrition::Normal,
        weak_recovery: crate::health_recovery::RecoveryNutrition::Normal,
        empty_recovery: crate::health_recovery::RecoveryNutrition::Normal,
        regeneration_cost: crate::nutrition::RegenerationCost {
            denominator: 1,
            health: 0,
            stamina: 0,
            focus: 0,
            mana: 0,
        },
    };
    let mut game = fixture_with_rules(
        AiProfile::general(),
        crate::rules::Ruleset::validate(document).unwrap(),
    );
    if required {
        game.configure_nutrition(ActorId(2), crate::nutrition::NutritionRequirement::Required)
            .unwrap();
    }
    game
}

#[test]
fn food_reserve_and_eating_require_owned_nutrition_and_known_identity() {
    for required in [false, true] {
        let mut game = hungry_game(required);
        game.place_item_stack(20, at(1, 2, 1), None, 99, food(false))
            .unwrap();
        assert_eq!(
            loot(&game),
            required.then_some(Action::Take {
                item: ItemId(20),
                quantity: Some(1)
            })
        );
        game.place_item_stack(21, at(1, 2, 1), Some(ActorId(2)), 3, food(true))
            .unwrap();
        assert!(!matches!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Eat { .. }
        ));
        game.identify_item(ActorId(2), ItemId(21)).unwrap();
        assert_eq!(loot(&game), None);
        let chosen = game.choose_ai(ActorId(2)).unwrap().action;
        assert_eq!(matches!(chosen, Action::Eat { item: ItemId(21) }), required);
    }
}

#[test]
fn ordinary_hunger_yields_to_a_personally_sensed_hostile() {
    let mut game = hungry_game(true);
    game.place_item_stack(20, at(1, 2, 1), Some(ActorId(2)), 1, food(false))
        .unwrap();
    assert!(matches!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Eat { .. }
    ));
    game.teleport(ActorId(1), at(1, 3, 1)).unwrap();
    game.refresh_navigation();
    let action = game.choose_ai(ActorId(2)).unwrap().action;
    assert!(
        !matches!(action, Action::Eat { .. }),
        "food must not replace immediate defense: {action:?}"
    );
}

#[test]
fn held_idle_actor_waits_without_recording_false_movement_and_resumes_after_expiry() {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.push(serde_json::from_value(serde_json::json!({
        "id":"idle_hold", "family":"idle_hold", "lifecycle":"timed", "name":"Held",
        "modifiers":[], "stacking":{"kind":"keep"}, "source_scoped":false,
        "descriptors":[], "capacity":"reject", "timing":{"kind":"actor_active", "on_expiry":{"kind":"remove"}},
        "restrictions":["ordinary_movement"], "preparation":"pause"
    })).unwrap());
    let mut game = fixture_with_rules(
        AiProfile::general(),
        crate::rules::Ruleset::validate(document).unwrap(),
    );
    game.apply_condition(
        ActorId(2),
        &crate::content_ids::ConditionId::new("idle_hold").unwrap(),
        ActorId(1),
        10,
    )
    .unwrap();
    game.place_item_stack(20, at(1, 3, 1), None, 1, potion("known", false))
        .unwrap();
    game.refresh_navigation();
    let before = game.clone();
    let chosen = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(chosen.action, Action::Wait);
    assert_eq!(chosen.ai.visits, before.combat.ai[&ActorId(2)].visits);
    assert_eq!(game, before);
    game.act(ActorId(1), Action::Wait).unwrap();
    game.admit_ai_intention(ActorId(2)).unwrap();
    let before_tick = game.tick;
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.actors[&ActorId(2)].location, at(1, 2, 1));
    assert_eq!(
        game.combat.ai[&ActorId(2)].visits,
        before.combat.ai[&ActorId(2)].visits
    );
    assert!(game.tick >= before_tick);
    game.tick += 10;
    game.settle_creature_time().unwrap();
    assert!(matches!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Move(_)
    ));
}

#[test]
fn frozen_and_detached_idle_actors_keep_history_without_taking_turns() {
    use crate::streaming::{MemoryRecords, RegionState, RegionTransition};
    for detach in [false, true] {
        let template = fixture(AiProfile::general());
        let world = tor_world::World::new(
            (1..=2)
                .map(|region| tor_world::Region {
                    id: RegionId(region),
                    name: format!("Isolated {region}"),
                    bounds: tor_world::Extent::new(6, 4, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let mut game = Game::new(world, 42, template.content().clone());
        for (expected, location) in [(ActorId(1), at(2, 4, 2)), (ActorId(2), at(1, 2, 1))] {
            let id = game
                .spawn_actor(location, std::num::NonZeroU64::new(100).unwrap())
                .unwrap();
            assert_eq!(id, expected);
            game.configure_creature(
                id,
                crate::CreatureIdentity {
                    name: format!("actor {}", id.0),
                    faction: "neutral".into(),
                },
                template.creature(id).unwrap().build().clone(),
            )
            .unwrap();
        }
        game.configure_run(
            ActorId(1),
            std::collections::BTreeSet::from([ActorId(1)]),
            None,
            Default::default(),
        )
        .unwrap();
        game.configure_ai(ActorId(2), AiProfile::general()).unwrap();
        game.refresh_navigation();
        fill_history(&mut game);
        let before_ai = game.combat.ai[&ActorId(2)].clone();
        let before_clock = game.actor_clock(ActorId(2));
        let before_ready = game.actors[&ActorId(2)].ready_at;
        let before_location = game.actors[&ActorId(2)].location;
        let mut records = MemoryRecords::default();
        // Disconnected regions remove neighboring scene pins, so the ordinary
        // transition can genuinely detach this actor rather than only freeze it.
        let active = std::collections::BTreeSet::from([RegionId(2)]);
        let loaded = if detach {
            active.clone()
        } else {
            std::collections::BTreeSet::from([RegionId(1), RegionId(2)])
        };
        game.transition_regions(&RegionTransition { active, loaded }, &mut records)
            .unwrap();
        assert_eq!(
            game.region_state(RegionId(1)),
            Some(if detach {
                RegionState::Detached
            } else {
                RegionState::Frozen
            })
        );
        for _ in 0..8 {
            assert_eq!(game.next_actor(), Some(ActorId(1)));
            game.act(ActorId(1), Action::Wait).unwrap();
        }
        if !detach {
            assert_eq!(game.actor_clock(ActorId(2)), before_clock);
        }
        let all = std::collections::BTreeSet::from([RegionId(1), RegionId(2)]);
        game.transition_regions(
            &RegionTransition {
                active: all.clone(),
                loaded: all,
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.combat.ai[&ActorId(2)], before_ai);
        assert_eq!(
            game.actors[&ActorId(2)].ready_at,
            before_ready + game.tick - before_clock
        );
        assert_eq!(game.actors[&ActorId(2)].location, before_location);
        assert!(matches!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Move(_)
        ));
    }
}

#[test]
fn stationary_idle_can_open_an_explicitly_enabled_reachable_door_without_moving() {
    for doors in [false, true] {
        let mut game = fixture(AiProfile {
            idle: IdleBehavior::Wait,
            doors,
            ..Default::default()
        });
        let door = game.place_door(at(1, 3, 1), false, 2).unwrap();
        let expected = if doors {
            Action::SetDoor { door, open: true }
        } else {
            Action::Wait
        };
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
        game.act(ActorId(1), Action::Wait).unwrap();
        game.admit_ai_intention(ActorId(2)).unwrap();
        let outcome = game.execute_next_intention().unwrap();
        outcome.outcome.unwrap();
        assert_eq!(outcome.action, Some(expected));
        assert_eq!(game.actors[&ActorId(2)].location, at(1, 2, 1));
        assert!(game.combat.ai[&ActorId(2)].visits.iter().next().is_none());
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
    }
}

#[test]
fn stationary_underfoot_pickup_needs_no_previous_navigation_record() {
    let mut game = fixture(AiProfile {
        idle: IdleBehavior::Wait,
        ..AiProfile::general()
    });
    game.place_item_stack(20, at(1, 2, 1), None, 1, potion("known", false))
        .unwrap();
    game.navigation.remove(&ActorId(2));
    assert!(game.route_search(ActorId(2)).is_err());
    let before = crate::diagnostics::work_counts();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Take {
            item: ItemId(20),
            quantity: Some(1)
        }
    );
    assert_eq!(
        crate::diagnostics::work_counts().route_searches - before.route_searches,
        0
    );
}

fn ring_probe(op: crate::rules::ModifierOp, scaled: bool) -> ItemSpec {
    let mut item = ItemSpec::ordinary("unfamiliar band".into());
    item.class = crate::ItemClass::Ring;
    item.concealed = true;
    let modifier = crate::rules::Modifier {
        cancellable: false,
        scope: crate::rules::Scope::Always,
        op,
    };
    item.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        slot: crate::EquipmentSlot::new("ring").unwrap(),
        armor: None,
        attack: None,
        weapon_profile: None,
        modifiers: if scaled {
            vec![]
        } else {
            vec![modifier.clone()]
        },
        potency_modifiers: if scaled { vec![modifier] } else { vec![] },
    });
    item
}

#[test]
fn npc_known_useful_rings_participate_in_loot_and_equipment_but_negative_damage_does_not() {
    use crate::rules::{ModifierOp, Selector, Stat};
    for (op, scaled, potency, useful) in [
        (
            ModifierOp::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 1,
            },
            true,
            2,
            true,
        ),
        (
            ModifierOp::SignedReduction {
                selector: Selector::Category {
                    category: crate::combat::DamageType::Psychic,
                },
                amount: 1,
            },
            true,
            1,
            true,
        ),
        (
            ModifierOp::SustainAttribute {
                attribute: crate::attributes::Attribute::Strength,
            },
            false,
            0,
            true,
        ),
        (
            ModifierOp::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 1,
            },
            true,
            -1,
            false,
        ),
    ] {
        let mut game = fixture(AiProfile::general());
        game.place_item_stack(20, at(1, 2, 1), None, 1, ring_probe(op, scaled))
            .unwrap();
        game.items
            .edit(ItemId(20), |item| item.condition.potency = potency)
            .unwrap();
        game.identify_item(ActorId(2), ItemId(20)).unwrap();
        assert_eq!(
            game.gear_destination(ActorId(2), ItemId(20)).is_some(),
            useful
        );
        assert_eq!(loot(&game).is_some(), useful);
        if useful {
            game.items
                .edit(ItemId(20), |item| {
                    item.location = crate::ItemLocation::Carried(ActorId(2))
                })
                .unwrap();
            assert!(matches!(
                game.choose_ai(ActorId(2)).unwrap().action,
                Action::Equip {
                    item: ItemId(20),
                    ..
                }
            ));
        }
    }
}

#[test]
fn npc_unknown_ring_potency_is_not_a_source_of_choice_information() {
    use crate::rules::{ModifierOp, Stat};
    let mut positive = fixture(AiProfile::general());
    positive
        .place_item_stack(
            20,
            at(1, 2, 1),
            None,
            1,
            ring_probe(
                ModifierOp::Add {
                    stat: Stat::AttackDamageRollBonus,
                    value: 1,
                },
                true,
            ),
        )
        .unwrap();
    let mut negative = positive.clone();
    positive
        .items
        .edit(ItemId(20), |item| item.condition.potency = 2)
        .unwrap();
    negative
        .items
        .edit(ItemId(20), |item| item.condition.potency = -1)
        .unwrap();
    assert!(positive.observe(ActorId(2)) == negative.observe(ActorId(2)));
    assert_eq!(
        positive.choose_ai(ActorId(2)).unwrap().action,
        negative.choose_ai(ActorId(2)).unwrap().action
    );
    for game in [&positive, &negative] {
        assert_eq!(loot(game), None);
        assert_eq!(game.gear_destination(ActorId(2), ItemId(20)), None);
    }
}
#[test]
fn npc_slow_digestion_values_effective_required_rate_without_vetoing_combat_upkeep() {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    for required in [false, true] {
        let mut game = hungry_game(required);
        for id in [20, 21, 22] {
            let spec = ring_probe(
                ModifierOp::Add {
                    stat: Stat::NutritionRateThirtieths,
                    value: -15,
                },
                false,
            );
            game.place_item_stack(id, at(1, 2, 1), Some(ActorId(2)), 1, spec)
                .unwrap();
            game.identify_item(ActorId(2), ItemId(id)).unwrap();
        }
        assert_eq!(
            game.gear_destination(ActorId(2), ItemId(20)).is_some(),
            required
        );
        if !required {
            continue;
        }
        for id in [20, 21] {
            let (slot, previous) = game.gear_destination(ActorId(2), ItemId(id)).unwrap();
            assert!(previous.is_none());
            game.equip_authored(ActorId(2), ItemId(id), slot).unwrap();
        }
        assert_eq!(
            game.gear_destination(ActorId(2), ItemId(22)),
            None,
            "already at zero basal cost"
        );
        let mut damage = ring_probe(
            ModifierOp::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 2,
            },
            false,
        );
        damage.equipment.as_mut().unwrap().modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::NutritionRateThirtieths,
                value: 30,
            },
        });
        game.place_item_stack(23, at(1, 2, 1), Some(ActorId(2)), 1, damage)
            .unwrap();
        game.identify_item(ActorId(2), ItemId(23)).unwrap();
        assert!(
            game.gear_destination(ActorId(2), ItemId(23)).is_some(),
            "useful combat gear may trade additional upkeep"
        );
    }
}
