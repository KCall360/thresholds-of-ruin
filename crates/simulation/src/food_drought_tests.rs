// Included by floor_activation::tests.

fn food_choice(content: &str) -> PreparedItemChoice {
    let mut spec = ItemSpec::ordinary(content.into());
    spec.stackable = true;
    PreparedItemChoice {
        content: content.into(),
        weight: 1,
        quantity: QuantityExpr::Fixed { quantity: 1 },
        spec,
    }
}

fn drought_stage(threshold: u16) -> PopulationStage {
    PopulationStage {
        id: "drought".into(),
        version: 1,
        count: [1, 1],
        population: PopulationContent::FoodDrought {
            bands: vec![DepthBand {
                depth: DepthInterval::Open { minimum: 1 },
                choices: vec![food_choice("ration")],
            }],
            threshold,
            food: vec!["fruit".into(), "ration".into()],
        },
    }
}

fn chance_stage(attempts: u16, numerator: u32, denominator: u32) -> PopulationStage {
    PopulationStage {
        id: "chance".into(),
        version: 1,
        count: [attempts, attempts],
        population: PopulationContent::ChanceItems {
            bands: vec![DepthBand {
                depth: DepthInterval::Open { minimum: 1 },
                choices: vec![food_choice("marker")],
            }],
            chance: CarryChance {
                numerator,
                denominator,
            },
        },
    }
}

fn floor_record(depth: u32, ordinal: u64, placed: Option<&str>) -> FloorActivationRecord {
    let attempts: Vec<_> = placed
        .map(|content| PlacementDecision::Placed {
            content: content.into(),
            location: at(1, 1, 1),
            entity: PopulationEntity::Item {
                id: ItemId(1000 + ordinal),
                quantity: 1,
            },
        })
        .into_iter()
        .collect();
    FloorActivationRecord {
        scope: format!("dungeon/{depth}"),
        entrant: ActorId(1),
        tick: 0,
        ordinal,
        cause: EntryCause::Initial,
        profile: "baseline".into(),
        profile_version: 1,
        decisions: vec![StageDecision {
            id: "stock".into(),
            version: 1,
            result: StageResult::Evaluated {
                requested: attempts.len() as u16,
                attempts,
            },
        }],
    }
}

/// A game whose retained first-entry history is exactly `history`.
fn with_history(stages: Vec<PopulationStage>, history: &[Option<&str>]) -> (Game, PopulationProfile) {
    let mut game = game();
    game.configure_population(plan(stages)).unwrap();
    let state = game.floor_activation.as_mut().unwrap();
    let mut floors = BTreeMap::new();
    let mut order = Vec::new();
    for (ordinal, placed) in history.iter().enumerate() {
        let record = floor_record(ordinal as u32 + 1, ordinal as u64, *placed);
        order.push(record.scope.clone());
        floors.insert(record.scope.clone(), Shared::new(record));
    }
    state.floors = Shared::new(floors);
    state.order = Shared::new(order);
    let profile = state.plan.profiles["baseline"].clone();
    (game, profile)
}

fn drought_request(history: &[Option<&str>], earlier: &[StageDecision], threshold: u16) -> u16 {
    let (game, profile) = with_history(vec![drought_stage(threshold)], history);
    game.requested_count(
        "dungeon/99",
        "baseline",
        &profile,
        &profile.stages[0],
        history.len() as u64,
        earlier,
    )
    .unwrap()
}

#[test]
fn drought_places_food_only_after_more_than_threshold_foodless_floors() {
    // Rogue's threshold of three: a fourth consecutive foodless floor is not
    // enough on its own, the fifth entry after them is guaranteed.
    assert_eq!(drought_request(&[None; 3], &[], 3), 0);
    assert_eq!(drought_request(&[None; 4], &[], 3), 1);
    assert_eq!(drought_request(&[None; 9], &[], 3), 1);
    // Food on any later floor resets the run, whichever food content it was.
    assert_eq!(drought_request(&[None, None, None, None, Some("fruit")], &[], 3), 0);
    assert_eq!(
        drought_request(&[None, None, None, None, Some("ration"), None, None], &[], 3),
        0
    );
    // Non-food placements do not count as food.
    assert_eq!(drought_request(&[Some("marker"); 4], &[], 3), 1);
    // Earlier stages on the same floor that placed food satisfy the guarantee.
    let fed = floor_record(99, 99, Some("ration")).decisions;
    assert_eq!(drought_request(&[None; 4], &fed, 3), 0);
    // A zero threshold guarantees food after one foodless floor.
    assert_eq!(drought_request(&[], &[], 0), 0);
    assert_eq!(drought_request(&[None], &[], 0), 1);
}

#[test]
fn drought_count_uses_only_floors_entered_before_the_audited_record() {
    let history = [None, None, None, None, Some("ration")];
    let (game, profile) = with_history(vec![drought_stage(3)], &history);
    // Auditing the fifth record (ordinal 4) sees only the four floors before it.
    let request = |ordinal| {
        game.requested_count("dungeon/5", "baseline", &profile, &profile.stages[0], ordinal, &[])
            .unwrap()
    };
    assert_eq!(request(4), 1);
    assert_eq!(request(3), 0);
    assert!(game
        .requested_count("dungeon/9", "baseline", &profile, &profile.stages[0], 9, &[])
        .is_err());
}

#[test]
fn chance_items_count_successes_of_exact_attempts_deterministically() {
    let (game, profile) = with_history(vec![chance_stage(9, 36, 100)], &[]);
    let stage = &profile.stages[0];
    let counts: Vec<u16> = (1..=200)
        .map(|depth| {
            game.requested_count(&format!("dungeon/{depth}"), "baseline", &profile, stage, 0, &[])
                .unwrap()
        })
        .collect();
    assert!(counts.iter().all(|count| *count <= 9));
    let mean = counts.iter().map(|count| f64::from(*count)).sum::<f64>() / counts.len() as f64;
    assert!((2.5..4.0).contains(&mean), "9 x 36% averages 3.24, got {mean}");
    // The same scope and stage always request the same count.
    assert_eq!(
        game.requested_count("dungeon/7", "baseline", &profile, stage, 0, &[]),
        game.requested_count("dungeon/7", "baseline", &profile, stage, 0, &[])
    );
    let (never, profile) = with_history(vec![chance_stage(9, 0, 1)], &[]);
    assert_eq!(
        never
            .requested_count("dungeon/1", "baseline", &profile, &profile.stages[0], 0, &[])
            .unwrap(),
        0
    );
}

#[test]
fn invalid_drought_and_chance_stages_are_rejected() {
    let mut game = game();
    let mut empty_food = drought_stage(3);
    if let PopulationContent::FoodDrought { food, .. } = &mut empty_food.population {
        food.clear();
    }
    assert!(game.configure_population(plan(vec![empty_food])).is_err());
    let mut repeated = drought_stage(3);
    repeated.count = [1, 2];
    assert!(game.configure_population(plan(vec![repeated])).is_err());
    let mut ranged = chance_stage(9, 36, 100);
    ranged.count = [0, 9];
    assert!(game.configure_population(plan(vec![ranged])).is_err());
    assert!(game.configure_population(plan(vec![chance_stage(9, 101, 100)])).is_err());
    assert!(game.configure_population(plan(vec![drought_stage(3), chance_stage(9, 36, 100)])).is_ok());
}

#[test]
fn first_entry_evaluates_and_audits_chance_and_drought_stages() {
    let mut game = game();
    game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    // Threshold zero with no history: no earlier floor, so nothing is forced.
    game.configure_population(plan(vec![chance_stage(4, 1, 1), drought_stage(0)]))
        .unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1), at(1, 3, 1), at(1, 4, 1), at(1, 5, 1)]))
        .unwrap();
    game.activate_initial_floor().unwrap();
    let records = game.activations_since(0).unwrap();
    assert!(matches!(
        &records[0].decisions[0].result,
        StageResult::Evaluated { requested: 4, attempts } if attempts.len() == 4
    ));
    assert!(matches!(
        records[0].decisions[1].result,
        StageResult::Evaluated { requested: 0, .. }
    ));
    let mut record = records[0].clone();
    game.validate_activation_record(&record).unwrap();
    // A forged drought request is rejected by the shared count derivation.
    if let StageResult::Evaluated { requested, .. } = &mut record.decisions[1].result {
        *requested = 1;
    }
    assert!(game.validate_activation_record(&record).is_err());
}
