// Birth stock uses ordinary item authority and the enclosing entry transaction.
fn carried_plan(chance: [u32; 2]) -> PopulationPlan {
    let actor = PreparedActor {
        build: None,
        nutrition: crate::nutrition::NutritionRequirement::Exempt,
        identity: None,
        anatomy: None,
        body: None,
        ai: None,
        known_identities: vec![],
        velocity: None,
        asset: None,
    };
    let mut value = serde_json::to_value(plan(vec![])).unwrap();
    value["carried_item_pools"] = serde_json::json!({
        "supplies": {"version": 1, "choices": [{
            "content": "marker", "weight": 1, "quantity": {"kind":"fixed","quantity":3}, "spec": marker()
        }]}
    });
    value["profiles"]["baseline"]["stages"] = serde_json::json!([{
        "id": "carriers", "version": 1, "count": [1, 1],
        "population": {"kind": "actors", "avoid_protagonist_sight": false,
            "bands": [{"depth": {"kind": "open", "minimum": 1}, "choices": [{
                "content": "carrier", "weight": 1, "turn_ticks": 100, "actor": actor,
                "depth_growth": {"kind": "fixed"}, "advancement":[],
                "carried_loot": {"version": 1, "pool": "supplies", "chance": {
                    "numerator": chance[0], "denominator": chance[1]
                }}
            }]}]
        }
    }]);
    serde_json::from_value(value).expect("current compiled plan accepts carried birth stock")
}

fn carrier_game(chance: [u32; 2]) -> Game {
    let mut game = game();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_population(carried_plan(chance)).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
        .unwrap();
    game.register_activation_scope(scope(2, vec![at(2, 2, 1)]))
        .unwrap();
    game
}

#[test]
fn guaranteed_birth_stock_is_an_ordinary_owned_stack_before_publication() {
    let mut game = carrier_game([1, 1]);
    let rng = game.rules_rng;
    game.activate_initial_floor().unwrap();
    assert_eq!(
        game.items[&ItemId(1)].location,
        ItemLocation::Carried(ActorId(2))
    );
    assert_eq!(game.items[&ItemId(1)].quantity, 3);
    assert_eq!(*game.items[&ItemId(1)].spec, marker());
    let records = serde_json::to_value(game.activations_since(0).unwrap()).unwrap();
    assert_eq!(
        records[0]["decisions"][0]["result"]["attempts"][0]["entity"],
        serde_json::json!({"kind": "actor", "id": 2, "carried_loot": {
            "kind": "carried", "content": "marker", "quantity": 3, "item": 1
        }})
    );
    assert_eq!(game.rules_rng, rng);
    assert!(game.activation_state_valid());
    let before = game.clone();
    game.activate_initial_floor().unwrap();
    assert_eq!(
        game, before,
        "a revisit cannot allocate or reroll birth stock"
    );
}

#[test]
fn zero_chance_records_a_miss_without_allocating_an_item() {
    let mut game = carrier_game([0, 1]);
    game.activate_initial_floor().unwrap();
    assert_eq!(game.next_item_id, 1);
    assert_eq!(game.next_actor_id, 3);
    let records = serde_json::to_value(game.activations_since(0).unwrap()).unwrap();
    assert_eq!(
        records[0]["decisions"][0]["result"]["attempts"][0]["entity"]["carried_loot"],
        serde_json::json!({"kind": "not_carried"})
    );
    assert!(game.activation_state_valid());
}

fn carrier_choice(plan: &mut PopulationPlan) -> &mut PreparedActorChoice {
    let PopulationContent::Actors { bands, .. } =
        &mut plan.profiles.get_mut("baseline").unwrap().stages[0].population
    else {
        panic!()
    };
    &mut bands[0].choices[0]
}

#[test]
fn carried_stock_failure_rolls_back_earlier_ground_and_actor_births_for_initial_and_teleport() {
    for initial in [true, false] {
        let mut game = game();
        game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut plan = carried_plan([1, 1]);
        plan.profiles.get_mut("baseline").unwrap().stages[0].count = [2, 2];
        plan.profiles
            .get_mut("baseline")
            .unwrap()
            .stages
            .insert(0, item_stage(1));
        game.configure_population(plan).unwrap();
        let region = if initial { 1 } else { 2 };
        game.register_activation_scope(scope(
            region as u32,
            vec![at(region, 2, 1), at(region, 3, 1), at(region, 4, 1)],
        ))
        .unwrap();
        // Ground stock and the first actor's pack fit, the second pack does not.
        game.next_item_id = u64::MAX - 2;
        let before = game.clone();
        let result = if initial {
            game.activate_initial_floor()
        } else {
            game.teleport(ActorId(1), at(2, 0, 1))
        };
        assert_eq!(result, Err(GameError::IdentityExhausted));
        assert_eq!(
            game, before,
            "every prior stage, actor and item must roll back"
        );
    }
}

#[test]
fn carried_stock_actor_allocation_failure_and_policy_budget_refusal_are_atomic() {
    for actor_exhaustion in [true, false] {
        let mut game = carrier_game([0, 1]);
        if actor_exhaustion {
            game.next_actor_id = u64::MAX;
        } else {
            game.floor_activation.as_mut().unwrap().decision_count = MAX_RETAINED_DECISIONS - 2;
        }
        let before = game.clone();
        assert_eq!(
            game.activate_initial_floor(),
            Err(GameError::IdentityExhausted)
        );
        assert_eq!(
            game, before,
            "a miss still consumes one checked policy decision"
        );
    }
}

#[test]
fn placement_exhaustion_has_no_birth_stock_decision_or_allocation() {
    let mut game = game();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_population(carried_plan([1, 1])).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 0, 1)]))
        .unwrap();
    game.activate_initial_floor().unwrap();
    assert_eq!((game.next_actor_id, game.next_item_id), (2, 1));
    assert_eq!(game.floor_activation.as_ref().unwrap().decision_count, 2);
    assert!(
        matches!(&game.activations_since(0).unwrap()[0].decisions[0].result,
        StageResult::Evaluated { attempts, .. } if matches!(attempts[0], PlacementDecision::Skipped { reason: SkipReason::NoEligibleCell, .. }))
    );
}

#[test]
fn birth_stock_policies_validate_unreachable_pools_and_all_exact_stack_constraints() {
    for case in 0..11 {
        let mut plan = carried_plan([0, 1]);
        match case {
            0 => {
                carrier_choice(&mut plan)
                    .carried_loot
                    .as_mut()
                    .unwrap()
                    .version = 2
            }
            1 => {
                carrier_choice(&mut plan)
                    .carried_loot
                    .as_mut()
                    .unwrap()
                    .pool = "missing".into()
            }
            2 => {
                carrier_choice(&mut plan)
                    .carried_loot
                    .as_mut()
                    .unwrap()
                    .chance
                    .denominator = 0
            }
            3 => {
                carrier_choice(&mut plan)
                    .carried_loot
                    .as_mut()
                    .unwrap()
                    .chance
                    .numerator = 2
            }
            4 => plan.carried_item_pools.get_mut("supplies").unwrap().version = 2,
            5 => plan
                .carried_item_pools
                .get_mut("supplies")
                .unwrap()
                .choices
                .clear(),
            6 => {
                plan.carried_item_pools.get_mut("supplies").unwrap().choices[0].quantity =
                    QuantityExpr::Fixed { quantity: 0 }
            }
            7 => {
                plan.carried_item_pools.get_mut("supplies").unwrap().choices[0]
                    .spec
                    .stackable = false
            }
            8 => plan.carried_item_pools.get_mut("supplies").unwrap().choices[0].weight = 0,
            9 => {
                let pool = plan.carried_item_pools.get_mut("supplies").unwrap();
                pool.choices.push(pool.choices[0].clone());
            }
            10 => {
                let pool = plan.carried_item_pools.get_mut("supplies").unwrap();
                pool.choices[0].weight = u32::MAX;
                let mut extra = pool.choices[0].clone();
                extra.content = "other".into();
                pool.choices.push(extra);
            }
            _ => unreachable!(),
        }
        let mut game = game();
        let before = game.clone();
        assert!(
            game.configure_population(plan).is_err(),
            "invalid case {case}"
        );
        assert_eq!(game, before);
    }
}

#[test]
fn compiled_birth_stock_and_receipts_require_named_complete_current_fields() {
    let plan = serde_json::to_value(carried_plan([1, 1])).unwrap();
    let choice_path = "/profiles/baseline/stages/0/population/bands/0/choices/0";
    for (path, field) in [
        ("", "carried_item_pools"),
        (choice_path, "carried_loot"),
        (choice_path, "depth_growth"),
    ] {
        let mut missing = plan.clone();
        missing
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<PopulationPlan>(missing).is_err(),
            "{field}"
        );
    }
    for path in [
        "/carried_item_pools/supplies",
        "/profiles/baseline/stages/0/population/bands/0/choices/0/carried_loot",
        "/profiles/baseline/stages/0/population/bands/0/choices/0/carried_loot/chance",
    ] {
        let mut positional = plan.clone();
        let values = positional
            .pointer(path)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        *positional.pointer_mut(path).unwrap() = serde_json::json!(values);
        assert!(serde_json::from_value::<PopulationPlan>(positional).is_err());
    }
    assert!(
        serde_json::from_value::<PopulationEntity>(serde_json::json!({"kind":"actor","id":2}))
            .is_err()
    );
    assert!(serde_json::from_value::<PopulationEntity>(
        serde_json::json!({"kind":"actor","id":2,"carried_loot":{"kind":"not_carried","extra":0}})
    )
    .is_err());
}

#[test]
fn nested_birth_receipts_bind_chance_content_quantity_and_item_bounds() {
    let mut game = carrier_game([1, 1]);
    game.activate_initial_floor().unwrap();
    let valid = game.activations_since(0).unwrap().remove(0);
    let encoded = serde_json::to_value(&valid).unwrap();
    let path = "/decisions/0/result/attempts/0/entity/carried_loot";
    for replacement in [
        serde_json::Value::Null,
        serde_json::json!({"kind":"not_carried"}),
        serde_json::json!({"kind":"carried","content":"wrong","quantity":3,"item":1}),
        serde_json::json!({"kind":"carried","content":"marker","quantity":2,"item":1}),
        serde_json::json!({"kind":"carried","content":"marker","quantity":3,"item":0}),
        serde_json::json!({"kind":"carried","content":"marker","quantity":3,"item":u64::MAX}),
    ] {
        let mut altered = encoded.clone();
        *altered.pointer_mut(path).unwrap() = replacement;
        let altered: FloorActivationRecord = serde_json::from_value(altered).unwrap();
        assert!(game.validate_activation_record(&altered).is_err());
    }
    let mut altered = encoded;
    altered.pointer_mut(path).unwrap()["item"] = serde_json::json!(game.next_item_id);
    let altered: FloorActivationRecord = serde_json::from_value(altered).unwrap();
    assert!(game
        .validate_activation_record_identity_bounds(&altered)
        .is_err());
    assert_eq!(
        game.activation_item_births_at(game.tick),
        BTreeSet::from([ItemId(1)])
    );
}

#[test]
fn carried_stock_survives_checkpoint_and_revisit_with_exact_ownership() {
    let mut game = carrier_game([1, 1]);
    game.activate_initial_floor().unwrap();
    let first = game.activations_since(0).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
    assert_eq!(restored, game);
    restored.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    restored.teleport(ActorId(1), at(1, 0, 1)).unwrap();
    assert_eq!(&restored.activations_since(0).unwrap()[..1], first);
    assert_eq!(
        restored.items[&ItemId(1)].location,
        ItemLocation::Carried(ActorId(2))
    );
    assert_eq!(restored.items[&ItemId(1)].quantity, 3);
    assert_eq!(restored.next_item_id, 3);
    assert!(restored.activation_state_valid());
}

#[test]
fn carried_stock_seed_vectors_pin_independent_chance_and_content_domains() {
    let plan = carried_plan([1, 5]);
    let profile = &plan.profiles["baseline"];
    let stage = &profile.stages[0];
    let PopulationContent::Actors { bands, .. } = &stage.population else {
        panic!()
    };
    let choice = &bands[0].choices[0];
    let policy = choice.carried_loot.as_ref().unwrap();
    for (seed, chance_root, content_root, carries) in [
        (0, 0x59ab6faefd80705e, 0x6a2c704d9047d4f1, true),
        (1, 0xa45f8130f278b793, 0x0f11a9b1feda87e0, false),
        (42, 0x064c89cd0702d0d0, 0x8ac205cb86cf1e1b, false),
    ] {
        let context = BirthStockContext {
            seed,
            scope: "dungeon/1",
            profile_id: "baseline",
            profile,
            stage,
            ordinal: 0,
            depth: Some(1),
        };
        assert_eq!(context.stream(choice, policy, None), chance_root);
        assert_eq!(
            context.stream(
                choice,
                policy,
                Some(("supplies", &plan.carried_item_pools["supplies"]))
            ),
            content_root
        );
        assert_eq!(
            context
                .select(choice, &plan.carried_item_pools)
                .unwrap()
                .is_some(),
            carries
        );
    }
}

#[test]
fn carried_stock_changes_cannot_perturb_actor_selection_placement_or_rules_rng() {
    let mut reference = None;
    for case in 0..4 {
        let mut game = game();
        game.seed = 0;
        game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut plan = carried_plan([1, 5]);
        if case == 1 {
            carrier_choice(&mut plan).carried_loot = None;
        }
        if case == 2 {
            plan.carried_item_pools.get_mut("supplies").unwrap().choices[0].quantity =
                QuantityExpr::Fixed { quantity: 99 };
        }
        if case == 3 {
            let pool = plan.carried_item_pools.get_mut("supplies").unwrap();
            pool.choices[0].weight = u32::MAX - 1;
            let mut extra = pool.choices[0].clone();
            extra.content = "other".into();
            extra.weight = 1;
            pool.choices.push(extra);
        }
        game.configure_population(plan).unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1)]))
            .unwrap();
        let rng = game.rules_rng;
        game.activate_initial_floor().unwrap();
        let facts = (
            game.world.clone(),
            game.actors.clone(),
            game.rules_rng,
            game.tick,
            game.next_actor_id,
        );
        assert_eq!(game.rules_rng, rng);
        if let Some(reference) = &reference {
            assert_eq!(&facts, reference);
        } else {
            reference = Some(facts);
        }
    }
}

#[test]
fn ordinary_death_drops_the_same_birth_stack_and_repeated_cleanup_never_restock() {
    let species = crate::test_creatures::species();
    let content = crate::test_creatures::content_with(species.clone(), vec![]);
    let build = crate::test_creatures::build(species);
    let mut game = Game::region_corridor(42, 2, content);
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
        .unwrap();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut plan = carried_plan([1, 1]);
    let definition = &mut carrier_choice(&mut plan).actor;
    definition.build = Some(crate::creatures::record::BuildRecord::capture(&build));
    definition.identity = Some(crate::CreatureIdentity {
        name: "carrier".into(),
        faction: "neutral".into(),
    });
    game.configure_population(plan).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
        .unwrap();
    game.activate_initial_floor().unwrap();
    let next_item = game.next_item_id;
    let damage = crate::damage::DamageSpec::new(
        vec![crate::damage::DamageComponent::fixed(
            crate::combat::DamageType::Vital,
            None,
            1000,
        )],
        None,
    )
    .unwrap();
    game.apply_damage(ActorId(2), &damage).unwrap();
    assert!(!game.alive(ActorId(2)));
    assert_eq!(
        game.items[&ItemId(1)].location,
        ItemLocation::Ground(at(1, 2, 1))
    );
    assert_eq!(game.items[&ItemId(1)].quantity, 3);
    assert_eq!(
        game.next_item_id,
        next_item + 1,
        "only the ordinary corpse allocates at death"
    );
    let after = game.clone();
    game.apply_damage(ActorId(2), &damage).unwrap();
    assert_eq!(game.next_item_id, after.next_item_id);
    game.activate_initial_floor().unwrap();
    assert_eq!(game.items, after.items);
    assert!(game.activation_state_valid());
}

#[test]
fn birth_item_aliases_other_floor_or_ground_origins_are_rejected() {
    let mut game = carrier_game([1, 1]);
    game.activate_initial_floor().unwrap();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let mut state = game.floor_activation.as_ref().unwrap().clone();
    let record = state.floors.get_mut("dungeon/2").unwrap();
    let StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result else {
        panic!()
    };
    let PlacementDecision::Placed {
        entity:
            PopulationEntity::Actor {
                carried_loot: Some(CarriedLootDecision::Carried { item, .. }),
                ..
            },
        ..
    } = &mut attempts[0]
    else {
        panic!()
    };
    *item = ItemId(1);
    game.floor_activation = Some(state);
    assert!(!game.activation_state_valid());
}

fn living_carrier_game(chance: [u32; 2], healing_stock: bool) -> Game {
    let mut species = crate::test_creatures::species();
    species.behavior = crate::behavior::BehaviorPolicy {
        disposition: crate::behavior::Disposition::Passive,
        idle: crate::behavior::IdleBehavior::Wait,
        memory_ticks: 1000,
        ..Default::default()
    };
    let content = crate::test_creatures::content_with(species.clone(), vec![]);
    let build = crate::test_creatures::build(species);
    let mut game = Game::region_corridor(42, 2, content);
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
        .unwrap();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut plan = carried_plan(chance);
    let definition = &mut carrier_choice(&mut plan).actor;
    definition.build = Some(crate::creatures::record::BuildRecord::capture(&build));
    definition.identity = Some(crate::CreatureIdentity {
        name: "carrier".into(),
        faction: "neutral".into(),
    });
    definition.ai = Some(crate::ai::AiProfile {
        care: false,
        equipment: false,
        loot: false,
        gold: false,
        flee_percent: 0,
        idle: crate::behavior::IdleBehavior::Wait,
        doors: false,
    });
    if healing_stock {
        let spec = &mut plan.carried_item_pools.get_mut("supplies").unwrap().choices[0].spec;
        spec.class = crate::ItemClass::Potion;
        spec.consumable = Some(crate::ConsumableSpec {
            effects: vec![crate::rules::Effect::Heal {
                amount: crate::rules::AmountExpr::Fixed { amount: 10 },
            }],
        });
    }
    game.configure_population(plan).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
        .unwrap();
    game.activate_initial_floor().unwrap();
    game
}

#[test]
fn birth_stock_never_enables_care_gear_loot_or_identification_without_opt_in() {
    let mut game = living_carrier_game([1, 1], true);
    assert!(game.actors[&ActorId(2)].knowledge.is_empty());
    assert!(game.actors[&ActorId(2)].equipment.is_empty());
    let maximum = game.health(ActorId(2)).unwrap().1;
    let damage = crate::damage::DamageSpec::new(
        vec![crate::damage::DamageComponent::fixed(
            crate::combat::DamageType::Vital,
            None,
            u32::try_from(maximum - 1).unwrap(),
        )],
        None,
    )
    .unwrap();
    game.apply_damage(ActorId(2), &damage).unwrap();
    assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, Action::Wait);
    assert_eq!(game.items[&ItemId(1)].quantity, 3);
    game.configure_ai(ActorId(2), crate::ai::AiProfile::general())
        .unwrap();
    assert_eq!(
        game.choose_ai(ActorId(2)).unwrap().action,
        Action::Drink { item: ItemId(1) }
    );
}

#[test]
fn birth_stock_policy_cannot_change_actual_owned_health_records_or_build_seed() {
    let carried = living_carrier_game([1, 1], false);
    let empty = living_carrier_game([0, 1], false);
    assert_eq!(carried.actors, empty.actors);
    assert_eq!(carried.rules_rng, empty.rules_rng);
    let carrier = carried.creature(ActorId(2)).unwrap();
    let no_stock = empty.creature(ActorId(2)).unwrap();
    assert_eq!(carrier.health(), no_stock.health());
    assert_eq!(carrier.build().ledger(), no_stock.build().ledger());
}

#[test]
fn maximum_birth_stock_pools_and_attempt_receipts_fit_the_unchanged_checkpoint_envelope() {
    let mut plan = carried_plan([1, 1]);
    let template = plan.carried_item_pools["supplies"].clone();
    for pool_index in 0..MAX_CARRIED_ITEM_POOLS {
        let mut pool = template.clone();
        pool.choices = (0..MAX_CHOICES)
            .map(|index| {
                let mut choice = template.choices[0].clone();
                choice.content = format!("stock-{index}");
                choice.quantity = QuantityExpr::Fixed { quantity: u64::MAX };
                choice
            })
            .collect();
        let name = if pool_index == 0 {
            "supplies".into()
        } else {
            format!("pool-{pool_index}")
        };
        plan.carried_item_pools.insert(name, pool);
    }
    plan.profiles.get_mut("baseline").unwrap().stages[0].count = [MAX_ATTEMPTS, MAX_ATTEMPTS];
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "bounded births".into(),
            bounds: tor_world::Extent::new(i32::from(MAX_ATTEMPTS) + 2, 3, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, game().content.clone());
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
        .unwrap();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_population(plan).unwrap();
    game.register_activation_scope(scope(
        1,
        (1..=MAX_ATTEMPTS).map(|x| at(1, i32::from(x), 1)).collect(),
    ))
    .unwrap();
    game.activate_initial_floor().unwrap();
    assert_eq!(game.next_item_id, u64::from(MAX_ATTEMPTS) + 1);
    assert_eq!(
        game.floor_activation.as_ref().unwrap().decision_count,
        1 + 2 * usize::from(MAX_ATTEMPTS)
    );
    assert!(game.activation_state_valid());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshots: Vec<_> = (0..128).map(|_| game.checkpoint(&mut shared)).collect();
    let bytes = serde_json::to_vec(&(snapshots, &shared)).unwrap();
    assert!(
        bytes.len() < 64 * 1024 * 1024,
        "bounded repeated-state regression must stay in the existing envelope"
    );
    let saved = serde_json::to_value(&shared).unwrap();
    assert_eq!(saved["floor_activations"].as_array().unwrap().len(), 1);
}

#[test]
fn compiled_birth_stock_pool_decoders_reject_excess_entries_and_duplicate_keys() {
    let plan = serde_json::to_value(carried_plan([1, 1])).unwrap();
    let mut too_many = plan.clone();
    let pool = too_many["carried_item_pools"]["supplies"].clone();
    for index in 0..MAX_CARRIED_ITEM_POOLS {
        too_many["carried_item_pools"][format!("extra-{index}")] = pool.clone();
    }
    assert!(serde_json::from_value::<PopulationPlan>(too_many).is_err());
    let mut too_many = plan.clone();
    let choice = too_many["carried_item_pools"]["supplies"]["choices"][0].clone();
    too_many["carried_item_pools"]["supplies"]["choices"] =
        serde_json::json!(vec![choice; MAX_CHOICES + 1]);
    assert!(serde_json::from_value::<PopulationPlan>(too_many).is_err());
    let encoded = serde_json::to_string(&plan).unwrap();
    let pools = serde_json::to_string(&plan["carried_item_pools"]).unwrap();
    let pool = serde_json::to_string(&pool).unwrap();
    let duplicate = format!("{{\"supplies\":{pool},\"supplies\":{pool}}}");
    assert!(
        serde_json::from_str::<PopulationPlan>(&encoded.replacen(&pools, &duplicate, 1)).is_err()
    );
}

#[test]
fn skipped_actor_attempt_does_not_renumber_later_birth_stock_decisions() {
    let mut game = game();
    game.seed = 0;
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut plan = carried_plan([1, 5]);
    let stage = &mut plan.profiles.get_mut("baseline").unwrap().stages[0];
    stage.count = [2, 2];
    let PopulationContent::Actors { bands, .. } = &mut stage.population else {
        panic!()
    };
    let mut large = bands[0].choices[0].clone();
    large.content = "large".into();
    large.actor.body = Some(BodySpec {
        cells: vec![[0, 0, 0], [0, 0, 8]],
        eye: [0, 0, 0],
        mass: 80,
    });
    bands[0].choices.insert(0, large);
    game.configure_population(plan).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
        .unwrap();
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    let StageResult::Evaluated { attempts, .. } = &record.decisions[0].result else {
        panic!()
    };
    assert!(
        matches!(&attempts[0],PlacementDecision::Skipped { content,reason:SkipReason::NoEligibleCell } if content=="large")
    );
    assert!(matches!(
        &attempts[1],
        PlacementDecision::Placed {
            entity: PopulationEntity::Actor {
                carried_loot: Some(CarriedLootDecision::NotCarried),
                ..
            },
            ..
        }
    ));
    assert_eq!(
        game.next_item_id, 1,
        "attempt zero would carry; the first successful birth is attempt one and misses"
    );
    assert_eq!(game.floor_activation.as_ref().unwrap().decision_count, 4);
    assert!(game.activation_state_valid());
}

#[test]
fn invalid_runtime_birth_item_keeps_its_typed_error_and_discards_the_actor() {
    for bad_rules in [false, true] {
        let mut game = carrier_game([1, 1]);
        let pool = game
            .floor_activation
            .as_mut()
            .unwrap()
            .plan
            .carried_item_pools
            .get_mut("supplies")
            .unwrap();
        let expected = if bad_rules {
            pool.choices[0].spec.supply =
                Some(crate::content_ids::SupplyId::new("missing").unwrap());
            GameError::Rule(crate::abilities::RuleError::InvalidDefinition)
        } else {
            pool.choices[0].quantity = QuantityExpr::Fixed { quantity: 0 };
            GameError::InvalidQuantity
        };
        let before = game.clone();
        assert_eq!(game.activate_initial_floor(), Err(expected));
        assert_eq!(game, before);
    }
}

#[test]
fn retired_and_consumed_nested_origin_remains_valid_without_live_ownership() {
    fn perform(game: &mut Game, owner: ActorId, action: Action) {
        for _ in 0..32 {
            if game.next_actor() == Some(owner) {
                break;
            }
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        assert_eq!(game.next_actor(), Some(owner));
        game.admit_intention(owner, action, crate::IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        for _ in 0..32 {
            if game.preparation(owner).is_none() {
                break;
            }
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        assert!(game.preparation(owner).is_none());
    }
    let mut game = living_carrier_game([1, 1], true);
    let owner = ActorId(2);
    // This lifecycle fixture controls the carrier directly, rather than asking
    // its deliberately disabled transfer/care tactics to choose these actions.
    game.combat.ai.remove(&owner);
    let origin = ItemId(1);
    let records = game.activations_since(0).unwrap();
    let spec = (*game.items[&origin].spec).clone();
    let location = game.actors[&owner].location;
    perform(
        &mut game,
        owner,
        Action::Drop {
            item: origin,
            quantity: Some(1),
        },
    );
    let split = ItemId(2);
    assert_eq!(game.items[&origin].quantity, 2);
    assert_eq!(game.items[&split].location, ItemLocation::Ground(location));
    perform(
        &mut game,
        owner,
        Action::Take {
            item: split,
            quantity: None,
        },
    );
    assert_eq!(game.items[&origin].quantity, 3);
    assert!(!game.items.contains_key(&split));
    perform(
        &mut game,
        owner,
        Action::Drop {
            item: origin,
            quantity: None,
        },
    );
    let replacement = game
        .spawn_item_stack(location, Some(owner), 1, spec)
        .unwrap();
    perform(
        &mut game,
        owner,
        Action::Take {
            item: origin,
            quantity: None,
        },
    );
    assert!(!game.items.contains_key(&origin));
    assert_eq!(game.items[&replacement].quantity, 4);
    for _ in 0..4 {
        perform(&mut game, owner, Action::Drink { item: replacement });
    }
    assert!(!game.items.contains_key(&replacement));
    assert_eq!(game.activations_since(0).unwrap(), records);
    game.validate_activation_record(&records[0]).unwrap();
    game.validate_activation_record_identity_bounds(&records[0])
        .unwrap();
    assert!(game.activation_state_valid());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap(),
        game
    );
}
