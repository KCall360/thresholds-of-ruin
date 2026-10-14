fn checkpoint_population_game() -> Game {
    let mut game = game();
    let protagonist = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let witness = PopulationStage {
        id: "witness".into(),
        version: 1,
        minimum_depth: 1,
        count: [1, 1],
        population: PopulationContent::Actors {
            choices: vec![PreparedActorChoice {
                content: "witness".into(),
                turn_ticks: 100,
                actor: PreparedActor {
                    build: None,
                    identity: None,
                    anatomy: None,
                    body: None,
                    ai: None,
                    known_identities: vec![],
                    velocity: None,
                    asset: None,
                },
            }],
            avoid_protagonist_sight: false,
        },
    };
    game.configure_population(plan(vec![item_stage(1), witness]))
        .unwrap();
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
    assert_eq!(encoded[0][0]["floor_activation"], serde_json::json!(0));
    assert_eq!(encoded[0][1]["floor_activation"], serde_json::json!(0));
    assert_eq!(encoded[1]["floor_activations"].as_array().unwrap().len(), 1);

    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_value(encoded).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
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
    assert_eq!(
        context
            .restore(serde_json::from_value(snapshot.clone()).unwrap())
            .unwrap(),
        game
    );
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
    }
}

#[test]
fn checkpoint_population_pool_rejects_invalid_references_and_unknown_fields() {
    use crate::checkpoint::{PersistError, RestoreContext, SharedState, Snapshot};
    let game = checkpoint_population_game();
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut context = RestoreContext::new(&shared, game.content.clone());
    for index in [1, usize::MAX] {
        let mut invalid = snapshot.clone();
        invalid["floor_activation"] = serde_json::json!(index);
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
fn checkpoint_without_population_omits_population_pool_and_reference() {
    let game = game();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared = serde_json::to_value(shared).unwrap();
    assert!(snapshot.get("floor_activation").is_none());
    assert!(shared.get("floor_activations").is_none());
}
