// Newly reconstructed tests. The retained implementation excerpt omitted this file.

#[test]
fn depth_intervals_keep_finite_endpoints_gaps_and_the_full_open_u32_tail() {
    let finite = DepthInterval::Finite {
        minimum: 2,
        maximum: 26,
    };
    assert!(finite.valid());
    assert!(!finite.contains(1));
    assert!(finite.contains(2));
    assert!(finite.contains(26));
    assert!(!finite.contains(27));
    let open = DepthInterval::Open { minimum: 28 };
    assert!(!open.contains(27));
    assert!(open.contains(u32::MAX));
    assert!(!DepthInterval::Open { minimum: 0 }.valid());
    assert!(!DepthInterval::Finite {
        minimum: 3,
        maximum: 2
    }
    .valid());
}

#[test]
fn weighted_content_mapping_uses_one_word_with_exact_cumulative_boundaries() {
    for (word, expected) in [
        (0, 0),
        ((1u64 << 62) - 1, 0),
        (1u64 << 62, 1),
        (u64::MAX, 3),
    ] {
        assert_eq!(bucket(word, 4), expected);
    }
    let choices = [("one", 1), ("three", 3)];
    let mut state = 42;
    let mut reference = state;
    let word = crate::dice::random_word(&mut reference);
    let selected = weighted_choice(&mut state, &choices, |row| row.1);
    assert_eq!(
        selected.0,
        if word < (1u64 << 62) { "one" } else { "three" }
    );
    assert_eq!(
        state, reference,
        "exactly one content-stream word is consumed"
    );
    let before = state;
    let _ = weighted_choice(&mut state, &choices[..1], |row| row.1);
    let mut reference = before;
    crate::dice::random_word(&mut reference);
    assert_eq!(state, reference, "singleton content also consumes one word");
}

#[test]
fn weighted_depth_validation_rejects_overlap_empty_zero_and_overflow_atomically() {
    for case in 0..7 {
        let mut game = game();
        let before = game.clone();
        let mut stage = item_stage(1);
        let PopulationContent::Items { bands } = &mut stage.population else {
            unreachable!()
        };
        match case {
            0 => bands[0].choices[0].weight = 0,
            1 => bands[0].choices.clear(),
            2 => bands.clear(),
            3 => {
                bands[0].depth = DepthInterval::Finite {
                    minimum: 3,
                    maximum: 2,
                }
            }
            4 => bands.push(bands[0].clone()),
            5 => {
                bands[0].choices[0].weight = u32::MAX;
                let mut other = bands[0].choices[0].clone();
                other.content = "other".into();
                bands[0].choices.push(other);
            }
            6 => {
                let duplicate = bands[0].choices[0].clone();
                bands[0].choices.push(duplicate);
            }
            _ => unreachable!(),
        }
        assert!(
            game.configure_population(plan(vec![stage])).is_err(),
            "case {case}"
        );
        assert_eq!(game, before, "case {case}");
    }
}

#[test]
fn weighted_depth_gap_receipt_survives_revisit_and_checkpoint_without_rng_or_stock() {
    let mut game = game();
    game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut stage = item_stage(1);
    let PopulationContent::Items { bands } = &mut stage.population else {
        unreachable!()
    };
    bands[0].depth = DepthInterval::Finite {
        minimum: 1,
        maximum: 1,
    };
    let mut tail = bands[0].clone();
    tail.depth = DepthInterval::Open { minimum: 3 };
    bands.push(tail);
    game.configure_population(plan(vec![stage])).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
        .unwrap();
    game.register_activation_scope(scope(2, vec![at(2, 2, 1)]))
        .unwrap();
    let rng = game.rules_rng;
    game.activate_initial_floor().unwrap();
    game.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    let records = game.activations_since(0).unwrap();
    assert_eq!(
        records[1].decisions[0].result,
        StageResult::Skipped {
            reason: SkipReason::NoEligibleContent
        }
    );
    assert_eq!(game.rules_rng, rng);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
    restored.teleport(ActorId(1), at(1, 0, 1)).unwrap();
    restored.teleport(ActorId(1), at(2, 0, 1)).unwrap();
    assert_eq!(restored.activations_since(0).unwrap(), records);
    assert_eq!(restored.next_item_id, game.next_item_id);
    assert_eq!(restored.rules_rng, rng);
}

#[test]
fn weighted_choices_do_not_change_count_cell_or_independent_supply_streams() {
    fn configured(weight: u32) -> Game {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut first = item_stage(1);
        first.count = [1, 3];
        let PopulationContent::Items { bands } = &mut first.population else {
            unreachable!()
        };
        bands[0].choices[0].weight = weight;
        let mut stock = item_stage(1);
        stock.id = "unchanged-supply".into();
        game.configure_population(plan(vec![first, stock])).unwrap();
        game.register_activation_scope(scope(
            1,
            vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1), at(1, 5, 1)],
        ))
        .unwrap();
        game.activate_initial_floor().unwrap();
        game
    }
    let unit = configured(1);
    let heavier = configured(u32::MAX);
    assert_eq!(unit.activations_since(0), heavier.activations_since(0));
    assert_eq!(unit.items, heavier.items);
    assert_eq!(unit.rules_rng, heavier.rules_rng);
}

#[test]
fn weighted_actor_and_item_receipts_bind_selected_content_and_survive_restore() {
    let mut game = game();
    game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
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
    let stage = PopulationStage {
        id: "weighted-actors".into(),
        version: 1,
        count: [1, 1],
        population: PopulationContent::Actors {
            bands: vec![DepthBand {
                depth: DepthInterval::Open { minimum: 1 },
                choices: vec![
                    PreparedActorChoice {
                        content: "first".into(),
                        weight: 1,
                        turn_ticks: 100,
                        actor: actor.clone(),
                    },
                    PreparedActorChoice {
                        content: "second".into(),
                        weight: 3,
                        turn_ticks: 100,
                        actor,
                    },
                ],
            }],
            avoid_protagonist_sight: false,
        },
    };
    game.configure_population(plan(vec![stage, item_stage(1)]))
        .unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1), at(1, 3, 1)]))
        .unwrap();
    game.activate_initial_floor().unwrap();
    let record = game.activations_since(0).unwrap().remove(0);
    game.validate_activation_record(&record).unwrap();
    let mut forged = record.clone();
    let StageResult::Evaluated { attempts, .. } = &mut forged.decisions[0].result else {
        panic!()
    };
    let PlacementDecision::Placed { content, .. } = &mut attempts[0] else {
        panic!()
    };
    *content = if content == "first" {
        "second"
    } else {
        "first"
    }
    .into();
    assert!(game.validate_activation_record(&forged).is_err());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
    assert_eq!(restored, game);
}

#[test]
fn count_and_cell_mapping_retains_its_published_seed_sequence() {
    let mut state = 42;
    let selected: Vec<_> = [5, 2, 13, 1, 7]
        .into_iter()
        .map(|size| index(&mut state, size))
        .collect();
    assert_eq!(selected, vec![3, 1, 10, 0, 6]);
    assert_eq!(state, 1663341875487337619);
}
