fn checkpoint_population_game() -> Game {
    checkpoint_population_game_with_wandering(false)
}

fn checkpoint_population_game_with_wandering(wandering: bool) -> Game {
    let mut game = game();
    let protagonist = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let witness = PopulationStage {
        id: "witness".into(),
        version: 1,
        count: [1, 1],
        population: PopulationContent::Actors {
            bands: vec![DepthBand {
                depth: DepthInterval::Open { minimum: 1 },
                choices: vec![PreparedActorChoice {
                    advancement: vec![],
                    carried_loot: None,
                    depth_growth: crate::prepared_spawn::DepthGrowth::Fixed {},
                    content: "witness".into(),
                    weight: 1,
                    turn_ticks: 100,
                    actor: PreparedActor {
                        build: None,
                        nutrition: crate::nutrition::NutritionRequirement::Exempt,
                        identity: None,
                        anatomy: None,
                        body: None,
                        ai: None,
                        known_identities: vec![],
                        velocity: None,
                        asset: None,
                    },
                }],
            }],
            avoid_protagonist_sight: false,
        },
    };
    game.configure_population(plan(vec![item_stage(1), witness]))
        .unwrap();
    if wandering {
        game.configure_wandering_population(WanderingPolicy {
            stage_id: "witness".into(),
            excluded_wandering_content: BTreeSet::new(),
            initial_delay: [6700, 7300],
            retry_ticks: 400,
            chance: CarryChance {
                numerator: 1,
                denominator: 6,
            },
            living_cap: 8,
        })
        .unwrap();
    }
    game.register_activation_scope(scope(1, vec![at(1, 2, 1), at(1, 3, 1)]))
        .unwrap();
    game.register_activation_scope(scope(2, vec![at(2, 2, 1)]))
        .unwrap();
    // Population configuration is admitted only at tick zero. Advance after
    // preparation so the receipt has a nonzero tick for boundary validation.
    game.act(protagonist, Action::Wait).unwrap();
    game.act(protagonist, Action::Wait).unwrap();
    assert!(game.tick() > 0);
    game.activate_initial_floor().unwrap();
    game
}

#[test]
fn checkpoint_population_pool_reuses_decoded_storage_and_isolates_mutation() {
    use crate::checkpoint::{RestoreContext, SharedState, Snapshot};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let first = game.checkpoint(&mut shared);
    let second = game.checkpoint(&mut shared);
    let encoded = serde_json::to_value((vec![first, second], shared)).unwrap();
    assert_eq!(
        encoded[0][0]["floor_activation"],
        serde_json::json!({"state":0,"wandering":null})
    );
    assert_eq!(
        encoded[0][1]["floor_activation"],
        serde_json::json!({"state":0,"wandering":null})
    );
    assert_eq!(encoded[1]["floor_activations"].as_array().unwrap().len(), 1);

    let (snapshots, shared): (Vec<Snapshot>, SharedState) =
        serde_json::from_value(encoded).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    context.bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>()).unwrap();
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|snapshot| context.restore(snapshot).unwrap())
        .collect();
    assert_eq!(restored[0], game);
    assert_eq!(restored[1], game);
    let left = restored[0].floor_activation.as_ref().unwrap();
    let right = restored[1].floor_activation.as_ref().unwrap();
    assert!(left.plan.shares_storage(&right.plan));
    assert!(left.scopes.shares_storage(&right.scopes));
    assert!(left.members.shares_storage(&right.members));
    assert!(left.floors.shares_storage(&right.floors));
    assert!(left.order.shares_storage(&right.order));
    drop(context);
    drop(shared);

    restored[0].teleport(ActorId(1), at(2, 0, 1)).unwrap();
    assert_eq!(restored[0].activation_count(), 2);
    assert_eq!(restored[1], game, "restored boundaries remain isolated");
    let mut shared = SharedState::default();
    let after = restored[0].checkpoint(&mut shared);
    let before = restored[1].checkpoint(&mut shared);
    let encoded = serde_json::to_value((vec![after, before], shared)).unwrap();
    assert_ne!(
        encoded[0][0]["floor_activation"],
        encoded[0][1]["floor_activation"]
    );
    assert_eq!(encoded[1]["floor_activations"].as_array().unwrap().len(), 2);
}

#[test]
fn checkpoint_population_pool_keeps_tick_and_identity_validation_per_boundary() {
    use crate::checkpoint::{PersistError, RestoreContext, SharedState, Snapshot};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared: SharedState =
        serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    context.bind_spatial_roots(&[&spatial_binding]).unwrap();
    assert_eq!(
        context
            .restore(serde_json::from_value(snapshot.clone()).unwrap())
            .unwrap(),
        game
    );
    assert_eq!(context.population_plan_validation_count(), 1);
    for (field, value) in [
        ("tick", game.tick() - 1),
        ("next_actor_id", 2),
        ("next_item_id", 1),
    ] {
        let mut invalid = snapshot.clone();
        invalid[field] = serde_json::json!(value);
        let invalid: Snapshot = serde_json::from_value(invalid).unwrap();
        assert_eq!(
            context.restore(invalid),
            Err(PersistError::Population),
            "{field}"
        );
        assert_eq!(context.population_plan_validation_count(), 1);
    }
}

#[test]
fn checkpoint_population_pool_rejects_invalid_references_and_unknown_fields() {
    use crate::checkpoint::{PersistError, RestoreContext, SharedState, Snapshot};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    context.bind_spatial_roots(&[&spatial_binding]).unwrap();
    for index in [1, usize::MAX] {
        let mut invalid = snapshot.clone();
        invalid["floor_activation"]["state"] = serde_json::json!(index);
        assert_eq!(
            context.restore(serde_json::from_value(invalid).unwrap()),
            Err(PersistError::MissingShared {
                table: "floor_activations",
                index
            })
        );
    }
    let mut invalid = snapshot.clone();
    invalid["floor_activation"] = serde_json::json!(-1);
    assert!(serde_json::from_value::<Snapshot>(invalid).is_err());
    let mut invalid = snapshot;
    invalid["unknown_population_field"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Snapshot>(invalid).is_err());

    let encoded = serde_json::to_value(&shared).unwrap();
    let mut invalid = encoded.clone();
    invalid["unknown_population_field"] = serde_json::json!([]);
    assert!(serde_json::from_value::<SharedState>(invalid).is_err());
    let mut invalid = encoded;
    invalid["floor_activations"][0]["unknown_population_field"] = serde_json::json!(0);
    assert!(serde_json::from_value::<SharedState>(invalid).is_err());
}

#[test]
fn checkpoint_population_pool_requires_strict_nested_nutrition_recipes() {
    use crate::checkpoint::{PersistError, SharedState};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let encoded = serde_json::to_value(&shared).unwrap();
    let recipe =
        "/floor_activations/0/plan/profiles/baseline/stages/1/population/bands/0/choices/0/actor";
    assert_eq!(
        encoded.pointer(recipe).unwrap()["nutrition"],
        serde_json::json!({"kind": "exempt"})
    );

    let mut missing = encoded.clone();
    missing
        .pointer_mut(recipe)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("nutrition");
    assert!(serde_json::from_value::<SharedState>(missing).is_err());
    for malformed in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!({"kind": "exempt", "reserve": 8}),
        serde_json::json!({"kind": "required", "elapsed": 0}),
    ] {
        let mut invalid = encoded.clone();
        invalid.pointer_mut(recipe).unwrap()["nutrition"] = malformed;
        assert!(serde_json::from_value::<SharedState>(invalid).is_err());
    }

    // The shape alone is legal, but a non-creature recipe under disabled
    // nutrition cannot enroll a required actor at a restored boundary.
    let mut invalid = encoded;
    invalid.pointer_mut(recipe).unwrap()["nutrition"] = serde_json::json!({"kind": "required"});
    let invalid: SharedState = serde_json::from_value(invalid).unwrap();
    assert_eq!(
        Game::restore_checkpoint(snapshot, &invalid, game.content.clone()),
        Err(PersistError::Population)
    );
}

#[test]
fn checkpoint_without_population_omits_population_pool_and_reference() {
    let game = game();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared = serde_json::to_value(shared).unwrap();
    assert!(snapshot.get("floor_activation").is_none());
    assert!(shared.get("floor_activations").is_none());
}

#[test]
fn checkpoint_population_plan_validation_is_once_per_decoded_pool_entry() {
    use crate::checkpoint::{RestoreContext, SharedState, Snapshot};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshots: Vec<_> = (0..8).map(|_| game.checkpoint(&mut shared)).collect();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) =
        serde_json::from_value(serde_json::to_value((snapshots, shared)).unwrap()).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    context.bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>()).unwrap();
    for snapshot in snapshots {
        assert_eq!(context.restore(snapshot).unwrap(), game);
    }
    assert_eq!(context.population_plan_validation_count(), 1);
}

#[test]
fn checkpoint_population_plan_cache_never_trusts_another_pool_or_context() {
    use crate::checkpoint::{PersistError, RestoreContext, SharedState};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut encoded = serde_json::to_value(shared).unwrap();
    let mut invalid = encoded["floor_activations"][0].clone();
    // Preserve profile and recipe labels; only the other immutable definition
    // is invalid. A successful sibling must never authorize this pool entry.
    invalid["plan"]["profiles"]["baseline"]["stages"][1]["population"]["bands"][0]["choices"][0]
        ["turn_ticks"] = serde_json::json!(0);
    encoded["floor_activations"]
        .as_array_mut()
        .unwrap()
        .push(invalid.clone());
    let shared: SharedState = serde_json::from_value(encoded.clone()).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    context.bind_spatial_roots(&[&spatial_binding]).unwrap();
    assert_eq!(
        context
            .restore(serde_json::from_value(snapshot.clone()).unwrap())
            .unwrap(),
        game
    );
    let mut another = snapshot.clone();
    another["floor_activation"]["state"] = serde_json::json!(1);
    for _ in 0..2 {
        assert_eq!(
            context.restore(serde_json::from_value(another.clone()).unwrap()),
            Err(PersistError::Population),
        );
    }
    assert_eq!(context.population_plan_validation_count(), 3);

    let mut fresh = RestoreContext::new(&shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    fresh.bind_spatial_roots(&[&spatial_binding]).unwrap();
    assert_eq!(
        fresh
            .restore(serde_json::from_value(snapshot.clone()).unwrap())
            .unwrap(),
        game
    );
    assert_eq!(fresh.population_plan_validation_count(), 1);

    // The same numeric pool index in another decoded checkpoint has no trust
    // relationship with the first context, even when Content is unchanged.
    encoded["floor_activations"][0] = invalid;
    let other_shared: SharedState = serde_json::from_value(encoded.clone()).unwrap();
    let mut other_context = RestoreContext::new(&other_shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    other_context.bind_spatial_roots(&[&spatial_binding]).unwrap();
    assert_eq!(
        other_context.restore(serde_json::from_value(snapshot).unwrap()),
        Err(PersistError::Population),
    );
    assert_eq!(other_context.population_plan_validation_count(), 1);
}

#[test]
fn checkpoint_population_cached_plan_still_checks_selected_character() {
    use crate::checkpoint::{PersistError, RestoreContext, SharedState};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared: SharedState =
        serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    context.bind_spatial_roots(&[&spatial_binding]).unwrap();
    assert_eq!(
        context
            .restore(serde_json::from_value(snapshot.clone()).unwrap())
            .unwrap(),
        game
    );
    let mut invalid = snapshot;
    invalid["scenario_policy"]["selected_character"] = serde_json::json!(2);
    assert_eq!(
        context.restore(serde_json::from_value(invalid).unwrap()),
        Err(PersistError::Population),
    );
    assert_eq!(context.population_plan_validation_count(), 1);
}

#[test]
fn wandering_clocks_keep_every_boundary_without_repeating_population_inputs() {
    use crate::checkpoint::{RestoreContext, SharedState, Snapshot};
    let mut game = checkpoint_population_game_with_wandering(true);
    let mut shared = SharedState::default();
    let mut states = Vec::new();
    let mut snapshots = Vec::new();
    for _ in 0..64 {
        // Capture ordinary scheduler boundaries, not a raw physics tick with
        // an actor still ready in the past (which is not a saveable state).
        let previous_tick = game.tick();
        while game.tick() == previous_tick {
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        states.push(game.clone());
        snapshots.push(game.checkpoint(&mut shared));
    }
    let encoded = serde_json::to_value((&snapshots, &shared)).unwrap();
    assert_eq!(encoded[1]["floor_activations"].as_array().unwrap().len(), 1);
    assert!(encoded[1]["floor_activations"][0]["wandering"].is_null());
    let clocks: BTreeSet<_> = encoded[0]
        .as_array()
        .unwrap()
        .iter()
        .map(|snapshot| {
            assert_eq!(snapshot["floor_activation"]["state"], serde_json::json!(0));
            assert_eq!(
                snapshot["floor_activation"]["wandering"]["births"],
                serde_json::json!({})
            );
            serde_json::to_string(&snapshot["floor_activation"]["wandering"]["clocks"]).unwrap()
        })
        .collect();
    assert_eq!(clocks.len(), 64, "every changing clock is retained");
    let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    context.bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>()).unwrap();
    for (snapshot, expected) in snapshots.into_iter().zip(states) {
        assert_eq!(context.restore(snapshot).unwrap(), expected);
    }
    assert_eq!(context.population_plan_validation_count(), 1);
}

#[test]
fn wandering_snapshot_shape_and_boundary_validation_remain_strict_after_pool_hit() {
    use crate::checkpoint::{PersistError, RestoreContext, SharedState, Snapshot};
    let game = checkpoint_population_game_with_wandering(true);
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let pooled = serde_json::to_value(&shared).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    let spatial_binding:crate::checkpoint::Snapshot=serde_json::from_value(snapshot.clone()).unwrap();
    context.bind_spatial_roots(&[&spatial_binding]).unwrap();
    assert_eq!(
        context
            .restore(serde_json::from_value(snapshot.clone()).unwrap())
            .unwrap(),
        game
    );
    for replacement in [
        serde_json::json!(0),
        serde_json::json!([0, null]),
        serde_json::json!({"state":0}),
        serde_json::json!({"state":0,"wandering":null,"extra":0}),
    ] {
        let mut invalid = snapshot.clone();
        invalid["floor_activation"] = replacement;
        assert!(serde_json::from_value::<Snapshot>(invalid).is_err());
    }
    let scope = snapshot["floor_activation"]["wandering"]["clocks"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    for (field, value) in [
        ("remaining", serde_json::json!(0)),
        ("settled_at", serde_json::json!(game.tick() + 1)),
        ("active", serde_json::json!(false)),
    ] {
        let mut invalid = snapshot.clone();
        invalid["floor_activation"]["wandering"]["clocks"][&scope][field] = value;
        let invalid: Snapshot = serde_json::from_value(invalid).unwrap();
        assert_eq!(context.restore(invalid), Err(PersistError::Population));
    }
    let mut invalid = pooled;
    invalid["floor_activations"][0]["wandering"] =
        snapshot["floor_activation"]["wandering"].clone();
    assert!(
        serde_json::from_value::<SharedState>(invalid).is_err(),
        "pool must never carry a second clock authority"
    );
}
