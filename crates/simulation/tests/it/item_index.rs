use std::num::NonZeroU64;
use tor_simulation::{diagnostics::work_counts, Action, Game, ItemId, ItemSpec};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn fixture(hidden: u64) -> (Game, tor_simulation::ActorId) {
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in [1, 2] {
        world
            .add_region(Region {
                id: RegionId(id),
                name: format!("room-{id}"),
                bounds: Extent::new(16, 16, 2).unwrap(),
            })
            .unwrap();
    }
    let at = |region| Location {
        region: RegionId(region),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let mut game = Game::new(world, 42);
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut spec = ItemSpec::ordinary("potion".into());
    spec.stackable = true;
    game.place_item_stack(100, at(1), None, 2, spec.clone())
        .unwrap();
    spec.concealed = true;
    for id in 200..200 + hidden {
        game.place_item_stack(id, at(2), None, 1, spec.clone())
            .unwrap();
    }
    (game, actor)
}

#[test]
fn undisclosed_items_do_not_increase_observation_or_identity_work() {
    for hidden in [16, 256, 4096] {
        let (game, actor) = fixture(hidden);
        let before = work_counts();
        let view = game.observe(actor).unwrap();
        let after = work_counts();
        assert_eq!(view.ground_items.len(), 1);
        assert_eq!(view.ground_items[0].id, ItemId(100));
        assert_eq!(
            after.item_candidates - before.item_candidates,
            1,
            "hidden: {hidden}"
        );
        assert_eq!(after.knowledge_checks - before.knowledge_checks, 0);
    }
}

#[test]
fn unrelated_regions_do_not_add_disclosure_candidates() {
    for regions in [16, 256, 4096] {
        let (mut game, actor) = fixture(0);
        for region in 2..=regions {
            if region != 2 {
                game.add_region(Region {
                    id: RegionId(region),
                    name: format!("unrelated-{region}"),
                    bounds: Extent::new(16, 16, 2).unwrap(),
                })
                .unwrap();
            }
            game.place_item_stack(
                200 + region,
                Location {
                    region: RegionId(region),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                None,
                1,
                ItemSpec::ordinary("unseen weight".into()),
            )
            .unwrap();
        }
        let before = work_counts().item_candidates;
        let view = game.observe(actor).unwrap();
        assert_eq!(view.ground_items.len(), 1);
        assert_eq!(
            work_counts().item_candidates - before,
            1,
            "regions: {regions}"
        );
    }
}

#[test]
fn stack_matching_only_examines_the_destination_location() {
    for hidden in [16, 256, 4096] {
        let (mut game, actor) = fixture(hidden);
        let before = work_counts().stack_candidates;
        game.act(
            actor,
            Action::Take {
                item: ItemId(100),
                quantity: Some(1),
            },
        )
        .unwrap();
        assert_eq!(
            work_counts().stack_candidates - before,
            0,
            "empty inventory, hidden: {hidden}"
        );
        let inventory = game.observe(actor).unwrap().inventory;
        assert_eq!(inventory.len(), 1);
        let before = work_counts().stack_candidates;
        game.act(
            actor,
            Action::Drop {
                item: inventory[0].id,
                quantity: None,
            },
        )
        .unwrap();
        assert_eq!(work_counts().stack_candidates - before, 1);
        assert_eq!(game.observe(actor).unwrap().ground_items[0].quantity, 2);
    }
}

#[test]
fn portal_transfer_and_checkpoint_restore_rebuild_local_indexes() {
    use tor_world::{Direction, Passage};
    let (mut game, actor) = fixture(256);
    let source = Location {
        region: RegionId(1),
        position: Position { x: 15, y: 1, z: 0 },
    };
    let target = Location {
        region: RegionId(2),
        position: Position { x: 0, y: 1, z: 0 },
    };
    game.connect(
        Passage {
            from: source,
            direction: Direction::East,
            to: target,
        },
        0,
    )
    .unwrap();
    game.teleport(actor, source).unwrap();
    game.place_item_stack(
        99,
        target,
        None,
        1,
        ItemSpec::ordinary("portal weight".into()),
    )
    .unwrap();
    assert!(game
        .observe(actor)
        .unwrap()
        .ground_items
        .iter()
        .any(|item| item.id == ItemId(99)));
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.observe(actor).unwrap().location, target);
    game.act(
        actor,
        Action::Take {
            item: ItemId(99),
            quantity: None,
        },
    )
    .unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(restored.observe(actor), game.observe(actor));
    for candidate in [&mut restored, &mut game] {
        candidate
            .act(
                actor,
                Action::Drop {
                    item: ItemId(99),
                    quantity: None,
                },
            )
            .unwrap();
        assert!(candidate.observe(actor).unwrap().inventory.is_empty());
        assert!(candidate
            .observe(actor)
            .unwrap()
            .ground_items
            .iter()
            .any(|item| item.id == ItemId(99) && item.location == target));
        assert!(candidate.set_wall(target, true).is_err());
    }
    assert_eq!(restored, game);
}
