// Bounded shared-room births use existing item/actor authority and rollback.
fn cluster_stage() -> PopulationStage {
    let items = match item_stage(1).population {
        PopulationContent::Items { bands } => bands,
        _ => unreachable!(),
    };
    let actors = match carried_plan([1, 1]).profiles["baseline"].stages[0]
        .population
        .clone()
    {
        PopulationContent::Actors { bands, .. } => bands,
        _ => unreachable!(),
    };
    serde_json::from_value(serde_json::json!({
        "id":"treasure", "version":1, "count":[1,1],
        "population":{"kind":"room_cluster", "chance":{"numerator":1,"denominator":1},
            "item_count":[2,2], "actor_count":[3,3], "actor_depth_offset":1,
            "item_bands":items, "actor_bands":actors, "avoid_protagonist_sight":false}
    }))
    .expect("shared room cluster is a supported bounded population stage")
}

fn cluster_game() -> Game {
    let mut game = game();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut definition = carried_plan([1, 1]);
    definition.profiles.get_mut("baseline").unwrap().stages = vec![cluster_stage()];
    game.configure_population(definition).unwrap();
    let mut prepared = scope(1, vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1)]);
    prepared.rooms = vec![PreparedRoomGroup {
        id: 3,
        cells: prepared.cells.clone(),
    }];
    game.register_activation_scope(prepared).unwrap();
    game
}

#[test]
fn room_cluster_shares_one_stable_room_and_keeps_real_floor_depth() {
    let mut game = cluster_game();
    let rng = game.rules_rng;
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    let value = serde_json::to_value(&record).unwrap();
    let result = &value["decisions"][0]["result"]["selection"];
    assert_eq!(result["kind"], "selected");
    assert_eq!(result["room"], 3);
    assert_eq!(result["actor_depth"], 2);
    assert_eq!(result["item_requested"], 2);
    assert_eq!(result["actor_requested"], 3);
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert_eq!(result["actors"].as_array().unwrap().len(), 3);
    assert_eq!(record.scope, "dungeon/1");
    assert_eq!(game.activation_scope("dungeon/1").unwrap().depth, 1);
    assert_eq!(record.decisions[0].result.attempts().count(), 5);
    assert_eq!(record.decisions[0].decision_count(), 10);
    assert_eq!(game.rules_rng, rng);
    game.validate_activation_record(&record).unwrap();
    assert!(game.activation_state_valid());
    let before = game.clone();
    game.activate_initial_floor().unwrap();
    assert_eq!(game, before);
}

#[test]
fn room_cluster_miss_absence_and_protected_skips_are_distinct() {
    for case in 0..3 {
        let mut game = cluster_game();
        if case == 0 {
            let state = game.floor_activation.as_mut().unwrap();
            let PopulationContent::RoomCluster { chance, .. } =
                &mut state.plan.profiles.get_mut("baseline").unwrap().stages[0].population
            else {
                panic!()
            };
            chance.numerator = 0;
        } else {
            let scope = game
                .floor_activation
                .as_mut()
                .unwrap()
                .scopes
                .get_mut("dungeon/1")
                .unwrap();
            if case == 1 {
                scope.rooms.clear();
            } else {
                scope.protected = scope.cells.iter().copied().collect();
            }
        }
        game.activate_initial_floor().unwrap();
        let record = game.activations_since(0).unwrap().remove(0);
        let StageResult::RoomCluster { selection } = &record.decisions[0].result else {
            panic!()
        };
        match (case, selection) {
            (0, ClusterSelection::ChanceMiss {}) | (1, ClusterSelection::NoEligibleRoom {}) => {
                assert_eq!(record.decisions[0].decision_count(), 2)
            }
            (2, ClusterSelection::Selected { items, actors, .. }) => {
                assert_eq!(items.len(), 2);
                assert_eq!(actors.len(), 3);
                assert!(items.iter().chain(actors).all(|attempt| matches!(
                    attempt,
                    PlacementDecision::Skipped {
                        reason: SkipReason::NoEligibleCell,
                        ..
                    }
                )));
                assert_eq!(record.decisions[0].decision_count(), 7);
            }
            _ => panic!("wrong cluster outcome"),
        }
        assert_eq!(game.next_item_id, 1);
        assert_eq!(game.next_actor_id, 2);
        assert!(game.activation_state_valid());
    }
}

#[test]
fn room_cluster_retains_requested_counts_before_area_caps() {
    let mut game = cluster_game();
    let state = game.floor_activation.as_mut().unwrap();
    let PopulationContent::RoomCluster {
        item_count,
        actor_count,
        ..
    } = &mut state.plan.profiles.get_mut("baseline").unwrap().stages[0].population
    else {
        panic!()
    };
    *item_count = [9, 9];
    *actor_count = [11, 11];
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    let StageResult::RoomCluster {
        selection:
            ClusterSelection::Selected {
                room_area,
                item_requested,
                actor_requested,
                items,
                actors,
                ..
            },
    } = &record.decisions[0].result
    else {
        panic!()
    };
    assert_eq!((*room_area, *item_requested, *actor_requested), (3, 9, 11));
    assert_eq!((items.len(), actors.len()), (3, 3));
    let item_cells: BTreeSet<_> = items
        .iter()
        .filter_map(|attempt| match attempt {
            PlacementDecision::Placed { location, .. } => Some(*location),
            _ => None,
        })
        .collect();
    let actor_cells: BTreeSet<_> = actors
        .iter()
        .filter_map(|attempt| match attempt {
            PlacementDecision::Placed { location, .. } => Some(*location),
            _ => None,
        })
        .collect();
    assert_eq!(item_cells.len(), 3);
    assert_eq!(
        actor_cells, item_cells,
        "ordinary actors may stand on room loot"
    );
    assert!(game.activation_state_valid());
}

#[test]
fn room_cluster_preflights_mixed_decisions_and_identity_failure_atomically() {
    for budget_failure in [true, false] {
        let mut game = cluster_game();
        if budget_failure {
            game.floor_activation.as_mut().unwrap().decision_count = MAX_RETAINED_DECISIONS - 9;
        } else {
            game.next_actor_id = u64::MAX;
        }
        let before = game.clone();
        assert_eq!(
            game.activate_initial_floor(),
            Err(GameError::IdentityExhausted)
        );
        assert_eq!(
            game, before,
            "no item births before later actor refusal may escape"
        );
    }
    let mut definition = carried_plan([1, 1]);
    definition.profiles.get_mut("baseline").unwrap().stages = vec![cluster_stage()];
    let stage = &mut definition.profiles.get_mut("baseline").unwrap().stages[0];
    let PopulationContent::RoomCluster {
        item_count,
        actor_count,
        ..
    } = &mut stage.population
    else {
        panic!()
    };
    *item_count = [128, 128];
    *actor_count = [129, 129];
    let mut game = game();
    let before = game.clone();
    assert!(game.configure_population(definition).is_err());
    assert_eq!(game, before);
}

#[test]
fn room_cluster_only_uses_the_selected_room_and_whole_actor_footprints() {
    let mut game = cluster_game();
    let state = game.floor_activation.as_mut().unwrap();
    let prepared = state.scopes.get_mut("dungeon/1").unwrap();
    prepared.rooms = vec![
        PreparedRoomGroup {
            id: 1,
            cells: vec![at(1, 2, 1)],
        },
        PreparedRoomGroup {
            id: 3,
            cells: vec![at(1, 4, 1)],
        },
    ];
    let PopulationContent::RoomCluster { actor_bands, .. } =
        &mut state.plan.profiles.get_mut("baseline").unwrap().stages[0].population
    else {
        panic!()
    };
    actor_bands[0].choices[0].actor.body = Some(BodySpec {
        cells: vec![[0, 0, 0], [1, 0, 0]],
        eye: [0, 0, 0],
        mass: 80,
    });
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    let StageResult::RoomCluster {
        selection:
            ClusterSelection::Selected {
                room,
                items,
                actors,
                ..
            },
    } = &record.decisions[0].result
    else {
        panic!()
    };
    assert!([1, 3].contains(room));
    assert_eq!((items.len(), actors.len()), (1, 1));
    assert!(matches!(
        &actors[0],
        PlacementDecision::Skipped {
            reason: SkipReason::NoEligibleCell,
            ..
        }
    ));
    let PlacementDecision::Placed { location, .. } = &items[0] else {
        panic!()
    };
    assert_eq!(location.position.x, if *room == 1 { 2 } else { 4 });
    assert_eq!(
        game.next_actor_id, 2,
        "cannot spill into an adjacent non-room floor cell"
    );
    assert!(game.activation_state_valid());
}

#[test]
fn room_cluster_actor_band_uses_checked_effective_depth_and_item_band_real_depth() {
    let mut stage = cluster_stage();
    let PopulationContent::RoomCluster {
        item_bands,
        actor_bands,
        ..
    } = &mut stage.population
    else {
        panic!()
    };
    item_bands[0].depth = DepthInterval::Finite {
        minimum: 1,
        maximum: 1,
    };
    actor_bands[0].depth = DepthInterval::Finite {
        minimum: 2,
        maximum: 2,
    };
    assert!(stage.population.eligible(1));
    assert!(!stage.population.eligible(2));
    assert!(!cluster_stage().population.eligible(u32::MAX));
    let mut game = cluster_game();
    game.floor_activation
        .as_mut()
        .unwrap()
        .plan
        .profiles
        .get_mut("baseline")
        .unwrap()
        .stages[0] = stage;
    game.activate_initial_floor().unwrap();
    assert_eq!(game.next_actor_id, 5);
    assert!(game.activation_state_valid());
}

#[test]
fn room_cluster_restores_strict_nested_receipts_after_consumption() {
    let mut game = cluster_game();
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    let original = serde_json::to_value(&record).unwrap();
    let good: FloorActivationRecord = serde_json::from_value(original.clone()).unwrap();
    game.validate_activation_record(&good).unwrap();
    for (field, value) in [
        ("room", serde_json::json!(1)),
        ("room_area", serde_json::json!(2)),
        ("actor_depth", serde_json::json!(1)),
        ("item_requested", serde_json::json!(1)),
        ("actor_requested", serde_json::json!(2)),
        ("items", serde_json::json!([])),
    ] {
        let mut bad = original.clone();
        bad["decisions"][0]["result"]["selection"][field] = value;
        let bad: FloorActivationRecord = serde_json::from_value(bad).unwrap();
        assert!(game.validate_activation_record(&bad).is_err(), "{field}");
    }
    for case in 0..4 {
        let mut bad = original.clone();
        let selected = &mut bad["decisions"][0]["result"]["selection"];
        match case {
            0 => {
                selected["unexpected"] = serde_json::json!(true);
            }
            1 => {
                selected.as_object_mut().unwrap().remove("room_area");
            }
            2 => {
                selected["actors"][0]["entity"]["carried_loot"]["unexpected"] =
                    serde_json::json!(1);
            }
            3 => {
                *selected = serde_json::json!([3, 3, 2, 2, 3, [], []]);
            }
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<FloorActivationRecord>(bad).is_err(),
            "case {case}"
        );
    }
    let item = record.decisions[0]
        .result
        .attempts()
        .find_map(|attempt| match attempt {
            PlacementDecision::Placed {
                entity: PopulationEntity::Item { id, .. },
                ..
            } => Some(*id),
            _ => None,
        })
        .unwrap();
    game.items.remove(&item);
    game.validate_activation_record(&record).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
    assert_eq!(restored, game);
}

#[test]
fn room_cluster_birth_stock_uses_effective_depth_without_changing_item_quantities() {
    let mut witnessed_next_depth = false;
    for seed in 0..32 {
        let mut game = cluster_game();
        game.seed = seed;
        let state = game.floor_activation.as_mut().unwrap();
        state
            .plan
            .carried_item_pools
            .get_mut("supplies")
            .unwrap()
            .choices[0]
            .quantity = QuantityExpr::DepthScaledUniform {
            minimum: 1,
            maximum_base: 0,
            per_depth: 1,
        };
        let PopulationContent::RoomCluster { item_bands, .. } =
            &mut state.plan.profiles.get_mut("baseline").unwrap().stages[0].population
        else {
            panic!()
        };
        item_bands[0].choices[0].quantity = QuantityExpr::DepthScaledUniform {
            minimum: 1,
            maximum_base: 0,
            per_depth: 1,
        };
        game.activate_initial_floor().unwrap();
        for item in game.items.values() {
            match item.location {
                ItemLocation::Carried(_) => {
                    assert!((1..=2).contains(&item.quantity));
                    witnessed_next_depth |= item.quantity == 2;
                }
                ItemLocation::Ground(_) => assert_eq!(item.quantity, 1),
            }
        }
        assert!(game.activation_state_valid());
    }
    assert!(
        witnessed_next_depth,
        "depth-one birth stock cannot produce two units"
    );
}

#[test]
fn appended_room_cluster_preserves_prior_stage_streams_and_npc_entry_never_activates() {
    let mut original = cluster_game();
    original
        .floor_activation
        .as_mut()
        .unwrap()
        .plan
        .profiles
        .get_mut("baseline")
        .unwrap()
        .stages = vec![item_stage(1)];
    let mut extended = original.clone();
    extended
        .floor_activation
        .as_mut()
        .unwrap()
        .plan
        .profiles
        .get_mut("baseline")
        .unwrap()
        .stages
        .push(cluster_stage());
    original.activate_initial_floor().unwrap();
    extended.activate_initial_floor().unwrap();
    assert_eq!(
        original.activations_since(0).unwrap()[0].decisions[0],
        extended.activations_since(0).unwrap()[0].decisions[0]
    );
    let mut unopened = cluster_game();
    let npc = unopened
        .spawn_actor(at(2, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    unopened.teleport(npc, at(1, 2, 1)).unwrap();
    assert_eq!(unopened.activation_count(), 0);
    assert_eq!(unopened.next_item_id, 1);
}

#[test]
fn room_cluster_retains_birth_evidence_after_actor_death_and_rejects_duplicate_ids() {
    let species = crate::test_creatures::species();
    let content = crate::test_creatures::content_with(species.clone(), vec![]);
    let build = crate::test_creatures::build(species);
    let mut game = Game::region_corridor(42, 2, content);
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
        .unwrap();
    game.spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut definition = carried_plan([1, 1]);
    let mut stage = cluster_stage();
    let PopulationContent::RoomCluster { actor_bands, .. } = &mut stage.population else {
        panic!()
    };
    actor_bands[0].choices[0].actor.build =
        Some(crate::creatures::record::BuildRecord::capture(&build));
    actor_bands[0].choices[0].actor.identity = Some(crate::CreatureIdentity {
        name: "cluster carrier".into(),
        faction: "neutral".into(),
    });
    definition.profiles.get_mut("baseline").unwrap().stages = vec![stage];
    game.configure_population(definition).unwrap();
    let mut prepared = scope(1, vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1)]);
    prepared.rooms = vec![PreparedRoomGroup {
        id: 3,
        cells: prepared.cells.clone(),
    }];
    game.register_activation_scope(prepared).unwrap();
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    game.validate_activation_record_identity_bounds(&record)
        .unwrap();
    let mut duplicated = record.clone();
    let StageResult::RoomCluster {
        selection: ClusterSelection::Selected { actors, .. },
    } = &mut duplicated.decisions[0].result
    else {
        panic!()
    };
    let PlacementDecision::Placed {
        entity: PopulationEntity::Actor { id, .. },
        ..
    } = &mut actors[1]
    else {
        panic!()
    };
    *id = ActorId(2);
    assert!(game
        .validate_activation_record_identity_bounds(&duplicated)
        .is_err());
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
    game.validate_activation_record(&record).unwrap();
    game.validate_activation_record_identity_bounds(&record)
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
    assert_eq!(restored, game);
}

#[test]
fn room_cluster_rational_chance_is_replay_stable_and_has_both_outcomes() {
    let mut hits = 0;
    for seed in 0..32 {
        let mut game = cluster_game();
        game.seed = seed;
        let PopulationContent::RoomCluster { chance, .. } = &mut game
            .floor_activation
            .as_mut()
            .unwrap()
            .plan
            .profiles
            .get_mut("baseline")
            .unwrap()
            .stages[0]
            .population
        else {
            panic!()
        };
        chance.denominator = 2;
        let mut independent = game.clone();
        game.activate_initial_floor().unwrap();
        independent.activate_initial_floor().unwrap();
        assert_eq!(game, independent);
        let record = game.activations_since(0).unwrap().remove(0);
        hits += usize::from(matches!(
            record.decisions[0].result,
            StageResult::RoomCluster {
                selection: ClusterSelection::Selected { .. }
            }
        ));
        assert!(game.activation_state_valid());
    }
    assert!(
        (5..27).contains(&hits),
        "independent rational chance witnesses both outcomes: {hits}"
    );
}
