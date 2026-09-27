use std::collections::BTreeMap;
use tor_simulation::{Action, ActorId, Game, GameError, ItemId, ItemSpec};
use tor_world::{Location, Position, RegionId};

fn setup() -> (Game, ActorId) {
    let mut game = Game::two_room(42);
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(101).unwrap())
        .unwrap();
    for (id, identity) in [(100, "healing"), (101, "healing"), (102, "poison")] {
        game.place_item_stack(
            id,
            at,
            None,
            10,
            ItemSpec {
                archetype: identity.into(),
                identity: identity.into(),
                name: identity.into(),
                appearance: "red potion".into(),
                concealed: true,
                stackable: true,
                properties: BTreeMap::new(),
            },
        )
        .unwrap();
    }
    (game, actor)
}

#[test]
fn nonstackable_transfers_do_not_scan_for_merge_candidates() {
    let (mut game, actor) = setup();
    let before = tor_simulation::diagnostics::work_counts().stack_candidates;
    game.act(
        actor,
        Action::Take {
            item: ItemId(1),
            quantity: None,
        },
    )
    .unwrap();
    game.act(
        actor,
        Action::Drop {
            item: ItemId(1),
            quantity: None,
        },
    )
    .unwrap();
    assert_eq!(
        tor_simulation::diagnostics::work_counts().stack_candidates,
        before
    );
}

#[test]
fn transfers_conserve_quantities_and_keep_deterministic_ids() {
    let (mut game, actor) = setup();
    game.act(
        actor,
        Action::Take {
            item: ItemId(100),
            quantity: Some(3),
        },
    )
    .unwrap();
    let view = game.observe(actor).unwrap();
    assert_eq!(view.tick, 51);
    assert_eq!(view.inventory[0].quantity, 3);
    let split = view.inventory[0].id;
    assert_eq!(split, ItemId(103));
    game.act(
        actor,
        Action::Take {
            item: ItemId(101),
            quantity: None,
        },
    )
    .unwrap();
    assert_eq!(game.observe(actor).unwrap().inventory[0].quantity, 13);
    game.act(
        actor,
        Action::Drop {
            item: split,
            quantity: Some(2),
        },
    )
    .unwrap();
    let view = game.observe(actor).unwrap();
    assert_eq!(view.inventory[0].quantity, 11);
    assert_eq!(
        view.ground_items
            .iter()
            .find(|i| i.id == ItemId(100))
            .unwrap()
            .quantity,
        9
    );
    game.act(
        actor,
        Action::Take {
            item: ItemId(102),
            quantity: None,
        },
    )
    .unwrap();
    assert_eq!(game.observe(actor).unwrap().inventory.len(), 2);
}

#[test]
fn invalid_transfers_are_atomic_and_knowledge_is_character_owned() {
    let (mut game, actor) = setup();
    let before = game.clone();
    for quantity in [0, 11, u64::MAX] {
        assert!(game
            .act(
                actor,
                Action::Take {
                    item: ItemId(100),
                    quantity: Some(quantity)
                }
            )
            .is_err());
        assert_eq!(game, before);
    }
    assert!(game
        .act(
            actor,
            Action::Drop {
                item: ItemId(100),
                quantity: None
            }
        )
        .is_err());
    assert_eq!(game, before);
    game.identify_item(actor, ItemId(100)).unwrap();
    let view = game.observe(actor).unwrap();
    assert_eq!(
        view.ground_items
            .iter()
            .find(|i| i.id == ItemId(101))
            .unwrap()
            .name,
        "healing"
    );
    assert_eq!(
        view.ground_items
            .iter()
            .find(|i| i.id == ItemId(102))
            .unwrap()
            .name,
        "red potion"
    );
    game.act(
        actor,
        Action::Take {
            item: ItemId(100),
            quantity: None,
        },
    )
    .unwrap();
    game.act(
        actor,
        Action::Drop {
            item: ItemId(100),
            quantity: None,
        },
    )
    .unwrap();
    assert!(game
        .observe(actor)
        .unwrap()
        .ground_items
        .iter()
        .any(|i| i.name == "healing"));
}

#[test]
fn overflow_ownership_and_distinct_properties_cannot_corrupt_stacks() {
    let (mut game, actor) = setup();
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let spec = ItemSpec {
        archetype: "arrow".into(),
        identity: "arrow".into(),
        name: "arrow".into(),
        appearance: "arrow".into(),
        concealed: false,
        stackable: true,
        properties: BTreeMap::new(),
    };
    game.place_item_stack(200, at, Some(actor), u64::MAX, spec.clone())
        .unwrap();
    game.place_item_stack(201, at, None, 1, spec.clone())
        .unwrap();
    let before = game.clone();
    assert!(game
        .act(
            actor,
            Action::Take {
                item: ItemId(201),
                quantity: None
            }
        )
        .is_err());
    assert_eq!(game, before);
    let mut different = spec;
    different.properties.insert("quality".into(), "fine".into());
    game.place_item_stack(202, at, None, 1, different).unwrap();
    game.act(
        actor,
        Action::Take {
            item: ItemId(202),
            quantity: None,
        },
    )
    .unwrap();
    assert_eq!(game.observe(actor).unwrap().inventory.len(), 2);
    let second = game
        .spawn_actor(
            Location {
                position: Position { x: 2, y: 1, z: 0 },
                ..at
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.act(actor, Action::Wait).unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(
            second,
            Action::Drop {
                item: ItemId(200),
                quantity: None
            }
        ),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
    game.identify_item(actor, ItemId(100)).unwrap();
    assert!(game
        .observe(second)
        .unwrap()
        .ground_items
        .iter()
        .filter(|i| i.appearance == "red potion")
        .all(|i| !i.identified));
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(Game::restore_checkpoint(snapshot, &shared).unwrap(), game);
}
