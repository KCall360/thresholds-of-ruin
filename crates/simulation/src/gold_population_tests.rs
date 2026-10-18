fn gold_quantity() -> QuantityExpr {
    QuantityExpr::DepthScaledUniform {
        minimum: 2,
        maximum_base: 51,
        per_depth: 10,
    }
}

fn room_gold_stage(chance: [u32; 2]) -> PopulationStage {
    let mut stage = item_stage(1);
    stage.id = "room-gold".into();
    let PopulationContent::Items { mut bands } = stage.population else {
        unreachable!()
    };
    bands[0].choices[0].quantity = gold_quantity();
    stage.population = PopulationContent::RoomItems {
        bands,
        chance: CarryChance {
            numerator: chance[0],
            denominator: chance[1],
        },
    };
    stage
}

fn room_gold_game(seed: u64, chance: [u32; 2], room_ids: &[u32]) -> Game {
    let mut game = game();
    game.seed = seed;
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_population(plan(vec![room_gold_stage(chance)]))
        .unwrap();
    let mut prepared = scope(1, vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1)]);
    prepared.rooms = room_ids
        .iter()
        .map(|id| PreparedRoomGroup {
            id: *id,
            cells: vec![at(1, *id as i32 + 1, 1)],
        })
        .collect();
    game.register_activation_scope(prepared).unwrap();
    game
}

#[test]
fn room_gold_has_distinct_once_only_chance_placement_and_absent_outcomes() {
    for (chance, rooms, blocked, expected_decisions) in [
        ([0, 1], vec![1], false, 2),
        ([1, 1], vec![1], true, 3),
        ([1, 1], vec![], false, 1),
        ([1, 1], vec![1], false, 3),
    ] {
        let mut game = room_gold_game(42, chance, &rooms);
        if blocked {
            game.floor_activation
                .as_mut()
                .unwrap()
                .scopes
                .get_mut("dungeon/1")
                .unwrap()
                .protected
                .insert(at(1, 2, 1));
        }
        let rng = game.rules_rng;
        game.activate_initial_floor().unwrap();
        let record = game.activations_since(0).unwrap().remove(0);
        let StageResult::RoomsEvaluated { rooms: outcomes } = &record.decisions[0].result else {
            panic!()
        };
        assert_eq!(outcomes.len(), rooms.len());
        if let Some(outcome) = outcomes.first() {
            match (&outcome.result, chance, blocked) {
                (RoomResult::ChanceMiss, [0, 1], _) => assert_eq!(game.next_item_id, 1),
                (
                    RoomResult::Attempt {
                        decision:
                            PlacementDecision::Skipped {
                                reason: SkipReason::NoEligibleCell,
                                ..
                            },
                    },
                    _,
                    true,
                ) => assert_eq!(game.next_item_id, 1),
                (
                    RoomResult::Attempt {
                        decision:
                            PlacementDecision::Placed {
                                entity: PopulationEntity::Item { quantity, .. },
                                ..
                            },
                    },
                    [1, 1],
                    false,
                ) => assert!((2..=61).contains(quantity)),
                _ => panic!("wrong room outcome"),
            }
        }
        assert_eq!(record.decisions[0].decision_count(), expected_decisions);
        assert_eq!(game.rules_rng, rng);
        assert!(game.activation_state_valid());
        let before = game.clone();
        game.activate_initial_floor().unwrap();
        assert_eq!(game, before);
    }
}

#[test]
fn omitted_rooms_keep_stable_chance_content_quantity_and_placement_streams() {
    let mut hits = 0;
    for seed in 0..128 {
        let mut complete = room_gold_game(seed, [1, 2], &[1, 2, 3]);
        let mut omitted = room_gold_game(seed, [1, 2], &[1, 3]);
        complete.activate_initial_floor().unwrap();
        omitted.activate_initial_floor().unwrap();
        let left = complete.activations_since(0).unwrap().remove(0);
        let right = omitted.activations_since(0).unwrap().remove(0);
        let StageResult::RoomsEvaluated { rooms: left } = &left.decisions[0].result else {
            panic!()
        };
        let StageResult::RoomsEvaluated { rooms: right } = &right.decisions[0].result else {
            panic!()
        };
        for (expected, actual) in left.iter().filter(|room| room.room != 2).zip(right) {
            assert_eq!(expected.room, actual.room);
            let normalized = |room: &RoomDecision| {
                let mut value = serde_json::to_value(room).unwrap();
                if let Some(entity) = value.pointer_mut("/result/decision/entity") {
                    entity.as_object_mut().unwrap().remove("id");
                }
                value
            };
            assert_eq!(normalized(expected), normalized(actual));
            hits += usize::from(matches!(actual.result, RoomResult::Attempt { .. }));
        }
    }
    assert!(
        (80..180).contains(&hits),
        "independent half-chance rooms: {hits}"
    );
}

#[test]
fn room_metadata_rejects_overlap_unsorted_empty_and_out_of_scope_cells() {
    let game = room_gold_game(42, [1, 1], &[1, 3]);
    let original = &game.floor_activation.as_ref().unwrap().scopes["dungeon/1"];
    for case in 0..9 {
        let mut prepared = (**original).clone();
        match case {
            0 => prepared.rooms[0].id = 0,
            1 => prepared.rooms[1].id = prepared.rooms[0].id,
            2 => prepared.rooms.reverse(),
            3 => prepared.rooms[0].cells.clear(),
            4 => {
                let duplicate = prepared.rooms[0].cells[0];
                prepared.rooms[0].cells.push(duplicate);
            }
            5 => prepared.rooms[1].cells = prepared.rooms[0].cells.clone(),
            6 => prepared.rooms[0].cells = vec![at(2, 2, 1)],
            7 => prepared.rooms[0].cells = vec![at(1, 99, 1)],
            8 => prepared.rooms[0].cells = vec![at(1, 4, 1), at(1, 2, 1)],
            _ => unreachable!(),
        }
        assert!(!prepared.valid(), "case {case}");
    }
    let mut encoded = serde_json::to_value(&**original).unwrap();
    encoded.as_object_mut().unwrap().remove("rooms");
    assert!(serde_json::from_value::<PreparedScope>(encoded).is_err());
    let oversized = serde_json::json!({"id":1,"cells":vec![at(1,2,1);MAX_ROOM_CELLS+1]});
    assert!(serde_json::from_value::<PreparedRoomGroup>(oversized).is_err());
}

#[test]
fn room_multiplicity_and_retained_decisions_are_preflighted_before_any_birth() {
    let mut game = room_gold_game(42, [1, 1], &[1, 2, 3]);
    game.floor_activation.as_mut().unwrap().decision_count = MAX_RETAINED_DECISIONS - 6;
    let before = game.clone();
    assert_eq!(
        game.activate_initial_floor(),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
    assert_eq!(room_gold_stage([1, 1]).maximum_decisions(usize::MAX), None);
    let mut plan = plan(vec![]);
    for i in 0..MAX_STAGES {
        let mut stage = room_gold_stage([1, 1]);
        stage.id = format!("room-gold-{i}");
        plan.profiles
            .get_mut("baseline")
            .unwrap()
            .stages
            .push(stage);
    }
    assert!(plan.profiles["baseline"].fits_scope(8));
    assert!(!plan.profiles["baseline"].fits_scope(9));
}

#[test]
fn room_gold_deep_quantities_and_receipts_survive_consumption_and_restore() {
    for quantity in [gold_quantity(), QuantityExpr::Fixed { quantity: u64::MAX }] {
        let mut game = room_gold_game(42, [1, 1], &[1]);
        let state = game.floor_activation.as_mut().unwrap();
        let PopulationContent::RoomItems { bands, .. } =
            &mut state.plan.profiles.get_mut("baseline").unwrap().stages[0].population
        else {
            unreachable!()
        };
        bands[0].choices[0].quantity = quantity;
        let mut prepared = (*state.scopes.remove("dungeon/1").unwrap()).clone();
        prepared.scope = format!("dungeon/{}", u32::MAX);
        prepared.depth = u32::MAX;
        state.members.insert(RegionId(1), prepared.scope.clone());
        state
            .scopes
            .insert(prepared.scope.clone(), Shared::new(prepared));
        game.activate_initial_floor().unwrap();
        let record = game.activations_since(0).unwrap().remove(0);
        let amount = game.items[&ItemId(1)].quantity;
        let range = quantity.resolve(Some(u32::MAX)).unwrap();
        assert!((range.minimum..=range.maximum).contains(&amount));
        game.items
            .edit(ItemId(1), |item| item.quantity = 1)
            .unwrap();
        game.validate_activation_record(&record).unwrap();
        let mut forged = record.clone();
        let StageResult::RoomsEvaluated { rooms } = &mut forged.decisions[0].result else {
            panic!()
        };
        let RoomResult::Attempt {
            decision:
                PlacementDecision::Placed {
                    entity: PopulationEntity::Item { quantity, .. },
                    ..
                },
        } = &mut rooms[0].result
        else {
            panic!()
        };
        *quantity = if amount == 2 { 3 } else { 2 };
        assert!(game.validate_activation_record(&forged).is_err());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
        assert_eq!(restored, game);
    }
}

#[test]
fn appended_gold_stage_preserves_historical_population_and_birth_stock_streams() {
    let mut reference = None;
    for append in [false, true] {
        let mut game = game();
        game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut policy = carried_plan([1, 1]);
        if append {
            policy
                .profiles
                .get_mut("baseline")
                .unwrap()
                .stages
                .push(room_gold_stage([1, 2]));
        }
        game.configure_population(policy).unwrap();
        let mut prepared = scope(1, vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1)]);
        prepared.rooms = vec![PreparedRoomGroup {
            id: 1,
            cells: prepared.cells.clone(),
        }];
        game.register_activation_scope(prepared).unwrap();
        game.activate_initial_floor().unwrap();
        let facts = (
            game.activations_since(0).unwrap()[0].decisions[0].clone(),
            game.actors.clone(),
            game.rules_rng,
        );
        if let Some(reference) = &reference {
            assert_eq!(&facts, reference);
        } else {
            reference = Some(facts);
        }
    }
}

#[test]
fn contextual_birth_purse_is_finite_at_birth_and_drops_without_resampling() {
    for depth in [1, 52, u32::MAX] {
        let species = crate::test_creatures::species();
        let content = crate::test_creatures::content_with(species.clone(), vec![]);
        let build = crate::test_creatures::build(species);
        let mut game = Game::region_corridor(42, 2, content);
        game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
            .unwrap();
        game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut policy = carried_plan([1, 1]);
        policy
            .carried_item_pools
            .get_mut("supplies")
            .unwrap()
            .choices[0]
            .quantity = gold_quantity();
        let definition = &mut carrier_choice(&mut policy).actor;
        definition.build = Some(crate::creatures::record::BuildRecord::capture(&build));
        definition.identity = Some(crate::CreatureIdentity {
            name: "carrier".into(),
            faction: "neutral".into(),
        });
        game.configure_population(policy).unwrap();
        let mut prepared = scope(1, vec![at(1, 2, 1)]);
        prepared.scope = format!("dungeon/{depth}");
        prepared.depth = depth;
        game.register_activation_scope(prepared).unwrap();
        game.activate_initial_floor().unwrap();
        let amount = game.items[&ItemId(1)].quantity;
        assert!((2..=51 + 10 * u64::from(depth)).contains(&amount));
        let birth = game.activations_since(0).unwrap().remove(0);
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
        assert_eq!(
            game.items[&ItemId(1)].location,
            ItemLocation::Ground(at(1, 2, 1))
        );
        assert_eq!(game.items[&ItemId(1)].quantity, amount);
        game.validate_activation_record(&birth).unwrap();
        let before = game.clone();
        game.activate_initial_floor().unwrap();
        assert_eq!(game, before);
    }
}

#[test]
fn contextual_named_birth_requires_explicit_depth_even_when_chance_misses() {
    let mut game = living_carrier_game([0, 1], false);
    game.floor_activation
        .as_mut()
        .unwrap()
        .plan
        .carried_item_pools
        .get_mut("supplies")
        .unwrap()
        .choices[0]
        .quantity = gold_quantity();
    let before = game.clone();
    assert!(game
        .spawn_named_population_actor("baseline", "carriers", "carrier", None, at(1, 4, 1))
        .is_err());
    assert_eq!(game, before);
    assert!(game
        .spawn_named_population_actor("baseline", "carriers", "carrier", Some(52), at(1, 4, 1))
        .is_ok());
}
