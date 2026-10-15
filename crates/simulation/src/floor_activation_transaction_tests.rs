#[test]
fn teleport_with_an_unloaded_population_member_changes_nothing() {
    let mut game = game();
    let protagonist = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_population(plan(vec![item_stage(1)]))
        .unwrap();
    let mut destination = scope(2, vec![at(2, 2, 1)]);
    destination.members.insert(RegionId(3));
    game.register_activation_scope(destination).unwrap();
    let before = game.clone();

    assert_eq!(
        game.teleport(protagonist, at(2, 0, 1)),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, before, "failed entry must not publish relocation");
}

#[test]
fn teleport_population_identity_exhaustion_changes_nothing() {
    let mut game = game();
    let protagonist = game
        .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut stock = item_stage(1);
    stock.count = [2, 2];
    game.configure_population(plan(vec![stock])).unwrap();
    game.register_activation_scope(scope(2, vec![at(2, 2, 1), at(2, 3, 1)]))
        .unwrap();
    // The first placement can allocate; the second must fail. The public
    // relocation must not leave either the moved actor or that stock prefix.
    game.next_item_id = u64::MAX - 1;
    let before = game.clone();

    assert_eq!(
        game.teleport(protagonist, at(2, 0, 1)),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before, "failed entry must discard every partial effect");
}

#[test]
fn public_initial_population_identity_exhaustion_changes_nothing() {
    let mut game = game();
    game
        .spawn_actor(at(1, 0, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut stock = item_stage(1);
    stock.count = [2, 2];
    game.configure_population(plan(vec![stock])).unwrap();
    game.register_activation_scope(scope(1, vec![at(1, 2, 1), at(1, 3, 1)]))
        .unwrap();
    game.next_item_id = u64::MAX - 1;
    let before = game.clone();

    assert_eq!(
        game.activate_initial_floor(),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before, "initial failure must discard every partial effect");
}
